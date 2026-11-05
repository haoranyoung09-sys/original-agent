//! OriginalAgent: claudius Agent with Pi-like tools + adaptive effort.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use claudius::{Agent, Effort, Model, OutputConfig, SystemPrompt, ThinkingConfig, Tool};

use crate::session::EffortLevel;
use crate::tools::{
    BashTool, EditTool, GlobTool, GrepTool, MultiEditTool, ProjectOverviewTool, ReadTool, ToolCtx,
    VerifyTool, WriteTool,
};

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
            "Windows PowerShell（bash/verify 通过 powershell -Command；默认禁用，需 --allow-shell）"
        } else {
            "POSIX sh（bash/verify 通过 sh -c；默认禁用，需 --allow-shell）"
        };
        format!(
            r#"你是 original，一个运行在本地工作区中的终端 vibe-coding agent（风格类似 Pi）。

工作区根目录: {cwd}
Shell: {shell}

工具（优先用工具，不要空谈或编造文件内容）:
- project_overview: 先摸清项目类型与目录树
- glob / grep: 无 shell 的文件发现与内容搜索
- read: 改动前先读
- edit: 单处唯一字符串替换
- multi_edit: 跨文件/多处协调修改（先全部校验再写入）
- write: 新建或整文件覆盖
- verify: 改后跑测试/构建反馈（需 --allow-shell；否则会给出建议命令）
- bash: 任意命令（需 --allow-shell）

Vibe coding 工作流:
1. 陌生仓库先 project_overview 或 glob，再用 grep/read 定位。
2. 小改动用 edit；同一意图涉及多处/多文件时用 multi_edit。
3. 路径一律相对工作区；禁止逃逸工作区。
4. Shell 默认关闭：不要假设能跑 bash/verify；没有权限时说明并给出建议命令。
5. 简洁汇报做了什么；需要时引用工具输出关键片段。
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
            Arc::new(ProjectOverviewTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(GlobTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(GrepTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(ReadTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(WriteTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(EditTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(MultiEditTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(VerifyTool { ctx: ctx.clone() }) as Arc<dyn Tool<Self>>,
            Arc::new(BashTool { ctx }) as Arc<dyn Tool<Self>>,
        ]
    }

    async fn bash(&self, command: &str, _restart: bool) -> Result<String, std::io::Error> {
        let (code, stdout, stderr) = crate::shell::execute_shell(command, &self.root_std, 60);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_mentions_vibe_tools() {
        let agent = OriginalAgent::new(
            std::env::temp_dir(),
            "m".into(),
            EffortLevel::Medium,
            1024,
        )
        .unwrap();
        let prompt = agent.system_prompt_text();
        assert!(prompt.contains("multi_edit"));
        assert!(prompt.contains("project_overview"));
        assert!(prompt.contains("verify"));
        assert!(prompt.contains("--allow-shell"));
    }
}
