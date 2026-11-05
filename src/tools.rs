//! Custom Pi-style tools: read / write / edit / bash (PowerShell on Windows).

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
static SHELL_ENABLED: AtomicBool = AtomicBool::new(false);
pub fn set_shell_enabled(enabled: bool) {
    SHELL_ENABLED.store(enabled, Ordering::Relaxed);
}
use std::time::Duration;

use async_trait::async_trait;
use claudius::{
    Agent, Anthropic, IntermediateToolResult, Tool, ToolCallback, ToolParam, ToolResult,
    ToolResultBlock, ToolResultBlockContent, ToolUnionParam, ToolUseBlock,
};
use serde::Deserialize;
use serde_json::json;

use crate::workspace::resolve_in_workspace;

fn ok_result(tool_use_id: &str, text: String) -> Box<dyn IntermediateToolResult> {
    Box::new(ControlFlow::Continue(Ok(ToolResultBlock {
        tool_use_id: tool_use_id.to_string(),
        content: Some(ToolResultBlockContent::String(text)),
        is_error: None,
        cache_control: None,
    })))
}

fn err_result(tool_use_id: &str, text: String) -> Box<dyn IntermediateToolResult> {
    Box::new(ControlFlow::Continue(Err(ToolResultBlock {
        tool_use_id: tool_use_id.to_string(),
        content: Some(ToolResultBlockContent::String(text)),
        is_error: Some(true),
        cache_control: None,
    })))
}

fn downcast_tool_result(intermediate: Box<dyn IntermediateToolResult>) -> ToolResult {
    match intermediate.as_any().downcast_ref::<ToolResult>() {
        Some(v) => v.clone(),
        None => ControlFlow::Break(claudius::Error::unknown(
            "intermediate tool result fails to deserialize",
        )),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}\n…(truncated)")
    }
}

/// Shared workspace root for all tools.
#[derive(Clone)]
pub struct ToolCtx {
    pub root: Arc<PathBuf>,
}

/* -------------------- read -------------------- */

pub struct ReadTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct ReadInput {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for ReadTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: ReadInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid read input: {e}")),
        };
        let path = match resolve_in_workspace(&self.ctx.root, &input.path) {
            Ok(p) => p,
            Err(e) => return err_result(&tool_use.id, e.to_string()),
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                return err_result(&tool_use.id, format!("read failed {}: {e}", path.display()))
            }
        };
        let lines: Vec<&str> = content.lines().collect();
        let start = input.offset.unwrap_or(1).saturating_sub(1);
        let end = match input.limit {
            Some(lim) => start.saturating_add(lim).min(lines.len()),
            None => lines.len(),
        };
        if start >= lines.len() {
            return ok_result(
                &tool_use.id,
                format!(
                    "(file has {} lines; offset {} is past EOF)",
                    lines.len(),
                    start + 1
                ),
            );
        }
        let mut out = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>6}|{}\n", start + i + 1, line));
        }
        ok_result(&tool_use.id, truncate(&out, 80_000))
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        downcast_tool_result(intermediate)
    }
}

impl<A: Agent> Tool<A> for ReadTool {
    fn name(&self) -> String {
        "read".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(ReadTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "read".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to workspace root" },
                        "offset": { "type": "integer", "description": "1-based start line (optional)" },
                        "limit": { "type": "integer", "description": "Max lines to return (optional)" }
                    },
                    "required": ["path"]
                }),
            )
            .with_description(
                "Read a text file from the workspace. Prefer this before editing.".to_string(),
            ),
        )
    }
}

/* -------------------- write -------------------- */

pub struct WriteTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct WriteInput {
    path: String,
    content: String,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for WriteTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: WriteInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid write input: {e}")),
        };
        let path = match resolve_in_workspace(&self.ctx.root, &input.path) {
            Ok(p) => p,
            Err(e) => return err_result(&tool_use.id, e.to_string()),
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return err_result(&tool_use.id, format!("mkdir {}: {e}", parent.display()));
            }
        }
        match std::fs::write(&path, &input.content) {
            Ok(()) => ok_result(
                &tool_use.id,
                format!("wrote {} bytes to {}", input.content.len(), path.display()),
            ),
            Err(e) => err_result(
                &tool_use.id,
                format!("write failed {}: {e}", path.display()),
            ),
        }
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        downcast_tool_result(intermediate)
    }
}

impl<A: Agent> Tool<A> for WriteTool {
    fn name(&self) -> String {
        "write".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(WriteTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "write".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to workspace root" },
                        "content": { "type": "string", "description": "Full file contents to write" }
                    },
                    "required": ["path", "content"]
                }),
            )
            .with_description("Create or overwrite a text file in the workspace.".to_string()),
        )
    }
}

/* -------------------- edit (str_replace) -------------------- */

