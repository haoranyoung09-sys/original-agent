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

/// Rough token estimate (~3 chars/token) for context-window budgeting.
fn estimate_tokens(msg: &ChatMessage) -> usize {
    let mut chars = msg.role.len();
    if let Some(c) = &msg.content {
        chars += c.to_string().len();
    }
    if let Some(tc) = &msg.tool_calls {
        chars += tc.to_string().len();
    }
    (chars / 3).max(4)
}

/// Keep one full copy of large tool outputs on the wire, and trim oldest middle turns
/// when approaching `context_window`. Local history and archive retain every original message.
fn request_messages(
    history: &[ChatMessage],
    context_window: u32,
) -> Vec<std::borrow::Cow<'_, ChatMessage>> {
    use std::borrow::Cow;
    use std::collections::HashMap;
    let mut seen = HashMap::new();
    let deduped: Vec<Cow<'_, ChatMessage>> = history
        .iter()
        .filter(|message| {
            !(message.role == "assistant"
                && message.content.is_none()
                && message.tool_calls.is_none())
        })
        .map(|message| {
            if message.role == "tool" {
                if let (Some(text), Some(id)) = (
                    message.content.as_ref().and_then(Value::as_str),
                    message.tool_call_id.as_deref(),
                ) {
                    if text.len() >= 1024 {
                        let key = (message.name.as_deref(), text);
                        if let Some(first) = seen.get(&key) {
                            let mut reference = message.clone();
                            reference.content = Some(json!(format!(
                                "Identical output to earlier tool call {first}."
                            )));
                            return Cow::Owned(reference);
                        }
                        seen.insert(key, id);
                    }
                }
            }
            Cow::Borrowed(message)
        })
        .collect();

    let budget = (context_window as usize).saturating_mul(80) / 100;
    if budget == 0 || deduped.len() <= 2 {
        return deduped;
    }

    let mut total: usize = deduped.iter().map(|m| estimate_tokens(m)).sum();
    if total <= budget {
        return deduped;
    }

    // Preserve system message (index 0 if role == "system") and drop oldest complete turns.
    let start = usize::from(deduped.first().is_some_and(|m| m.role == "system"));
    let mut drop_until = start;
    // Always keep at least the latest user/assistant/tool exchange.
    let min_keep_idx = deduped.len().saturating_sub(2).max(start);
    while total > budget && drop_until < min_keep_idx {
        total = total.saturating_sub(estimate_tokens(&deduped[drop_until]));
        drop_until += 1;
        // Never leave orphaned `tool` messages without their preceding `assistant` tool_calls.
        while drop_until < min_keep_idx && deduped[drop_until].role == "tool" {
            total = total.saturating_sub(estimate_tokens(&deduped[drop_until]));
            drop_until += 1;
        }
    }

    if drop_until <= start {
        return deduped;
    }

    let mut out = Vec::with_capacity(deduped.len() - (drop_until - start) + 1);
    if start == 1 {
        out.push(deduped[0].clone());
    }
    out.push(Cow::Owned(ChatMessage {
        role: "system".into(),
        content: Some(Value::String(
            "[Earlier conversation turns omitted to fit context window.]".into(),
        )),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }));
    out.extend(deduped.into_iter().skip(drop_until));
    out
}

fn system_prompt(cwd: &Path) -> String {
    crate::agent::system_prompt_text(cwd)
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

async fn send_with_retry(client: &OpenAiClient, body: &Value) -> Result<reqwest::Response> {
    const MAX_RETRIES: u32 = 3;
    let url = client.completions_url();
    let mut attempt = 0;
    loop {
        let resp = client
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", client.api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                if status.is_success() {
                    return Ok(r);
                }
                let retryable = matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504);
                let retry_after = r
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|s| s.min(30));
                let text = r.text().await.unwrap_or_default();
                if retryable && attempt < MAX_RETRIES {
                    let wait = retry_after.unwrap_or(1u64 << attempt);
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    continue;
                }
                bail!("API {status}: {text}");
            }
            Err(err) => {
                if attempt < MAX_RETRIES
                    && (err.is_timeout() || err.is_connect() || err.is_request())
                {
                    let wait = 1u64 << attempt;
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    continue;
                }
                return Err(err).context("OpenAI 兼容请求失败");
            }
        }
    }
}

#[derive(Default)]
struct StreamToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct StreamAccumulator {
    content: String,
    reasoning: String,
    tool_calls: Vec<StreamToolCall>,
    usage: OpenAiUsage,
    finish_reason: Option<String>,
    thought_started: bool,
    printed_answer: bool,
}

