//! Custom Pi-style tools: read / write / edit / multi_edit / glob / grep /
//! project_overview / verify / bash (PowerShell on Windows).

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use claudius::{
    Agent, Anthropic, IntermediateToolResult, Tool, ToolCallback, ToolParam, ToolResult,
    ToolResultBlock, ToolResultBlockContent, ToolUnionParam, ToolUseBlock,
};
use serde::Deserialize;
use serde_json::json;

use crate::fsops::{
    edit_once, glob_files, grep_workspace, multi_edit, project_overview, read_text, write_text,
    FileEdit, ReadRange,
};
use crate::shell::execute_shell;
use crate::tool_dispatch::run_verify;

pub use crate::shell::set_shell_enabled;

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

fn map_result(tool_use_id: &str, result: anyhow::Result<String>) -> Box<dyn IntermediateToolResult> {
    match result {
        Ok(text) => ok_result(tool_use_id, text),
        Err(e) => err_result(tool_use_id, e.to_string()),
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
        map_result(
            &tool_use.id,
            read_text(
                &self.ctx.root,
                &input.path,
                ReadRange {
                    offset: input.offset,
                    limit: input.limit,
                },
            ),
        )
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
        map_result(
            &tool_use.id,
            write_text(&self.ctx.root, &input.path, &input.content),
        )
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
        map_result(
            &tool_use.id,
            edit_once(&self.ctx.root, &input.path, &input.old_str, &input.new_str),
        )
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

/* -------------------- multi_edit -------------------- */

pub struct MultiEditTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct MultiEditItem {
    path: String,
    old_str: String,
    new_str: String,
}

#[derive(Deserialize)]
struct MultiEditInput {
    edits: Vec<MultiEditItem>,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for MultiEditTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: MultiEditInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid multi_edit input: {e}")),
        };
        let edits: Vec<FileEdit> = input
            .edits
            .into_iter()
            .map(|e| FileEdit {
                path: e.path,
                old_str: e.old_str,
                new_str: e.new_str,
            })
            .collect();
        map_result(&tool_use.id, multi_edit(&self.ctx.root, &edits))
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

impl<A: Agent> Tool<A> for MultiEditTool {
    fn name(&self) -> String {
        "multi_edit".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(MultiEditTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "multi_edit".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "edits": {
                            "type": "array",
                            "description": "Ordered unique str_replace edits; all validated before any write",
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
                }),
            )
            .with_description(
                "Apply multiple unique string replacements across one or more workspace files in one batch."
                    .to_string(),
            ),
        )
    }
}

/* -------------------- glob -------------------- */

pub struct GlobTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct GlobInput {
    pattern: String,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for GlobTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: GlobInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid glob input: {e}")),
        };
        map_result(&tool_use.id, glob_files(&self.ctx.root, &input.pattern))
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

impl<A: Agent> Tool<A> for GlobTool {
    fn name(&self) -> String {
        "glob".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(GlobTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "glob".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob relative to workspace, e.g. src/**/*.rs" }
                    },
                    "required": ["pattern"]
                }),
            )
            .with_description(
                "List workspace files matching a glob (* ? **/). No shell required.".to_string(),
            ),
        )
    }
}

/* -------------------- grep -------------------- */

pub struct GrepTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct GrepInput {
    query: String,
    #[serde(default)]
    glob: Option<String>,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for GrepTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: GrepInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid grep input: {e}")),
        };
        map_result(
            &tool_use.id,
            grep_workspace(&self.ctx.root, &input.query, input.glob.as_deref()),
        )
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

impl<A: Agent> Tool<A> for GrepTool {
    fn name(&self) -> String {
        "grep".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(GrepTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "grep".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Literal substring to find" },
                        "glob": { "type": "string", "description": "Optional file glob filter" }
                    },
                    "required": ["query"]
                }),
            )
            .with_description(
                "Search workspace file contents (literal). Optional glob filter. No shell."
                    .to_string(),
            ),
        )
    }
}

/* -------------------- project_overview -------------------- */

pub struct ProjectOverviewTool {
    pub ctx: ToolCtx,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for ProjectOverviewTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        map_result(&tool_use.id, project_overview(&self.ctx.root))
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

impl<A: Agent> Tool<A> for ProjectOverviewTool {
    fn name(&self) -> String {
        "project_overview".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(ProjectOverviewTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "project_overview".to_string(),
                json!({ "type": "object", "properties": {} }),
            )
            .with_description(
                "Summarize project type, shallow tree, and README snippet for orientation."
                    .to_string(),
            ),
        )
    }
}

/* -------------------- verify -------------------- */

pub struct VerifyTool {
    pub ctx: ToolCtx,
}

#[derive(Deserialize)]
struct VerifyInput {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[async_trait]
impl<A: Agent> ToolCallback<A> for VerifyTool {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let input: VerifyInput = match serde_json::from_value(tool_use.input.clone()) {
            Ok(v) => v,
            Err(e) => return err_result(&tool_use.id, format!("invalid verify input: {e}")),
        };
        let timeout = input.timeout_secs.unwrap_or(180).clamp(1, 600);
        let root = self.ctx.root.as_path().to_path_buf();
        let command = input.command.clone();
        let body = tokio::task::spawn_blocking(move || {
            run_verify(&root, command.as_deref(), timeout)
        })
        .await
        .unwrap_or_else(|e| format!("verify join error: {e}"));
        // Informational block / no-command messages are ok; non-zero exit is an error.
        if body.contains("exit_code:") && !body.contains("exit_code: 0") {
            err_result(&tool_use.id, body)
        } else {
            ok_result(&tool_use.id, body)
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

impl<A: Agent> Tool<A> for VerifyTool {
    fn name(&self) -> String {
        "verify".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(VerifyTool {
            ctx: self.ctx.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(
            ToolParam::new(
                "verify".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Optional override; default auto-detects cargo/npm/pytest/go" },
                        "timeout_secs": { "type": "integer", "description": "Timeout seconds (default 180, max 600)" }
                    }
                }),
            )
            .with_description(
                "Run build/test feedback after edits. Requires --allow-shell; without it returns a suggested command."
                    .to_string(),
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
            "Run a shell command in the workspace via PowerShell (-Command). Requires --allow-shell."
        } else {
            "Run a shell command in the workspace via /bin/sh -c. Requires --allow-shell."
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}\n…(truncated)")
    }
}
