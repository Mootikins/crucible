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
    /// Model picker: `:model` command
    Model,
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
    /// Model/backend for switching: displayed in :model popup
    Model {
        /// Backend spec (e.g., "ollama/llama3.2", "acp/opencode")
        spec: String,
        /// Human-readable description
        description: String,
        /// Whether this is currently selected
        current: bool,
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

    /// Create a new model popup item for backend switching
    pub fn model(spec: impl Into<String>) -> Self {
        PopupItem::Model {
            spec: spec.into(),
            description: String::new(),
            current: false,
            score: 0,
        }
    }

    // =========================================================================
    // Builder methods - chain after constructor
    // =========================================================================

    /// Builder: set description (for Command, Agent, Skill, ReplCommand, Session, Model)
    pub fn desc(mut self, description: impl Into<String>) -> Self {
        let d = description.into();
        match &mut self {
            PopupItem::Command { description, .. } => *description = d,
            PopupItem::Agent { description, .. } => *description = d,
            PopupItem::Skill { description, .. } => *description = d,
            PopupItem::ReplCommand { description, .. } => *description = d,
            PopupItem::Session { description, .. } => *description = d,
            PopupItem::Model { description, .. } => *description = d,
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
            PopupItem::Model { score, .. } => *score = s,
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
            PopupItem::ReplCommand { .. } | PopupItem::Session { .. } | PopupItem::Model { .. } => {
            } // Always available
        }
        self
    }

    /// Builder: set current flag (Model only)
    pub fn with_current(mut self, c: bool) -> Self {
        if let PopupItem::Model { current, .. } = &mut self {
            *current = c;
        }
        self
    }

    // =========================================================================
    // Accessors - uniform interface across variants
    // =========================================================================

    /// Display title (e.g., "/search", "@agent", ":quit", "src/main.rs", "ollama/llama3.2")
    pub fn title(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{}", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => format!("note:{}", path),
            PopupItem::Skill { name, .. } => format!("skill:{}", name),
            PopupItem::ReplCommand { name, .. } => format!(":{}", name),
            PopupItem::Session { id, .. } => id.clone(),
            PopupItem::Model { spec, current, .. } => {
                if *current {
                    format!("{} (current)", spec)
                } else {
                    spec.clone()
                }
            }
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
            PopupItem::Model { description, .. } => description,
        }
    }

    /// Token to insert when selected (for model, returns the spec for switching)
    pub fn token(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{} ", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => path.clone(),
            PopupItem::Skill { name, .. } => format!("skill:{} ", name),
            PopupItem::ReplCommand { name, .. } => format!(":{}", name),
            PopupItem::Session { id, .. } => id.clone(),
            PopupItem::Model { spec, .. } => spec.clone(),
        }
    }

    /// Kind label for display (e.g., "cmd", "agent", "repl", "session", "model")
    pub fn kind_label(&self) -> &'static str {
        match self {
            PopupItem::Command { .. } => "cmd",
            PopupItem::Agent { .. } => "agent",
            PopupItem::File { .. } => "file",
            PopupItem::Note { .. } => "note",
            PopupItem::Skill { .. } => "skill",
            PopupItem::ReplCommand { .. } => "repl",
            PopupItem::Session { .. } => "session",
            PopupItem::Model { .. } => "model",
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
            PopupItem::Model { score, .. } => *score,
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
            PopupItem::ReplCommand { .. } | PopupItem::Session { .. } | PopupItem::Model { .. } => {
                true
            } // Always available
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

    /// Check if this is a Model variant
    pub fn is_model(&self) -> bool {
        matches!(self, PopupItem::Model { .. })
    }
}

/// Legacy type alias for code that still references PopupItemKind
///
/// # Deprecated
/// Use `PopupItem` directly instead.
pub type PopupItemKind = PopupItem;

/// Implement the generic popup widget trait directly on PopupItem
///
/// This allows PopupItem to work seamlessly with the generic Popup<T> widget
/// without requiring a wrapper.
impl crate::tui::widgets::PopupItem for PopupItem {
    fn match_text(&self) -> &str {
        // For matching, use the identifier directly
        match self {
            PopupItem::Command { name, .. } => name,
            PopupItem::Agent { id, .. } => id,
            PopupItem::File { path, .. } => path,
            PopupItem::Note { path, .. } => path,
            PopupItem::Skill { name, .. } => name,
            PopupItem::ReplCommand { name, .. } => name,
            PopupItem::Session { id, .. } => id,
            PopupItem::Model { spec, .. } => spec,
        }
    }

    fn label(&self) -> &str {
        // For label, return match_text
        self.match_text()
    }

    fn description(&self) -> Option<&str> {
        let subtitle = self.subtitle();
        if subtitle.is_empty() {
            None
        } else {
            Some(subtitle)
        }
    }

    fn kind_label(&self) -> Option<&str> {
        // Call the enum's own kind_label() method
        match self {
            PopupItem::Command { .. } => Some("cmd"),
            PopupItem::Agent { .. } => Some("agent"),
            PopupItem::File { .. } => Some("file"),
            PopupItem::Note { .. } => Some("note"),
            PopupItem::Skill { .. } => Some("skill"),
            PopupItem::ReplCommand { .. } => Some("repl"),
            PopupItem::Session { .. } => Some("session"),
            PopupItem::Model { .. } => Some("model"),
        }
    }

    fn icon(&self) -> Option<char> {
        // Don't show prefix icons - the trigger char already indicates type
        None
    }

    fn is_enabled(&self) -> bool {
        self.is_available()
    }

    fn token(&self) -> &str {
        // token() method allocates, but we need &str - use match_text
        // This is a known limitation
        self.match_text()
    }
}

// =============================================================================
// From implementation for PopupEntry
// =============================================================================

impl From<PopupItem> for crucible_core::types::PopupEntry {
    fn from(item: PopupItem) -> Self {
        use serde_json::json;

        let (label, description, kind) = match &item {
            PopupItem::Command {
                name, description, ..
            } => (format!("/{}", name), Some(description.clone()), "command"),
            PopupItem::Agent {
                id, description, ..
            } => (format!("@{}", id), Some(description.clone()), "agent"),
            PopupItem::File { path, .. } => (path.clone(), None, "file"),
            PopupItem::Note { path, .. } => (path.clone(), None, "note"),
            PopupItem::Skill {
                name,
                description,
                scope,
                ..
            } => {
                let desc = format!("{} ({})", description, scope);
                (format!("skill:{}", name), Some(desc), "skill")
            }
            PopupItem::ReplCommand {
                name, description, ..
            } => (format!(":{}", name), Some(description.clone()), "repl"),
            PopupItem::Session {
                id,
                description,
                message_count,
                ..
            } => {
                let desc = if description.is_empty() {
                    format!("{} messages", message_count)
                } else {
                    format!("{} ({} messages)", description, message_count)
                };
                (id.clone(), Some(desc), "session")
            }
            PopupItem::Model {
                spec,
                description,
                current,
                ..
            } => {
                let desc = if *current {
                    format!("{} (current)", description)
                } else {
                    description.clone()
                };
                (spec.clone(), Some(desc), "model")
            }
        };

        let mut entry = crucible_core::types::PopupEntry::new(label);
        if let Some(desc) = description {
            if !desc.is_empty() {
                entry = entry.with_description(desc);
            }
        }
        entry.with_data(json!({ "kind": kind }))
    }
}
