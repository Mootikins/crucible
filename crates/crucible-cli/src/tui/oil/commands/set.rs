//! SetCommand parser for vim-style `:set` commands.
//!
//! [`SetCommand`] and [`SetEffect`] are closed sets that decide which store a
//! `:set` reaches. The two denies below turn a wildcard arm — the arm that
//! sends a new spelling or a new effect somewhere nobody chose — into a
//! compile error.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use crate::tui::oil::config::ConfigValue;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("Empty command")]
    Empty,
    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),
}

/// The `:set` spellings. A closed set: the dispatch in `command_handling.rs`
/// matches every variant, and the tests walk them through `EnumIter` so a new
/// spelling cannot reach a store the other spellings do not.
/// Which layers a `:set` drop takes away.
///
/// Three spellings, three answers, so a boolean cannot carry it: `&` resets,
/// `^` pops one layer, `=` with nothing after it removes the key. An
/// enumerated table rather than two bools, so a fourth spelling cannot be
/// added without every match seeing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum DropKind {
    /// `:set key&` — drop the ephemeral layer a `:set` writes.
    Reset,
    /// `:set key^` — drop the highest-ranked layer holding the leaf.
    Pop,
    /// `:set key=` — remove the key and everything under it.
    Unset,
}

impl DropKind {
    /// The RPC verb that performs this drop.
    pub fn method(self) -> &'static str {
        match self {
            DropKind::Reset => "config.reset",
            DropKind::Pop => "config.pop",
            DropKind::Unset => "config.unset",
        }
    }

    /// The spelling that asked for it, for an error message.
    pub fn spelling(self) -> &'static str {
        match self {
            DropKind::Reset => "&",
            DropKind::Pop => "^",
            DropKind::Unset => "=",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum SetCommand {
    ShowModified,
    ShowAll,
    Query {
        key: String,
    },
    QueryHistory {
        key: String,
    },
    Enable {
        key: String,
    },
    Disable {
        key: String,
    },
    Toggle {
        key: String,
    },
    Reset {
        key: String,
    },
    Pop {
        key: String,
    },
    /// `:set key=` — an assignment with nothing after the `=`.
    ///
    /// Vim spells "give me back the default" `&`, and has no spelling for
    /// "remove this key" because its options are a fixed set that cannot be
    /// removed. Crucible's are not: `llm.providers.<name>` is user-named, and
    /// a stale one has to be removable. An empty assignment is the natural
    /// spelling — the user says the value is nothing, and nothing is not the
    /// empty string.
    Unset {
        key: String,
    },
    Set {
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetRpcAction {
    SwitchModel(String),
    SetContextStrategy(String),
    SetPrecognition(bool),
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
        | SetCommand::Pop { .. }
        | SetCommand::Unset { .. } => Err(SetError::NotSupportedAsCli),
        SetCommand::Enable { key } => classify_key_without_value(key, CliValue::Enable),
        SetCommand::Disable { key } => classify_key_without_value(key, CliValue::Disable),
        SetCommand::Toggle { key } => classify_key_without_value(key, CliValue::Toggle),
        SetCommand::Set { key, value } => classify_set_value(key, value),
    }
}

/// Classify and validate a `key=value` assignment into its effect.
///
/// Single source of truth for the set-key space: both the CLI `--set` path
/// (via [`validate_set_for_cli`]) and the live TUI `:set` dispatch call this,
/// so a key can't be accepted by one surface and silently dropped by the
/// other (the routing-seam bug class).
pub fn classify_set_value(key: String, value: String) -> Result<SetEffect, SetError> {
    match key.as_str() {
        "model" => Ok(SetEffect::DaemonRpc(SetRpcAction::SwitchModel(value))),
        "contextstrategy" | "context_strategy" => {
            // Validate the strategy value
            match value.to_lowercase().as_str() {
                strategy @ ("truncate" | "summarize") => Ok(SetEffect::DaemonRpc(
                    SetRpcAction::SetContextStrategy(strategy.to_string()),
                )),
                _ => Err(SetError::InvalidValue {
                    key,
                    message: format!("unknown strategy '{}'. Valid: truncate, summarize", value),
                }),
            }
        }
        "perm.show_diff" | "perm.autoconfirm_session" | "perm.full_commands" => {
            parse_bool(&value).map_err(|message| SetError::InvalidValue {
                key: key.clone(),
                message,
            })?;
            Ok(SetEffect::TuiLocal {
                key,
                value: CliValue::Set(value),
            })
        }
        // `thinking` and `show_diffs` stay TUI-local on purpose: they hide or
        // show reasoning blocks and diff bodies in this client's transcript
        // and change nothing about the turn. `precognition` looked like their
        // neighbour and was not — it decides whether the daemon injects kiln
        // context, so a TUI-local toggle only ever changed the `:set` readout.
        //
        // `show_diffs` reached this classifier as an unknown key until 2026-09,
        // which mirrored a display key into the daemon app-config store.
        "thinking" | "show_diffs" => {
            parse_bool(&value).map_err(|message| SetError::InvalidValue {
                key: key.clone(),
                message,
            })?;
            Ok(SetEffect::TuiLocal {
                key,
                value: CliValue::Set(value),
            })
        }
        "precognition" => {
            let enabled = parse_bool(&value).map_err(|message| SetError::InvalidValue {
                key: key.clone(),
                message,
            })?;
            Ok(SetEffect::DaemonRpc(SetRpcAction::SetPrecognition(enabled)))
        }
        // Syntax-highlight theme (syntect). Was spelled `theme`, which collided
        // with the UI colorscheme — three different things were called "theme".
        // Validated against the loaded theme set so a typo fails loudly instead
        // of silently falling back.
        "syntax_theme" | "syntaxtheme" => {
            if value == crate::formatting::syntax::DERIVED_THEME
                || crate::formatting::SyntaxHighlighter::available_themes()
                    .iter()
                    .any(|t| *t == value)
            {
                Ok(SetEffect::TuiLocal {
                    key,
                    value: CliValue::Set(value),
                })
            } else {
                let mut valid = crate::formatting::SyntaxHighlighter::available_themes();
                valid.insert(0, crate::formatting::syntax::DERIVED_THEME);
                let valid = valid.join(", ");
                Err(SetError::InvalidValue {
                    key,
                    message: format!("unknown theme '{}'. Valid: {}", value, valid),
                })
            }
        }
        // Popup presentation: auto = minimal for inline (@file/[[note) triggers,
        // panel strip for command-line (`:` and `/`) completions.
        "completionstyle" | "completion_style" => match value.to_lowercase().as_str() {
            "auto" | "panel" | "minimal" => Ok(SetEffect::TuiLocal {
                key,
                value: CliValue::Set(value.to_lowercase()),
            }),
            _ => Err(SetError::InvalidValue {
                key,
                message: format!(
                    "unknown completion_style '{}'. Valid: auto, panel, minimal",
                    value
                ),
            }),
        },
        _ => Err(SetError::UnknownKey(key)),
    }
}

impl SetRpcAction {
    /// Map to the TUI message that performs the daemon sync.
    ///
    /// `None` for actions with no message equivalent.
    pub fn into_chat_msg(self) -> Option<crate::tui::oil::chat_app::ChatAppMsg> {
        use crate::tui::oil::chat_app::ChatAppMsg;
        match self {
            SetRpcAction::SwitchModel(m) => Some(ChatAppMsg::SwitchModel(m)),
            SetRpcAction::SetContextStrategy(s) => Some(ChatAppMsg::SetContextStrategy(s)),
            SetRpcAction::SetPrecognition(enabled) => Some(ChatAppMsg::SetPrecognition(enabled)),
        }
    }
}

/// Which store answers for one `:set` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyHome {
    /// This client's own overlay: display state, and the session knobs the
    /// TUI mirrors so a redraw needs no round trip.
    Client,
    /// The daemon app-config store, which owns every key no rule declares.
    Daemon,
}

/// Where the key of a value-less `:set` spelling lives.
///
/// `key?`, `key??`, `key&` and `key^` carry nothing to classify, so they ask
/// here instead of assuming the local overlay. Answering that question twice
/// is the defect this replaces: `:set foo=9` reached the daemon while
/// `:set foo?` read an overlay that never held `foo` and printed "not set".
///
/// The answer comes from [`classify_key_without_value`] rather than from a
/// second list of key names, so one rule names the client's keys.
pub fn key_home(key: &str) -> KeyHome {
    // `CliValue::Toggle` only decides WHICH error a declared key returns. The
    // question here is whether any rule names the key at all.
    match classify_key_without_value(key.to_string(), CliValue::Toggle) {
        // Nothing declares it, so the daemon app-config store owns it.
        Err(SetError::UnknownKey(_)) => KeyHome::Daemon,
        Ok(SetEffect::TuiLocal { .. } | SetEffect::DaemonRpc(_))
        | Err(SetError::InvalidValue { .. } | SetError::NotSupportedAsCli | SetError::Parse(_)) => {
            KeyHome::Client
        }
    }
}

/// Classify a value-less `:set key` / `:set nokey` / `:set key!`.
///
/// The same door as [`classify_set_value`], for the same reason: the TUI used
/// to answer these three spellings from its own store, so a key could be
/// `true` in the TUI and absent from the daemon at the same time.
pub fn classify_key_without_value(key: String, effect: CliValue) -> Result<SetEffect, SetError> {
    if is_tui_local_key(&key) {
        Ok(SetEffect::TuiLocal { key, value: effect })
    } else if key == "precognition" {
        // `:set precognition` / `:set noprecognition` are the spellings the
        // help text advertises, so they have to reach the daemon too. A bare
        // toggle cannot be resolved here — this classifier has no session
        // state — so the TUI resolves it against its own copy and the
        // value-less CLI form asks for an explicit one.
        match effect {
            CliValue::Enable => Ok(SetEffect::DaemonRpc(SetRpcAction::SetPrecognition(true))),
            CliValue::Disable => Ok(SetEffect::DaemonRpc(SetRpcAction::SetPrecognition(false))),
            CliValue::Toggle | CliValue::Set(_) => Err(SetError::InvalidValue {
                key,
                message: "toggling needs the current value; use precognition=on|off".to_string(),
            }),
        }
    } else if is_daemon_rpc_key(&key) || needs_an_explicit_value(&key) {
        Err(SetError::InvalidValue {
            key,
            message: "this key requires an explicit value".to_string(),
        })
    } else {
        Err(SetError::UnknownKey(key))
    }
}

/// The keys this client owns outright: boolean display state that no other
/// client and no plugin reads. A value-less `:set` spelling writes these
/// locally; every other key belongs to the daemon.
fn is_tui_local_key(key: &str) -> bool {
    matches!(
        key,
        "thinking"
            | "show_diffs"
            | "precognition"
            | "perm.show_diff"
            | "perm.autoconfirm_session"
            | "perm.full_commands"
    )
}

fn is_daemon_rpc_key(key: &str) -> bool {
    matches!(key, "model" | "contextstrategy" | "context_strategy")
}

/// Declared `:set` targets whose value is not a boolean and whose home is
/// this client. `:set syntax_theme` cannot mean "true", so the value-less
/// spellings ask for a value rather than writing one.
fn needs_an_explicit_value(key: &str) -> bool {
    matches!(
        key,
        "syntax_theme" | "syntaxtheme" | "completionstyle" | "completion_style"
    )
}

/// Parse a boolean option value with the same tokens `ConfigValue` accepts.
pub(crate) fn parse_bool(value: &str) -> Result<bool, String> {
    ConfigValue::try_parse_bool(value).ok_or_else(|| {
        format!(
            "invalid value: '{}'. Use true/false, yes/no, on/off, y/n or 1/0",
            value
        )
    })
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
            if value.is_empty() {
                return Ok(SetCommand::Unset {
                    key: key.to_string(),
                });
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
    use crate::tui::oil::config::SHORTCUTS;

    /// Every declared `:set` target is classified. A target the classifier
    /// does not know falls through to the app-config store, which mirrors a
    /// TUI display key into the daemon and gives one key two homes.
    ///
    /// The expectation walks the shortcut table itself rather than a second
    /// list, so a new shortcut cannot pass by being absent from both.
    #[test]
    fn every_declared_set_target_is_classified() {
        for shortcut in SHORTCUTS {
            let classified = classify_set_value(shortcut.short.to_string(), "true".to_string());
            assert!(
                !matches!(classified, Err(SetError::UnknownKey(_))),
                "`:set {}` reaches the daemon app-config store because nothing classifies it",
                shortcut.short
            );
            // The value-less spellings ask `key_home`, not the classifier.
            // The two must name the same store, or a target is written in
            // one place and read from the other.
            assert_eq!(
                key_home(shortcut.short),
                KeyHome::Client,
                "`:set {}?` and `:set {}=v` disagree about which store owns the key",
                shortcut.short,
                shortcut.short
            );
        }
    }

    /// `show_diffs` decides whether THIS transcript renders diff bodies. It
    /// is display state, so it stays in the TUI.
    #[test]
    fn show_diffs_stays_in_the_tui() {
        assert_eq!(
            classify_set_value("show_diffs".to_string(), "false".to_string()),
            Ok(SetEffect::TuiLocal {
                key: "show_diffs".to_string(),
                value: CliValue::Set("false".to_string()),
            })
        );
        assert!(matches!(
            classify_set_value("show_diffs".to_string(), "maybe".to_string()),
            Err(SetError::InvalidValue { .. })
        ));
    }

    #[test]
    fn parse_bool_accepts_the_config_value_tokens() {
        for input in ["y", "Y", "yes", "on", "1", "TRUE"] {
            assert_eq!(parse_bool(input), Ok(true), "{input}");
        }
        for input in ["n", "N", "no", "off", "0", "FALSE"] {
            assert_eq!(parse_bool(input), Ok(false), "{input}");
        }
        assert!(parse_bool("maybe").is_err());
    }

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
            SetCommand::parse(":set syntax_theme??"),
            Ok(SetCommand::QueryHistory {
                key: "syntax_theme".into()
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
            SetCommand::parse(":set syntax_theme=monokai"),
            Ok(SetCommand::Set {
                key: "syntax_theme".into(),
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
            SetCommand::parse(":set contextstrategy truncate"),
            Ok(SetCommand::Set {
                key: "contextstrategy".into(),
                value: "truncate".into()
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
            SetCommand::parse("syntax_theme base16-ocean.dark"),
            Ok(SetCommand::Set {
                key: "syntax_theme".into(),
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
    fn completion_style_valid_values_are_tui_local() {
        for v in ["auto", "panel", "minimal"] {
            assert_eq!(
                validate_set_for_cli(&format!("completion_style={v}")),
                Ok(SetEffect::TuiLocal {
                    key: "completion_style".to_string(),
                    value: CliValue::Set(v.to_string()),
                })
            );
        }
        assert!(matches!(
            validate_set_for_cli("completionstyle=minimal"),
            Ok(SetEffect::TuiLocal { .. })
        ));
    }

    #[test]
    fn completion_style_invalid_value_rejected() {
        assert!(matches!(
            validate_set_for_cli("completion_style=fancy"),
            Err(SetError::InvalidValue { .. })
        ));
    }

    #[test]
    fn validate_set_for_cli_toggle_tui_local() {
        assert_eq!(
            validate_set_for_cli("thinking!"),
            Ok(SetEffect::TuiLocal {
                key: "thinking".to_string(),
                value: CliValue::Toggle,
            })
        );
    }

    #[test]
    fn syntax_theme_valid_value_is_tui_local() {
        assert_eq!(
            validate_set_for_cli("syntax_theme=InspiredGitHub"),
            Ok(SetEffect::TuiLocal {
                key: "syntax_theme".to_string(),
                value: CliValue::Set("InspiredGitHub".to_string()),
            })
        );
    }

    #[test]
    fn syntax_theme_unknown_value_rejected_with_valid_list() {
        match validate_set_for_cli("syntax_theme=no-such-theme") {
            Err(SetError::InvalidValue { message, .. }) => assert!(
                message.contains("base16-ocean.dark"),
                "error should list valid themes: {message}"
            ),
            other => panic!("expected InvalidValue, got {other:?}"),
        }
    }

    /// `verbose` had no consumer in a running TUI — it is no longer a handled
    /// key. (Via `:set` it now mirrors to the daemon config store like any
    /// unknown key; via `--set` it is rejected.)
    #[test]
    fn verbose_is_no_longer_a_handled_key() {
        assert_eq!(
            validate_set_for_cli("verbose=true"),
            Err(SetError::UnknownKey("verbose".to_string()))
        );
    }
}
