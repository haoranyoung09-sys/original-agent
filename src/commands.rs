//! Slash-command parsing for the Pi-style REPL (pure, testable).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashAction {
    Exit,
    Clear,
    Help,
    Status,
    Settings,
    Usage,
    Login,
    Thinking {
        state: Option<bool>,
    },
    Shell {
        state: Option<bool>,
    },
    Theme {
        name: Option<String>,
    },
    /// `None` means show current model; `Some(name)` switches.
    Model {
        name: Option<String>,
    },
    Unknown(String),
    /// Not a slash command — treat as chat/user text.
    NotSlash,
}

/// Known slash commands for completion and hinting.
pub const SLASH_COMMANDS: &[&str] = &[
    "/login",
    "/model",
    "/settings",
    "/thinking",
    "/shell",
    "/theme",
    "/status",
    "/usage",
    "/history",
    "/sessions",
    "/export",
    "/diff",
    "/undo",
    "/clear",
    "/help",
    "/exit",
    "/quit",
];

fn parse_toggle_arg(rest: &str) -> Option<bool> {
    match rest.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "enable" => Some(true),
        "off" | "0" | "false" | "disable" => Some(false),
        _ => None,
    }
}

/// Parse a trimmed REPL line into a slash action.
pub fn parse_slash(line: &str) -> SlashAction {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('/') {
        return SlashAction::NotSlash;
    }
    let (cmd, rest) = match line.split_once(char::is_whitespace) {
        Some((c, r)) => (c, r.trim()),
        None => (line, ""),
    };
    match cmd {
        "/exit" | "/quit" => SlashAction::Exit,
        "/clear" => SlashAction::Clear,
        "/help" | "/?" => SlashAction::Help,
        "/settings" | "/setting" | "/config" => SlashAction::Settings,
        "/usage" => SlashAction::Usage,
        "/status" => SlashAction::Status,
        "/login" => SlashAction::Login,
        "/thinking" => SlashAction::Thinking {
            state: parse_toggle_arg(rest),
        },
        "/shell" => SlashAction::Shell {
            state: parse_toggle_arg(rest),
        },
        "/theme" => SlashAction::Theme {
            name: if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            },
        },
        "/model" => {
            if rest.is_empty() {
                SlashAction::Model { name: None }
            } else {
                SlashAction::Model {
                    name: Some(rest.to_string()),
                }
            }
        }
        other => SlashAction::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_commands() {
        for cmd in ["/setting", "/settings", "/config"] {
            assert_eq!(parse_slash(cmd), SlashAction::Settings);
        }
        assert_eq!(parse_slash("/usage"), SlashAction::Usage);
        assert_eq!(parse_slash("/exit"), SlashAction::Exit);
        assert_eq!(parse_slash("/quit"), SlashAction::Exit);
        assert_eq!(parse_slash("  /clear  "), SlashAction::Clear);
        assert_eq!(parse_slash("/help"), SlashAction::Help);
        assert_eq!(parse_slash("/?"), SlashAction::Help);
        assert_eq!(parse_slash("/status"), SlashAction::Status);
        assert_eq!(parse_slash("/login"), SlashAction::Login);
        assert_eq!(parse_slash("/login extra"), SlashAction::Login);
        assert_eq!(
            parse_slash("/thinking"),
            SlashAction::Thinking { state: None }
        );
        assert_eq!(
            parse_slash("/thinking on"),
            SlashAction::Thinking { state: Some(true) }
        );
        assert_eq!(
            parse_slash("/shell off"),
            SlashAction::Shell { state: Some(false) }
        );
        assert_eq!(
            parse_slash("/theme dark"),
            SlashAction::Theme {
                name: Some("dark".into())
            }
        );
        assert_eq!(parse_slash("/model"), SlashAction::Model { name: None });
        assert_eq!(
            parse_slash("/model gpt-4.1"),
            SlashAction::Model {
                name: Some("gpt-4.1".into())
            }
        );
        assert_eq!(
            parse_slash("/model   claude-sonnet-4-20250514"),
            SlashAction::Model {
                name: Some("claude-sonnet-4-20250514".into())
            }
        );
    }

    #[test]
    fn non_slash_and_unknown() {
        assert_eq!(parse_slash("hello"), SlashAction::NotSlash);
        assert_eq!(parse_slash(""), SlashAction::NotSlash);
        assert_eq!(parse_slash("/nope"), SlashAction::Unknown("/nope".into()));
    }
}
