//! SetCommand parser for vim-style `:set` commands.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Empty,
    InvalidSyntax(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "Empty command"),
            ParseError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
pub enum SetCommand {
    ShowModified,
    ShowAll,
    Query { key: String },
    QueryHistory { key: String },
    Enable { key: String },
    Disable { key: String },
    Toggle { key: String },
    Reset { key: String },
    Pop { key: String },
    Set { key: String, value: String },
}

impl SetCommand {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let input = input
            .strip_prefix(":set")
            .or_else(|| input.strip_prefix("set"))
            .unwrap_or(input)
            .trim();

        if input.is_empty() {
            return Ok(SetCommand::ShowModified);
        }

        if input == "all" {
            return Ok(SetCommand::ShowAll);
        }

        if let Some(key) = input.strip_suffix("??") {
            let key = key.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::QueryHistory {
                key: key.to_string(),
            });
        }

        if let Some(key) = input.strip_suffix('?') {
            let key = key.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Query {
                key: key.to_string(),
            });
        }

        if let Some(key) = input.strip_suffix('&') {
            let key = key.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Reset {
                key: key.to_string(),
            });
        }

        if let Some(key) = input.strip_suffix('^') {
            let key = key.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Pop {
                key: key.to_string(),
            });
        }

        if let Some(key) = input.strip_suffix('!') {
            let key = key.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Toggle {
                key: key.to_string(),
            });
        }

        if let Some(key) = input.strip_prefix("inv") {
            if !key.is_empty() && !key.contains(['=', ':', ' ']) {
                return Ok(SetCommand::Toggle {
                    key: key.to_string(),
                });
            }
        }

        if let Some(key) = input.strip_prefix("no") {
            if !key.is_empty() && !key.contains(['=', ':', ' ']) {
                return Ok(SetCommand::Disable {
                    key: key.to_string(),
                });
            }
        }

        if let Some((key, value)) = input.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Set {
                key: key.to_string(),
                value: value.to_string(),
            });
        }

        if let Some((key, value)) = split_on_value_colon(input) {
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() {
                return Err(ParseError::InvalidSyntax("missing option name".into()));
            }
            return Ok(SetCommand::Set {
                key: key.to_string(),
                value: value.to_string(),
            });
        }

        if let Some((key, value)) = input.split_once(' ') {
            let key = key.trim();
            let value = value.trim();
            if !value.is_empty() {
                return Ok(SetCommand::Set {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        }

        Ok(SetCommand::Enable {
            key: input.to_string(),
        })
    }
}

fn split_on_value_colon(input: &str) -> Option<(&str, &str)> {
    if let Some(pos) = input.find(':') {
        let key = &input[..pos];
        let value = &input[pos + 1..];
        if !value.is_empty() {
            return Some((key, value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_shows_modified() {
        assert_eq!(SetCommand::parse(""), Ok(SetCommand::ShowModified));
        assert_eq!(SetCommand::parse(":set"), Ok(SetCommand::ShowModified));
        assert_eq!(SetCommand::parse(":set "), Ok(SetCommand::ShowModified));
    }

    #[test]
    fn parse_all_shows_all() {
        assert_eq!(SetCommand::parse("all"), Ok(SetCommand::ShowAll));
        assert_eq!(SetCommand::parse(":set all"), Ok(SetCommand::ShowAll));
    }

    #[test]
    fn parse_query() {
        assert_eq!(
            SetCommand::parse("model?"),
            Ok(SetCommand::Query {
                key: "model".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set verbose?"),
            Ok(SetCommand::Query {
                key: "verbose".into()
            })
        );
    }

    #[test]
    fn parse_query_history() {
        assert_eq!(
            SetCommand::parse("model??"),
            Ok(SetCommand::QueryHistory {
                key: "model".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set theme??"),
            Ok(SetCommand::QueryHistory {
                key: "theme".into()
            })
        );
    }

    #[test]
    fn parse_reset() {
        assert_eq!(
            SetCommand::parse("model&"),
            Ok(SetCommand::Reset {
                key: "model".into()
            })
        );
    }

    #[test]
    fn parse_pop() {
        assert_eq!(
            SetCommand::parse("model^"),
            Ok(SetCommand::Pop {
                key: "model".into()
            })
        );
    }

    #[test]
    fn parse_toggle_bang() {
        assert_eq!(
            SetCommand::parse("verbose!"),
            Ok(SetCommand::Toggle {
                key: "verbose".into()
            })
        );
    }

    #[test]
    fn parse_toggle_inv() {
        assert_eq!(
            SetCommand::parse("invverbose"),
            Ok(SetCommand::Toggle {
                key: "verbose".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set invthinking"),
            Ok(SetCommand::Toggle {
                key: "thinking".into()
            })
        );
    }

    #[test]
    fn parse_disable_no() {
        assert_eq!(
            SetCommand::parse("noverbose"),
            Ok(SetCommand::Disable {
                key: "verbose".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set nothinking"),
            Ok(SetCommand::Disable {
                key: "thinking".into()
            })
        );
    }

    #[test]
    fn parse_enable() {
        assert_eq!(
            SetCommand::parse("verbose"),
            Ok(SetCommand::Enable {
                key: "verbose".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set thinking"),
            Ok(SetCommand::Enable {
                key: "thinking".into()
            })
        );
    }

    #[test]
    fn parse_set_equals() {
        assert_eq!(
            SetCommand::parse("model=llama3.2"),
            Ok(SetCommand::Set {
                key: "model".into(),
                value: "llama3.2".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set theme=monokai"),
            Ok(SetCommand::Set {
                key: "theme".into(),
                value: "monokai".into()
            })
        );
    }

    #[test]
    fn parse_set_equals_with_spaces() {
        assert_eq!(
            SetCommand::parse("model = llama3.2"),
            Ok(SetCommand::Set {
                key: "model".into(),
                value: "llama3.2".into()
            })
        );
    }

    #[test]
    fn parse_set_space() {
        assert_eq!(
            SetCommand::parse("model llama3.2"),
            Ok(SetCommand::Set {
                key: "model".into(),
                value: "llama3.2".into()
            })
        );
        assert_eq!(
            SetCommand::parse(":set thinkingbudget high"),
            Ok(SetCommand::Set {
                key: "thinkingbudget".into(),
                value: "high".into()
            })
        );
    }

    #[test]
    fn parse_set_colon() {
        assert_eq!(
            SetCommand::parse("model:llama3.2"),
            Ok(SetCommand::Set {
                key: "model".into(),
                value: "llama3.2".into()
            })
        );
    }

    #[test]
    fn parse_full_path() {
        assert_eq!(
            SetCommand::parse("llm.providers.local.temperature=0.9"),
            Ok(SetCommand::Set {
                key: "llm.providers.local.temperature".into(),
                value: "0.9".into()
            })
        );
    }

    #[test]
    fn parse_full_path_query() {
        assert_eq!(
            SetCommand::parse("cli.highlighting.theme?"),
            Ok(SetCommand::Query {
                key: "cli.highlighting.theme".into()
            })
        );
    }

    #[test]
    fn parse_error_empty_key() {
        assert!(matches!(
            SetCommand::parse("?"),
            Err(ParseError::InvalidSyntax(_))
        ));
        assert!(matches!(
            SetCommand::parse("=value"),
            Err(ParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn parse_value_with_spaces() {
        assert_eq!(
            SetCommand::parse("theme base16-ocean.dark"),
            Ok(SetCommand::Set {
                key: "theme".into(),
                value: "base16-ocean.dark".into()
            })
        );
    }

    #[test]
    fn parse_preserves_value_case() {
        assert_eq!(
            SetCommand::parse("model DeepSeek-R1"),
            Ok(SetCommand::Set {
                key: "model".into(),
                value: "DeepSeek-R1".into()
            })
        );
    }
}
