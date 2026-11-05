//! original — multi-provider Pi-like terminal coding agent (claudius + OpenAI-compatible).

mod agent;
mod config;
mod openai_client;
mod provider;
mod session;
mod tools;
mod ui;
mod workspace;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use clap::Parser;
use claudius::{
    Agent, Anthropic, Budget, ContentBlock, MessageParam, MessageParamContent, MessageRole,
};
use rustyline::error::ReadlineError;
use rustyline::validate::ValidationResult;
use rustyline::{
    Cmd, ConditionalEventHandler, Editor, Event, EventContext, EventHandler, KeyCode, KeyEvent,
    Modifiers, RepeatCount,
};

use crate::agent::{AgentRuntime, OriginalAgent};
use crate::config::Config;
use crate::openai_client::{ChatMessage, OpenAiClient};
use crate::provider::{presets, ApiStyle, ProviderId};
use crate::session::{SessionState, SharedSession};

enum Backend {
    Anthropic(Anthropic),
    OpenAi(OpenAiClient),
}

#[derive(Debug, Parser)]
#[command(
    name = "original",
    about = "original — Pi-like multi-provider terminal coding agent",
    version
)]
struct Cli {
    #[arg(long)]
    cwd: Option<PathBuf>,
    /// Allow model-generated shell commands with your full user permissions.
    #[arg(long)]
    allow_shell: bool,
    #[arg(short = 'p', long = "print")]
    print: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    message: Vec<String>,
    #[arg(long)]
    max_tokens: Option<u32>,
    #[arg(long)]
    budget_tokens: Option<u32>,
}

fn load_dotenv() {
    // Only load explicit user configuration, never an untrusted workspace .env.
    if let Ok(dir) = Config::config_dir() {
        let alt = dir.join(".env");
        if alt.exists() {
            let _ = dotenvy::from_path(alt);
        }
    }
}

fn build_backend(cfg: &Config) -> Result<Backend> {
    cfg.validate()?;
    let key = cfg.effective_api_key().context("缺少 API Key")?;
    match cfg.api_style {
        ApiStyle::Anthropic => {
            let client = Anthropic::new(Some(key))
                .context("创建 Anthropic 兼容客户端失败")?
                .with_base_url(cfg.base_url.trim_end_matches('/').to_string());
            Ok(Backend::Anthropic(client))
        }
        ApiStyle::OpenAI => Ok(Backend::OpenAi(OpenAiClient::new(key, &cfg.base_url)?)),
    }
}

fn print_help() {
    ui::print_help_panel();
}

fn extract_assistant_text(content: &MessageParamContent) -> String {
    match content {
        MessageParamContent::String(s) => s.clone(),
        MessageParamContent::Array(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block {
                    ContentBlock::Text(t) => parts.push(t.text.clone()),
                    ContentBlock::ToolUse(tu) => {
                        let input = serde_json::to_string(&tu.input).unwrap_or_default();
                        let short = if input.len() > 200 {
                            format!("{}…", input.chars().take(200).collect::<String>())
                        } else {
                            input
                        };
                        parts.push(format!("[tool_use] {}({})", tu.name, short));
                    }
                    ContentBlock::ToolResult(tr) => {
                        let body = match &tr.content {
                            Some(claudius::ToolResultBlockContent::String(s)) => {
                                let s = s.trim();
                                if s.len() > 240 {
                                    format!("{}…", s.chars().take(240).collect::<String>())
                                } else {
                                    s.to_string()
                                }
                            }
                            Some(_) => "[non-text tool result]".to_string(),
                            None => String::new(),
                        };
                        let err = if tr.is_error.unwrap_or(false) {
                            " ERROR"
                        } else {
                            ""
                        };
                        parts.push(format!("[tool_result{err}] {body}"));
                    }
                    _ => {}
                }
            }
            parts.join("\n")
        }
    }
}

