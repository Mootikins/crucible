//! Core types for zellij-inbox

use serde::{Deserialize, Serialize};

/// Status of an inbox item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Waiting for user input - renders as [ ]
    Waiting,
    /// Working in background - renders as [/]
    Working,
}

impl Status {
    /// Parse from checkbox character
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            ' ' => Some(Status::Waiting),
            '/' => Some(Status::Working),
            _ => None,
        }
    }

    /// Convert to checkbox character
    pub fn to_char(self) -> char {
        match self {
            Status::Waiting => ' ',
            Status::Working => '/',
        }
    }

    /// Get section name for this status
    pub fn section_name(self) -> &'static str {
        match self {
            Status::Waiting => "Waiting for Input",
            Status::Working => "Background",
        }
    }
}

/// An item in the inbox
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    /// The display text (e.g., "claude-code: Auth question")
    pub text: String,
    /// Zellij pane ID (unique key)
    pub pane_id: u32,
    /// Project name (for grouping)
    pub project: String,
    /// Current status
    pub status: Status,
}

/// The complete inbox state
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inbox {
    pub items: Vec<InboxItem>,
}

impl Inbox {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add or update an item (pane_id is the key)
    pub fn upsert(&mut self, item: InboxItem) {
        if let Some(existing) = self.items.iter_mut().find(|i| i.pane_id == item.pane_id) {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }

    /// Remove an item by pane ID
    pub fn remove(&mut self, pane_id: u32) -> bool {
        let len_before = self.items.len();
        self.items.retain(|i| i.pane_id != pane_id);
        self.items.len() < len_before
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
