# Technical Assessment Report: `original-agent`

## 1. Executive Summary

`original-agent` (binary `original`, Cargo version `0.3.9`, source tag `v0.3.10`) is a Rust 2021 terminal coding agent inspired by Pi. Across 19 Rust source modules (6,707 lines) and an auxiliary Python Office worker (`src/office.py`), the project delivers a multi-provider coding assistant with workspace-scoped file tools, in-memory edit rollback, automated test verification, local session archiving, and a full-screen PTY terminal host.

Overall, the codebase demonstrates disciplined engineering around safety defaults, error handling, and offline testability. Its primary gaps involve minor `clippy` technical debt, a version mismatch stalling automated binary releases, structural concentration in `src/main.rs`, and inherent security limits of user-space path checks compared to OS-level sandboxing.

## 2. Architecture and Capabilities

The system separates provider communication, tool execution, session state, and terminal rendering into clear modules:

- **Dual-Backend Model Integration:** Anthropic Messages uses a vendored community SDK (`vendor/claudius` 0.33.0, Apache-2.0) patched for Windows and custom tools. OpenAI-compatible Chat Completions (`src/openai_client.rs`, 684 lines) uses `reqwest` and `serde` with SSE streaming, `429`/`5xx` exponential-backoff retries, malformed tool-call JSON recovery, and 80%-budget context compaction.
- **Workspace Tools and Edit Safety:** Nine shared tools (`project_overview`, `glob`, `grep`, `read`, `write`, `edit`, `multi_edit`, `verify`, `bash`) operate strictly inside the resolved workspace root (`src/workspace.rs`). `multi_edit` validates all edits before writing, and `src/changes.rs` retains in-memory byte snapshots enabling conflict-aware `/undo` and `/diff`. Optional DOCX/PPTX support delegates to Python (`python-docx`/`python-pptx`) under a 60-second deadline.
- **Verification Loop and Persistence:** With `--auto-verify`, detected project tests (`cargo test --locked`, `npm test --silent`, `pytest`, `go test`) run automatically after edits, returning truncated diagnostics to the model for up to three repair attempts. `src/archive.rs` records append-only local session logs with known-secret redaction, while `src/subscription.rs` hands off to installed `codex`, `claude`, or `gemini` CLIs.
- **Terminal Host:** `src/terminal.rs` wraps the interactive loop in a `portable-pty` alternate-screen host with a dynamic 28-column session sidebar at ≥110 columns and a `--classic` fallback.

## 3. Empirical Verification Results

Direct checks executed against the current workspace (`feat/fullscreen-terminal`, commit `c8dcced`) confirm strong baseline health alongside actionable lint findings:

- **Unit Tests (`cargo test --locked`):** 65 passed, 0 failed, 2 ignored (`office_worker_roundtrip` and `office_worker_timeout`, skipped due to missing `python-docx`/`python-pptx`).
- **Formatting (`cargo fmt --check`):** Passed with zero formatting discrepancies.
- **Installer Fixture Suite (`scripts/test-installer.ps1`):** 10 of 10 offline PowerShell installer checks passed (SHA-256 verification, checksum mismatch rejection, HTTP 404/timeout diagnostics, and local-build fallback).
- **Static Analysis (`cargo clippy --locked --all-targets`):** Exited 0, but emitted 6 target warnings (`clippy::dead_code` on `Config::has_api_key` and four legacy frame helpers in `src/ui.rs`; `clippy::needless_borrow` at `src/main.rs:760`; and `clippy::field_reassign_with_default` in `config.rs` and `session.rs` tests).

## 4. Risks, Limitations, and Technical Debt

1. **Security Boundaries:** As documented in `SECURITY.md`, canonical path checks are not an OS sandbox; TOCTOU races, hard links, and untrusted repository files remain risks. Shell execution defaults to off (`--allow-shell` or per-command approval), yet Unix command timeouts only terminate the immediate shell rather than full process trees. Stored `/login` API keys remain plaintext in `~/.original-agent/config.json` (`0o600` on Unix).
2. **Release Pipeline Drift:** `.github/workflows/release.yml` enforces `tag == 'v' + Cargo.toml version`. Because `Cargo.toml` remains `0.3.9` while `v0.3.10` was tagged, automated multi-platform releases are blocked and prebuilt assets lag at `v0.3.7`.
3. **Provider & Code Asymmetry:** `Shift+Tab` effort cycling updates `output_config.effort` on Anthropic but has no wire effect on OpenAI-compatible backends. Meanwhile, `src/main.rs` (1,111 lines) mixes CLI parsing, REPL control, verification orchestration, and backend dispatch.
4. **Testing Scope:** Live API calls, multi-OS CI runs, and dependency vulnerability audits (`cargo audit`) are unverified locally, and vendored `claudius` increases maintenance burden.

## 5. Recommendations

1. **Enforce Clean Lints:** Remove dead UI/config helpers, fix the remaining `clippy` warnings, and add `-- -D warnings` to `.github/workflows/ci.yml`.
2. **Unblock Releases:** Bump `Cargo.toml` to match release tags and document a release checklist linking `Cargo.toml`, `README` badges, and `release.yml`.
3. **Modularize `main.rs`:** Extract REPL turn execution and verification loops into dedicated modules, and map `EffortLevel` to `reasoning_effort` for supported OpenAI-compatible models.
4. **Harden Process & Secret Hygiene:** Adopt Unix process-group termination (`setsid`/`killpg`), add optional OS keychain integration, and schedule `cargo audit` in CI.
