use std::collections::VecDeque;
use std::path::PathBuf;

use crate::tui::oil::style::Color;
use crate::tui::oil::theme::ThemeTokens;

#[derive(Debug, Clone)]
pub enum ChatItem {
    Message {
        id: String,
        role: Role,
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: String,
        result: String,
        complete: bool,
    },
    /// Shell command execution - display only, never sent to agent
    ShellExecution {
        id: String,
        command: String,
        exit_code: i32,
        output_tail: Vec<String>,
        output_path: Option<PathBuf>,
    },
}

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
        let theme = ThemeTokens::default_ref();
        match self {
            InputMode::Normal => theme.input_bg,
            InputMode::Command => theme.command_bg,
            InputMode::Shell => theme.shell_bg,
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
