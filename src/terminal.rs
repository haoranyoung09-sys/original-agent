//! Full-screen host for the existing interactive agent and its authorization prompts.
//! Wide terminals show a session index on the right; narrower ones hide it and
//! recompute the layout on resize. Login, settings, and command approval still
//! run in the embedded classic interface.
use anyhow::Result;
use crossterm::{
    cursor,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    time::Duration,
};

use crate::config::Theme;
use crate::session::EffortLevel;

const SIDEBAR_WIDTH: u16 = 28;
const SIDEBAR_MIN_COLUMNS: u16 = 110;
const SCROLL_STEP: usize = 10;

const SIDEBAR_FG: Color = Color::AnsiValue(245);
const SIDEBAR_RULE: Color = Color::AnsiValue(239);
const SIDEBAR_TITLE: Color = Color::AnsiValue(189);

struct Restore {
    previous_title: String,
}

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut stdout = std::io::stdout();
        if !self.previous_title.is_empty() {
            let _ = execute!(stdout, SetTitle(self.previous_title.as_str()));
        }
        let _ = execute!(
            stdout,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            cursor::Show
        );
    }
}

fn current_title() -> String {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleTitleW;
        let mut buf = [0u16; 256];
        let n = GetConsoleTitleW(buf.as_mut_ptr(), buf.len() as u32);
        if n > 0 {
            return String::from_utf16_lossy(&buf[..n as usize]);
        }
    }
    String::new()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostStatus {
    model: String,
    effort: EffortLevel,
    shell_enabled: bool,
    show_thinking: bool,
    logged_in: bool,
    ctx: String,
    cost: String,
    theme: Theme,
    api_style: String,
    busy_since: Option<std::time::Instant>,
}

impl HostStatus {
    fn from_config() -> Self {
        let allow_shell = std::env::args_os().any(|a| a == "--allow-shell");
        let show_thinking = std::env::args_os().any(|a| a == "--show-thinking");
        match crate::config::Config::load() {
            Ok(cfg) => Self {
                logged_in: !cfg.model.trim().is_empty(),
                model: if cfg.model.trim().is_empty() {
                    "未登录".into()
                } else {
                    cfg.model
                },
                effort: cfg.effort,
                shell_enabled: allow_shell,
                show_thinking,
                ctx: String::new(),
                cost: String::new(),
                theme: cfg.theme,
                api_style: cfg.api_style.as_str().to_string(),
                busy_since: None,
            },
            Err(_) => Self {
                model: "未登录".into(),
                effort: EffortLevel::Medium,
                shell_enabled: allow_shell,
                show_thinking,
                logged_in: false,
                ctx: String::new(),
                cost: String::new(),
                theme: Theme::None,
                api_style: "anthropic".into(),
                busy_since: None,
            },
        }
    }

    fn apply_osc(&mut self, payload: &str) -> bool {
        let prev_busy = self.busy_since.is_some();
        let prev = self.clone();
        for part in payload.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k {
                "model" => self.model = v.to_string(),
                "effort" => {
                    if let Ok(e) = v.parse::<EffortLevel>() {
                        self.effort = e;
                    }
                }
                "shell" => self.shell_enabled = v == "1" || v == "true",
                "thinking" => self.show_thinking = v == "1" || v == "true",
                "logged_in" => self.logged_in = v == "1" || v == "true",
                "ctx" => self.ctx = v.to_string(),
                "cost" => self.cost = v.to_string(),
                "theme" => {
                    if let Ok(t) = v.parse::<Theme>() {
                        self.theme = t;
                    }
                }
                "api_style" => self.api_style = v.to_string(),
                "busy" => {
                    let is_busy = v == "1" || v == "true";
                    if is_busy {
                        if self.busy_since.is_none() {
                            self.busy_since = Some(std::time::Instant::now());
                        }
                    } else {
                        self.busy_since = None;
                    }
                }
                _ => {}
            }
        }
        self.model != prev.model
            || self.effort != prev.effort
            || self.shell_enabled != prev.shell_enabled
            || self.show_thinking != prev.show_thinking
            || self.logged_in != prev.logged_in
            || self.ctx != prev.ctx
            || self.cost != prev.cost
            || self.theme != prev.theme
            || self.api_style != prev.api_style
            || self.busy_since.is_some() != prev_busy
    }
}

/// Streaming byte filter that intercepts `ESC ] 7700 ; ... BEL/ST` sequences
/// across arbitrary PTY read boundaries without disturbing other bytes.
#[derive(Default)]
struct OscFilter {
    buf: Vec<u8>,
    in_osc: bool,
    saw_esc: bool,
}

