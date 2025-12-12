//! List types for ordered and unordered lists with nesting support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// List block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBlock {
    /// List type (ordered or unordered)
    pub list_type: ListType,

    /// List items
    pub items: Vec<ListItem>,

    /// Character offset in source
    pub offset: usize,

    /// Total item count (including nested items)
    pub item_count: usize,

    /// Maximum depth of nesting (0 = flat list)
    pub max_depth: usize,

    /// Primary marker style used in this list
    pub marker_style: ListMarkerStyle,

    /// Whether this list contains any task items
    pub has_tasks: bool,

    /// Whether this list is tightly packed (no blank lines between items)
    pub is_tight: bool,

    /// Raw markdown content of the entire list
    pub raw_content: String,

    /// Number of top-level items (excluding nested items)
    pub top_level_count: usize,
}

impl ListBlock {
    /// Create a new list block
    pub fn new(list_type: ListType, offset: usize) -> Self {
        Self {
            list_type,
            items: Vec::new(),
            offset,
            item_count: 0,
            max_depth: 0,
            marker_style: ListMarkerStyle::default_for_type(list_type),
            has_tasks: false,
            is_tight: true, // Default to tight, will be updated during parsing
            raw_content: String::new(),
            top_level_count: 0,
        }
    }

    /// Create a new list block with marker style
    pub fn with_marker_style(
        list_type: ListType,
        marker_style: ListMarkerStyle,
        offset: usize,
    ) -> Self {
        Self {
            list_type,
            items: Vec::new(),
            offset,
            item_count: 0,
            max_depth: 0,
            marker_style,
            has_tasks: false,
            is_tight: true,
            raw_content: String::new(),
            top_level_count: 0,
        }
    }

    /// Add an item to the list
    pub fn add_item(&mut self, item: ListItem) {
        // Update depth information
        self.max_depth = self.max_depth.max(item.level);

        // Update task status
        if item.task_status.is_some() {
            self.has_tasks = true;
        }

        // Update counts
        self.item_count += 1;
        if item.level == 0 {
            self.top_level_count += 1;
        }

        self.items.push(item);
    }

    /// Get items at a specific nesting level
    pub fn items_at_level(&self, level: usize) -> Vec<&ListItem> {
        self.items
            .iter()
            .filter(|item| item.level == level)
            .collect()
    }

    /// Get nested items under a parent item (by index)
    pub fn nested_items(&self, parent_index: usize) -> Vec<&ListItem> {
        if parent_index >= self.items.len() {
            return Vec::new();
        }

        let parent_level = self.items[parent_index].level;
        let next_level = parent_level + 1;

        self.items
            .iter()
            .skip(parent_index + 1)
            .take_while(|item| item.level >= next_level)
            .filter(|item| item.level == next_level)
            .collect()
    }

    /// Get statistics about the list structure
    pub fn stats(&self) -> ListStats {
        let level_counts = self.items.iter().fold(HashMap::new(), |mut counts, item| {
            *counts.entry(item.level).or_insert(0) += 1;
            counts
        });

        ListStats {
            total_items: self.item_count,
            top_level_items: self.top_level_count,
            max_depth: self.max_depth,
            has_tasks: self.has_tasks,
            is_tight: self.is_tight,
            level_counts,
        }
    }
}

/// List type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListType {
    /// Unordered list (-, *, +)
    Unordered,
    /// Ordered list (1., 2., etc.)
    Ordered,
}

impl ListType {
    /// Check if this list type is ordered
    pub fn is_ordered(&self) -> bool {
        match self {
            ListType::Ordered => true,
            ListType::Unordered => false,
        }
    }
}

/// List marker style for different list types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ListMarkerStyle {
    /// Unordered markers
    Dash, // -
    Asterisk, // *
    Plus,     // +

    /// Ordered markers (Arabic numerals only - standard markdown)
    Arabic, // 1., 2., 3.
}

impl ListMarkerStyle {
    /// Get default marker style for list type
    pub fn default_for_type(list_type: ListType) -> Self {
        match list_type {
            ListType::Unordered => ListMarkerStyle::Dash,
            ListType::Ordered => ListMarkerStyle::Arabic,
        }
    }

    /// Check if this style supports ordered lists
    pub fn is_ordered(&self) -> bool {
        match self {
            ListMarkerStyle::Dash | ListMarkerStyle::Asterisk | ListMarkerStyle::Plus => false,
            _ => true,
        }
    }

    /// Check if this style supports unordered lists
    pub fn is_unordered(&self) -> bool {
        !self.is_ordered()
    }

