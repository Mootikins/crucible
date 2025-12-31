//! PopupItem adapters for existing types
//!
//! This module provides [`PopupItem`] implementations for types used in the
//! existing popup system, enabling a smooth migration to the generic popup.

use super::PopupItem;
use crucible_core::traits::chat::CommandDescriptor;

// ─────────────────────────────────────────────────────────────────────────────
// Command Item
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for slash commands in popups
#[derive(Clone, Debug)]
pub struct CommandItem {
    /// Command name (without leading /)
    pub name: String,
    /// Command description
    pub description: String,
    /// Input hint (e.g., "<query>")
    pub input_hint: Option<String>,
    /// Full token to insert (e.g., "/help ")
    pub token: String,
}

impl CommandItem {
    /// Create from a CommandDescriptor
    pub fn from_descriptor(cmd: &CommandDescriptor) -> Self {
        Self {
            name: cmd.name.clone(),
            description: cmd.description.clone(),
            input_hint: cmd.input_hint.clone(),
            token: format!("/{} ", cmd.name),
        }
    }

    /// Create a new command item
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        let name = name.into();
        let token = format!("/{} ", name);
        Self {
            name,
            description: description.into(),
            input_hint: None,
            token,
        }
    }

    /// Set input hint
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.input_hint = Some(hint.into());
        self
    }
}

impl PopupItem for CommandItem {
    fn match_text(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some(&self.description)
    }

    fn kind_label(&self) -> Option<&str> {
        Some("cmd")
    }

    fn icon(&self) -> Option<char> {
        Some('/')
    }

    fn token(&self) -> &str {
        &self.token
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent Item
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for agents in popups
#[derive(Clone, Debug)]
pub struct AgentItem {
    /// Agent ID/slug
    pub id: String,
    /// Agent description
    pub description: String,
    /// Whether the agent is available
    pub available: bool,
}

impl AgentItem {
    /// Create a new agent item
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            available: true,
        }
    }

    /// Mark as unavailable
    pub fn unavailable(mut self) -> Self {
        self.available = false;
        self
    }
}

impl PopupItem for AgentItem {
    fn match_text(&self) -> &str {
        &self.id
    }

    fn label(&self) -> &str {
        &self.id
    }

    fn description(&self) -> Option<&str> {
        Some(&self.description)
    }

    fn kind_label(&self) -> Option<&str> {
        Some("agent")
    }

    fn icon(&self) -> Option<char> {
        Some('@')
    }

    fn is_enabled(&self) -> bool {
        self.available
    }

    fn token(&self) -> &str {
        &self.id
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// File Item
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for files in popups
#[derive(Clone, Debug)]
pub struct FileItem {
    /// File path (relative to workspace)
    pub path: String,
    /// Optional source label (e.g., "workspace", "kiln")
    pub source: Option<String>,
}

impl FileItem {
    /// Create a new file item
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: None,
        }
    }

    /// Set the source label
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

impl PopupItem for FileItem {
    fn match_text(&self) -> &str {
        &self.path
    }

    fn label(&self) -> &str {
        &self.path
    }

    fn description(&self) -> Option<&str> {
        self.source.as_deref()
    }

    fn kind_label(&self) -> Option<&str> {
        Some("file")
    }

    fn icon(&self) -> Option<char> {
        Some('📄')
    }

    fn token(&self) -> &str {
        &self.path
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Note Item
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for notes in popups
#[derive(Clone, Debug)]
pub struct NoteItem {
    /// Note path or identifier
    pub path: String,
    /// Kiln name (if from a specific kiln)
    pub kiln: Option<String>,
}

impl NoteItem {
    /// Create a new note item
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kiln: None,
        }
    }

    /// Set the kiln name
    pub fn with_kiln(mut self, kiln: impl Into<String>) -> Self {
        self.kiln = Some(kiln.into());
        self
    }
}

impl PopupItem for NoteItem {
    fn match_text(&self) -> &str {
        &self.path
    }

    fn label(&self) -> &str {
        &self.path
    }

    fn description(&self) -> Option<&str> {
        self.kiln.as_deref()
    }

