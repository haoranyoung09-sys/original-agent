//! Shell gate and process runner. Disabled unless `--allow-shell`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static TIMEOUT_LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(600);
pub fn set_timeout_limit(seconds: u64) {
    TIMEOUT_LIMIT.store(seconds.clamp(1, 600), Ordering::Relaxed);
}
static SHELL_ENABLED: AtomicBool = AtomicBool::new(false);

static PROMPTS: AtomicBool = AtomicBool::new(false);
static APPROVAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn enable_prompts() {
    PROMPTS.store(true, Ordering::Relaxed);
}
fn can_prompt() -> bool {
    use std::io::IsTerminal;
    PROMPTS.load(Ordering::Relaxed)
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
}
fn approve(command: &str, cwd: &PathBuf) -> bool {
    if !can_prompt() {
        return false;
    }
    let _guard = APPROVAL.lock().unwrap();
    eprintln!(
        "执行目录 {:?}\n命令 {:?}\n仅允许这一次？[y/N]",
        cwd, command
    );
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}
pub fn set_shell_enabled(enabled: bool) {
    SHELL_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn shell_enabled() -> bool {
    SHELL_ENABLED.load(Ordering::Relaxed) || can_prompt()
}

/// Run a shell command in `cwd` with a soft timeout.
pub fn execute_shell(command: &str, cwd: &PathBuf, timeout_secs: u64) -> (i32, String, String) {
    if !SHELL_ENABLED.load(Ordering::Relaxed) && !approve(command, cwd) {
        return (
            126,
            String::new(),
            "Shell disabled; restart with --allow-shell to authorize unrestricted commands.".into(),
        );
    }
    run_command(command, cwd, timeout_secs)
}

pub fn verify_project(command: &str, cwd: &PathBuf) -> (i32, String, String) {
    if crate::fsops::suggest_verify_command(cwd) != Some(command) {
        return (
            126,
            String::new(),
            "Unrecognized verification command".into(),
        );
    }
    execute_shell(command, cwd, 600)
}

fn run_command(command: &str, cwd: &PathBuf, timeout_secs: u64) -> (i32, String, String) {
    let timeout = timeout_secs
        .clamp(1, 600)
        .min(TIMEOUT_LIMIT.load(Ordering::Relaxed));
    let start = std::time::Instant::now();
    let result = execute_inner(command, cwd, timeout);
    crate::tool_display::status(
        &format!("command (limit {timeout}s, exit {})", result.0),
        result.0 == 0,
        start.elapsed().as_secs_f64(),
    );
    result
}
fn capture_output(mut reader: impl std::io::Read) -> String {
    const HALF: usize = 40_000;
    let mut head = Vec::new();
    let mut tail = std::collections::VecDeque::with_capacity(HALF);
    let mut total = 0usize;
    let mut buf = [0; 8192];
    while let Ok(n) = reader.read(&mut buf) {
        if n == 0 {
            break;
        }
        total += n;
        let keep = (HALF - head.len()).min(n);
        head.extend_from_slice(&buf[..keep]);
        let rest = &buf[keep..n];
        tail.drain(..(tail.len() + rest.len()).saturating_sub(HALF));
        tail.extend(rest.iter().copied());
    }
    let skipped = total - head.len() - tail.len();
    if skipped > 0 {
        head.extend_from_slice(format!("\n… [{skipped} bytes omitted] …\n").as_bytes());
    }
    head.extend(tail);
    String::from_utf8_lossy(&head).into_owned()
}

#[cfg(test)]
#[test]
fn bounded_log_preserves_failure_tail() {
    let text = format!("{}final failure", "x".repeat(100_000));
    let result = capture_output(text.as_bytes());
    assert!(result.contains("20013 bytes omitted") && result.ends_with("final failure"));
    assert_eq!(capture_output("中文".as_bytes()), "中文");
}

fn execute_inner(command: &str, cwd: &PathBuf, timeout_secs: u64) -> (i32, String, String) {
    use std::io::Read;
    use std::process::Stdio;
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let mut child = match cmd
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (1, String::new(), e.to_string()),
    };
    fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || capture_output(&mut reader))
    }
    let out = drain(child.stdout.take().unwrap());
    let err = drain(child.stderr.take().unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.clamp(1, 600));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let drain_deadline = std::time::Instant::now() + Duration::from_millis(200);
                while !(out.is_finished() && err.is_finished())
                    && std::time::Instant::now() < drain_deadline
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let stdout = if out.is_finished() {
                    out.join().unwrap_or_default()
                } else {
                    String::new()
                };
                let stderr = if err.is_finished() {
                    err.join().unwrap_or_default()
                } else {
                    String::new()
                };
                return (status.code().unwrap_or(1), stdout, stderr);
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .output();
                }
                let _ = child.kill();
                let _ = child.wait();
                return (
                    124,
                    String::new(),
                    "Shell timed out and termination was requested".into(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_defaults_disabled() {
        set_shell_enabled(false);
        let cwd = std::env::temp_dir();
        let (code, _, err) = execute_shell("echo hi", &cwd, 5);
        assert_eq!(code, 126);
        assert!(err.contains("Shell disabled"));
    }
}
