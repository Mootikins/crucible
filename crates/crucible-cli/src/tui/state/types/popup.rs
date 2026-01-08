//! Popup-related types for TUI state
//!
//! This module contains types for inline popup picker items and their metadata.

/// Type of popup trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    /// Slash commands: `/search`, `/clear`, skills
    Command,
    /// Context mentions: `@agent`, `@file`, `@note`
    AgentOrFile,
    /// REPL commands: `:quit`, `:help`, `:mode`
    ReplCommand,
    /// Session picker: `/resume` command
    Session,
}

/// Popup entry displayed in the inline picker
///
/// Each variant contains only the data relevant to that item type.
/// Use the accessor methods (`title()`, `token()`, etc.) for uniform access.
#[derive(Debug, Clone, PartialEq)]
pub enum PopupItem {
    /// Slash command: `/name`
    Command {
        name: String,
        description: String,
        /// Argument hint shown as faded text (e.g., "<query>" for /search)
        argument_hint: Option<String>,
        score: i32,
        available: bool,
    },
    /// Agent mention: `@id`
    Agent {
        id: String,
        description: String,
        score: i32,
        available: bool,
    },
    /// Workspace file reference
    File {
        path: String,
        score: i32,
        available: bool,
    },
    /// Note reference: `note:path`
    Note {
        path: String,
        score: i32,
        available: bool,
    },
    /// Skill invocation: `skill:name`
    Skill {
        name: String,
        description: String,
        scope: String,
        score: i32,
        available: bool,
    },
    /// REPL command: `:name`
    ReplCommand {
        name: String,
        description: String,
        score: i32,
    },
    /// Session for resuming: displayed in /resume popup
    Session {
        /// Session ID (e.g., "chat-20260104-1530-a1b2")
        id: String,
        /// Human-readable description (date/time)
        description: String,
        /// Number of messages in the session
        message_count: u32,
        score: i32,
    },
}

impl PopupItem {
    // =========================================================================
    // Constructors - create items with sensible defaults
    // =========================================================================

    /// Create a new command popup item: `/name`
    pub fn cmd(name: impl Into<String>) -> Self {
        PopupItem::Command {
            name: name.into(),
            description: String::new(),
            argument_hint: None,
            score: 0,
            available: true,
        }
    }

    /// Create a new agent popup item: `@id`
    pub fn agent(id: impl Into<String>) -> Self {
        PopupItem::Agent {
            id: id.into(),
            description: String::new(),
            score: 0,
            available: true,
        }
    }

    /// Create a new file popup item
    pub fn file(path: impl Into<String>) -> Self {
        PopupItem::File {
            path: path.into(),
            score: 0,
            available: true,
        }
    }

    /// Create a new note popup item
    pub fn note(path: impl Into<String>) -> Self {
        PopupItem::Note {
            path: path.into(),
            score: 0,
            available: true,
        }
    }

    /// Create a new skill popup item
    pub fn skill(name: impl Into<String>) -> Self {
        PopupItem::Skill {
            name: name.into(),
            description: String::new(),
            scope: String::new(),
            score: 0,
            available: true,
        }
    }

    /// Create a new REPL command popup item: `:name`
    pub fn repl(name: impl Into<String>) -> Self {
        PopupItem::ReplCommand {
            name: name.into(),
            description: String::new(),
            score: 0,
        }
    }

    /// Create a new session popup item for resuming
    pub fn session(id: impl Into<String>) -> Self {
        PopupItem::Session {
            id: id.into(),
            description: String::new(),
            message_count: 0,
            score: 0,
        }
    }

    // =========================================================================
    // Builder methods - chain after constructor
    // =========================================================================

    /// Builder: set description (for Command, Agent, Skill, ReplCommand, Session)
    pub fn desc(mut self, description: impl Into<String>) -> Self {
        let d = description.into();
        match &mut self {
            PopupItem::Command { description, .. } => *description = d,
            PopupItem::Agent { description, .. } => *description = d,
            PopupItem::Skill { description, .. } => *description = d,
            PopupItem::ReplCommand { description, .. } => *description = d,
            PopupItem::Session { description, .. } => *description = d,
            PopupItem::File { .. } | PopupItem::Note { .. } => {}
        }
        self
    }