    fn kind_label(&self) -> Option<&str> {
        Some("note")
    }

    fn icon(&self) -> Option<char> {
        Some('📝')
    }

    fn token(&self) -> &str {
        &self.path
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Skill Item
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for skills in popups
#[derive(Clone, Debug)]
pub struct SkillItem {
    /// Skill name
    pub name: String,
    /// Skill description
    pub description: String,
    /// Skill scope (e.g., "user", "project", "plugin")
    pub scope: String,
}

impl SkillItem {
    /// Create a new skill item
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            scope: scope.into(),
        }
    }
}

impl PopupItem for SkillItem {
    fn match_text(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some(&self.description)
    }

    fn kind_label(&self) -> Option<&str> {
        Some("skill")
    }

    fn icon(&self) -> Option<char> {
        Some('⚡')
    }

    fn token(&self) -> &str {
        &self.name
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Choice Item (for AskRequest)
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for choices in ask popups
#[derive(Clone, Debug)]
pub struct ChoiceItem {
    /// Index in the original choices list
    pub index: usize,
    /// Choice text
    pub text: String,
    /// Whether this choice is enabled
    pub enabled: bool,
}

impl ChoiceItem {
    /// Create a new choice item
    pub fn new(index: usize, text: impl Into<String>) -> Self {
        Self {
            index,
            text: text.into(),
            enabled: true,
        }
    }

    /// Mark as disabled
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

impl PopupItem for ChoiceItem {
    fn match_text(&self) -> &str {
        &self.text
    }

    fn label(&self) -> &str {
        &self.text
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn token(&self) -> &str {
        &self.text
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::Popup;

    #[test]
    fn command_item_from_descriptor() {
        let desc = CommandDescriptor {
            name: "help".to_string(),
            description: "Show help".to_string(),
            input_hint: Some("<command>".to_string()),
            secondary_options: vec![],
        };

        let item = CommandItem::from_descriptor(&desc);
        assert_eq!(item.match_text(), "help");
        assert_eq!(item.token(), "/help ");
        assert_eq!(item.kind_label(), Some("cmd"));
    }

    #[test]
    fn command_popup_works() {
        let commands = vec![
            CommandItem::new("help", "Show help"),
            CommandItem::new("quit", "Exit"),
            CommandItem::new("search", "Search files"),
        ];

        let mut popup = Popup::new(commands);
        assert_eq!(popup.filtered_count(), 3);

        popup.set_query("he");
        // Should match "help"
        let selected = popup.selected_item().unwrap();
        assert_eq!(selected.name, "help");
    }

    #[test]
    fn agent_item_basic() {
        let item = AgentItem::new("claude", "Claude AI assistant");
        assert_eq!(item.match_text(), "claude");
        assert_eq!(item.kind_label(), Some("agent"));
        assert_eq!(item.icon(), Some('@'));
        assert!(item.is_enabled());
    }

    #[test]
    fn unavailable_agent_is_disabled() {
        let item = AgentItem::new("offline", "Offline agent").unavailable();
        assert!(!item.is_enabled());
    }

    #[test]
    fn file_item_with_source() {
        let item = FileItem::new("src/main.rs").with_source("workspace");
        assert_eq!(item.label(), "src/main.rs");
        assert_eq!(item.description(), Some("workspace"));
    }

    #[test]
    fn skill_item_basic() {
        let item = SkillItem::new("commit", "Create git commit", "user");
        assert_eq!(item.kind_label(), Some("skill"));
        assert_eq!(item.icon(), Some('⚡'));
    }

    #[test]
    fn choice_item_for_ask() {
        let choices = vec![
            ChoiceItem::new(0, "Option A"),
            ChoiceItem::new(1, "Option B"),
            ChoiceItem::new(2, "Option C").disabled(),
        ];

        let popup = Popup::new(choices);
        assert_eq!(popup.filtered_count(), 3);

        // Third choice is disabled
        let visible = popup.visible_items();
        assert!(visible[0].1.is_enabled());
        assert!(visible[1].1.is_enabled());
        assert!(!visible[2].1.is_enabled());
    }
}
