//! Bash PTY session helpers.
//!
//! On Unix this wraps a real PTY-backed shell. On Windows the types exist for API
//! compatibility but spawning is unsupported (pi-ds uses its own PowerShell tool).

#[cfg(unix)]
#[path = "pty_unix.rs"]
mod imp;

#[cfg(unix)]
pub use imp::{BashPtyConfig, BashPtyResult, BashPtySession};

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use std::time::Duration;

    /// Configuration for a bash PTY session.
    #[derive(Debug, Clone)]
    pub struct BashPtyConfig {
        /// Shell binary to spawn.
        pub shell: PathBuf,
        /// Working directory for the session.
        pub cwd: Option<PathBuf>,
        /// Per-command timeout.
        pub command_timeout: Duration,
        /// Extra environment variables.
        pub env: Vec<(String, String)>,
    }

    impl Default for BashPtyConfig {
        fn default() -> Self {
            Self {
                shell: PathBuf::from("bash"),
                cwd: None,
                command_timeout: Duration::from_secs(60),
                env: Vec::new(),
            }
        }
    }

    /// Result of running a command in a bash PTY session.
    #[derive(Debug, Clone)]
    pub struct BashPtyResult {
        /// Combined terminal output.
        pub output: String,
        /// Process exit status if known.
        pub status: Option<i32>,
        /// Whether the command timed out.
        pub timed_out: bool,
    }

    /// A reusable bash session backed by a PTY (Unix only).
    #[derive(Debug)]
    pub struct BashPtySession {
        _config: BashPtyConfig,
    }

    impl BashPtySession {
        /// Create a new session.
        pub async fn new(config: BashPtyConfig) -> std::io::Result<Self> {
            let _ = config;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "BashPtySession is not supported on Windows; use a custom shell tool instead",
            ))
        }

        /// Run a command in the session.
        pub async fn run(
            &mut self,
            command: &str,
            restart: bool,
        ) -> std::io::Result<BashPtyResult> {
            let _ = (command, restart);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "BashPtySession is not supported on Windows",
            ))
        }

        /// Run a command, streaming output lines to `on_line`.
        pub async fn run_with_output<F>(
            &mut self,
            command: &str,
            restart: bool,
            mut on_line: F,
        ) -> std::io::Result<BashPtyResult>
        where
            F: FnMut(&str) + Send,
        {
            let _ = (&mut on_line, command, restart);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "BashPtySession is not supported on Windows",
            ))
        }
    }
}

#[cfg(windows)]
pub use win::{BashPtyConfig, BashPtyResult, BashPtySession};