impl OscFilter {
    fn process(&mut self, input: &[u8]) -> (Vec<u8>, Vec<String>) {
        let mut out = Vec::with_capacity(input.len());
        let mut events = Vec::new();

        for &b in input {
            if self.in_osc {
                if self.saw_esc {
                    self.saw_esc = false;
                    if b == b'\\' {
                        // ST terminator (\x1b\\)
                        self.finish_osc(&mut out, &mut events);
                        continue;
                    }
                    // False alarm inside OSC
                    self.buf.push(0x1b);
                }
                if b == 0x07 {
                    self.finish_osc(&mut out, &mut events);
                } else if b == 0x1b {
                    self.saw_esc = true;
                } else if self.buf.len() < 4096 {
                    self.buf.push(b);
                } else {
                    // Too long, abort and flush as normal bytes
                    out.extend_from_slice(&self.buf);
                    out.push(b);
                    self.buf.clear();
                    self.in_osc = false;
                }
                continue;
            }

            if self.saw_esc {
                self.saw_esc = false;
                if b == b']' {
                    self.in_osc = true;
                    self.buf.clear();
                    self.buf.extend_from_slice(b"\x1b]");
                    continue;
                }
                if b == 0x1b {
                    out.push(0x1b);
                    self.saw_esc = true;
                    continue;
                }
                out.push(0x1b);
                out.push(b);
                continue;
            }

            if b == 0x1b {
                self.saw_esc = true;
            } else {
                out.push(b);
            }
        }

        (out, events)
    }

    fn finish_osc(&mut self, out: &mut Vec<u8>, events: &mut Vec<String>) {
        self.in_osc = false;
        if let Some(payload) = self.buf.strip_prefix(b"\x1b]7700;") {
            if let Ok(s) = std::str::from_utf8(payload) {
                events.push(s.to_string());
            }
        } else {
            out.extend_from_slice(&self.buf);
            out.push(0x07);
        }
        self.buf.clear();
    }
}

fn layout(columns: u16) -> u16 {
    if columns >= SIDEBAR_MIN_COLUMNS {
        columns.saturating_sub(SIDEBAR_WIDTH).max(1)
    } else {
        columns.max(1)
    }
}

fn char_display_width(c: char) -> usize {
    if c.is_ascii()
        || matches!(
            c,
            '─' | '│'
                | '·'
                | '…'
                | '◆'
                | '●'
                | '⚠'
                | '↑'
                | '⠋'
                | '⠙'
                | '⠹'
                | '⠸'
                | '⠼'
                | '⠴'
                | '⠦'
                | '⠧'
                | '⠇'
                | '⠏'
        )
    {
        1
    } else {
        2
    }
}

fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        let w = char_display_width(c);
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
    std::env::args_os()
        .skip(1)
        .filter(|arg| arg != "--classic")
        .collect()
}

fn key_bytes(key: event::KeyEvent) -> Option<String> {
    if key.modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = key.code {
            return Some(format!("\x1b{c}"));
        }
    }
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
        KeyCode::Insert => "\x1b[2~".into(),
        KeyCode::Delete => "\x1b[3~".into(),
        KeyCode::F(1) => "\x1bOP".into(),
        KeyCode::F(2) => "\x1bOQ".into(),
        KeyCode::F(3) => "\x1bOR".into(),
        KeyCode::F(4) => "\x1bOS".into(),
        KeyCode::F(5) => "\x1b[15~".into(),
        KeyCode::F(6) => "\x1b[17~".into(),
        KeyCode::F(7) => "\x1b[18~".into(),
        KeyCode::F(8) => "\x1b[19~".into(),
        KeyCode::F(9) => "\x1b[20~".into(),
        _ => return None,
    })
}

fn panel_bg(theme: Theme) -> Option<Color> {
    if !crate::ui::color_enabled() {
        return None;
    }
    match theme {
        Theme::None => None,
        Theme::Dark => Some(Color::AnsiValue(235)),
        Theme::Light => Some(Color::AnsiValue(254)),
    }
}

fn effort_badge_colors(effort: EffortLevel) -> (Color, Color) {
    match effort {
        EffortLevel::Low => (Color::AnsiValue(240), Color::AnsiValue(255)),
        EffortLevel::Medium => (Color::AnsiValue(28), Color::AnsiValue(255)),
        EffortLevel::High => (Color::AnsiValue(172), Color::AnsiValue(16)),
        EffortLevel::Max => (Color::AnsiValue(196), Color::AnsiValue(255)),
    }
}

