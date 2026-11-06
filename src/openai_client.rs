//! Minimal OpenAI-compatible Chat Completions + tools loop (reqwest).

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool_dispatch::{dispatch, openai_tool_defs};
use crate::tools::ToolCtx;

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

    #[cfg(test)]
    pub fn completions_url_for_test(&self) -> String {
        self.completions_url()
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

fn system_prompt(cwd: &Path) -> String {
    let shell = if cfg!(windows) {
        "Windows PowerShell（bash/verify 默认禁用，需 --allow-shell）"
    } else {
        "POSIX sh（bash/verify 默认禁用，需 --allow-shell）"
    };
    format!(
        "你是 original，一个运行在本地工作区中的终端 vibe-coding agent。\n\
         工作区: {}\nShell: {shell}\n\
         工具: project_overview / glob / grep / read / write / edit / multi_edit / verify / bash。\n\
         先摸清项目再改；路径相对工作区；小改 edit、多处 multi_edit；不要编造文件内容。",
        cwd.display()
    )
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
            "tools": openai_tool_defs(),
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

        if crate::tool_display::thinking_enabled() {
            if let Some(t) = message.get("reasoning_content").and_then(Value::as_str) {
                println!("[provider thinking]\n{t}");
            }
        }
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
            let result = dispatch(&fname, &args, cwd);
            let preview: String = result.chars().take(120).collect();
            println!("  tool result: {}", preview.replace('\n', " "));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_url_variants() {
        let a = OpenAiClient::new("k".into(), "https://api.openai.com/v1").unwrap();
        assert_eq!(
            a.completions_url_for_test(),
            "https://api.openai.com/v1/chat/completions"
        );
        let b = OpenAiClient::new("k".into(), "https://api.deepseek.com").unwrap();
        assert_eq!(
            b.completions_url_for_test(),
            "https://api.deepseek.com/chat/completions"
        );
        let c = OpenAiClient::new("k".into(), "https://x.test/v1/chat/completions").unwrap();
        assert_eq!(
            c.completions_url_for_test(),
            "https://x.test/v1/chat/completions"
        );
    }

    #[test]
    fn usage_extracts_cached_tokens() {
        let v = serde_json::json!({
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 3,
                "prompt_tokens_details": { "cached_tokens": 4 }
            }
        });
        let u = extract_usage(&v);
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 3);
        assert_eq!(u.cache_read_tokens, Some(4));
    }

    #[test]
    fn system_prompt_lists_vibe_tools() {
        let p = system_prompt(Path::new("/tmp/demo"));
        assert!(p.contains("multi_edit"));
        assert!(p.contains("project_overview"));
    }
}
