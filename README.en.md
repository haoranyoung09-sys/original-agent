# original-agent

[简体中文](README.md) | **English**

A Rust terminal coding agent inspired by [Pi](https://github.com/badlogic/pi-mono). The executable is `original`. This is an independent project, not an official Pi port or a product affiliated with Anthropic, OpenAI, or Google.

## Architecture

- Rust 2021, Tokio for async execution, Clap for CLI arguments, and Rustyline for terminal input.
- Anthropic Messages uses **[claudius](https://github.com/rescrv/claudius) 0.33.0**, a community Rust SDK vendored in [vendor/claudius](vendor/claudius). It is **not an official Anthropic Rust SDK**. Local patches include Windows compatibility and untagged custom-tool serialization.
- OpenAI-compatible Chat Completions uses reqwest and Serde with a local tool loop. There is no native Responses API backend.
- Shared tools provide read, write, edit, multi_edit, glob, literal grep, project_overview, verify, and shell execution.
- `changes.rs` tracks in-process file snapshots; `archive.rs` stores local conversation records. `subscription.rs` hands control to installed official CLIs.

## Model compatibility

These are implemented presets and integration paths, not a claim that every model has been tested with a live account.

| Provider | Preset or integration | Notes |
| --- | --- | --- |
| Anthropic | `claude-sonnet-4-20250514`, Messages | Other model IDs require compatible tool/thinking parameters |
| OpenAI | `gpt-4.1`, Chat Completions | Models requiring other API protocols are not automatically supported |
| DeepSeek | `deepseek-flash`, OpenAI/Anthropic formats | Other account-supported IDs can be configured; recheck prices and context limits |
| xAI / Grok | Custom OpenAI-compatible URL: `https://api.x.ai/v1` | API key required; no native subscription OAuth integration |
| Gemini | Official Gemini CLI handoff | No native Gemini API backend |
| Custom providers | Configurable URL, model and API style | Tool schemas and optional parameters must be compatible |

Prices and context windows come from a built-in public catalog (not live provider data). `/login` and `/model` apply them automatically; unknown models keep current values with a note. Subscription usage is not calculated from API token prices.

## Installation

Prebuilt installation requires published GitHub Release assets and network access to GitHub. No Rust toolchain is needed for this path.

Windows x64, PowerShell:

```powershell
iex (irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1)
```

Linux x64 / macOS Intel or Apple Silicon:

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
```

Installers verify SHA-256. Windows configures the user PATH; Unix installs to `~/.local/bin`. Linux binaries require compatible glibc/OpenSSL libraries. A checksum is not an independent code signature.

Build from source with stable Rust and platform build tools:

```sh
cargo install --locked --path . --force
original
```

Exit running `original` processes before updating on Windows. If dependencies are already cached, add `--offline`. Published binaries may lag behind local source changes.

## Authentication

### API mode

Run `original`, then `/login`. Configure provider, API key, base URL and model; context window and token prices are filled from a built-in public catalog. Configuration is stored in `~/.original-agent/config.json`; saved API keys are plaintext. `ORIGINAL_API_KEY` explicitly overrides the saved key. Provider environment variables are also supported, but avoid leaving keys for several providers set simultaneously. Never commit credentials.

### Official client handoff

Install the official client and put it on PATH:

```sh
original --subscription codex
original --subscription claude
original --subscription gemini
```

The official client owns login, permissions, sessions and billing. This opens its interactive terminal interface; it does not provide subscription models inside original's REPL. Existing API-key authentication is not automatically converted to subscription billing. No browser cookies or official credential caches are copied. Grok subscription handoff is unavailable.

## Commands and safeguards

| Command | Behavior |
| --- | --- |
| `/login`, `/model`, `/status` | Configure and inspect the API session |
| Shift+Tab | Cycle UI effort and its color; backend support varies |
| `/diff` | Show recorded file-tool changes as full-file diffs |
| `/undo`, `/undo all` | Undo recorded edits, rejecting detected external changes |
| `/sessions` | List archives for the current project |
| `/session show <id>` | Display an archive |
| `/export` | Export the current archive as Markdown |
| `/clear`, `/help`, `/exit` | Clear conversation, show help, exit |

Shell execution is disabled by default. For trusted projects, `original --allow-shell --auto-verify` enables interactive automatic verification with up to three repair rounds. Build/test scripts execute project code; this is not a sandbox. One-shot `-p` mode does not currently run the automatic verification loop.

Snapshots are in-memory and cover file tools only, not shell or external CLI changes. Batch write failures trigger attempted restoration, not crash-proof transactions. Undo can partially fail on I/O errors.

Archives live under `~/.original-agent/sessions/`. Use `--no-record` to disable this archive for a run; existing line-editing history is separate. Completed turns and verification output are recorded. Login forms are excluded and known keys are filtered, but arbitrary secrets or source code may remain. Archive viewing does not resume a model conversation or restore files. Retention and deletion are currently manual.

## Development and licensing

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets
cargo build --locked --release
```

GitHub workflows cover Windows, Linux and macOS. Check actual run results rather than assuming every platform passed. Release tags must match Cargo.toml. Changes and limitations are recorded in [docs/archive](docs/archive).

The application uses [MIT](LICENSE); vendored claudius uses Apache-2.0. See [third-party notices](THIRD_PARTY_NOTICES.md), [security notes](SECURITY.md), and [contribution guidelines](CONTRIBUTING.md).