impl StreamAccumulator {
    fn apply_sse_data(&mut self, data: &str, show_thinking: bool) {
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if v.get("usage").is_some_and(|u| !u.is_null()) {
            let u = extract_usage(&v);
            if u.input_tokens > 0 || u.output_tokens > 0 || u.cache_read_tokens.is_some() {
                self.usage = u;
            }
        }
        if let Some(reason) = v
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            if !reason.is_empty() {
                self.finish_reason = Some(reason.to_string());
            }
        }
        let Some(delta) = v.pointer("/choices/0/delta") else {
            return;
        };
        if let Some(r) = delta.get("reasoning_content").and_then(Value::as_str) {
            self.reasoning.push_str(r);
            if show_thinking {
                crate::ui::print_thought_chunk(r, &mut self.thought_started);
            }
        }
        if let Some(c) = delta.get("content").and_then(Value::as_str) {
            if self.thought_started {
                crate::ui::finish_thought_stream(true);
                self.thought_started = false;
            }
            if !c.is_empty() {
                self.content.push_str(c);
                crate::ui::print_answer_chunk(c);
                self.printed_answer = true;
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(Value::as_array) {
            for tc in tcs {
                let idx = tc.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if self.tool_calls.len() <= idx {
                    self.tool_calls
                        .resize_with(idx + 1, StreamToolCall::default);
                }
                let entry = &mut self.tool_calls[idx];
                if let Some(id) = tc.get("id").and_then(Value::as_str) {
                    entry.id.push_str(id);
                }
                if let Some(name) = tc.pointer("/function/name").and_then(Value::as_str) {
                    entry.name.push_str(name);
                }
                if let Some(args) = tc.pointer("/function/arguments").and_then(Value::as_str) {
                    entry.arguments.push_str(args);
                }
            }
        }
    }

    fn finish(self) -> (OpenAiUsage, Option<Value>, Option<Value>, Option<String>) {
        if self.thought_started {
            crate::ui::finish_thought_stream(true);
        }
        if self.printed_answer {
            println!("\n");
        }
        let content = if self.content.is_empty() {
            None
        } else {
            Some(Value::String(self.content))
        };
        let tool_calls = if self.tool_calls.is_empty() {
            None
        } else {
            Some(Value::Array(
                self.tool_calls
                    .into_iter()
                    .map(|tc| {
                        json!({
                            "id": if tc.id.is_empty() { "call".to_string() } else { tc.id },
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments,
                            }
                        })
                    })
                    .collect(),
            ))
        };
        (self.usage, content, tool_calls, self.finish_reason)
    }
}

async fn consume_response(
    mut resp: reqwest::Response,
    show_thinking: bool,
) -> Result<(OpenAiUsage, Option<Value>, Option<Value>, Option<String>)> {
    let is_json = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.to_ascii_lowercase().contains("application/json"));

    if is_json {
        let text = resp.text().await.context("读取响应失败")?;
        let v: Value = serde_json::from_str(&text).context("解析 JSON 失败")?;
        let usage = extract_usage(&v);
        let choice = v
            .pointer("/choices/0")
            .cloned()
            .context("响应缺少 choices")?;
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let message = choice.get("message").cloned().context("响应缺少 message")?;
        if show_thinking {
            if let Some(t) = message.get("reasoning_content").and_then(Value::as_str) {
                crate::ui::print_thought(t);
            }
        }
        let content = message.get("content").cloned().filter(|v| !v.is_null());
        let tool_calls = message
            .get("tool_calls")
            .cloned()
            .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()));
        if let Some(Value::String(s)) = &content {
            if !s.trim().is_empty() {
                crate::ui::print_answer(s);
            }
        } else if let Some(Value::Array(parts)) = &content {
            let mut buf = String::new();
            for p in parts {
                if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                    buf.push_str(t);
                }
            }
            if !buf.trim().is_empty() {
                crate::ui::print_answer(&buf);
            }
        }
        return Ok((usage, content, tool_calls, finish_reason));
    }

    let mut acc = StreamAccumulator::default();
    let mut line_buf = Vec::<u8>::new();
    while let Some(chunk) = resp.chunk().await.context("读取流式响应失败")? {
        line_buf.extend_from_slice(&chunk);
        while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = line_buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let trimmed = line.trim();
            if let Some(data) = trimmed.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    return Ok(acc.finish());
                }
                acc.apply_sse_data(data, show_thinking);
            }
        }
    }
    if !line_buf.is_empty() {
        let line = String::from_utf8_lossy(&line_buf);
        if let Some(data) = line.trim().strip_prefix("data:") {
            let data = data.trim();
            if data != "[DONE]" {
                acc.apply_sse_data(data, show_thinking);
            }
        }
    }
    Ok(acc.finish())
}

fn parse_tool_arguments(fname: &str, args_raw: &str) -> std::result::Result<Value, String> {
    let trimmed = args_raw.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(trimmed)
        .map_err(|err| format!("Error: invalid JSON arguments for tool '{fname}': {err}"))
}

