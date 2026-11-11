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

const TOPIC_WIDTH: usize = 24;

struct Archive {
    dir: PathBuf,
    secrets: Vec<String>,
    topic: Option<String>,
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

fn cell_width(c: char) -> usize {
    if c.is_ascii() || c == '·' || c == '…' {
        1
    } else {
        2
    }
}

fn compact_topic(text: &str) -> String {
    let mut collapsed = String::new();
    let mut gap = false;
    for c in text.chars() {
        if c.is_control() || c.is_whitespace() {
            if !collapsed.is_empty() {
                gap = true;
            }
            continue;
        }
        if gap {
            collapsed.push(' ');
            gap = false;
        }
        collapsed.push(c);
    }

    let cutoff = collapsed.find(['。', '！', '？', '.', '!', '?']);
    let source = if let Some(end) = cutoff {
        let sentence = collapsed[..end].trim();
        if substantial(sentence) {
            sentence
        } else {
            collapsed.as_str()
        }
    } else {
        collapsed.as_str()
    };

    let chars: Vec<char> = source.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    let mut used = 0;
    for (i, c) in chars.iter().copied().enumerate() {
        let w = cell_width(c);
        let last = i + 1 == chars.len();
        if used + w > TOPIC_WIDTH {
            while used + 1 > TOPIC_WIDTH && !out.is_empty() {
                if let Some(popped) = out.pop() {
                    used -= cell_width(popped);
                }
            }
            if used < TOPIC_WIDTH {
                out.push('…');
            }
            break;
        }
        out.push(c);
        used += w;
        if last {
            break;
        }
    }
    out
}

fn substantial(text: &str) -> bool {
    text.chars()
        .filter(|c| c.is_alphanumeric() || !c.is_ascii())
        .count()
        >= 2
}

fn topic_from_user_payload(data: &Value) -> Option<String> {
    let text = data.as_str()?;
    let topic = compact_topic(text);
    substantial(&topic).then_some(topic)
}

fn event_payload(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!(raw))
}

fn upsert_topic(dir: &Path, topic: &str) -> Result<()> {
    let path = dir.join("metadata.json");
    let mut value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("topic".into(), json!(topic));
    }
    private_write(&path, &value.to_string(), false)
}

fn maybe_set_topic(archive: &mut Archive, data: &Value) {
    if archive
        .topic
        .as_ref()
        .is_some_and(|topic| !topic.is_empty())
    {
        return;
    }
    let Some(topic) = topic_from_user_payload(data) else {
        return;
    };
    let topic = redact(&topic, &archive.secrets);
    if topic.trim().is_empty() {
        return;
    }
    if upsert_topic(&archive.dir, &topic).is_ok() {
        archive.topic = Some(topic);
    }
}

fn topic_from_events(dir: &Path) -> Option<String> {
    let path = dir.join("events.jsonl");
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value["kind"].as_str() != Some("user") {
            continue;
        }
        if let Some(raw) = value["text"].as_str() {
            if let Some(topic) = topic_from_user_payload(&event_payload(raw)) {
                return Some(topic);
            }
        }
    }
    None
}

fn session_topic(metadata: &Value, dir: &Path) -> String {
    metadata["topic"]
        .as_str()
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| topic_from_events(dir))
        .filter(|topic| !topic.is_empty())
        .unwrap_or_else(|| "未命名会话".into())
}

