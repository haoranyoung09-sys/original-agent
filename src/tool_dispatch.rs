//! Shared tool-name dispatch used by Anthropic wrappers and the OpenAI loop.

use std::path::Path;

use serde_json::Value;

use crate::fsops::{
    edit_once, glob_files, grep_workspace, multi_edit, project_overview, read_text,
    suggest_verify_command, write_text, FileEdit, ReadRange,
};
use crate::shell::{execute_shell, shell_enabled};

pub fn openai_tool_defs() -> Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a text file from the workspace (1-based offset/limit). Prefer before editing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to workspace root" },
                        "offset": { "type": "integer", "description": "1-based start line" },
                        "limit": { "type": "integer", "description": "Max lines to return" }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write",
                "description": "Create or overwrite a text file in the workspace.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string", "description": "Full file contents" }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit",
                "description": "Replace one unique exact string in a file (str_replace).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "old_str": { "type": "string" },
                        "new_str": { "type": "string" }
                    },
                    "required": ["path", "old_str", "new_str"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "multi_edit",
                "description": "Apply multiple unique str_replace edits across one or more files in one batch. Validates all edits before writing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "path": { "type": "string" },
                                    "old_str": { "type": "string" },
                                    "new_str": { "type": "string" }
                                },
                                "required": ["path", "old_str", "new_str"]
                            }
                        }
                    },
                    "required": ["edits"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "glob",
                "description": "List workspace files matching a glob (supports * ? **/). No shell.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "e.g. src/**/*.rs" }
                    },
                    "required": ["pattern"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "grep",
                "description": "Search file contents in the workspace (literal substring). Optional glob filter. No shell.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "glob": { "type": "string", "description": "Optional file glob filter" }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "project_overview",
                "description": "Summarize project type, shallow tree, and README snippet for vibe coding orientation.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "verify",
                "description": "Run project tests/build feedback. Requires --allow-shell. Omit command to use an auto suggestion (cargo test / npm test / …).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Optional override command" },
                        "timeout_secs": { "type": "integer" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Run a shell command in the workspace. Disabled unless original --allow-shell.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout_secs": { "type": "integer" }
                    },
                    "required": ["command"]
                }
            }
        }
    ])
}

pub fn dispatch(name: &str, args: &Value, root: &Path) -> String {
    match name {
        "read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let offset = args
                .get("offset")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            match read_text(root, path, ReadRange { offset, limit }) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match write_text(root, path, content) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "edit" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old = args.get("old_str").and_then(|v| v.as_str()).unwrap_or("");
            let new = args.get("new_str").and_then(|v| v.as_str()).unwrap_or("");
            match edit_once(root, path, old, new) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "multi_edit" => {
            let Some(arr) = args.get("edits").and_then(|v| v.as_array()) else {
                return "invalid multi_edit input: edits array required".into();
            };
            let mut edits = Vec::new();
            for (i, item) in arr.iter().enumerate() {
                let path = item
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let old_str = item
                    .get("old_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let new_str = item
                    .get("new_str")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if path.is_empty() {
                    return format!("invalid multi_edit edits[{i}]: path required");
                }
                edits.push(FileEdit {
                    path,
                    old_str,
                    new_str,
                });
            }
            match multi_edit(root, &edits) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "glob" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            match glob_files(root, pattern) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "grep" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let glob = args.get("glob").and_then(|v| v.as_str());
            match grep_workspace(root, query, glob) {
                Ok(s) => s,
                Err(e) => e.to_string(),
            }
        }
        "project_overview" => match project_overview(root) {
            Ok(s) => s,
            Err(e) => e.to_string(),
        },
        "verify" => {
            let command = args.get("command").and_then(|v| v.as_str());
            let timeout = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(180)
                .clamp(1, 600);
            run_verify(root, command, timeout)
        }
        "bash" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let timeout = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(60)
                .clamp(1, 600);
            if command.trim().is_empty() {
                return "command must not be empty".into();
            }
            format_shell_result(execute_shell(command, &root.to_path_buf(), timeout))
        }
        other => format!("unknown tool: {other}"),
    }
}

pub fn run_verify(root: &Path, command: Option<&str>, timeout_secs: u64) -> String {
    if !shell_enabled() {
        let suggested = suggest_verify_command(root).unwrap_or("cargo test / npm test / …");
        return format!(
            "verify blocked: shell disabled. Restart with --allow-shell to run tests/builds.\nSuggested: {suggested}"
        );
    }
    let cmd = match command.map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) => c.to_string(),
        None => match suggest_verify_command(root) {
            Some(c) => c.to_string(),
            None => {
                return "No auto verify command detected for this workspace; pass `command` explicitly."
                    .into()
            }
        },
    };
    let mut body = format!("verify command: {cmd}\n");
    body.push_str(&format_shell_result(execute_shell(
        &cmd,
        &root.to_path_buf(),
        timeout_secs,
    )));
    body
}

fn format_shell_result(result: (i32, String, String)) -> String {
    let (code, stdout, stderr) = result;
    let mut body = format!("exit_code: {code}\n");
    if !stdout.is_empty() {
        body.push_str("stdout:\n");
        body.push_str(&truncate_chars(&stdout, 60_000));
        body.push('\n');
    }
    if !stderr.is_empty() {
        body.push_str("stderr:\n");
        body.push_str(&truncate_chars(&stderr, 20_000));
        body.push('\n');
    }
    body
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}\n…(truncated)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::set_shell_enabled;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("original-dispatch-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), "pub fn x() -> i32 { 1 }\n").unwrap();
        dir
    }

    #[test]
    fn dispatch_read_and_multi_edit() {
        let root = temp_root();
        let read = dispatch("read", &serde_json::json!({"path":"src/lib.rs"}), &root);
        assert!(read.contains("pub fn x"));
        let edited = dispatch(
            "multi_edit",
            &serde_json::json!({
                "edits": [{"path":"src/lib.rs","old_str":"1","new_str":"2"}]
            }),
            &root,
        );
        assert!(edited.contains("replacement"));
        assert!(fs::read_to_string(root.join("src/lib.rs"))
            .unwrap()
            .contains("2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_blocked_without_shell() {
        set_shell_enabled(false);
        let root = temp_root();
        let msg = dispatch("verify", &serde_json::json!({}), &root);
        assert!(msg.contains("shell disabled"));
        assert!(msg.contains("cargo test"));
        let bash = dispatch("bash", &serde_json::json!({"command":"echo hi"}), &root);
        assert!(bash.contains("126") || bash.contains("Shell disabled"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tool_defs_include_vibe_tools() {
        let defs = openai_tool_defs();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.pointer("/function/name")?.as_str())
            .collect();
        for need in [
            "read",
            "write",
            "edit",
            "multi_edit",
            "glob",
            "grep",
            "project_overview",
            "verify",
            "bash",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
    }
}
