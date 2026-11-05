//! OriginalAgent: claudius Agent with Pi-like tools + adaptive effort.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use claudius::{Agent, Effort, Model, OutputConfig, SystemPrompt, ThinkingConfig, Tool};

use crate::session::EffortLevel;
use crate::tools::{BashTool, EditTool, ReadTool, ToolCtx, WriteTool};

#[derive(Debug, Clone)]
pub struct AgentRuntime {
    pub model: String,
    pub effort: EffortLevel,
}

pub struct OriginalAgent {
    pub root_std: PathBuf,
    pub max_tokens: u32,
    tool_ctx: ToolCtx,
    runtime: Arc<Mutex<AgentRuntime>>,
}

impl OriginalAgent {
    pub fn new(
        root_std: PathBuf,
        model: String,
        effort: EffortLevel,
        max_tokens: u32,
    ) -> Result<Self> {
        let tool_ctx = ToolCtx {
            root: Arc::new(root_std.clone()),
        };
        Ok(Self {
            root_std,
            max_tokens,
            tool_ctx,
            runtime: Arc::new(Mutex::new(AgentRuntime { model, effort })),
        })
    }

    pub fn runtime_handle(&self) -> Arc<Mutex<AgentRuntime>> {
        self.runtime.clone()
    }

    pub fn set_model(&self, model: String) {
        if let Ok(mut g) = self.runtime.lock() {
            g.model = model;
        }
    }

    pub fn set_effort(&self, effort: EffortLevel) {
        if let Ok(mut g) = self.runtime.lock() {
            g.effort = effort;
        }
    }

    fn system_prompt_text(&self) -> String {
        let shell = if cfg!(windows) {
            "Windows PowerShell（bash 工具会通过 powershell -Command 执行）"
        } else {
            "POSIX sh（bash 工具通过 sh -c 执行）"
        };
        format!(
            r#"你是 original，一个运行在本地工作区中的终端 coding agent（风格类似 Pi）。

工作区根目录: {cwd}
Shell: {shell}

你有以下工具，请优先用工具完成任务，而不是空谈：
- read: 读取文件（改动前先读）
- write: 创建/覆盖文件
- edit: 精确字符串替换（old_str 必须唯一）
- bash: 在工作区内执行命令

规则:
1. 路径一律相对工作区；不要尝试访问工作区外的文件。
2. 修改已有文件前先 read；小改动优先用 edit，大改动可用 write。
3. 用 bash 跑测试、查看目录、执行构建命令。
4. 简洁汇报你做了什么；需要时展示关键命令输出。
5. 不要编造文件内容；不确定就 read / bash。
"#,
            cwd = self.root_std.display(),
            shell = shell,
        )
    }
}

#[async_trait]
impl Agent for OriginalAgent {
    async fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    async fn model(&self) -> Model {
        let model = self
            .runtime
            .lock()
            .map(|g| g.model.clone())
            .unwrap_or_else(|_| "model".to_string());
        Model::Custom(model)
    }

    async fn system(&self) -> Option<SystemPrompt> {
        Some(SystemPrompt::from_string(self.system_prompt_text()))
    }

    /// Adaptive thinking + output_config.effort (Anthropic-compatible providers).
    async fn thinking(&self) -> Option<ThinkingConfig> {
        Some(ThinkingConfig::adaptive_summarized())
    }

    async fn output_config(&self) -> Option<OutputConfig> {
        let effort: Effort = self
            .runtime
            .lock()
            .map(|g| g.effort.to_api_effort())
            .unwrap_or(Effort::Medium);
        Some(OutputConfig::new().with_effort(effort))
    }

    async fn tools(&self) -> Vec<Arc<dyn Tool<Self>>> {
        let ctx = self.tool_ctx.clone();
        vec![
            Arc::new(ReadTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(WriteTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(EditTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(BashTool { ctx }) as Arc<dyn Tool<Self>>,
        ]
    }

    async fn bash(&self, command: &str, _restart: bool) -> Result<String, std::io::Error> {
        let (code, stdout, stderr) = crate::tools::execute_shell(command, &self.root_std, 60);
        let mut out = format!("exit_code: {code}\n");
        if !stdout.is_empty() {
            out.push_str("stdout:\n");
            out.push_str(&stdout);
            out.push('\n');
        }
        if !stderr.is_empty() {
            out.push_str("stderr:\n");
            out.push_str(&stderr);
            out.push('\n');
        }
        if code == 0 {
            Ok(out)
        } else {
            Err(std::io::Error::other(out))
        }
    }
}
