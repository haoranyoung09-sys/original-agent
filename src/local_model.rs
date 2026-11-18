//! Local GGUF model discovery (Ollama blobs + standalone files) and llama-server lifecycle.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub const DEFAULT_LOCAL_CONTEXT_WINDOW: u32 = 32_768;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub display_name: String,
    pub model_id: String,
    pub gguf_path: PathBuf,
    pub size_bytes: u64,
    pub source: &'static str,
}

pub struct LocalServerGuard {
    child: Child,
    pub base_url: String,
}

impl Drop for LocalServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB)
    } else {
        format!("{:.0} MB", (bytes as f64 / MIB).max(1.0))
    }
}

pub fn ollama_models_dir() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("OLLAMA_MODELS") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    dirs::home_dir().map(|h| h.join(".ollama").join("models"))
}

pub fn local_models_dir() -> Option<PathBuf> {
    crate::config::Config::config_dir()
        .ok()
        .map(|d| d.join("models"))
}

pub fn discover_local_models() -> Vec<DiscoveredModel> {
    let mut found = Vec::new();
    if let Some(dir) = ollama_models_dir() {
        found.extend(discover_ollama_models_in(&dir));
    }
    if let Some(dir) = local_models_dir() {
        found.extend(discover_gguf_dir_in(&dir));
    }
    found.sort_by(|a, b| a.model_id.cmp(&b.model_id));
    found
}

pub fn discover_ollama_models_in(models_root: &Path) -> Vec<DiscoveredModel> {
    let manifests_dir = models_root.join("manifests");
    let blobs_dir = models_root.join("blobs");
    if !manifests_dir.is_dir() || !blobs_dir.is_dir() {
        return Vec::new();
    }

    let mut files = Vec::new();
    collect_files_recursive(&manifests_dir, 0, &mut files);
    let mut models = Vec::new();
    for manifest_path in files {
        let Ok(rel) = manifest_path.strip_prefix(&manifests_dir) else {
            continue;
        };
        let Some(model_id) = format_ollama_manifest_rel(rel) else {
            continue;
        };
        let Some((blob_path, size_bytes)) = parse_ollama_manifest(&manifest_path, &blobs_dir)
        else {
            continue;
        };
        models.push(DiscoveredModel {
            display_name: format!(
                "{model_id} [Ollama 离线权重 · {}]",
                format_bytes(size_bytes)
            ),
            model_id,
            gguf_path: blob_path,
            size_bytes,
            source: "ollama",
        });
    }
    models
}

pub fn discover_gguf_dir_in(dir: &Path) -> Vec<DiscoveredModel> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files = Vec::new();
    collect_files_recursive(dir, 0, &mut files);
    let mut models = Vec::new();
    for path in files {
        let is_gguf = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
        if !is_gguf {
            continue;
        }
        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let model_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("local-model")
            .to_string();
        models.push(DiscoveredModel {
            display_name: format!("{model_id} [本地 GGUF · {}]", format_bytes(size_bytes)),
            model_id,
            gguf_path: path,
            size_bytes,
            source: "gguf",
        });
    }
    models
}

fn collect_files_recursive(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, depth + 1, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn format_ollama_manifest_rel(rel: &Path) -> Option<String> {
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(ToOwned::to_owned))
        .collect();
    if parts.len() < 2 {
        return None;
    }
    let tag = &parts[parts.len() - 1];
    let model = &parts[parts.len() - 2];
    if parts.len() >= 4 {
        let registry = &parts[0];
        let namespace = &parts[parts.len() - 3];
        if registry == "registry.ollama.ai" && namespace == "library" {
            return Some(format!("{model}:{tag}"));
        }
        if registry == "registry.ollama.ai" {
            return Some(format!("{namespace}/{model}:{tag}"));
        }
    }
    Some(format!("{model}:{tag}"))
}

fn parse_ollama_manifest(manifest_path: &Path, blobs_dir: &Path) -> Option<(PathBuf, u64)> {
    let raw = fs::read_to_string(manifest_path).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let layers = json.get("layers")?.as_array()?;
    for layer in layers {
        let media = layer.get("mediaType").and_then(Value::as_str)?;
        if media == "application/vnd.ollama.image.model" {
            let digest = layer.get("digest").and_then(Value::as_str)?;
            let blob_name = digest.replace(':', "-");
            let blob_path = blobs_dir.join(blob_name);
            if blob_path.is_file() {
                let size = layer
                    .get("size")
                    .and_then(Value::as_u64)
                    .or_else(|| fs::metadata(&blob_path).ok().map(|m| m.len()))
                    .unwrap_or(0);
                return Some((blob_path, size));
            }
        }
    }
    None
}

