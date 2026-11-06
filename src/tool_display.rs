//! Compact tool execution status; never presents synthetic model reasoning.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};
static RUNS: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
fn runs() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(300).collect()
}
pub fn start(id: &str, name: &str, args: &serde_json::Value) {
    let detail = args
        .get("command")
        .or_else(|| args.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let title = safe(&format!("{name} {detail}"));
    println!("▶ {title}");
    runs()
        .lock()
        .unwrap()
        .insert(id.into(), (title, Instant::now()));
}
pub fn finish(id: &str, success: bool) {
    if let Some((title, start)) = runs().lock().unwrap().remove(id) {
        status(&title, success, start.elapsed().as_secs_f64());
    }
}
pub fn status(title: &str, success: bool, seconds: f64) {
    use std::io::IsTerminal;
    let label = format!(
        " {} {} · {:.2}s ",
        if success { "✓" } else { "✗" },
        safe(title),
        seconds
    );
    if std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal() {
        println!(
            "\x1b[{}m{}\x1b[0m",
            if success { "42;30" } else { "41;97" },
            label
        );
    } else {
        println!("{label}");
    }
}

pub static SHOW_THINKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn thinking_enabled() -> bool {
    SHOW_THINKING.load(std::sync::atomic::Ordering::Relaxed)
}
