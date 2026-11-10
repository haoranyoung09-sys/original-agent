# original-agent

[简体中文](README.md) · **English** · [日本語](README.ja.md) · [한국어](README.ko.md)

Explore code, edit files, and run project tests from your terminal. Original is a Rust coding agent with Anthropic and OpenAI-compatible backends, file-edit undo, and local session history. Its terminal interface draws on [Pi](https://github.com/badlogic/pi-mono).

## Install the v0.3.10 source tag

Tag `v0.3.10` includes compact feedback for repeated verification failures and a terminal input-frame wrapping fix. Its Cargo package reports `0.3.9`. The release workflow requires matching versions, so install this tag from source with Rust.

```sh
cargo install --locked --git https://github.com/haoranyoung09-sys/original-agent --tag v0.3.10 original-agent
original --version
```

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
| DeepSeek | `/login` selects the OpenAI-compatible preset `https://api.deepseek.com`, model `deepseek-flash` | An Anthropic-compatible preset exists in code but is not offered by the current menu. `/model` fills catalog IDs such as `deepseek-v4-pro` |
| xAI / Grok | Custom OpenAI-compatible URL: `https://api.x.ai/v1` | Choose custom service in `/login`, then set the account's model with `/model`. No subscription OAuth, and xAI models are not in the price catalog |
| Gemini | Official Gemini CLI handoff | No native Gemini API backend |
| Custom providers | `/login` asks only for the URL and fixes OpenAI-compatible style with model `my-model` | The menu does not change API style. Tool schemas and optional parameters must be compatible |

Prices and context windows come from a built-in public catalog in `src/model_catalog.rs` (snapshot reviewed 2026-11-06, not live provider data). `/login` writes the preset's default model, context window, and input/output prices. `/model <name>` looks the ID up and overwrites those fields; unknown models keep the current values and print a note. Subscription usage is not calculated from API token prices.

## Installation

Prebuilt installation downloads GitHub Release assets. No Rust toolchain is required. Release `v0.3.7` includes Windows x64, Linux x64, macOS Intel, and macOS Apple Silicon, each with a `.sha256` file.

Windows x64, PowerShell. Run `iex (irm …)` as one expression. Do not paste the script into an interactive prompt. `irm … | iex` often works, but the grouped form is more reliable:

```powershell
iex (irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1)
```

Pin a version:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1))) -Version v0.3.7
```

Without `iex`, download the script and run it with `-File`:

```powershell
Invoke-WebRequest -UseBasicParsing -Uri https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1 -OutFile $env:TEMP\original-install.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File $env:TEMP\original-install.ps1
```

Linux x64 / macOS Intel or Apple Silicon:

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
# pin a version after downloading the script
sh install.sh v0.3.7
```

Installers verify SHA-256. That checksum is not an independent code signature. Windows x64 installs `%USERPROFILE%\.original-agent\bin\original.exe` and updates the user PATH (the current PowerShell session is updated too; reopen other terminals). Unix installs `~/.local/bin/original` and prints an `export` line when that directory is not on `PATH`. Linux binaries are built on Ubuntu 22.04, need a compatible glibc, and use the system OpenSSL through native-tls. The macOS and Windows builds do not.

Update by running the same installer again. It replaces the binary in place. On Windows, quit any running `original.exe` first: a locked file aborts the copy and leaves the previous binary unchanged.

If the requested Release has no matching asset, installation fails and does not overwrite an existing binary. For an offline check, point `-BaseUrl` or `ORIGINAL_INSTALL_BASE` at a local directory of release files. After `cargo build --locked --release` on Windows, `scripts/install-from-local-build.ps1` copies `target\release\original.exe`, and `scripts/package-local-release.ps1` writes a fake Release directory that `install.ps1 -BaseUrl` can verify.

Build from source with stable Rust and platform build tools:

```sh
cargo install --locked --git https://github.com/haoranyoung09-sys/original-agent.git original-agent
```

From a local checkout: `cargo install --locked --path . --force`. Windows can run `scripts/install-original.ps1`; Unix can run `sh scripts/install-original.sh`. The Cargo bin directory must be on `PATH`. The crate is not published to crates.io. If dependencies are already cached, `cargo install` accepts `--offline`. A published Release can lag behind unreleased local commits.

## Authentication

### API mode

Run `original`, then `/login`, and choose API. The next menu is Anthropic, OpenAI, DeepSeek, or a custom service, followed by an API key. The menu needs an interactive terminal. Built-in providers keep their preset URL and default model, and store that preset's context window and input/output prices (the same snapshot as those default IDs; a cache-read price is stored only when the catalog has one). DeepSeek here is the OpenAI-compatible preset. Custom service asks for the base URL, sets the model to `my-model`, context window 128000, and no token prices. For the official xAI API, enter `https://api.x.ai/v1` and then change the model with `/model`. An empty API key keeps the saved key; changing the base URL clears it first. Switch models with `/model <name>`.

