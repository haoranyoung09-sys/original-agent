//! Minimal OpenAI-compatible Chat Completions + tools loop (reqwest).

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::{execute_shell, ToolCtx};
use crate::workspace::resolve_in_workspace;

#[derive(Debug, Clone, Default)]
pub struct OpenAiUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_read_tokens: Option<i32>,
    pub cache_creation_tokens: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiClient {
    pub fn new(api_key: String, base_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .context("创建 HTTP 客户端失败")?;
        Ok(Self {
            http,
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn completions_url(&self) -> String {
        let b = &self.base_url;
        if b.ends_with("/chat/completions") {
            b.clone()
        } else if b.ends_with("/v1") {
            format!("{b}/chat/completions")
        } else {
            // DeepSeek OpenAI root is https://api.deepseek.com → /chat/completions
            format!("{b}/chat/completions")
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn tool_defs() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a text file from the workspace.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "offset": { "type": "integer" },
                        "limit": { "type": "integer" }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write",
                "description": "Create or overwrite a text file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit",
                "description": "Replace a unique exact string in a file.",
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
                "name": "bash",
                "description": "Run a shell command in the workspace.",
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

fn system_prompt(cwd: &Path) -> String {
    let shell = if cfg!(windows) {
        "Windows PowerShell"
    } else {
        "POSIX sh"
    };
    format!(
        "你是 original，一个运行在本地工作区中的终端 coding agent。\n\
         工作区: {}\nShell: {shell}\n\
         工具: read / write / edit / bash。路径相对工作区；改前先 read；不要编造文件内容。",
        cwd.display()
    )
}

fn run_tool(name: &str, args: &Value, root: &Path) -> String {
    let ctx_root = root.to_path_buf();
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
            match resolve_in_workspace(&ctx_root, path) {
                Ok(p) => match std::fs::read_to_string(&p) {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = offset.unwrap_or(1).saturating_sub(1);
                        let end = match limit {
                            Some(lim) => start.saturating_add(lim).min(lines.len()),
                            None => lines.len(),
                        };
                        if start >= lines.len() {
                            format!("(file has {} lines; offset past EOF)", lines.len())
                        } else {
                            let mut out = String::new();
                            for (i, line) in lines[start..end].iter().enumerate() {
                                out.push_str(&format!("{:>6}|{}\n", start + i + 1, line));
                            }
                            out
                        }
                    }
                    Err(e) => format!("read failed: {e}"),
                },
                Err(e) => e.to_string(),
            }
        }
        "write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match resolve_in_workspace(&ctx_root, path) {
                Ok(p) => {
                    if let Some(parent) = p.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&p, content) {
                        Ok(()) => format!("wrote {} bytes to {}", content.len(), p.display()),
                        Err(e) => format!("write failed: {e}"),
                    }
                }
                Err(e) => e.to_string(),
            }
        }
        "edit" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old = args.get("old_str").and_then(|v| v.as_str()).unwrap_or("");
            let new = args.get("new_str").and_then(|v| v.as_str()).unwrap_or("");
            if old.is_empty() {
                return "old_str must not be empty".into();
            }
            match resolve_in_workspace(&ctx_root, path) {
                Ok(p) => match std::fs::read_to_string(&p) {
                    Ok(content) => {
                        let matches = content.matches(old).count();
                        if matches == 0 {
                            "old_str not found".into()
                        } else if matches > 1 {
                            format!("old_str found {matches} times; need unique snippet")
                        } else {
                            let updated = content.replacen(old, new, 1);
                            match std::fs::write(&p, updated) {
                                Ok(()) => format!("edited {} (1 replacement)", p.display()),
                                Err(e) => format!("edit write failed: {e}"),
                            }
                        }
                    }
                    Err(e) => format!("edit read failed: {e}"),
                },
                Err(e) => e.to_string(),
            }
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
            let (code, stdout, stderr) = execute_shell(command, &ctx_root, timeout);
            let mut body = format!("exit_code: {code}\n");
            if !stdout.is_empty() {
                body.push_str("stdout:\n");
                body.push_str(&stdout);
                body.push('\n');
            }
            if !stderr.is_empty() {
                body.push_str("stderr:\n");
                body.push_str(&stderr);
                body.push('\n');
            }
            body
        }
        other => format!("unknown tool: {other}"),
    }
}