pub fn find_llama_server() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("ORIGINAL_LLAMA_SERVER") {
        let p = PathBuf::from(custom.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    let bin_name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    if let Ok(dir) = crate::config::Config::config_dir() {
        let managed = dir.join("bin").join(bin_name);
        if managed.is_file() {
            return Some(managed);
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn allocate_local_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("无法分配本地监听端口")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

fn log_file_path() -> Option<PathBuf> {
    crate::config::Config::config_dir()
        .ok()
        .map(|d| d.join("llama-server.log"))
}

fn tail_log(path: &Path, max_lines: usize) -> String {
    let Ok(file) = File::open(path) else {
        return String::new();
    };
    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

pub async fn start_llama_server(
    gguf_path: &Path,
    alias: &str,
    context_window: u32,
) -> Result<LocalServerGuard> {
    if !gguf_path.is_file() {
        bail!("找不到本地模型文件: {}", gguf_path.display());
    }
    let server_bin = find_llama_server().with_context(|| {
        let hint_dir = crate::config::Config::config_dir()
            .map(|d| d.join("bin"))
            .unwrap_or_else(|_| PathBuf::from("~/.original-agent/bin"));
        format!(
            "未找到 llama-server（无需运行 Ollama）。请下载预编译 llama.cpp（推荐 Windows Vulkan 或 CPU 版）并将其中的 llama-server{} 放到 {} 或加入 PATH（或设置 ORIGINAL_LLAMA_SERVER）",
            if cfg!(windows) { ".exe" } else { "" },
            hint_dir.display()
        )
    })?;

    let port = allocate_local_port()?;
    let ctx = context_window.max(2048);

    let log_path = log_file_path();
    let (stdout_cfg, stderr_cfg) = match &log_path {
        Some(lp) => {
            if let Some(parent) = lp.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(f1) = File::create(lp) {
                if let Ok(f2) = f1.try_clone() {
                    (Stdio::from(f1), Stdio::from(f2))
                } else {
                    (Stdio::null(), Stdio::null())
                }
            } else {
                (Stdio::null(), Stdio::null())
            }
        }
        None => (Stdio::null(), Stdio::null()),
    };

    let mut cmd = Command::new(&server_bin);
    cmd.arg("-m")
        .arg(gguf_path)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--jinja",
            "-c",
            &ctx.to_string(),
            "--alias",
            alias,
        ])
        .stdin(Stdio::null())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("启动 llama-server 失败: {}", server_bin.display()))?;

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let health_url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + Duration::from_secs(90);

    eprintln!("正在通过 llama-server 加载本地模型 {alias} (port {port})...");
    loop {
        if let Some(status) = child.try_wait()? {
            let tail = log_path
                .as_deref()
                .map(|p| tail_log(p, 12))
                .unwrap_or_default();
            bail!("llama-server 提前退出 ({status})\n{tail}");
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok() {
            if let Ok(resp) = http.get(&health_url).send().await {
                if resp.status().is_success() {
                    return Ok(LocalServerGuard {
                        child,
                        base_url: format!("http://127.0.0.1:{port}/v1"),
                    });
                }
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let tail = log_path
                .as_deref()
                .map(|p| tail_log(p, 12))
                .unwrap_or_default();
            bail!("等待 llama-server 就绪超时 (90s)\n{tail}");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discovers_ollama_manifest_and_blob() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("orig-ollama-test-{n}"));
        let manifest = root
            .join("manifests")
            .join("registry.ollama.ai")
            .join("library")
            .join("gemma4")
            .join("e4b");
        let blob = root.join("blobs").join("sha256-deadbeef");
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::create_dir_all(blob.parent().unwrap()).unwrap();
        fs::write(&blob, b"GGUF-mock-bytes").unwrap();
        fs::write(
            &manifest,
            r#"{"layers":[{"mediaType":"application/vnd.ollama.image.model","digest":"sha256:deadbeef","size":15}]}"#,
        )
        .unwrap();

        let models = discover_ollama_models_in(&root);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "gemma4:e4b");
        assert_eq!(models[0].gguf_path, blob);
        assert_eq!(models[0].size_bytes, 15);
        assert_eq!(models[0].source, "ollama");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_standalone_gguf_files() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("orig-gguf-test-{n}"));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("Qwen3-8B-Q4_K_M.gguf");
        fs::write(&file, b"gguf").unwrap();
        fs::write(root.join("ignore.txt"), b"txt").unwrap();

        let models = discover_gguf_dir_in(&root);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "Qwen3-8B-Q4_K_M");
        assert_eq!(models[0].gguf_path, file);
        assert_eq!(models[0].source, "gguf");
        let _ = fs::remove_dir_all(&root);
    }
}
