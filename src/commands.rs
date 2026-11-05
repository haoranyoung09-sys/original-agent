//! Slash-command parsing for the Pi-style REPL (pure, testable).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashAction {
    Exit,
    Clear,
    Help,
    Status,
    Login,
    /// `None` means show current model; `Some(name)` switches.
    Model {
        name: Option<String>,
    },
    Unknown(String),
    /// Not a slash command — treat as chat/user text.
    NotSlash,
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
        "/status" => SlashAction::Status,
        "/login" => SlashAction::Login,
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
        assert_eq!(parse_slash("/exit"), SlashAction::Exit);
        assert_eq!(parse_slash("/quit"), SlashAction::Exit);
        assert_eq!(parse_slash("  /clear  "), SlashAction::Clear);
        assert_eq!(parse_slash("/help"), SlashAction::Help);
        assert_eq!(parse_slash("/?"), SlashAction::Help);
        assert_eq!(parse_slash("/status"), SlashAction::Status);
        assert_eq!(parse_slash("/login"), SlashAction::Login);
        assert_eq!(parse_slash("/login extra"), SlashAction::Login);
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