    /// Builder: set argument hint (Command only)
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        if let PopupItem::Command { argument_hint, .. } = &mut self {
            *argument_hint = Some(hint.into());
        }
        self
    }

    /// Builder: set scope (Skill only)
    pub fn with_scope(mut self, s: impl Into<String>) -> Self {
        if let PopupItem::Skill { scope, .. } = &mut self {
            *scope = s.into();
        }
        self
    }

    /// Builder: set message count (Session only)
    pub fn with_message_count(mut self, count: u32) -> Self {
        if let PopupItem::Session { message_count, .. } = &mut self {
            *message_count = count;
        }
        self
    }

    /// Builder: set score
    pub fn with_score(mut self, s: i32) -> Self {
        match &mut self {
            PopupItem::Command { score, .. } => *score = s,
            PopupItem::Agent { score, .. } => *score = s,
            PopupItem::File { score, .. } => *score = s,
            PopupItem::Note { score, .. } => *score = s,
            PopupItem::Skill { score, .. } => *score = s,
            PopupItem::ReplCommand { score, .. } => *score = s,
            PopupItem::Session { score, .. } => *score = s,
        }
        self
    }

    /// Builder: set availability
    pub fn with_available(mut self, a: bool) -> Self {
        match &mut self {
            PopupItem::Command { available, .. } => *available = a,
            PopupItem::Agent { available, .. } => *available = a,
            PopupItem::File { available, .. } => *available = a,
            PopupItem::Note { available, .. } => *available = a,
            PopupItem::Skill { available, .. } => *available = a,
            PopupItem::ReplCommand { .. } | PopupItem::Session { .. } => {} // Always available
        }
        self
    }

    // =========================================================================
    // Accessors - uniform interface across variants
    // =========================================================================

    /// Display title (e.g., "/search", "@agent", ":quit", "src/main.rs")
    pub fn title(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{}", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => format!("note:{}", path),
            PopupItem::Skill { name, .. } => format!("skill:{}", name),
            PopupItem::ReplCommand { name, .. } => format!(":{}", name),
            PopupItem::Session { id, .. } => id.clone(),
        }
    }

    /// Subtitle/description text
    pub fn subtitle(&self) -> &str {
        match self {
            PopupItem::Command { description, .. } => description,
            PopupItem::Agent { description, .. } => description,
            PopupItem::File { .. } => "workspace",
            PopupItem::Note { .. } => "note",
            PopupItem::Skill {
                description,
                scope: _,
                ..
            } => {
                // For skills, we want "description (scope)" but we can't allocate here
                // Return just description; caller can format with scope if needed
                description
            }
            PopupItem::ReplCommand { description, .. } => description,
            PopupItem::Session { description, .. } => description,
        }
    }

    /// Token to insert when selected (for session, returns ID for resume)
    pub fn token(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{} ", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => path.clone(),
            PopupItem::Skill { name, .. } => format!("skill:{} ", name),
            PopupItem::ReplCommand { name, .. } => format!(":{}", name),
            PopupItem::Session { id, .. } => id.clone(),
        }
    }

    /// Kind label for display (e.g., "cmd", "agent", "repl", "session")
    pub fn kind_label(&self) -> &'static str {
        match self {
            PopupItem::Command { .. } => "cmd",
            PopupItem::Agent { .. } => "agent",
            PopupItem::File { .. } => "file",
            PopupItem::Note { .. } => "note",
            PopupItem::Skill { .. } => "skill",
            PopupItem::ReplCommand { .. } => "repl",
            PopupItem::Session { .. } => "session",
        }
    }

    /// Score for sorting/filtering
    pub fn score(&self) -> i32 {
        match self {
            PopupItem::Command { score, .. } => *score,
            PopupItem::Agent { score, .. } => *score,
            PopupItem::File { score, .. } => *score,
            PopupItem::Note { score, .. } => *score,
            PopupItem::Skill { score, .. } => *score,
            PopupItem::ReplCommand { score, .. } => *score,
            PopupItem::Session { score, .. } => *score,
        }
    }

    /// Whether item is available/enabled
    pub fn is_available(&self) -> bool {
        match self {
            PopupItem::Command { available, .. } => *available,
            PopupItem::Agent { available, .. } => *available,
            PopupItem::File { available, .. } => *available,
            PopupItem::Note { available, .. } => *available,
            PopupItem::Skill { available, .. } => *available,
            PopupItem::ReplCommand { .. } | PopupItem::Session { .. } => true, // Always available
        }
    }

    /// Argument hint (Command only)
    pub fn argument_hint(&self) -> Option<&str> {
        match self {
            PopupItem::Command { argument_hint, .. } => argument_hint.as_deref(),
            _ => None,
        }
    }

    /// Skill scope (Skill only)
    pub fn scope(&self) -> Option<&str> {
        match self {
            PopupItem::Skill { scope, .. } => Some(scope),
            _ => None,
        }
    }

    // =========================================================================
    // Compatibility - for code that still uses old field access patterns
    // =========================================================================

    /// Check if this is a Command variant
    pub fn is_command(&self) -> bool {
        matches!(self, PopupItem::Command { .. })
    }

    /// Check if this is an Agent variant
    pub fn is_agent(&self) -> bool {
        matches!(self, PopupItem::Agent { .. })
    }

    /// Check if this is a File variant
    pub fn is_file(&self) -> bool {
        matches!(self, PopupItem::File { .. })
    }

    /// Check if this is a Note variant
    pub fn is_note(&self) -> bool {
        matches!(self, PopupItem::Note { .. })
    }

    /// Check if this is a Skill variant
    pub fn is_skill(&self) -> bool {
        matches!(self, PopupItem::Skill { .. })
    }

    /// Check if this is a ReplCommand variant
    pub fn is_repl_command(&self) -> bool {
        matches!(self, PopupItem::ReplCommand { .. })
    }

    /// Check if this is a Session variant
    pub fn is_session(&self) -> bool {
        matches!(self, PopupItem::Session { .. })
    }
}

/// Legacy type alias for code that still references PopupItemKind
///
/// # Deprecated
/// Use `PopupItem` directly instead.
pub type PopupItemKind = PopupItem;
