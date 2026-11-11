//! Full-screen host for the existing interactive agent and its authorization prompts.
//! Wide terminals show a session index on the right; narrower ones hide it and
//! recompute the layout on resize. Login, settings, and command approval still
//! run in the embedded classic interface.
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    time::Duration,
};

const SIDEBAR_WIDTH: u16 = 28;
const SIDEBAR_MIN_COLUMNS: u16 = 110;
const SCROLL_STEP: usize = 10;

struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show);
    }
}

fn layout(columns: u16) -> u16 {
    if columns >= SIDEBAR_MIN_COLUMNS {
        columns.saturating_sub(SIDEBAR_WIDTH).max(1)
    } else {
        columns.max(1)
    }
}

fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        let w = if c.is_ascii() { 1 } else { 2 };
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    while used < width {
        out.push(' ');
        used += 1;
    }
    out
}

fn forwarded_args() -> Vec<OsString> {
    let mut out = Vec::new();
    let mut args = std::env::args_os().skip(1).peekable();
    while let Some(arg) = args.next() {
        if arg == "--classic" {
            continue;
        }
        if arg == "--cwd" {
            let _ = args.next();
            continue;
        }
        if arg.to_string_lossy().starts_with("--cwd=") {
            continue;
        }
        out.push(arg);
    }
    out
}

fn key_bytes(key: event::KeyEvent) -> Option<String> {
    Some(match key.code {
        KeyCode::Char(c)
            if key.modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_alphabetic() =>
        {
            String::from((c.to_ascii_lowercase() as u8 - b'a' + 1) as char)
        }
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "\r".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Up => "\x1b[A".into(),
        KeyCode::Down => "\x1b[B".into(),
        KeyCode::Right => "\x1b[C".into(),
        KeyCode::Left => "\x1b[D".into(),
        KeyCode::Home => "\x1b[H".into(),
        KeyCode::End => "\x1b[F".into(),
        KeyCode::Delete => "\x1b[3~".into(),
        _ => return None,
    })
}

fn draw_sidebar(
    out: &mut impl Write,
    width: u16,
    rows: u16,
    columns: u16,
    listing: &str,
) -> Result<()> {
    if columns < SIDEBAR_MIN_COLUMNS {
        return Ok(());
    }
    let text_width = SIDEBAR_WIDTH.saturating_sub(1) as usize;
    let mut lines: Vec<String> = vec!["Sessions".into(), String::new()];
    lines.extend(
        listing
            .lines()
            .take(rows.saturating_sub(7) as usize)
            .map(|line| line.to_string()),
    );
    let hints = [
        "/history: open archive",
        "/settings: preferences",
        "F10: exit",
        "PgUp/PgDn: scroll",
    ];
    for y in 0..rows {
        let text = if y + 4 >= rows {
            hints[(y + 4 - rows) as usize].to_string()
        } else {
            lines.get(y as usize).cloned().unwrap_or_default()
        };
        queue!(
            out,
            cursor::MoveTo(width, y),
            crossterm::style::Print(format!("│{}", fit(&text, text_width)))
        )?;
    }
    Ok(())
}

pub fn run(root: &Path) -> Result<()> {
    let (columns, rows) = terminal::size()?;
    let mut width = layout(columns);
    let pair = native_pty_system().openpty(PtySize {
        rows: rows.max(1),
        cols: width,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = CommandBuilder::new(std::env::current_exe()?);
    command.env("ORIGINAL_AGENT_HOST", "1");
    command.cwd(root);
    command.arg("--classic");
    command.arg("--cwd");
    command.arg(root);
    command.args(forwarded_args());
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let (send, receive) = mpsc::sync_channel(128);
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        while let Ok(n) = reader.read(&mut bytes) {
            if n == 0 || send.send(bytes[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut parser = vt100::Parser::new(rows.max(1), width, 5000);
    terminal::enable_raw_mode()?;
    let _restore = Restore;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        Clear(ClearType::All),
        cursor::Show
    )?;
    let result = (|| -> Result<()> {
        let mut dirty = true;
        let mut listing = String::new();
        let mut refreshed = std::time::Instant::now() - Duration::from_secs(3);
        loop {
            while let Ok(data) = receive.try_recv() {
                parser.process(&data);
                dirty = true;
            }
            if dirty {
                let (columns, rows) = terminal::size()?;
                let mut out = std::io::stdout().lock();
                out.write_all(&parser.screen().contents_formatted())?;
                if refreshed.elapsed() >= Duration::from_secs(2) {
                    listing = crate::archive::command("/sessions", root)
                        .and_then(|r| r.ok())
                        .unwrap_or_default();
                    refreshed = std::time::Instant::now();
                }
                draw_sidebar(&mut out, width, rows, columns, &listing)?;
                let (row, col) = parser.screen().cursor_position();
                queue!(out, cursor::MoveTo(col, row))?;
                out.flush()?;
                dirty = false;
            }
            if child.try_wait()?.is_some() {
                break;
            }
            if !event::poll(Duration::from_millis(30))? {
                continue;
            }
            match event::read()? {
                Event::Resize(columns, rows) => {
                    width = layout(columns);
                    pair.master.resize(PtySize {
                        rows: rows.max(1),
                        cols: width,
                        pixel_width: 0,
                        pixel_height: 0,
                    })?;
                    parser.set_size(rows.max(1), width);
                    execute!(std::io::stdout(), Clear(ClearType::All))?;
                    dirty = true;
                }
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if key.code == KeyCode::F(10) {
                        break;
                    }
                    if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
                        let offset = parser.screen().scrollback();
                        parser.set_scrollback(if key.code == KeyCode::PageUp {
                            offset + SCROLL_STEP
                        } else {
                            offset.saturating_sub(SCROLL_STEP)
                        });
                        dirty = true;
                        continue;
                    }
                    parser.set_scrollback(0);
                    if let Some(text) = key_bytes(key) {
                        writer.write_all(text.as_bytes())?;
                        writer.flush()?;
                        dirty = true;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_threshold() {
        assert_eq!(layout(80), 80);
        assert_eq!(layout(109), 109);
        assert_eq!(layout(110), 82);
    }

    #[test]
    fn resized_screen_retains_input() {
        let mut parser = vt100::Parser::new(24, layout(120), 100);
        parser.process("hello\r\n❯ draft".as_bytes());
        parser.set_size(20, layout(80));
        assert!(parser.screen().contents().contains("draft"));
        assert_eq!(parser.screen().size(), (20, 80));
    }

    #[test]
    fn fit_pads_and_truncates_wide_chars() {
        assert_eq!(fit("abc", 5), "abc  ");
        assert_eq!(fit("会话记录AB", 6), "会话记");
        assert_eq!(fit("ok\u{07}!", 4), "ok! ");
    }
}
