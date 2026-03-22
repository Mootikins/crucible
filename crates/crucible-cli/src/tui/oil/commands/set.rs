//! SetCommand parser for vim-style `:set` commands.

#[allow(unused_imports)] // WIP: fmt not yet used
use std::fmt;

use crate::tui::oil::config::ThinkingPreset;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("Empty command")]
    Empty,
    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum SetRpcAction {
    SwitchModel(String),
    SetThinkingBudget(Option<i64>),
    SetTemperature(f64),
    SetMaxTokens(Option<u32>),
    SetMaxIterations(Option<u32>),
    SetExecutionTimeout(Option<u64>),
    SetContextBudget(Option<usize>),
    SetContextStrategy(String),
    SetContextWindow(Option<usize>),
    SetOutputValidation(String),
    SetValidationRetries(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliValue {
    Enable,
    Disable,
    Toggle,
    Set(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetEffect {
    TuiLocal { key: String, value: CliValue },
    DaemonRpc(SetRpcAction),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SetError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("not supported as CLI flag")]
    NotSupportedAsCli,
    #[error("{key}: {message}")]
    InvalidValue { key: String, message: String },
    #[error("unknown key '{0}'")]
    UnknownKey(String),
}

pub fn validate_set_for_cli(input: &str) -> Result<SetEffect, SetError> {
    let command = SetCommand::parse(input).map_err(SetError::Parse)?;

    match command {
        SetCommand::ShowModified
        | SetCommand::ShowAll
        | SetCommand::Query { .. }
        | SetCommand::QueryHistory { .. }
        | SetCommand::Reset { .. }
        | SetCommand::Pop { .. } => Err(SetError::NotSupportedAsCli),
        SetCommand::Enable { key } => classify_key_without_value(key, CliValue::Enable),
        SetCommand::Disable { key } => classify_key_without_value(key, CliValue::Disable),
        SetCommand::Toggle { key } => classify_key_without_value(key, CliValue::Toggle),
        SetCommand::Set { key, value } => match key.as_str() {
            "model" => Ok(SetEffect::DaemonRpc(SetRpcAction::SwitchModel(value))),
            "thinkingbudget" => {
                if let Some(preset) = ThinkingPreset::by_name(&value) {
                    Ok(SetEffect::DaemonRpc(SetRpcAction::SetThinkingBudget(Some(
                        preset.to_budget(),
                    ))))
                } else {
                    let valid = ThinkingPreset::names().collect::<Vec<_>>().join(", ");
                    Err(SetError::InvalidValue {
                        key,
                        message: format!("unknown preset '{}'. Valid: {}", value, valid),
                    })
                }
            }
            "temperature" => match value.parse::<f64>() {
                Ok(temp) if (0.0..=2.0).contains(&temp) => {
                    Ok(SetEffect::DaemonRpc(SetRpcAction::SetTemperature(temp)))
                }
                Ok(_) => Err(SetError::InvalidValue {
                    key,
                    message: "temperature must be between 0.0 and 2.0".to_string(),
                }),
                Err(_) => Err(SetError::InvalidValue {
                    key,
                    message: format!("invalid temperature value: {}", value),
                }),
            },
            "maxtokens" => {
                let max_tokens =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("null") {
                        None
                    } else {
                        match value.parse::<u32>() {
                            Ok(n) => Some(n),
                            Err(_) => {
                                return Err(SetError::InvalidValue {
                                    key,
                                    message: format!(
                                        "invalid maxtokens value: {} (use a number or 'none')",
                                        value
                                    ),
                                });
                            }
                        }
                    };

                Ok(SetEffect::DaemonRpc(SetRpcAction::SetMaxTokens(max_tokens)))
            }
            "maxiterations" => {
                let max_iterations =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("null") {
                        None
                    } else {
                        match value.parse::<u32>() {
                            Ok(n) => Some(n),
                            Err(_) => {
                                return Err(SetError::InvalidValue {
                                    key,
                                    message: format!(
                                        "invalid maxiterations value: {} (use a number or 'none')",
                                        value
                                    ),
                                });
                            }
                        }
                    };

                Ok(SetEffect::DaemonRpc(SetRpcAction::SetMaxIterations(
                    max_iterations,
                )))
            }
            "executiontimeout" => {
                let timeout_secs =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("null") {
                        None
                    } else {
                        match value.parse::<u64>() {
                            Ok(n) => Some(n),
                            Err(_) => {
                                return Err(SetError::InvalidValue {
                                    key,
                                    message: format!(
                                    "invalid executiontimeout value: {} (use seconds or 'none')",
                                    value
                                ),
                                });
                            }
                        }
                    };

                Ok(SetEffect::DaemonRpc(SetRpcAction::SetExecutionTimeout(
                    timeout_secs,
                )))
            }
            "contextbudget" | "context_budget" => {
                let budget =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("null") {
                        None
                    } else {
                        match value.parse::<usize>() {
                            Ok(n) => Some(n),
                            Err(_) => {
                                return Err(SetError::InvalidValue {
                                    key,
                                    message: format!(
                                        "invalid context_budget value: {} (use a number or 'none')",
                                        value
                                    ),
                                });
                            }
                        }
                    };
                Ok(SetEffect::DaemonRpc(SetRpcAction::SetContextBudget(budget)))
            }
            "contextstrategy" | "context_strategy" => {
                // Validate the strategy value
                match value.to_lowercase().as_str() {
                    "truncate" | "sliding_window" | "slidingwindow" => {
                        let normalized = if value.to_lowercase() == "slidingwindow" {
                            "sliding_window".to_string()
                        } else {
                            value.to_lowercase()
                        };
                        Ok(SetEffect::DaemonRpc(SetRpcAction::SetContextStrategy(
                            normalized,
                        )))
                    }
                    _ => Err(SetError::InvalidValue {
                        key,
                        message: format!(
                            "unknown strategy '{}'. Valid: truncate, sliding_window",
                            value
                        ),
                    }),
                }
            }
            "contextwindow" | "context_window" => {
                let window =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("null") {
                        None
                    } else {
                        match value.parse::<usize>() {
                            Ok(n) => Some(n),
                            Err(_) => {
                                return Err(SetError::InvalidValue {
                                    key,
                                    message: format!(
                                        "invalid context_window value: {} (use a number or 'none')",
                                        value
                                    ),
                                });
                            }
                        }
                    };
                Ok(SetEffect::DaemonRpc(SetRpcAction::SetContextWindow(window)))
            }
            "outputvalidation" | "output_validation" => {
                // Validate the value parses correctly
                value
                    .parse::<crucible_core::session::OutputValidation>()
                    .map_err(|message| SetError::InvalidValue {
                        key: key.clone(),
                        message,
                    })?;
                Ok(SetEffect::DaemonRpc(SetRpcAction::SetOutputValidation(
                    value,
                )))
            }
            "validationretries" | "validation_retries" => {
                let retries = value.parse::<u32>().map_err(|_| SetError::InvalidValue {
                    key: key.clone(),
                    message: format!(
                        "invalid validation_retries value: {} (use a non-negative integer)",
                        value
                    ),
                })?;
                Ok(SetEffect::DaemonRpc(SetRpcAction::SetValidationRetries(
                    retries,
                )))
            }
            "perm.show_diff" | "perm.autoconfirm_session" => {
                parse_bool(&value).map_err(|message| SetError::InvalidValue {
                    key: key.clone(),
                    message,
                })?;
                Ok(SetEffect::TuiLocal {
                    key,
                    value: CliValue::Set(value),
                })
            }
            "thinking" | "precognition" | "verbose" | "theme" => Ok(SetEffect::TuiLocal {
                key,
                value: CliValue::Set(value),
            }),
            "precognition.results" => {
                let parsed = value.parse::<usize>().map_err(|_| SetError::InvalidValue {
                    key: key.clone(),
                    message: "precognition.results must be 1-20".to_string(),
                })?;
                if !(1..=20).contains(&parsed) {
                    return Err(SetError::InvalidValue {
                        key,
                        message: "precognition.results must be 1-20".to_string(),
                    });
                }
                Ok(SetEffect::TuiLocal {
                    key,
                    value: CliValue::Set(value),
                })
            }
            _ => Err(SetError::UnknownKey(key)),
        },
    }
}

fn classify_key_without_value(key: String, effect: CliValue) -> Result<SetEffect, SetError> {
    if is_tui_local_key(&key) {
        Ok(SetEffect::TuiLocal { key, value: effect })
    } else if is_daemon_rpc_key(&key) {
        Err(SetError::InvalidValue {
            key,
            message: "this key requires an explicit value".to_string(),
        })
    } else {
        Err(SetError::UnknownKey(key))
    }
}

fn is_tui_local_key(key: &str) -> bool {
    matches!(
        key,
        "thinking"
            | "precognition"
            | "precognition.results"
            | "perm.show_diff"
            | "perm.autoconfirm_session"
            | "theme"
            | "verbose"
    )
}

fn is_daemon_rpc_key(key: &str) -> bool {
    matches!(
        key,
        "model"
            | "thinkingbudget"
            | "temperature"
            | "maxtokens"
            | "maxiterations"
            | "executiontimeout"
            | "contextbudget"
            | "context_budget"
            | "contextstrategy"
            | "context_strategy"
            | "contextwindow"
            | "context_window"
    )
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid value: '{}'. Use true/false", value)),
    }
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

    #[test]
    fn validate_set_for_cli_temperature_invalid() {
        assert!(matches!(
            validate_set_for_cli("temperature=abc"),
            Err(SetError::InvalidValue { .. })
        ));
    }

    #[test]
    fn validate_set_for_cli_query_not_supported() {
        assert_eq!(
            validate_set_for_cli("model?"),
            Err(SetError::NotSupportedAsCli)
        );
    }

    #[test]
    fn validate_set_for_cli_model_ok() {
        assert_eq!(
            validate_set_for_cli("model=llama3"),
            Ok(SetEffect::DaemonRpc(SetRpcAction::SwitchModel(
                "llama3".to_string()
            )))
        );
    }

    #[test]
    fn validate_set_for_cli_perm_enable_ok() {
        assert_eq!(
            validate_set_for_cli("perm.autoconfirm_session"),
            Ok(SetEffect::TuiLocal {
                key: "perm.autoconfirm_session".to_string(),
                value: CliValue::Enable,
            })
        );
    }

    #[test]
    fn validate_set_for_cli_temperature_ok() {
        assert_eq!(
            validate_set_for_cli("temperature=1.5"),
            Ok(SetEffect::DaemonRpc(SetRpcAction::SetTemperature(1.5)))
        );
    }

    #[test]
    fn validate_set_for_cli_maxtokens_none_ok() {
        assert_eq!(
            validate_set_for_cli("maxtokens=none"),
            Ok(SetEffect::DaemonRpc(SetRpcAction::SetMaxTokens(None)))
        );
    }

    #[test]
    fn validate_set_for_cli_unknown_key() {
        assert_eq!(
            validate_set_for_cli("unknownkey"),
            Err(SetError::UnknownKey("unknownkey".to_string()))
        );
    }

    #[test]
    fn validate_set_for_cli_empty_not_supported() {
        assert_eq!(validate_set_for_cli(""), Err(SetError::NotSupportedAsCli));
    }

    #[test]
    fn validate_set_for_cli_toggle_tui_local() {
        assert_eq!(
            validate_set_for_cli("verbose!"),
            Ok(SetEffect::TuiLocal {
                key: "verbose".to_string(),
                value: CliValue::Toggle,
            })
        );
    }
}
