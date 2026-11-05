# original-agent

Rust 编写的多厂商终端 coding agent，交互参考 [Pi](https://github.com/badlogic/pi-mono)。命令名为 `original`。

## 安装

无需 Rust 的安装方式（首次发布 Release 后可用）：

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1 | iex
```

Linux x64 / macOS Intel、Apple Silicon：

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
```

脚本下载 GitHub Release 并校验 SHA-256。Windows 自动添加用户 PATH；Unix 安装到 `~/.local/bin`，若尚未在 PATH 会提示配置命令。Linux 版本基于 Ubuntu 22.04，需要兼容 glibc 和 OpenSSL 运行库。更新时重复安装命令；固定版本可下载脚本后传入 `v0.3.3`（PowerShell 使用 `-Version v0.3.3`）。

维护者发布：提交并推送代码后，推送与 Cargo.toml 版本相同的标签（如 `v0.3.3`）；Release 工作流构建四个平台并发布附件。脚本校验完整性，不提供独立代码签名。首次 Release 完成前下载会失败，已有安装不会因此被替换。

### 从源码安装

需要稳定版 Rust 工具链及平台编译工具。仓库发布后可用一条命令从源码安装：

```sh
cargo install --locked --git https://github.com/haoranyoung09-sys/original-agent.git original-agent
original
```

本地仓库安装：`cargo install --locked --path . --force`。也可执行 Windows 的 `scripts/install-original.ps1` 或 Unix 的 `sh scripts/install-original.sh`。确保 Cargo bin 目录在 PATH 中。尚未发布到 crates.io。

## 使用

先运行 `original`，输入 `/login` 配置服务商、模型和 API Key。配置保存在 `~/.original-agent/config.json`。可通过 `ORIGINAL_API_KEY` 覆盖密钥；用户配置目录中的 `.env` 可加载，项目目录的 `.env` 不会自动加载。

```sh
original --cwd /path/to/project
original -p "解释项目结构"
original --allow-shell
```

默认允许工作区文件读写，禁止 Shell。`--allow-shell` 允许模型以你的用户权限执行任意命令。请先阅读 [安全边界](SECURITY.md)。

| 操作 | 用途 |
| --- | --- |
| `/login` | 服务商、Key、URL、模型、价格 |
| `/model <name>` | 切换模型（价格和窗口需重新核对） |
| `/status` | 模型、用量、费用估算 |
| Shift+Tab | low 青色 → medium 黄色 → high 紫色 |
| `/clear` | 清除当前对话；保留累计用量 |
| `/help`、`/exit` | 帮助、退出 |

### Vibe coding 工具

自然语言改代码时，模型可调用（两端后端共用 schema）：

| 工具 | 用途 |
| --- | --- |
| `project_overview` | 项目类型、浅层目录树、README 摘要 |
| `glob` / `grep` | 无 shell 的文件发现与内容搜索 |
| `read` / `edit` / `write` | 读、单处替换、整文件写 |
| `multi_edit` | 跨文件多处唯一替换（先校验再写入） |
| `verify` | 改后测试/构建回灌（需 `--allow-shell`，否则只给建议命令） |
| `bash` | 任意命令（需 `--allow-shell`） |

文件工具不出工作区；默认禁止 shell。详见 [SECURITY.md](SECURITY.md)。

输入区采用连续上下横线，中间输入，底部显示“模型名 · 思考强度”。Shift+Tab 原位重绘，不追加提示行；设置 `NO_COLOR` 可禁用颜色。OpenAI 兼容后端尚未发送 reasoning_effort，颜色表示 UI 选择，不代表所有模型都支持该参数。

## 模型默认值

价格单位为 USD / 1M tokens，核对日期 2026-11-05。实际账单以服务商为准。

| 模型 | 上下文 | 输入 | 输出 |
| --- | ---: | ---: | ---: |
| Claude Sonnet 4 | 200,000 | 3 | 15 |
| GPT-4.1 | 1,047,576 | 2 | 8 |
| DeepSeek Flash | 1,000,000 | 0.30 | 1.20 |

DeepSeek 使用峰值费率估算，非高峰费率减半。缓存折扣等可能未完整计入。自定义模型必须自行设置价格和上下文。[官方资料与审查归档](docs/archive/2026-11-05-review.md)。

## 开发

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets
cargo build --locked --release
```

GitHub CI 覆盖 Windows、Linux、macOS。源码位于 `src/`；vendored SDK 位于 `vendor/claudius/`。本次修改、验证范围及未解决限制归档于 `docs/archive/`（含 [vibe coding 拓展与检验](docs/archive/2026-11-05-vibe-coding.md)）。

[贡献指南](CONTRIBUTING.md) · [安全说明](SECURITY.md) · [MIT 许可证](LICENSE) · [第三方声明](THIRD_PARTY_NOTICES.md)