fn effort_fg_color(effort: EffortLevel) -> Color {
    match effort {
        EffortLevel::Low => Color::AnsiValue(250),
        EffortLevel::Medium => Color::AnsiValue(77),
        EffortLevel::High => Color::AnsiValue(214),
        EffortLevel::Max => Color::AnsiValue(197),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarAction {
    CycleEffort,
    ToggleShell,
    ToggleThinking,
    ClearScrollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BarHitbox {
    start_col: u16,
    end_col: u16,
    action: BarAction,
}

fn draw_sidebar(
    out: &mut impl Write,
    width: u16,
    pty_rows: u16,
    columns: u16,
    entries: &[crate::archive::SessionEntry],
    status: &HostStatus,
) -> Result<()> {
    queue!(out, ResetColor)?;
    if columns < SIDEBAR_MIN_COLUMNS {
        return Ok(());
    }
    let styled = crate::ui::color_enabled();
    let bg = panel_bg(status.theme);
    let text_width = SIDEBAR_WIDTH.saturating_sub(1) as usize;
    let effort_label = format!(
        "● {}{}",
        status.effort.as_str().to_ascii_uppercase(),
        match (status.shell_enabled, status.show_thinking) {
            (true, true) => " · SHELL · THINK",
            (true, false) => " · SHELL",
            (false, true) => " · THINK",
            (false, false) => "",
        }
    );
    let mut lines: Vec<(Color, String)> = vec![
        (SIDEBAR_TITLE, "original".into()),
        (SIDEBAR_FG, status.model.clone()),
        (effort_fg_color(status.effort), effort_label),
    ];
    lines.push((SIDEBAR_RULE, "─".repeat(text_width)));
    lines.push((SIDEBAR_TITLE, "Sessions (Click)".into()));
    let max_sessions = pty_rows.saturating_sub(9) as usize;
    if entries.is_empty() {
        lines.push((SIDEBAR_RULE, "No sessions".into()));
    } else {
        lines.extend(
            entries
                .iter()
                .take(max_sessions)
                .map(|entry| (SIDEBAR_FG, entry.topic.clone())),
        );
    }
    let hints = [
        "Click: open session",
        "F2: settings · F10: exit",
        "Wheel/PgUp: scroll",
        "Shift+Drag: select text",
    ];
    for y in 0..pty_rows {
        let (color, text) = if y + 4 >= pty_rows {
            (SIDEBAR_RULE, hints[(y + 4 - pty_rows) as usize].to_string())
        } else {
            lines
                .get(y as usize)
                .cloned()
                .unwrap_or((SIDEBAR_FG, String::new()))
        };
        queue!(out, cursor::MoveTo(width, y), ResetColor)?;
        if let Some(bg_col) = bg {
            queue!(out, SetBackgroundColor(bg_col))?;
        }
        if styled {
            queue!(
                out,
                SetForegroundColor(SIDEBAR_RULE),
                Print("│"),
                SetForegroundColor(color),
                Print(fit(&text, text_width)),
                ResetColor
            )?;
        } else {
            queue!(out, Print("│"), Print(fit(&text, text_width)))?;
        }
    }
    Ok(())
}

struct BadgeStyle {
    styled: bool,
    bar_bg: Color,
    bg: Color,
    fg: Color,
    bold: bool,
}

fn emit_status_badge<W: Write>(
    out: &mut W,
    used: &mut usize,
    max_cols: usize,
    text: &str,
    style: BadgeStyle,
) -> Result<Option<(u16, u16)>> {
    let w: usize = text.chars().map(char_display_width).sum();
    if *used + w > max_cols {
        return Ok(None);
    }
    let start = *used as u16;
    if style.styled {
        queue!(
            out,
            SetBackgroundColor(style.bg),
            SetForegroundColor(style.fg)
        )?;
        if style.bold {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }
        queue!(
            out,
            Print(text),
            SetAttribute(Attribute::Reset),
            SetBackgroundColor(style.bar_bg)
        )?;
    } else {
        queue!(out, Print(text))?;
    }
    *used += w;
    Ok(Some((start, *used as u16)))
}

fn draw_status_bar(
    out: &mut impl Write,
    rows: u16,
    columns: u16,
    status: &HostStatus,
    scrollback: usize,
) -> Result<Vec<BarHitbox>> {
    let mut hitboxes = Vec::new();
    if rows == 0 || columns == 0 {
        return Ok(hitboxes);
    }
    let y = rows - 1;
    let styled = crate::ui::color_enabled();
    let bar_bg = panel_bg(status.theme).unwrap_or(Color::AnsiValue(236));
    let dim_bg = match status.theme {
        Theme::Light => Color::AnsiValue(252),
        _ => Color::AnsiValue(238),
    };
    let dim_fg = match status.theme {
        Theme::Light => Color::AnsiValue(242),
        _ => Color::AnsiValue(245),
    };

    queue!(out, cursor::MoveTo(0, y), ResetColor)?;
    if styled {
        queue!(out, SetBackgroundColor(bar_bg))?;
    }

    let mut used = 0usize;
    let max_cols = columns as usize;

    let (eff_bg, eff_fg) = effort_badge_colors(status.effort);
    let eff_text = if styled {
        format!(" ● {} ", status.effort.as_str().to_ascii_uppercase())
    } else {
        format!("[{}]", status.effort.as_str().to_ascii_uppercase())
    };
    if let Some((start_col, end_col)) = emit_status_badge(
        out,
        &mut used,
        max_cols,
        &eff_text,
        BadgeStyle {
            styled,
            bar_bg,
            bg: eff_bg,
            fg: eff_fg,
            bold: true,
        },
    )? {
        hitboxes.push(BarHitbox {
            start_col,
            end_col,
            action: BarAction::CycleEffort,
        });
    }

    let (shell_text, shell_bg, shell_fg, shell_bold) = if status.shell_enabled {
        (
            " ! SHELL ",
            Color::AnsiValue(160),
            Color::AnsiValue(255),
            true,
        )
    } else {
        (" SHELL ", dim_bg, dim_fg, false)
    };
    if let Some((start_col, end_col)) = emit_status_badge(
        out,
        &mut used,
        max_cols,
        shell_text,
        BadgeStyle {
            styled,
            bar_bg,
            bg: shell_bg,
            fg: shell_fg,
            bold: shell_bold,
        },
    )? {
        hitboxes.push(BarHitbox {
            start_col,
            end_col,
            action: BarAction::ToggleShell,
        });
    }

    let (think_bg, think_fg) = if status.show_thinking {
        (Color::AnsiValue(31), Color::AnsiValue(255))
    } else {
        (dim_bg, dim_fg)
    };
    if let Some((start_col, end_col)) = emit_status_badge(
        out,
        &mut used,
        max_cols,
        " THINK ",
        BadgeStyle {
            styled,
            bar_bg,
            bg: think_bg,
            fg: think_fg,
            bold: status.show_thinking,
        },
    )? {
        hitboxes.push(BarHitbox {
            start_col,
            end_col,
            action: BarAction::ToggleThinking,
        });
    }

    if let Some(since) = status.busy_since {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let ms = since.elapsed().as_millis();
        let frame = SPINNER[((ms / 120) as usize) % SPINNER.len()];
        let secs = since.elapsed().as_secs();
        let _ = emit_status_badge(
            out,
            &mut used,
            max_cols,
            &format!(" {frame} 生成中 {secs}s "),
            BadgeStyle {
                styled,
                bar_bg,
                bg: Color::AnsiValue(62),
                fg: Color::AnsiValue(255),
                bold: true,
            },
        )?;
    }

    if scrollback > 0 {
        if let Some((start_col, end_col)) = emit_status_badge(
            out,
            &mut used,
            max_cols,
            &format!(" ↑ 回看({scrollback}) "),
            BadgeStyle {
                styled,
                bar_bg,
                bg: Color::AnsiValue(99),
                fg: Color::AnsiValue(255),
                bold: true,
            },
        )? {
            hitboxes.push(BarHitbox {
                start_col,
                end_col,
                action: BarAction::ClearScrollback,
            });
        }
    }

    let mut meta_parts = vec![status.model.clone()];
    if !status.ctx.is_empty() {
        meta_parts.push(format!("ctx {}", status.ctx));
    }
    if !status.cost.is_empty() {
        meta_parts.push(status.cost.clone());
    }
    let meta = format!("  {}", meta_parts.join(" · "));
    let hints = if status.busy_since.is_some() {
        "Ctrl+C 中断本轮 · F10 退出 "
    } else {
        "Shift+Tab 强度 · F2 设置 · F3 思考 · F4 Shell "
    };

    let remaining = max_cols.saturating_sub(used);
    let hints_w: usize = hints.chars().map(char_display_width).sum();
    let meta_w: usize = meta.chars().map(char_display_width).sum();

    if remaining > 0 {
        let meta_fg = match status.theme {
            Theme::Light => Color::AnsiValue(236),
            _ => Color::AnsiValue(250),
        };
        let hint_fg = match status.theme {
            Theme::Light => Color::AnsiValue(242),
            _ => Color::AnsiValue(243),
        };
        if styled {
            queue!(out, SetForegroundColor(meta_fg))?;
        }
        if remaining >= meta_w + hints_w + 2 {
            queue!(out, Print(&meta))?;
            let pad = remaining - meta_w - hints_w;
            if pad > 0 {
                queue!(out, Print(" ".repeat(pad)))?;
            }
            if styled {
                queue!(out, SetForegroundColor(hint_fg))?;
            }
            queue!(out, Print(hints))?;
        } else {
            queue!(out, Print(fit(&meta, remaining)))?;
        }
    }
    queue!(out, ResetColor)?;
    Ok(hitboxes)
}

fn sync_startup_subtitle(
    out: &mut impl Write,
    screen: &vt100::Screen,
    width: u16,
    pty_rows: u16,
    status: &HostStatus,
) -> Result<()> {
    if screen.scrollback() > 0 || width <= 10 || pty_rows < 2 {
        return Ok(());
    }
    let contents = screen.contents();
    let lines: Vec<&str> = contents.lines().collect();
    let title_row = lines
        .iter()
        .take(pty_rows.saturating_sub(1) as usize)
        .position(|line| line.contains('◆') && line.contains("original"));
    let Some(r) = title_row else {
        return Ok(());
    };
    let styled = crate::ui::color_enabled();

    // 1. Update row r + 1 right next to the top-left gem icon.
    let subtitle_row = (r + 1) as u16;
    if subtitle_row < pty_rows {
        let avail = width.saturating_sub(9) as usize;
        queue!(out, cursor::MoveTo(9, subtitle_row), ResetColor)?;
        if status.logged_in {
            let prefix = format!("{} · {} · ", status.api_style, status.model);
            let eff = status.effort.as_str();
            let prefix_w: usize = prefix.chars().map(char_display_width).sum();
            let eff_w: usize = eff.chars().map(char_display_width).sum();
            if prefix_w + eff_w <= avail {
                let pad = avail - prefix_w - eff_w;
                if styled {
                    queue!(
                        out,
                        SetForegroundColor(SIDEBAR_FG),
                        Print(&prefix),
                        SetForegroundColor(effort_fg_color(status.effort)),
                        SetAttribute(Attribute::Bold),
                        Print(eff),
                        SetAttribute(Attribute::Reset),
                        Print(" ".repeat(pad))
                    )?;
                } else {
                    queue!(out, Print(format!("{prefix}{eff}{}", " ".repeat(pad))))?;
                }
            } else {
                let line = format!("{prefix}{eff}");
                if styled {
                    queue!(out, SetForegroundColor(SIDEBAR_FG))?;
                }
                queue!(out, Print(fit(&line, avail)), ResetColor)?;
            }
        } else {
            if styled {
                queue!(out, SetForegroundColor(SIDEBAR_FG))?;
            }
            queue!(out, Print(fit("终端 coding agent", avail)), ResetColor)?;
        }
    }

    // 2. Also update the startup status line right below the gem icon (row r + 6)
    // if it is still visible on screen so model and effort stay in sync there too.
    let status_idx = r + 6;
    if status_idx < pty_rows as usize {
        if let Some(status_row_text) = lines.get(status_idx) {
            if let Some(in_pos) = status_row_text.find(" · in ") {
                let suffix = &status_row_text[in_pos..];
                let model = if status.logged_in && !status.model.is_empty() {
                    status.model.as_str()
                } else {
                    "未登录"
                };
                let eff = status.effort.as_str();
                let prefix = format!("{model} · ");
                let avail = width as usize;
                let prefix_w: usize = prefix.chars().map(char_display_width).sum();
                let eff_w: usize = eff.chars().map(char_display_width).sum();
                let suffix_w: usize = suffix.chars().map(char_display_width).sum();
                queue!(out, cursor::MoveTo(0, status_idx as u16), ResetColor)?;
                if prefix_w + eff_w + suffix_w <= avail {
                    let pad = avail - prefix_w - eff_w - suffix_w;
                    if styled {
                        queue!(
                            out,
                            SetForegroundColor(SIDEBAR_FG),
                            Print(&prefix),
                            SetForegroundColor(effort_fg_color(status.effort)),
                            SetAttribute(Attribute::Bold),
                            Print(eff),
                            SetAttribute(Attribute::Reset),
                            SetForegroundColor(SIDEBAR_FG),
                            Print(suffix),
                            Print(" ".repeat(pad)),
                            ResetColor
                        )?;
                    } else {
                        queue!(
                            out,
                            Print(format!("{prefix}{eff}{suffix}{}", " ".repeat(pad)))
                        )?;
                    }
                } else {
                    let full = format!("{prefix}{eff}{suffix}");
                    if styled {
                        queue!(out, SetForegroundColor(SIDEBAR_FG))?;
                    }
                    queue!(out, Print(fit(&full, avail)), ResetColor)?;
                }
            }
        }
    }
    Ok(())
}

pub fn run(root: &Path) -> Result<()> {
    let (columns, rows) = terminal::size()?;
    let mut width = layout(columns);
    let mut pty_rows = rows.saturating_sub(1).max(1);
    let pair = native_pty_system().openpty(PtySize {
        rows: pty_rows,
        cols: width,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = CommandBuilder::new(std::env::current_exe()?);
    command.env("ORIGINAL_AGENT_HOST", "1");
    command.cwd(root);
    command.arg("--classic");
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

    // Background thread for session archive listing so drawing never blocks on disk I/O.
    let (listing_tx, listing_rx) = mpsc::channel::<Vec<crate::archive::SessionEntry>>();
    let root_bg = root.to_path_buf();
    std::thread::spawn(move || {
        let mut last: Option<Vec<crate::archive::SessionEntry>> = None;
        loop {
            let next = crate::archive::list_entries(&root_bg).unwrap_or_default();
            if last.as_ref() != Some(&next) {
                last = Some(next.clone());
                if listing_tx.send(next).is_err() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });

    let mut parser = vt100::Parser::new(pty_rows, width, 5000);
    terminal::enable_raw_mode()?;
    let _restore = Restore {
        previous_title: current_title(),
    };
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        Clear(ClearType::All),
        cursor::Show,
        SetTitle("original")
    )?;
    let result = (|| -> Result<()> {
        let mut dirty = true;
        let mut full_redraw = true;
        let mut chrome_dirty = true;
        let mut entries: Vec<crate::archive::SessionEntry> = Vec::new();
        let mut bar_hitboxes: Vec<BarHitbox> = Vec::new();
        let mut host_status = HostStatus::from_config();
        let mut osc_filter = OscFilter::default();
        let mut prev_screen: Option<vt100::Screen> = None;
        let mut last_cols = columns;
        let mut last_rows = rows;
        let mut last_draw = std::time::Instant::now() - Duration::from_millis(50);
        let mut last_spinner_tick = std::time::Instant::now();

        loop {
            while let Ok(data) = receive.try_recv() {
                let (clean, osc_events) = osc_filter.process(&data);
                if !clean.is_empty() {
                    parser.process(&clean);
                    dirty = true;
                }
                for ev in osc_events {
                    if host_status.apply_osc(&ev) {
                        chrome_dirty = true;
                        dirty = true;
                    }
                }
            }
            while let Ok(next_entries) = listing_rx.try_recv() {
                if next_entries != entries {
                    entries = next_entries;
                    chrome_dirty = true;
                    dirty = true;
                }
            }

            if host_status.busy_since.is_some()
                && last_spinner_tick.elapsed() >= Duration::from_millis(120)
            {
                chrome_dirty = true;
                dirty = true;
                last_spinner_tick = std::time::Instant::now();
            }

            if dirty && (full_redraw || last_draw.elapsed() >= Duration::from_millis(16)) {
                let (columns, rows) = terminal::size()?;
                if columns != last_cols || rows != last_rows {
                    last_cols = columns;
                    last_rows = rows;
                    width = layout(columns);
                    pty_rows = rows.saturating_sub(1).max(1);
                    pair.master.resize(PtySize {
                        rows: pty_rows,
                        cols: width,
                        pixel_width: 0,
                        pixel_height: 0,
                    })?;
                    parser.set_size(pty_rows, width);
                    prev_screen = None;
                    full_redraw = true;
                    chrome_dirty = true;
                }
                let mut out = std::io::stdout().lock();
                // Synchronized output avoids tearing/flicker on modern terminals.
                out.write_all(b"\x1b[?2026h")?;
                let mut pty_changed = false;
                if full_redraw || prev_screen.is_none() {
                    out.write_all(&parser.screen().contents_formatted())?;
                    chrome_dirty = true;
                    pty_changed = true;
                } else if let Some(prev) = &prev_screen {
                    let diff = parser.screen().contents_diff(prev);
                    if !diff.is_empty() {
                        out.write_all(&diff)?;
                        pty_changed = true;
                    }
                }
                queue!(out, ResetColor, SetTitle("original"))?;
                let need_chrome = chrome_dirty || pty_changed || full_redraw;
                if need_chrome {
                    // Always repaint the sidebar after any PTY diff because vt100 emits
                    // ESC[K (EL) to clear trailing spaces, which reaches past `width`.
                    draw_sidebar(&mut out, width, pty_rows, columns, &entries, &host_status)?;
                }
                if chrome_dirty || full_redraw {
                    bar_hitboxes = draw_status_bar(
                        &mut out,
                        rows,
                        columns,
                        &host_status,
                        parser.screen().scrollback(),
                    )?;
                    chrome_dirty = false;
                }
                if need_chrome {
                    sync_startup_subtitle(
                        &mut out,
                        parser.screen(),
                        width,
                        pty_rows,
                        &host_status,
                    )?;
                }
                if parser.screen().scrollback() > 0 {
                    queue!(out, cursor::Hide)?;
                } else {
                    let (row, col) = parser.screen().cursor_position();
                    queue!(
                        out,
                        cursor::MoveTo(col, row.min(pty_rows.saturating_sub(1))),
                        cursor::Show
                    )?;
                }
                out.write_all(b"\x1b[?2026l")?;
                out.flush()?;
                prev_screen = Some(parser.screen().clone());
                full_redraw = false;
                dirty = false;
                last_draw = std::time::Instant::now();
            }
            if child.try_wait()?.is_some() {
                break;
            }
            let wait = if dirty {
                Duration::from_millis(8)
            } else if host_status.busy_since.is_some() {
                Duration::from_millis(40)
            } else {
                Duration::from_millis(30)
            };
            if !event::poll(wait)? {
                continue;
            }
            match event::read()? {
                Event::Resize(columns, rows) => {
                    last_cols = columns;
                    last_rows = rows;
                    width = layout(columns);
                    pty_rows = rows.saturating_sub(1).max(1);
                    pair.master.resize(PtySize {
                        rows: pty_rows,
                        cols: width,
                        pixel_width: 0,
                        pixel_height: 0,
                    })?;
                    parser.set_size(pty_rows, width);
                    prev_screen = None;
                    full_redraw = true;
                    chrome_dirty = true;
                    dirty = true;
                }
                Event::Paste(text) => {
                    if parser.screen().scrollback() > 0 {
                        parser.set_scrollback(0);
                        chrome_dirty = true;
                    }
                    let framed = format!("\x1b[200~{text}\x1b[201~");
                    writer.write_all(framed.as_bytes())?;
                    writer.flush()?;
                    dirty = true;
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        let before = parser.screen().scrollback();
                        parser.set_scrollback(before + 3);
                        if parser.screen().scrollback() != before {
                            chrome_dirty = true;
                            dirty = true;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        let before = parser.screen().scrollback();
                        if before > 0 {
                            parser.set_scrollback(before.saturating_sub(3));
                            if parser.screen().scrollback() != before {
                                chrome_dirty = true;
                                dirty = true;
                            }
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        // Clickable bottom status bar badges.
                        if mouse.row == last_rows.saturating_sub(1) {
                            if let Some(hit) = bar_hitboxes
                                .iter()
                                .find(|h| mouse.column >= h.start_col && mouse.column < h.end_col)
                            {
                                match hit.action {
                                    BarAction::CycleEffort => {
                                        if host_status.busy_since.is_none() {
                                            writer.write_all(b"\x1b[Z")?;
                                            writer.flush()?;
                                            dirty = true;
                                        }
                                    }
                                    BarAction::ToggleThinking => {
                                        if host_status.busy_since.is_none() {
                                            writer.write_all(b"\x1bOR")?;
                                            writer.flush()?;
                                            dirty = true;
                                        }
                                    }
                                    BarAction::ToggleShell => {
                                        if host_status.busy_since.is_none() {
                                            writer.write_all(b"\x1bOS")?;
                                            writer.flush()?;
                                            dirty = true;
                                        }
                                    }
                                    BarAction::ClearScrollback => {
                                        if parser.screen().scrollback() > 0 {
                                            parser.set_scrollback(0);
                                            chrome_dirty = true;
                                            dirty = true;
                                        }
                                    }
                                }
                            }
                        } else if last_cols >= SIDEBAR_MIN_COLUMNS
                            && mouse.column >= width
                            && mouse.row >= 5
                            && mouse.row < pty_rows.saturating_sub(4)
                            && host_status.busy_since.is_none()
                        {
                            // Clickable session entry in right sidebar.
                            let idx = (mouse.row - 5) as usize;
                            if let Some(entry) = entries.get(idx) {
                                if parser.screen().scrollback() > 0 {
                                    parser.set_scrollback(0);
                                    chrome_dirty = true;
                                }
                                let cmd = format!("\x15/history {}\r", entry.id);
                                writer.write_all(cmd.as_bytes())?;
                                writer.flush()?;
                                dirty = true;
                            }
                        }
                    }
                    _ => {}
                },
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if key.code == KeyCode::F(10) {
                        break;
                    }
                    if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
                        let before = parser.screen().scrollback();
                        parser.set_scrollback(if key.code == KeyCode::PageUp {
                            before + SCROLL_STEP
                        } else {
                            before.saturating_sub(SCROLL_STEP)
                        });
                        if parser.screen().scrollback() != before {
                            chrome_dirty = true;
                            dirty = true;
                        }
                        continue;
                    }
                    if parser.screen().scrollback() > 0 {
                        parser.set_scrollback(0);
                        chrome_dirty = true;
                    }
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
        assert_eq!(fit("───", 3), "───");
    }

    #[test]
    fn sidebar_keeps_effort_on_its_own_line() {
        let status: Vec<String> = vec!["claude-sonnet-4-5-20250929".into(), "high".into()];
        assert_eq!(fit(&status[0], 27).trim(), "claude-sonnet-4-5-20250929");
        assert_eq!(fit(&status[1], 27).trim(), "high");
        let squeezed = fit("claude-sonnet-4-5-20250929 · high", 27);
        assert!(!squeezed.contains("high"), "{squeezed}");
    }

    #[test]
    fn osc_filter_extracts_split_status_frames() {
        let mut f = OscFilter::default();
        let (out1, ev1) = f.process(b"hello\x1b]7700;model=gpt-4.1;");
        let (out2, ev2) = f.process(b"effort=high;shell=1\x07world");
        assert_eq!(out1, b"hello");
        assert!(ev1.is_empty());
        assert_eq!(out2, b"world");
        assert_eq!(ev2, vec!["model=gpt-4.1;effort=high;shell=1"]);

        let mut status = HostStatus::from_config();
        assert!(status.apply_osc(&ev2[0]));
        assert_eq!(status.model, "gpt-4.1");
        assert_eq!(status.effort, EffortLevel::High);
        assert!(status.shell_enabled);
        assert!(status.apply_osc("busy=1"));
        assert!(status.busy_since.is_some());
        assert!(status.apply_osc("busy=0"));
        assert!(status.busy_since.is_none());
    }

    #[test]
    fn osc_filter_handles_double_esc_before_osc() {
        let mut f = OscFilter::default();
        let (out, ev) = f.process(b"\x1b\x1b]7700;effort=max\x07ok");
        assert_eq!(out, b"\x1bok");
        assert_eq!(ev, vec!["effort=max"]);
    }

    #[test]
    fn sync_startup_subtitle_updates_both_header_and_status_line() {
        let mut parser = vt100::Parser::new(12, 80, 0);
        parser.process(
            concat!(
                "  ██     ◆  original  0.3.13\r\n",
                " ▓██▓    openai · old-model · low\r\n",
                "░▓██▓░\r\n",
                " ▓██▓\r\n",
                "  ██\r\n",
                "\r\n",
                "old-model · low · in — · out — · cache — · ctx — · —\r\n"
            )
            .as_bytes(),
        );
        let mut status = HostStatus::from_config();
        status.logged_in = true;
        status.api_style = "anthropic".into();
        status.model = "claude-sonnet-4".into();
        status.effort = EffortLevel::High;

        let mut buf = Vec::new();
        sync_startup_subtitle(&mut buf, parser.screen(), 80, 12, &status).unwrap();
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("anthropic · claude-sonnet-4 · "));
        assert!(text.contains("claude-sonnet-4 · "));
        assert!(text.contains("high"));
        assert!(text.contains(" · in — · out —"));
    }
}