    /// Get the marker pattern for regex matching
    pub fn pattern(&self) -> &'static str {
        match self {
            ListMarkerStyle::Dash => r"-",
            ListMarkerStyle::Asterisk => r"\*",
            ListMarkerStyle::Plus => r"\+",
            ListMarkerStyle::Arabic => r"\d+\.",
        }
    }
}

/// List item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListItem {
    /// Item text content
    pub content: String,

    /// Item level (for nested lists)
    pub level: usize,

    /// Task status (for task lists)
    pub task_status: Option<TaskStatus>,

    /// The specific marker used for this item
    pub marker: ListMarkerStyle,

    /// The raw marker text (e.g., "1.", "-")
    pub marker_text: String,

    /// The numbered/lettered value for ordered items (None for unordered)
    pub sequence_number: Option<String>,

    /// Character offset in source for this specific item
    pub offset: usize,

    /// Indentation level in spaces (for proper rendering)
    pub indent_spaces: usize,

    /// Whether this item has nested items
    pub has_nested: bool,
}

impl ListItem {
    /// Create a new list item with basic information
    pub fn new(content: String, level: usize) -> Self {
        Self {
            content,
            level,
            task_status: None,
            marker: ListMarkerStyle::default_for_type(ListType::Unordered),
            marker_text: "-".to_string(),
            sequence_number: None,
            offset: 0,
            indent_spaces: level * 2, // Default 2 spaces per level
            has_nested: false,
        }
    }

    /// Create a list item with full metadata
    pub fn with_metadata(
        content: String,
        level: usize,
        marker: ListMarkerStyle,
        marker_text: String,
        sequence_number: Option<String>,
        offset: usize,
        indent_spaces: usize,
    ) -> Self {
        Self {
            content,
            level,
            task_status: None,
            marker,
            marker_text,
            sequence_number,
            offset,
            indent_spaces,
            has_nested: false,
        }
    }

    /// Create a task list item
    pub fn new_task(content: String, level: usize, completed: bool) -> Self {
        Self {
            content,
            level,
            task_status: Some(if completed {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            }),
            marker: ListMarkerStyle::default_for_type(ListType::Unordered),
            marker_text: "-".to_string(),
            sequence_number: None,
            offset: 0,
            indent_spaces: level * 2,
            has_nested: false,
        }
    }

    /// Create a task list item with full metadata
    pub fn new_task_with_metadata(
        content: String,
        level: usize,
        completed: bool,
        marker: ListMarkerStyle,
        marker_text: String,
        sequence_number: Option<String>,
        offset: usize,
        indent_spaces: usize,
    ) -> Self {
        Self {
            content,
            level,
            task_status: Some(if completed {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            }),
            marker,
            marker_text,
            sequence_number,
            offset,
            indent_spaces,
            has_nested: false,
        }
    }

    /// Set the nested flag for this item
    pub fn set_nested(&mut self, has_nested: bool) {
        self.has_nested = has_nested;
    }

    /// Check if this item is ordered
    pub fn is_ordered(&self) -> bool {
        self.marker.is_ordered()
    }

    /// Check if this item is unordered
    pub fn is_unordered(&self) -> bool {
        self.marker.is_unordered()
    }

    /// Get the effective indentation (including marker width)
    pub fn effective_indent(&self) -> usize {
        let marker_width = self.marker_text.len();
        self.indent_spaces + marker_width
    }

    /// Extract the content without task checkbox
    pub fn content_without_task(&self) -> String {
        if self.task_status.is_some() {
            // Remove task checkbox patterns like "[x] " or "[ ] "
            let content = self.content.trim();
            if let Some(remaining) = content
                .strip_prefix("[x] ")
                .or_else(|| content.strip_prefix("[ ] "))
            {
                remaining.trim().to_string()
            } else {
                content.to_string()
            }
        } else {
            self.content.clone()
        }
    }
}

/// Statistics about a list's structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListStats {
    /// Total number of items including nested
    pub total_items: usize,
    /// Number of top-level items
    pub top_level_items: usize,
    /// Maximum nesting depth
    pub max_depth: usize,
    /// Whether the list contains task items
    pub has_tasks: bool,
    /// Whether the list is tightly packed
    pub is_tight: bool,
    /// Count of items at each level
    pub level_counts: HashMap<usize, usize>,
}

/// Task status for task list items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is completed ([x])
    Completed,
    /// Task is pending ([ ])
    Pending,
}