fn extract_usage(v: &Value) -> OpenAiUsage {
    let u = v.get("usage");
    let input = u
        .and_then(|u| u.get("prompt_tokens").or_else(|| u.get("input_tokens")))
        .and_then(|x| x.as_i64())
        .unwrap_or(0) as i32;
    let output = u
        .and_then(|u| {
            u.get("completion_tokens")
                .or_else(|| u.get("output_tokens"))
        })
        .and_then(|x| x.as_i64())
        .unwrap_or(0) as i32;
    // Some gateways expose prompt_tokens_details.cached_tokens
    let cache_read = u
        .and_then(|u| u.get("prompt_tokens_details"))
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|x| x.as_i64())
        .map(|n| n as i32);
    OpenAiUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: None,
    }
}

pub async fn take_turn(
    client: &OpenAiClient,
    model: &str,
    cwd: &Path,
    history: &mut Vec<ChatMessage>,
    user_text: &str,
    max_tokens: u32,
) -> Result<OpenAiUsage> {
    if history.is_empty() {
        history.push(ChatMessage {
            role: "system".into(),
            content: Some(Value::String(system_prompt(cwd))),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
    history.push(ChatMessage {
        role: "user".into(),
        content: Some(Value::String(user_text.to_string())),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });

    let mut total = OpenAiUsage::default();
    let _tool_ctx = ToolCtx {
        root: Arc::new(cwd.to_path_buf()),
    };

    for _round in 0..24 {
        let body = json!({
            "model": model,
            "messages": history,
            "tools": tool_defs(),
            "tool_choice": "auto",
            "max_tokens": max_tokens,
        });

        let resp = client
            .http
            .post(client.completions_url())
            .header("Authorization", format!("Bearer {}", client.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("OpenAI 兼容请求失败")?;

        let status = resp.status();
        let text = resp.text().await.context("读取响应失败")?;
        if !status.is_success() {
            bail!("API {status}: {text}");
        }
        let v: Value = serde_json::from_str(&text).context("解析 JSON 失败")?;
        let usage = extract_usage(&v);
        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        if let Some(cr) = usage.cache_read_tokens {
            total.cache_read_tokens = Some(total.cache_read_tokens.unwrap_or(0) + cr);
        }

        let choice = v
            .pointer("/choices/0")
            .cloned()
            .context("响应缺少 choices")?;
        let message = choice.get("message").cloned().context("响应缺少 message")?;

        let content = message.get("content").cloned();
        let tool_calls = message.get("tool_calls").cloned();

        history.push(ChatMessage {
            role: "assistant".into(),
            content: content.clone(),
            tool_calls: tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        if let Some(Value::String(s)) = &content {
            if !s.trim().is_empty() {
                println!("assistant:\n{s}\n");
            }
        } else if let Some(Value::Array(parts)) = &content {
            // Some models return content parts
            let mut buf = String::new();
            for p in parts {
                if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                    buf.push_str(t);
                }
            }
            if !buf.trim().is_empty() {
                println!("assistant:\n{buf}\n");
            }
        }

        let Some(Value::Array(calls)) = tool_calls else {
            return Ok(total);
        };
        if calls.is_empty() {
            return Ok(total);
        }

        for call in calls {
            let id = call
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("call")
                .to_string();
            let fname = call
                .pointer("/function/name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let args_raw = call
                .pointer("/function/arguments")
                .and_then(|x| x.as_str())
                .unwrap_or("{}");
            let args: Value = serde_json::from_str(args_raw).unwrap_or(json!({}));
            let short = if args_raw.len() > 120 {
                format!("{}…", args_raw.chars().take(120).collect::<String>())
            } else {
                args_raw.to_string()
            };
            println!("  → tool {fname}({short})");
            let result = run_tool(&fname, &args, cwd);
            let preview: String = result.chars().take(120).collect();
            println!("  ✓ {}", preview.replace('\n', " "));
            history.push(ChatMessage {
                role: "tool".into(),
                content: Some(Value::String(result)),
                tool_calls: None,
                tool_call_id: Some(id),
                name: Some(fname),
            });
        }
    }

    bail!("工具循环超过上限")
}