pub async fn take_turn(
    client: &OpenAiClient,
    model: &str,
    cwd: &Path,
    history: &mut Vec<ChatMessage>,
    user_text: &str,
    max_tokens: u32,
    context_window: u32,
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
            "messages": request_messages(history, context_window),
            "tools": openai_tool_defs(),
            "tool_choice": "auto",
            "max_tokens": max_tokens,
            "stream": true,
        });

        let resp = send_with_retry(client, &body).await?;
        let (usage, content, tool_calls, finish_reason) =
            consume_response(resp, crate::tool_display::thinking_enabled()).await?;

        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        if let Some(cr) = usage.cache_read_tokens {
            total.cache_read_tokens = Some(total.cache_read_tokens.unwrap_or(0) + cr);
        }

        let has_content = match &content {
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Array(a)) => !a.is_empty(),
            Some(v) => !v.is_null(),
            None => false,
        };
        let has_tools = tool_calls
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty());

        if !has_content && !has_tools {
            if finish_reason.as_deref() == Some("length") {
                eprintln!(
                    "模型达到 max_tokens={max_tokens} 上限且未生成正文（可能思考过程耗尽输出预算）；可提高 --max-tokens、降低 effort（Shift+Tab），或开启 --show-thinking。"
                );
            } else {
                eprintln!("模型本轮未返回正文或工具调用（已跳过无效空消息以免污染对话历史）。");
            }
            return Ok(total);
        }

        history.push(ChatMessage {
            role: "assistant".into(),
            content,
            tool_calls: tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

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
            let short = if args_raw.len() > 120 {
                format!("{}…", args_raw.chars().take(120).collect::<String>())
            } else {
                args_raw.to_string()
            };
            if crate::tool_display::tools_enabled() {
                println!("  → tool {fname}({short})");
            }
            let result = match parse_tool_arguments(&fname, args_raw) {
                Ok(args) => dispatch(&fname, &args, cwd),
                Err(err_msg) => err_msg,
            };
            let preview: String = result.chars().take(120).collect();
            if crate::tool_display::tools_enabled() {
                println!("  tool result: {}", preview.replace('\n', " "));
            }
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
    fn duplicate_results_keep_call_ids_and_original_history() {
        let text = "abc".repeat(512);
        let result = |id: &str, body: &str| ChatMessage {
            role: "tool".into(),
            name: Some("read".into()),
            content: Some(json!(body)),
            tool_call_id: Some(id.into()),
            tool_calls: None,
        };
        let history = vec![
            result("a", &text),
            result("b", &text),
            result("c", "different"),
        ];
        let wire = request_messages(&history, 200_000);
        assert_eq!(wire[0].content, history[0].content);
        assert_eq!(wire[1].tool_call_id.as_deref(), Some("b"));
        assert_eq!(
            wire[1].content,
            Some(json!("Identical output to earlier tool call a."))
        );
        assert_eq!(wire[2].content, history[2].content);
        assert_eq!(history[1].content, Some(json!(text)));
    }

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

    #[test]
    fn malformed_tool_json_returns_explicit_error() {
        let err = parse_tool_arguments("edit", "{\"path\": \"a.rs\"").unwrap_err();
        assert!(err.contains("invalid JSON arguments"));
        assert!(err.contains("edit"));
        assert_eq!(
            parse_tool_arguments("project_overview", "").unwrap(),
            json!({})
        );
    }

    #[test]
    fn context_compaction_keeps_system_and_recent_without_orphaning_tools() {
        let big = "x".repeat(600);
        let msg = |role: &str, content: &str| ChatMessage {
            role: role.into(),
            content: Some(json!(content)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        let history = vec![
            msg("system", "sys"),
            msg("user", &big),
            ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(json!([{"id": "1"}])),
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "tool".into(),
                content: Some(json!(&big)),
                tool_calls: None,
                tool_call_id: Some("1".into()),
                name: Some("read".into()),
            },
            msg("user", "latest question"),
            msg("assistant", "latest answer"),
        ];
        // Tight context window forces compaction of older middle messages (including assistant+tool pair).
        let wire = request_messages(&history, 100);
        assert_eq!(wire[0].role, "system");
        assert_eq!(wire[0].content, Some(json!("sys")));
        assert!(wire
            .iter()
            .any(|m| m.content == Some(json!("latest question"))));
        assert!(!wire.iter().any(|m| m.role == "tool"));
    }

    #[test]
    fn sse_accumulator_merges_tool_calls_and_usage() {
        let mut acc = StreamAccumulator::default();
        acc.apply_sse_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read","arguments":"{\"path\":"}}]}}]}"#,
            false,
        );
        acc.apply_sse_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a.rs\"}"}}]}}],"usage":{"prompt_tokens":12,"completion_tokens":5}}"#,
            false,
        );
        let (usage, content, tool_calls, finish_reason) = acc.finish();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 5);
        assert!(content.is_none());
        assert!(finish_reason.is_none());
        let tc = tool_calls.unwrap();
        assert_eq!(tc[0]["id"], "call_1");
        assert_eq!(tc[0]["function"]["name"], "read");
        assert_eq!(tc[0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
    }

    #[test]
    fn request_messages_drops_empty_assistant_messages() {
        let history = vec![
            ChatMessage {
                role: "system".into(),
                content: Some(json!("sys")),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".into(),
                content: Some(json!("hello")),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".into(),
                content: Some(json!("?")),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let wire = request_messages(&history, 200_000);
        assert_eq!(wire.len(), 3);
        assert!(!wire.iter().any(|m| m.role == "assistant"));
    }
}
