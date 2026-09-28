//! The slash commands of a session.
//!
//! The daemon builds one catalog per session from five sources: the commands
//! every client provides ([`BuiltinCommand`]), the session's modes, plugin
//! commands, discovered skills, and the commands that an ACP agent
//! advertises. A client completes and dispatches from that catalog.
//!
//! A built-in command is the client's own action, because each client shows
//! it in its own way: the TUI opens a picker where the web client prints a
//! list. Each client matches [`BuiltinCommand`] without a wildcard, so the
//! compiler makes every client handle every built-in command. Every other
//! command travels as the message text `/name args`, and the daemon routes
//! it. That is also how an ACP host invokes an agent's command.

use serde::{Deserialize, Serialize};

/// A command that every client provides as its own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumIter)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BuiltinCommand {
    Help,
    Clear,
    Model,
    Mode,
    Undo,
    Resume,
    Export,
    Search,
}

// A wildcard arm here would let a new built-in command compile unnamed.
#[deny(clippy::wildcard_enum_match_arm)]
#[deny(clippy::match_wildcard_for_single_variants)]
impl BuiltinCommand {
    /// The name after the slash.
    pub fn name(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Clear => "clear",
            Self::Model => "model",
            Self::Mode => "mode",
            Self::Undo => "undo",
            Self::Resume => "resume",
            Self::Export => "export",
            Self::Search => "search",
        }
    }

    /// The argument placeholder for completion and help, or `None` when the
    /// command takes no argument.
    pub fn input_hint(self) -> Option<&'static str> {
        match self {
            Self::Model => Some("[name]"),
            Self::Undo => Some("[turns]"),
            Self::Resume => Some("[session]"),
            Self::Export => Some("[path]"),
            Self::Search => Some("<query>"),
            Self::Help | Self::Clear | Self::Mode => None,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Help => "Show the commands of this session",
            Self::Clear => "Clear the model context (the transcript stays)",
            Self::Model => "Switch the model, or list the models",
            Self::Mode => "Switch to the next mode",
            Self::Undo => "Undo the last turns",
            Self::Resume => "Resume an earlier session",
            Self::Export => "Export the session to markdown",
            Self::Search => "Search sessions",
        }
    }

    /// Every built-in command as a catalog entry, in declaration order.
    pub fn entries() -> Vec<SessionCommand> {
        use strum::IntoEnumIterator;
        Self::iter().map(Self::entry).collect()
    }

    /// The built-in command with this name.
    pub fn from_name(name: &str) -> Option<Self> {
        use strum::IntoEnumIterator;
        Self::iter().find(|command| command.name() == name)
    }

    /// This command as a catalog entry.
    pub fn entry(self) -> SessionCommand {
        SessionCommand {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_hint: self.input_hint().map(str::to_string),
            kind: CommandKind::Builtin { command: self },
        }
    }
}

/// Where a command comes from, and so who runs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandKind {
    /// The client runs it.
    Builtin { command: BuiltinCommand },
    /// The daemon switches the session to this mode.
    Mode { mode_id: String },
    /// The daemon runs the command of this plugin.
    Plugin { plugin: String },
    /// The daemon gives the turn this skill's instructions.
    Skill,
    /// The ACP agent of the session runs it; the daemon sends the text on.
    Agent,
}

/// One entry of a session's command catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionCommand {
    /// The name after the slash.
    pub name: String,
    pub description: String,
    /// The argument placeholder, when the command takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hint: Option<String>,
    #[serde(flatten)]
    pub kind: CommandKind,
}

/// What `session.send_message` did with the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SendOutcome {
    /// A turn started. Its events follow on the session's event stream.
    Turn { message_id: String },
    /// The text named a command that the daemon ran without a turn, for
    /// example a plugin command or a bare mode switch.
    Command {
        command: String,
        #[cfg_attr(feature = "openapi", schema(value_type = serde_json::Value))]
        result: serde_json::Value,
    },
}

impl SendOutcome {
    /// The result of a command as text for a person: a string as it is,
    /// other JSON in its pretty form.
    pub fn result_text(result: &serde_json::Value) -> String {
        match result {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Null => String::new(),
            other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
        }
    }
}

/// Split `/name rest` into the command name and the rest, or `None` when the
/// text is not a slash command.
pub fn split_slash_command(text: &str) -> Option<(&str, &str)> {
    let body = text.trim_start().strip_prefix('/')?;
    let (name, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    (!name.is_empty() && !name.contains('/')).then_some((name, rest.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn each_builtin_command_has_a_unique_name_that_finds_it_again() {
        let mut names = std::collections::HashSet::new();
        for command in BuiltinCommand::iter() {
            assert!(names.insert(command.name()), "{command:?}");
            assert_eq!(BuiltinCommand::from_name(command.name()), Some(command));
        }
        assert_eq!(BuiltinCommand::from_name("nothing"), None);
    }

    #[test]
    fn a_catalog_entry_carries_its_kind_on_the_wire() {
        let entry = SessionCommand {
            name: "review".into(),
            description: "Review the branch".into(),
            input_hint: None,
            kind: CommandKind::Plugin {
                plugin: "reviewer".into(),
            },
        };
        let wire = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "name": "review",
                "description": "Review the branch",
                "kind": "plugin",
                "plugin": "reviewer",
            })
        );
        assert_eq!(
            serde_json::from_value::<SessionCommand>(wire).unwrap(),
            entry
        );
        assert_eq!(
            serde_json::to_value(BuiltinCommand::Clear.entry()).unwrap()["command"],
            "clear"
        );
    }

    #[test]
    fn a_send_outcome_names_its_kind_on_the_wire() {
        let turn = SendOutcome::Turn {
            message_id: "m1".into(),
        };
        assert_eq!(
            serde_json::to_value(&turn).unwrap(),
            serde_json::json!({"outcome": "turn", "message_id": "m1"})
        );
        let command: SendOutcome = serde_json::from_value(serde_json::json!({
            "outcome": "command", "command": "plan", "result": "Switched to plan"
        }))
        .unwrap();
        assert_eq!(
            command,
            SendOutcome::Command {
                command: "plan".into(),
                result: "Switched to plan".into()
            }
        );
    }

    #[test]
    fn a_slash_command_splits_into_its_name_and_the_rest() {
        assert_eq!(
            split_slash_command("/review main"),
            Some(("review", "main"))
        );
        assert_eq!(split_slash_command("  /help"), Some(("help", "")));
        assert_eq!(
            split_slash_command("/model  gpt-4o "),
            Some(("model", "gpt-4o"))
        );
        for text in ["no slash", "/", "/ spaced", "/usr/bin/env", ""] {
            assert_eq!(split_slash_command(text), None, "{text:?}");
        }
    }
}
