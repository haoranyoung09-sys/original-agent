//! Interactive handoff to official clients without copying their credentials.
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Client {
    Codex,
    Claude,
    Gemini,
    Grok,
}
impl Client {
    fn command(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("codex"),
            Self::Claude => Some("claude"),
            Self::Gemini => Some("gemini"),
            Self::Grok => None,
        }
    }
}
fn find_client(name: &str) -> Option<PathBuf> {
    // Search explicit absolute PATH directories; never implicitly search the workspace.
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        if !dir.is_absolute() {
            continue;
        }
        #[cfg(windows)]
        let candidates = [
            dir.join(format!("{name}.exe")),
            dir.join(format!("{name}.cmd")),
        ];
        #[cfg(not(windows))]
        let candidates = [dir.join(name)];
        for path in candidates {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}
pub fn launch(client: Client, cwd: &Path) -> Result<()> {
    let Some(name) = client.command() else {
        bail!("Grok subscription handoff is not available. Use /login with the official xAI API endpoint and API key; subscription entitlement is not assumed.");
    };
    let path = find_client(name).with_context(|| format!("Official {name} CLI not found on PATH. Install it from the provider's official documentation, then retry."))?;
    println!("Opening {name}. Sign in using its official account flow. Active authentication, billing, permissions and session history are controlled by that CLI, not original. No subscription quota or API price is inferred.");
    let status = std::process::Command::new(path)
        .current_dir(cwd)
        .status()
        .context("Could not start official CLI")?;
    if !status.success() {
        bail!("Official {name} CLI exited with {status}");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_verified_clients_have_launch_commands() {
        assert_eq!(Client::Codex.command(), Some("codex"));
        assert_eq!(Client::Claude.command(), Some("claude"));
        assert_eq!(Client::Gemini.command(), Some("gemini"));
        assert_eq!(Client::Grok.command(), None);
    }
    #[test]
    fn grok_does_not_spawn_or_assume_subscription_access() {
        assert!(launch(Client::Grok, Path::new("."))
            .unwrap_err()
            .to_string()
            .contains("not available"));
    }
}