pub struct EditTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct EditInput {
    path: String,
    old_str: String,
    new_str: String,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for EditTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: EditInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid edit input: {e}")),
        };
        if input.old_str.is_empty() {
            return err_result(&tool_use.id, "old_str must not be empty".to_string());
        }
        let path = match resolve_in_workspace(&self.ctx.root, &input.path) {
            Ok(p) => p,
            Err(e) => return err_result(&tool_use.id, e.to_string()),
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                return err_result(
                    &tool_use.id,
                    format!("edit read failed {}: {e}", path.display()),
                )
            }
        };
        let matches = content.matches(&input.old_str).count();
        if matches == 0 {
            return err_result(
                &tool_use.id,
                "old_str not found in file (make sure it matches exactly, including whitespace)"
                    .to_string(),
            );
        }
        if matches > 1 {
            return err_result(
                &tool_use.id,
                format!(
                    "old_str found {matches} times; provide a unique snippet so exactly one match is replaced"
                ),
            );
        }
        let updated = content.replacen(&input.old_str, &input.new_str, 1);
        match std::fs::write(&path, updated) {
            Ok(()) => ok_result(
                &tool_use.id,
                format!("edited {} (1 replacement)", path.display()),
            ),
            Err(e) => err_result(&tool_use.id, format!("edit write failed: {e}")),
        }
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        downcast_tool_result(intermediate)
    }
}

impl<A: Agent> Tool<A> for EditTool {
    fn name(&self) -> String {
        "edit".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(EditTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "edit".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to workspace root" },
                        "old_str": { "type": "string", "description": "Exact text to find (must be unique)" },
                        "new_str": { "type": "string", "description": "Replacement text" }
                    },
                    "required": ["path", "old_str", "new_str"]
                }),
            )
            .with_description(
                "Replace a unique exact string in a workspace file (str_replace).".to_string(),
            ),
        )
    }
}

/* -------------------- bash / shell -------------------- */

pub struct BashTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct BashInput {
    command: String,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Run a shell command in `cwd` with a soft timeout.
pub fn execute_shell(command: &str, cwd: &PathBuf, timeout_secs: u64) -> (i32, String, String) {
    if !SHELL_ENABLED.load(Ordering::Relaxed) {
        return (
            126,
            String::new(),
            "Shell disabled; restart with --allow-shell to authorize unrestricted commands.".into(),
        );
    }
    use std::io::Read;
    use std::process::Stdio;
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let mut child = match cmd
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (1, String::new(), e.to_string()),
    };
    fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut saved = Vec::new();
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let keep = n.min(80_000usize.saturating_sub(saved.len()));
                saved.extend_from_slice(&buf[..keep]);
            }
            String::from_utf8_lossy(&saved).into_owned()
        })
    }
    let out = drain(child.stdout.take().unwrap());
    let err = drain(child.stderr.take().unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.clamp(1, 600));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Allow pipe readers to drain without waiting indefinitely for descendants.
                let drain_deadline = std::time::Instant::now() + Duration::from_millis(200);
                while !(out.is_finished() && err.is_finished())
                    && std::time::Instant::now() < drain_deadline
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let stdout = if out.is_finished() {
                    out.join().unwrap_or_default()
                } else {
                    String::new()
                };
                let stderr = if err.is_finished() {
                    err.join().unwrap_or_default()
                } else {
                    String::new()
                };
                return (status.code().unwrap_or(1), stdout, stderr);
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .output();
                }
                let _ = child.kill();
                let _ = child.wait();
                return (
                    124,
                    String::new(),
                    "Shell timed out and termination was requested".into(),
                );
            }
        }
    }
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for BashTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: BashInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid bash input: {e}")),
        };
        if input.command.trim().is_empty() {
            return err_result(&tool_use.id, "command must not be empty".to_string());
        }
        let timeout = input.timeout_secs.unwrap_or(60).clamp(1, 600);
        let root = self.ctx.root.as_path().to_path_buf();
        let cmd = input.command.clone();
        let (code, stdout, stderr) =
            tokio::task::spawn_blocking(move || execute_shell(&cmd, &root, timeout))
                .await
                .unwrap_or_else(|e| (1, String::new(), format!("join error: {e}")));

        let mut body = String::new();
        body.push_str(&format!("exit_code: {code}\n"));
        if !stdout.is_empty() {
            body.push_str("stdout:\n");
            body.push_str(&truncate(&stdout, 60_000));
            body.push('\n');
        }
        if !stderr.is_empty() {
            body.push_str("stderr:\n");
            body.push_str(&truncate(&stderr, 20_000));
            body.push('\n');
        }
        if code == 0 {
            ok_result(&tool_use.id, body)
        } else {
            err_result(&tool_use.id, body)
        }
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        downcast_tool_result(intermediate)
    }
}

impl<A: Agent> Tool<A> for BashTool {
    fn name(&self) -> String {
        "bash".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(BashTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        let shell_hint = if cfg!(windows) {
            "Run a shell command in the workspace via PowerShell (-Command)."
        } else {
            "Run a shell command in the workspace via /bin/sh -c."
        };
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "bash".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute" },
                        "timeout_secs": { "type": "integer", "description": "Timeout seconds (default 60, max 600)" }
                    },
                    "required": ["command"]
                }),
            )
            .with_description(shell_hint.to_string()),
        )
    }
}
