use std::collections::VecDeque;

use crate::tui::oil::style::Color;
use crate::tui::oil::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatMode {
    #[default]
    Normal,
    Plan,
    Auto,
}

impl ChatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatMode::Normal => "normal",
            ChatMode::Plan => "plan",
            ChatMode::Auto => "auto",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "plan" => ChatMode::Plan,
            "auto" => ChatMode::Auto,
            _ => ChatMode::Normal,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            ChatMode::Normal => ChatMode::Plan,
            ChatMode::Plan => ChatMode::Auto,
            ChatMode::Auto => ChatMode::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Command,
    Shell,
}

impl InputMode {
    pub fn bg_color(&self) -> Color {
        let t = theme::active();
        match self {
            InputMode::Normal => t.resolve_color(t.colors.background),
            InputMode::Command => t.resolve_color(t.colors.command_bg),
            InputMode::Shell => t.resolve_color(t.colors.shell_bg),
        }
    }

    pub fn prompt(&self) -> &'static str {
        match self {
            InputMode::Normal => " > ",
            InputMode::Command => " : ",
            InputMode::Shell => " ! ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AutocompleteKind {
    #[default]
    None,
    File,
    Note,
    Command,
    SlashCommand,
    ReplCommand,
    Model,
    CommandArg {
        command: String,
        arg_index: usize,
    },
    SetOption {
        option: Option<String>,
    },
    /// Full-screen picker opened by `:pick [source]`.
    Pick {
        source: PickSource,
    },
}

/// Source category for the `:pick` command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PickSource {
    #[default]
    All,
    Notes,
    Sessions,
    Commands,
    Files,
}

/// Message queue state — deferred messages and message counter
#[derive(Default)]
pub(crate) struct MessageQueueState {
    /// Messages deferred until the current stream completes
    pub deferred_messages: VecDeque<String>,
    /// Monotonic counter for assigning message IDs
    pub message_counter: usize,
    /// Timestamp of the last Ctrl-C press (for double-tap quit)
    pub last_ctrl_c: Option<std::time::Instant>,
}
