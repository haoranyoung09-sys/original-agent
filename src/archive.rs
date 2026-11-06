//! Local append-only conversation archives. No credential files are archived.
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
struct Archive {
    dir: PathBuf,
    secrets: Vec<String>,
}
static ACTIVE: OnceLock<Mutex<Option<Archive>>> = OnceLock::new();
fn active() -> &'static Mutex<Option<Archive>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}
fn base() -> Result<PathBuf> {
    Ok(crate::config::Config::config_dir()?.join("sessions"))
}
fn private_write(path: &Path, text: &str, append: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()?;
    Ok(())
}
fn redact(text: &str, secrets: &[String]) -> String {
    let mut result = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            result = result.replace(secret, "[REDACTED]");
        }
    }
    result
        .lines()
        .map(|line| {
            if line.to_ascii_lowercase().contains("authorization:")
                || line.to_ascii_lowercase().contains("bearer ")
            {
                "[REDACTED authentication line]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn start(root: &Path, model: &str, disabled: bool) -> Result<()> {
    if disabled {
        return Ok(());
    }
    let id = format!(
        "{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    let dir = base()?.join(&id);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    private_write(&dir.join("metadata.json"), &json!({"id":id,"cwd":root,"model":model,"started_unix":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()}).to_string(), false)?;
    *active().lock().unwrap() = Some(Archive {
        dir,
        secrets: Vec::new(),
    });
    Ok(())
}
pub fn record(kind: &str, data: Value) {
    let mut guard = active().lock().unwrap();
    let Some(a) = guard.as_mut() else {
        return;
    };
    if let Ok(cfg) = crate::config::Config::load() {
        if let Some(k) = cfg.api_key {
            a.secrets.push(k);
        }
        if let Some(k) = crate::config::Config::resolve_api_key_from_env() {
            a.secrets.push(k);
        }
    }
    a.secrets.sort();
    a.secrets.dedup();
    let payload = redact(&data.to_string(), &a.secrets);
    let event = json!({"kind":kind,"time_unix":SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_secs()).unwrap_or(0),"text":payload});
    if let Err(e) = private_write(
        &a.dir.join("events.jsonl"),
        &(event.to_string() + "\n"),
        true,
    ) {
        eprintln!("Archive write failed: {e}");
    }
}
fn checked_id(id: &str) -> Result<PathBuf> {
    if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit() || c == b'-') {
        bail!("Invalid session ID");
    }
    let root = base()?.canonicalize()?;
    let dir = root.join(id).canonicalize()?;
    if !dir.starts_with(root) {
        bail!("Session path escapes archive directory");
    }
    Ok(dir)
}
fn markdown(dir: &Path) -> Result<String> {
    let mut out = String::from("# original session\n\n");
    let path = dir.join("events.jsonl");
    if !path.exists() {
        return Ok(out);
    }
    for line in fs::read_to_string(path)?.lines() {
        match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                out.push_str(&format!("## {}\n\n", v["kind"].as_str().unwrap_or("event")));
                for line in v["text"].as_str().unwrap_or("").lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                out.push('\n');
            }
            Err(_) => out.push_str("[Unreadable or incomplete event]\n"),
        }
    }
    Ok(out)
}
pub fn command(line: &str, cwd: &Path) -> Option<Result<String>> {
    if line == "/sessions" {
        return Some((|| {
            let root = base()?;
            if !root.exists() {
                return Ok("No sessions".into());
            }
            let mut rows = Vec::new();
            for entry in fs::read_dir(root)? {
                let dir = entry?.path();
                if let Ok(raw) = fs::read_to_string(dir.join("metadata.json")) {
                    if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                        if v["cwd"].as_str() == Some(cwd.to_string_lossy().as_ref()) {
                            rows.push(format!(
                                "{}  {}",
                                v["id"].as_str().unwrap_or("?"),
                                v["model"].as_str().unwrap_or("?")
                            ));
                        }
                    }
                }
            }
            rows.sort();
            Ok(rows.join("\n"))
        })());
    }
    if let Some(id) = line.strip_prefix("/session show ") {
        return Some(checked_id(id.trim()).and_then(|dir| markdown(&dir)));
    }
    if line == "/export" {
        return Some((|| {
            let guard = active().lock().unwrap();
            let a = guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Recording is disabled"))?;
            let path = a.dir.join("summary.md");
            private_write(&path, &markdown(&a.dir)?, false)?;
            Ok(path.display().to_string())
        })());
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_events_and_exports_partial_log() {
        let dir =
            std::env::temp_dir().join(format!("original-archive-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.jsonl");
        private_write(&path, "{\"kind\":\"user\",\"text\":\"hello\"}\n", false).unwrap();
        private_write(&path, "{incomplete", true).unwrap();
        let text = markdown(&dir).unwrap();
        assert!(text.contains("hello"));
        assert!(text.contains("incomplete event"));
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn rejects_traversal() {
        assert!(checked_id("../secret").is_err());
    }
    #[test]
    fn removes_known_keys_and_auth_lines() {
        assert_eq!(
            redact("key abc\nAuthorization: Bearer x", &["abc".into()]),
            "key [REDACTED]\n[REDACTED authentication line]"
        );
    }
}