Configuration is stored in `~/.original-agent/config.json`; saved API keys are plaintext. Environment variables override the file. The first non-empty value wins, in this order: `ORIGINAL_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `DEEPSEEK_API_KEY`, `CLAUDIUS_API_KEY`. The lookup does not follow the selected provider, so leave only one key set. A `.env` file in the user config directory is loaded. A project `.env` is not. Never commit credentials.

### Official client handoff

Install the official client and put it on PATH, then either:

```sh
original --subscription codex
original --subscription claude
original --subscription gemini
```

or choose 订阅 inside `/login` (Codex, Claude, or Gemini). The official client owns login, permissions, sessions and billing. This opens its interactive terminal interface; it does not provide subscription models inside original's REPL. Existing API-key authentication is not automatically converted to subscription billing. No browser cookies or official credential caches are copied. `--subscription grok` returns unsupported. `original --allow-shell` does not apply to this handoff.

`original --subscription …` returns before the user-config `.env` is loaded, so it inherits only the environment of the launching terminal. Choosing subscription from `/login` after the REPL has started inherits variables already loaded from that `.env`. Neither path reads a project `.env`.

## Working directory

- `--cwd <path>` uses that directory. A relative path is resolved from the process's current directory, the path must exist, and it is canonicalized. This skips both the Git-root walk and the home-directory picker.
- With no `--cwd`, outside the home directory, the workspace is the nearest ancestor that contains `.git`, or the current directory when none exists. Starting inside a repository subdirectory therefore roots file tools at the Git root.
- With no `--cwd`, in the home directory, an interactive terminal (no `-p` and no positional prompt) lists immediate children of `Documents/GitHub`, `Desktop`, and the home directory that themselves contain `.git`, or accepts a typed project path. Non-interactive starts, including one-shot mode and a non-terminal stdin, stay in the home directory. `original --subscription …` launched from home goes through the same picker, then starts the official CLI in the chosen directory.
- File tools cannot leave this workspace. At the end of an interactive turn the program lists paths recorded by file tools and Git-visible unstaged or untracked changes. Without Git, only file-tool records are listed.

## Quick start

```sh
original --cwd /path/to/project
original -p "explain the project structure"
original explain the project structure
original --allow-shell
original --auto-verify
```

`-p` and trailing positional arguments are both one-shot prompts (`-p` wins when both are present). One-shot mode needs a saved config or an API key in the environment. File reads and writes inside the workspace are allowed by default. Shell is off until `--allow-shell`, which lets the model run arbitrary commands as your user. Read [SECURITY.md](SECURITY.md) first.

## Commands and safeguards

| Command | Behavior |
| --- | --- |
| `/login` | Choose API or subscription; API then picks a provider and an API key |
| `/model <name>` | Switch model and apply catalog context and prices |
| `/status` | Model, usage, and cost estimate |
| Shift+Tab | Cycle low (white) → medium (green) → high (magenta) → max (red) |
| `/diff` | Show recorded file-tool changes as full-file diffs |
| `/undo`, `/undo all` | Undo recorded edits, rejecting detected external changes |
| `/sessions` | List archives whose saved cwd equals the current workspace path |
| `/session show <id>` | Display an archive |
| `/export` | Export the current archive as Markdown |
| `/clear`, `/help`, `/exit` | Clear conversation, show help, exit |

The input frame is a rule, the prompt, and a footer rule labeled with the model and effort. Shift+Tab repaints in place. `NO_COLOR` disables color. The Anthropic path sends the level as `output_config.effort`. OpenAI-compatible requests do not send `reasoning_effort`; the color is only a local choice.

Shared model tools are `project_overview`, `glob`, `grep`, `read`, `edit`, `write`, `multi_edit`, `verify`, and `bash`. `verify` and `bash` need `--allow-shell`; without it, `verify` only suggests a command. `original --auto-verify` is a separate switch. It runs one detected project test and does not enable arbitrary shell: `Cargo.toml` → `cargo test --locked`, `package.json` → `npm test --silent`, `pyproject.toml` or `pytest.ini` → `python -m pytest -q`, `go.mod` → `go test ./...`. Those commands execute repository code. Use them only in a trusted project.

In an interactive session, verification runs when this turn recorded a file-tool edit or Git reports a new unstaged or untracked change. On failure, shortened stdout and stderr go back to the model (preferring the first line that starts with error, fatal, failed, panic, or traceback) for at most three repair turns. A repair turn with no new workspace change stops the loop. If no project command is detected, the program prints that and continues. One-shot mode (`-p` or positional arguments) verifies once and does not ask the model to repair. A missing command or a non-zero test makes the process fail.

`--command-timeout` defaults to 600 seconds and accepts 1–600. It caps both shell commands and automatic verification. It does not change the HTTP timeout. `original --show-thinking` prints provider thinking after each model response (not token streaming): Anthropic thinking blocks, or `reasoning_content` on an OpenAI-compatible response.

Snapshots are in-memory and cover file tools only, not shell or external CLI changes. They are discarded on exit. Batch write failures trigger attempted restoration, not crash-proof transactions. Undo can partially fail on I/O errors. Without Git, automatic verification looks only at file-tool records.

Archives live under `~/.original-agent/sessions/`. Use `--no-record` to disable this archive for a run; existing line-editing history is separate. Official CLI handoff does not archive. Completed turns and verification output (command, exit code, stdout, stderr, elapsed time) are recorded. Login forms are excluded and known keys are filtered, but arbitrary secrets or source code may remain. Archive viewing does not resume a model conversation or restore files. Retention and deletion are currently manual.

## Development and licensing

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets
cargo build --locked --release
# installer fixtures; they do not contact GitHub
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-installer.ps1
sh scripts/test-installer.sh
```

GitHub workflows cover Windows, Linux and macOS. Check actual run results rather than assuming every platform passed. Release tags must match Cargo.toml. Changes and limitations are recorded in [docs/archive](docs/archive).

The application uses [MIT](LICENSE); vendored claudius uses Apache-2.0. See [third-party notices](THIRD_PARTY_NOTICES.md), [security notes](SECURITY.md), and [contribution guidelines](CONTRIBUTING.md).

Local commands: `/settings` (also `/setting`, `/config`) shows configuration and changes effort. `/usage` shows session token counters. `/history` lists project sessions and `/history ID` opens an archive.