fn print_turn_delta(before_len: usize, messages: &[MessageParam]) {
    for msg in messages.iter().skip(before_len) {
        match msg.role {
            MessageRole::Assistant => {
                let text = extract_assistant_text(&msg.content);
                if !text.trim().is_empty() {
                    println!("assistant:\n{text}\n");
                }
            }
            MessageRole::User => {
                if let MessageParamContent::Array(blocks) = &msg.content {
                    for block in blocks {
                        if let ContentBlock::ToolResult(tr) = block {
                            let preview = match &tr.content {
                                Some(claudius::ToolResultBlockContent::String(s)) => {
                                    let s = s.replace('\n', " ");
                                    if s.len() > 120 {
                                        format!("{}…", s.chars().take(120).collect::<String>())
                                    } else {
                                        s
                                    }
                                }
                                _ => String::from("(ok)"),
                            };
                            let mark = if tr.is_error.unwrap_or(false) {
                                "!"
                            } else {
                                "✓"
                            };
                            println!("  {mark} tool_result: {preview}");
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

async fn run_anthropic_turn(
    agent: &mut OriginalAgent,
    client: &Anthropic,
    messages: &mut Vec<MessageParam>,
    budget: &Arc<Budget>,
    user_text: &str,
    session: &SharedSession,
) -> Result<()> {
    let before = messages.len();
    messages.push(MessageParam {
        role: MessageRole::User,
        content: MessageParamContent::String(user_text.to_string()),
    });

    match agent.take_turn(client, messages, budget).await {
        Ok(outcome) => {
            print_turn_delta(before, messages);
            if let Ok(mut s) = session.lock() {
                s.usage.record_turn(
                    outcome.usage.input_tokens,
                    outcome.usage.output_tokens,
                    outcome.usage.cache_read_input_tokens,
                    outcome.usage.cache_creation_input_tokens,
                );
            }
            Ok(())
        }
        Err(err) => {
            if messages.len() == before + 1 {
                messages.pop();
            }
            Err(anyhow::anyhow!("agent turn failed: {err}"))
        }
    }
}

async fn run_openai_turn(
    client: &OpenAiClient,
    model: &str,
    cwd: &std::path::Path,
    history: &mut Vec<ChatMessage>,
    user_text: &str,
    max_tokens: u32,
    session: &SharedSession,
) -> Result<()> {
    let usage =
        openai_client::take_turn(client, model, cwd, history, user_text, max_tokens).await?;
    if let Ok(mut s) = session.lock() {
        s.usage.record_turn(
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_creation_tokens,
        );
    }
    Ok(())
}

struct CycleEffortHandler {
    session: SharedSession,
    runtime: Arc<Mutex<AgentRuntime>>,
}

impl ConditionalEventHandler for CycleEffortHandler {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let new_effort = {
            let mut s = self.session.lock().ok()?;
            s.effort = s.effort.cycle();
            s.effort
        };
        if let Ok(mut rt) = self.runtime.lock() {
            rt.effort = new_effort;
        }
        if let Ok(mut cfg) = Config::load() {
            cfg.effort = new_effort;
            let _ = cfg.save();
        }
        println!("\n思考强度 → {} ({})", new_effort, new_effort.label());
        Some(Cmd::Repaint)
    }
}

#[derive(Clone, Default)]
struct ReplHelper {
    session: Option<SharedSession>,
}

impl rustyline::Helper for ReplHelper {}
impl rustyline::completion::Completer for ReplHelper {
    type Candidate = rustyline::completion::Pair;
}
impl rustyline::hint::Hinter for ReplHelper {
    type Hint = String;
}
impl rustyline::highlight::Highlighter for ReplHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> std::borrow::Cow<'b, str> {
        if prompt == "│ ❯ " {
            if let Some(session) = &self.session {
                if let Ok(s) = session.lock() {
                    return std::borrow::Cow::Owned(ui::effort_prompt(s.effort));
                }
            }
        }
        std::borrow::Cow::Borrowed(prompt)
    }
}
impl rustyline::validate::Validator for ReplHelper {
    fn validate(
        &self,
        _ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
}

fn read_line(
    rl: &mut Editor<ReplHelper, rustyline::history::DefaultHistory>,
    prompt: &str,
) -> String {
    rl.readline(prompt).unwrap_or_default().trim().to_string()
}

fn do_login(
    rl: &mut Editor<ReplHelper, rustyline::history::DefaultHistory>,
    cfg: &mut Config,
    session: &SharedSession,
    agent: &OriginalAgent,
    backend: &mut Option<Backend>,
) -> Result<()> {
    println!("— /login — 选择服务商预设 —");
    let list = presets();
    for (i, p) in list.iter().enumerate() {
        println!(
            "  {}. {}  [{}]  model={}",
            i + 1,
            p.label,
            p.api_style,
            p.default_model
        );
    }
    println!("  0. 保持当前（仅改 Key / URL / 模型）");

    let choice = read_line(rl, "选择编号: ");
    if !choice.is_empty() && choice != "0" {
        let idx: usize = choice.parse().context("请输入数字编号")?;
        let preset = list.get(idx.wrapping_sub(1)).context("编号超出范围")?;
        // Custom appears twice — distinguish by api_style via label index.
        cfg.provider = if preset.label.starts_with("自定义") {
            ProviderId::Custom
        } else {
            preset.id
        };
        cfg.api_style = preset.api_style;
        cfg.base_url = preset.default_base_url.to_string();
        cfg.model = preset.default_model.to_string();
        cfg.context_window = preset.suggest_context_window;
        {
            cfg.pricing.input_per_mtok = preset.suggest_input_per_mtok;
        }
        {
            cfg.pricing.output_per_mtok = preset.suggest_output_per_mtok;
        }
        println!(
            "已选: {} · {} · {}",
            preset.label, preset.api_style, preset.default_model
        );
    }

    let existing = cfg
        .api_key
        .as_ref()
        .map(|_| "***".to_string())
        .unwrap_or_else(|| "(空)".into());

    let key_line = read_line(rl, &format!("API Key [{existing}]（回车保留）: "));
    if !key_line.is_empty() {
        cfg.api_key = Some(key_line);
    }
    if !cfg.has_api_key() && Config::resolve_api_key_from_env().is_none() {
        bail!("未提供 API Key。可粘贴 Key，或设置 ORIGINAL_API_KEY / ANTHROPIC_API_KEY / OPENAI_API_KEY。");
    }

    let style_hint = format!("{}（回车保留，或输入 anthropic / openai）", cfg.api_style);
    let style_line = read_line(rl, &format!("API 风格 [{style_hint}]: "));
    if !style_line.is_empty() {
        match style_line.to_ascii_lowercase().as_str() {
            "anthropic" | "a" => cfg.api_style = ApiStyle::Anthropic,
            "openai" | "o" => cfg.api_style = ApiStyle::OpenAI,
            other => bail!("未知 API 风格: {other}"),
        }
    }

    let base_default = if cfg.base_url.is_empty() {
        match cfg.api_style {
            ApiStyle::Anthropic => "https://api.anthropic.com",
            ApiStyle::OpenAI => "https://api.openai.com/v1",
        }
    } else {
        cfg.base_url.as_str()
    };
    let base_line = read_line(rl, &format!("base URL [{base_default}]: "));
    if !base_line.is_empty() {
        cfg.base_url = base_line;
    } else if cfg.base_url.is_empty() {
        cfg.base_url = base_default.to_string();
    }

    let model_default = if cfg.model.is_empty() {
        "my-model"
    } else {
        cfg.model.as_str()
    };
    let model_line = read_line(rl, &format!("模型名 [{model_default}]: "));
    if !model_line.is_empty() {
        cfg.model = model_line;
    } else if cfg.model.is_empty() {
        cfg.model = model_default.to_string();
    }

    let cw = read_line(rl, &format!("上下文窗口 tokens [{}]: ", cfg.context_window));
    if !cw.is_empty() {
        cfg.context_window = cw.parse().context("上下文窗口须为整数")?;
    }

    let pin = read_line(
        rl,
        &format!(
            "输入单价 USD/1M tokens [{}]（回车跳过）: ",
            cfg.pricing
                .input_per_mtok
                .map(|v| format!("{v}"))
                .unwrap_or_else(|| "—".into())
        ),
    );
    if !pin.is_empty() {
        cfg.pricing.input_per_mtok = Some(pin.parse().context("单价须为数字")?);
    }
    let pout = read_line(
        rl,
        &format!(
            "输出单价 USD/1M tokens [{}]（回车跳过）: ",
            cfg.pricing
                .output_per_mtok
                .map(|v| format!("{v}"))
                .unwrap_or_else(|| "—".into())
        ),
    );
    if !pout.is_empty() {
        cfg.pricing.output_per_mtok = Some(pout.parse().context("单价须为数字")?);
    }

    cfg.save().context("保存配置失败")?;
    *backend = Some(build_backend(cfg)?);

    {
        let mut s = session.lock().unwrap();
        s.logged_in = true;
        s.model = cfg.model.clone();
        s.base_url = cfg.base_url.clone();
        s.effort = cfg.effort;
        s.provider = cfg.provider;
        s.api_style = cfg.api_style;
        s.context_window = cfg.context_window;
    }
    agent.set_model(cfg.model.clone());
    agent.set_effort(cfg.effort);

    {
        let s = session.lock().unwrap();
        println!("已登录。");
        ui::print_status_line(&s, cfg);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    let cli = Cli::parse();
    tools::set_shell_enabled(cli.allow_shell);

    let cwd = match cli.cwd {
        Some(p) => {
            let p = if p.is_absolute() {
                p
            } else {
                std::env::current_dir()?.join(p)
            };
            std::fs::canonicalize(&p).with_context(|| format!("无法解析 --cwd: {}", p.display()))?
        }
        None => std::env::current_dir().context("无法获取当前工作目录")?,
    };

    let oneshot = if let Some(p) = cli.print {
        Some(p)
    } else if !cli.message.is_empty() {
        Some(cli.message.join(" "))
    } else {
        None
    };

    let mut cfg = Config::load()?;
    if let Ok(v) = std::env::var("ANTHROPIC_BASE_URL").or_else(|_| std::env::var("OPENAI_BASE_URL"))
    {
        let v = v.trim().to_string();
        if !v.is_empty() {
            cfg.base_url = v;
        }
    }
    let max_tokens = cli.max_tokens.unwrap_or(cfg.max_tokens);
    let budget_tokens = cli.budget_tokens.unwrap_or(cfg.budget_tokens);
    anyhow::ensure!(
        max_tokens > 0 && budget_tokens > 0,
        "Token limits must be positive"
    );

    let initially_logged_in = cfg.effective_api_key().is_some()
        && !cfg.model.trim().is_empty()
        && !cfg.base_url.trim().is_empty();
    let session: SharedSession = Arc::new(Mutex::new(SessionState::from_config(
        &cfg,
        initially_logged_in,
    )));

    let mut agent = OriginalAgent::new(
        cwd.clone(),
        if cfg.model.is_empty() {
            "model".into()
        } else {
            cfg.model.clone()
        },
        cfg.effort,
        max_tokens,
    )?;
    let runtime = agent.runtime_handle();

    let budget = Arc::new({
        #[allow(deprecated)]
        Budget::new(budget_tokens)
    });
    let mut anthropic_messages: Vec<MessageParam> = Vec::new();
    let mut openai_messages: Vec<ChatMessage> = Vec::new();

    let mut backend: Option<Backend> = if initially_logged_in {
        Some(build_backend(&cfg)?)
    } else {
        None
    };

    if oneshot.is_none() {
        let s = session.lock().unwrap();
        ui::print_startup(&cwd, &s, &cfg);
    }

    if let Some(text) = oneshot {
        let backend = backend
            .as_ref()
            .context("一次性模式需要已保存的配置或环境变量 API Key；请先 original → /login")?;
        match backend {
            Backend::Anthropic(c) => {
                run_anthropic_turn(
                    &mut agent,
                    c,
                    &mut anthropic_messages,
                    &budget,
                    &text,
                    &session,
                )
                .await?;
            }
            Backend::OpenAi(c) => {
                run_openai_turn(
                    c,
                    &cfg.model,
                    &cwd,
                    &mut openai_messages,
                    &text,
                    max_tokens,
                    &session,
                )
                .await?;
            }
        }
        return Ok(());
    }

    let mut rl = Editor::<ReplHelper, rustyline::history::DefaultHistory>::new()
        .context("初始化 REPL 失败")?;
    rl.set_helper(Some(ReplHelper {
        session: Some(session.clone()),
    }));
    rl.bind_sequence(
        KeyEvent(KeyCode::BackTab, Modifiers::NONE),
        EventHandler::Conditional(Box::new(CycleEffortHandler {
            session: session.clone(),
            runtime: runtime.clone(),
        })),
    );

    if let Ok(hp) = Config::history_path() {
        let _ = std::fs::create_dir_all(hp.parent().unwrap_or(hp.as_path()));
        let _ = rl.load_history(&hp);
    }

    loop {
        ui::print_input_border(session.lock().unwrap().effort);
        let prompt = ui::input_prompt();
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line.as_str());

                if line == "/exit" || line == "/quit" {
                    println!("bye");
                    break;
                }
                if line == "/clear" {
                    anthropic_messages.clear();
                    openai_messages.clear();
                    println!("(对话历史已清空；会话累计用量保留，见 /status)");
                    continue;
                }
                if line == "/help" || line == "/?" {
                    print_help();
                    continue;
                }
                if line == "/status" {
                    let s = session.lock().unwrap();
                    ui::print_status_full(&s, &cfg, &cwd);
                    continue;
                }
                if line == "/login" || line.starts_with("/login ") {
                    if let Err(e) = do_login(&mut rl, &mut cfg, &session, &agent, &mut backend) {
                        eprintln!("登录失败: {e:#}");
                    } else {
                        // Reset chat histories when switching backends.
                        anthropic_messages.clear();
                        openai_messages.clear();
                    }
                    continue;
                }
                if line == "/model" || line.starts_with("/model ") {
                    let logged_in = session.lock().map(|s| s.logged_in).unwrap_or(false);
                    if !logged_in {
                        println!("请先 /login。");
                        continue;
                    }
                    let rest = line.strip_prefix("/model").unwrap_or("").trim();
                    if rest.is_empty() {
                        let s = session.lock().unwrap();
                        println!("当前模型: {}", s.model);
                        println!("切换: /model <name>");
                    } else {
                        cfg.model = rest.to_string();
                        let _ = cfg.save();
                        session.lock().unwrap().model = cfg.model.clone();
                        agent.set_model(cfg.model.clone());
                        println!("model → {}", cfg.model);
                    }
                    continue;
                }
                if line.starts_with('/') {
                    println!("未知命令: {line}（试试 /help）");
                    continue;
                }

                let logged_in = session.lock().map(|s| s.logged_in).unwrap_or(false);
                if !logged_in || backend.is_none() {
                    println!("尚未登录，无法调用模型。请先输入 /login。");
                    continue;
                }

                let result = match backend.as_ref().unwrap() {
                    Backend::Anthropic(c) => {
                        run_anthropic_turn(
                            &mut agent,
                            c,
                            &mut anthropic_messages,
                            &budget,
                            &line,
                            &session,
                        )
                        .await
                    }
                    Backend::OpenAi(c) => {
                        run_openai_turn(
                            c,
                            &cfg.model,
                            &cwd,
                            &mut openai_messages,
                            &line,
                            max_tokens,
                            &session,
                        )
                        .await
                    }
                };
                if let Err(err) = result {
                    eprintln!("错误: {err:#}");
                } else {
                    let s = session.lock().unwrap();
                    ui::print_turn_footer(&s, &cfg);
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C（再输入 /exit 退出）");
            }
            Err(ReadlineError::Eof) => {
                println!("bye");
                break;
            }
            Err(err) => {
                eprintln!("读取输入失败: {err}");
                break;
            }
        }
    }

    if let Ok(hp) = Config::history_path() {
        let _ = rl.save_history(&hp);
    }

    Ok(())
}
