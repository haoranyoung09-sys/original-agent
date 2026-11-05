# 2026-11-05 vibe coding 拓展与检验归档

## 目标

在本机仓库完善已有功能的自动化检验，并增强自然语言改代码（vibe coding）路径：多文件编辑、项目上下文、改后校验回灌。提交由仓库所有者完成；本轮未 git commit / push。

## 新增能力

- `project_overview`：识别常见清单文件、浅层目录树、README 摘要（无需 shell）。
- `glob` / `grep`：工作区内文件发现与字面量内容搜索（无需 shell；跳过 target/node_modules/.git 等）。
- `multi_edit`：跨文件/多处唯一字符串替换；先全部校验再写入，失败不落盘。
- `verify`：改后跑测试/构建反馈；未加 `--allow-shell` 时返回建议命令（如 `cargo test --locked`）。
- 共享 `fsops` + `tool_dispatch`：Anthropic 工具与 OpenAI 兼容循环使用同一套实现与 schema。
- 系统提示改为 vibe coding 工作流；`/help` 列出新工具。Pi 风格输入框 / Shift+Tab 未改交互形态。
- `commands::parse_slash`：REPL 斜杠命令可单测。

## 自动化检验覆盖

`cargo test --locked` 覆盖（无真实 API Key，使用本地临时目录 / stub）：

| 区域 | 内容 |
| --- | --- |
| workspace | `..` 拒绝、解析无副作用、相对路径 |
| config | 价格/URL 校验、save_to/load_from 持久化往返、费用估算 |
| session | effort 解析与循环、usage/cache、from_config |
| commands | /login /model /status /clear /help /exit 与未知命令 |
| ui | 输入框宽、status 行、effort 颜色契约 |
| fsops | read/write/edit、multi_edit 原子校验、glob/grep、project_overview |
| tool_dispatch | 共享 dispatch、verify/bash 默认禁用、OpenAI tool schema |
| shell | 默认 exit 126 |
| openai_client | completions URL 变体、usage 解析、system prompt |
| agent | system prompt 含 vibe 工具 |
| provider | DeepSeek Anthropic base 规范化 |
| main wire | 自定义 tool JSON、斜杠契约、effort 循环 |

未覆盖：真实 Anthropic/OpenAI 网络调用、交互式 `/login` TTY 录入、rustyline Shift+Tab 按键事件本身（逻辑由 effort.cycle 单测代替）。

## 安全边界（保持）

- 文件工具仍受 `resolve_in_workspace` 约束，默认无 shell。
- `verify` / `bash` 仅在 `--allow-shell` 下执行命令。
- 不自动加载工作区 `.env`。

## 已知限制

- OpenAI 兼容请求仍不发送 `reasoning_effort`；Shift+Tab 颜色表示 UI 选择。
- `grep` 为字面量子串，非正则；大仓库有命中/扫描上限。
- `multi_edit` 非统一 diff 格式，是 JSON 多段 str_replace。
- 真实 API 与安装脚本端到端需推送后由 CI / 本机手动验证。

## 本机验证结果

2026-11-05（Asia/Shanghai）：cargo test --locked **32 passed**；cargo build --locked --release 通过。未 git commit / push。