fn list_sessions(cwd: &Path, compact: bool) -> Result<String> {
    let root = base()?;
    if !root.exists() {
        return Ok("No sessions".into());
    }
    let mut rows = Vec::new();
    for entry in fs::read_dir(root)? {
        let dir = entry?.path();
        let Ok(raw) = fs::read_to_string(dir.join("metadata.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if value["cwd"].as_str() != Some(cwd.to_string_lossy().as_ref()) {
            continue;
        }
        let id = value["id"].as_str().unwrap_or("?").to_string();
        let topic = session_topic(&value, &dir);
        let model = value["model"].as_str().unwrap_or("?");
        let line = if compact {
            topic
        } else {
            format!("{topic}  ·  {model}\n  {id}")
        };
        rows.push((id, line));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    if rows.is_empty() {
        return Ok("当前项目暂无会话记录".into());
    }
    rows.reverse();
    Ok(rows
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n"))
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
    private_write(
        &dir.join("metadata.json"),
        &json!({
            "id": id,
            "cwd": root,
            "model": model,
            "started_unix": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
        })
        .to_string(),
        false,
    )?;
    *active().lock().unwrap() = Some(Archive {
        dir,
        secrets: Vec::new(),
        topic: None,
    });
    Ok(())
}

pub fn record(kind: &str, data: Value) {
    let mut guard = active().lock().unwrap();
    let Some(archive) = guard.as_mut() else {
        return;
    };
    if let Ok(cfg) = crate::config::Config::load() {
        if let Some(key) = cfg.api_key {
            archive.secrets.push(key);
        }
        if let Some(key) = crate::config::Config::resolve_api_key_from_env() {
            archive.secrets.push(key);
        }
    }
    archive.secrets.sort();
    archive.secrets.dedup();
    if kind == "user" {
        maybe_set_topic(archive, &data);
    }
    let payload = redact(&data.to_string(), &archive.secrets);
    let event = json!({
        "kind": kind,
        "time_unix": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "text": payload
    });
    if let Err(e) = private_write(
        &archive.dir.join("events.jsonl"),
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
    if !dir.starts_with(&root) {
        bail!("Session path escapes archive directory");
    }
    Ok(dir)
}

fn markdown(dir: &Path) -> Result<String> {
    let topic = fs::read_to_string(dir.join("metadata.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|value| session_topic(&value, dir))
        .unwrap_or_else(|| "original session".into());
    let mut out = format!("# {topic}\n\n");
    let path = dir.join("events.jsonl");
    if !path.exists() {
        return Ok(out);
    }
    for line in fs::read_to_string(path)?.lines() {
        match serde_json::from_str::<Value>(line) {
            Ok(value) => {
                out.push_str(&format!(
                    "## {}\n\n",
                    value["kind"].as_str().unwrap_or("event")
                ));
                for line in value["text"].as_str().unwrap_or("").lines() {
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
        return Some(list_sessions(cwd, true));
    }
    if line == "/history" {
        return Some(list_sessions(cwd, false));
    }
    if let Some(id) = line
        .strip_prefix("/session show ")
        .or_else(|| line.strip_prefix("/history "))
    {
        return Some(checked_id(id.trim()).and_then(|dir| markdown(&dir)));
    }
    if line == "/export" {
        return Some((|| {
            let guard = active().lock().unwrap();
            let archive = guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Recording is disabled"))?;
            let path = archive.dir.join("summary.md");
            private_write(&path, &markdown(&archive.dir)?, false)?;
            Ok(path.display().to_string())
        })());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "original-archive-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_events_and_exports_partial_log() {
        let dir = temp_dir("partial");
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

    #[test]
    fn compact_topic_keeps_first_sentence_and_truncates() {
        assert_eq!(
            compact_topic("  修复  侧栏颜色。随后再改文档。"),
            "修复 侧栏颜色"
        );
        assert_eq!(compact_topic("ok"), "ok");
        let long = "我想让history自动总结当前会话的主题并显示在侧栏和列表里";
        let topic = compact_topic(long);
        assert!(topic.ends_with('…'), "{topic}");
        assert!(topic.starts_with("我想让history"), "{topic}");
        assert!(topic.chars().map(cell_width).sum::<usize>() <= TOPIC_WIDTH);
    }

    #[test]
    fn session_topic_prefers_metadata_then_first_user_event() {
        let dir = temp_dir("topic");
        private_write(
            &dir.join("events.jsonl"),
            "{\"kind\":\"user\",\"text\":\"\\\"为 history 自动总结主题\\\"\"}\n",
            false,
        )
        .unwrap();
        private_write(
            &dir.join("metadata.json"),
            "{\"id\":\"1\",\"model\":\"m\"}",
            false,
        )
        .unwrap();
        let metadata = json!({"id":"1","model":"m"});
        assert_eq!(session_topic(&metadata, &dir), "为 history 自动总结主题");
        let named = json!({"id":"1","model":"m","topic":"固定主题"});
        assert_eq!(session_topic(&named, &dir), "固定主题");
        fs::remove_file(dir.join("events.jsonl")).unwrap();
        fs::remove_file(dir.join("metadata.json")).unwrap();
        let empty = temp_dir("empty-topic");
        assert_eq!(session_topic(&json!({}), &empty), "未命名会话");
        fs::remove_dir(empty).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn first_user_payload_sets_topic_once_and_redacts_secrets() {
        let dir = temp_dir("set-topic");
        private_write(
            &dir.join("metadata.json"),
            "{\"id\":\"1\",\"cwd\":\".\"}",
            false,
        )
        .unwrap();
        let mut archive = Archive {
            dir: dir.clone(),
            secrets: vec!["sk-secret".into()],
            topic: None,
        };
        maybe_set_topic(&mut archive, &json!("用 sk-secret 改文档"));
        assert_eq!(archive.topic.as_deref(), Some("用 [REDACTED] 改文档"));
        maybe_set_topic(&mut archive, &json!("第二条消息不应覆盖主题"));
        assert_eq!(archive.topic.as_deref(), Some("用 [REDACTED] 改文档"));
        let saved: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(saved["topic"].as_str(), Some("用 [REDACTED] 改文档"));
        fs::remove_file(dir.join("metadata.json")).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
