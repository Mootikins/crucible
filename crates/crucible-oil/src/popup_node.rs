//! Popup menu node types and builders (split from `node.rs`).

use crate::node::Node;
use crate::style::{Color, Style};

/// Default popup background color (dark blue-gray).
pub const DEFAULT_POPUP_BG: Color = Color::Rgb(40, 44, 52);

/// Default popup selected-item background color (lighter blue-gray).
pub const DEFAULT_POPUP_SELECTED_BG: Color = Color::Rgb(50, 56, 68);

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PopupNode {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub items: Vec<PopupItemNode>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub selected: usize,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub viewport_offset: usize,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub max_visible: usize,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub bg_style: Style,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub selected_style: Style,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub unselected_style: Style,
    /// Minimal (nvim-pmenu-style) mode: anchor the popup at this column and
    /// size it to its content instead of painting full-width rows.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub anchor_col: Option<u16>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PopupItemNode {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub label: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub description: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "crate::is_default"))]
    pub kind: Option<String>,
}

pub fn popup(items: Vec<PopupItemNode>, selected: usize, max_visible: usize) -> Node {
    Node::Popup(PopupNode::new(items, selected, max_visible))
}

pub fn popup_item(label: impl Into<String>) -> PopupItemNode {
    PopupItemNode {
        label: label.into(),
        description: None,
        kind: None,
    }
}

impl PopupNode {
    /// Create a new popup builder with items, selected index, and max visible.
    pub fn new(items: Vec<PopupItemNode>, selected: usize, max_visible: usize) -> Self {
        let viewport_offset = if selected >= max_visible {
            selected.saturating_sub(max_visible.saturating_sub(1))
        } else {
            0
        };
        PopupNode {
            items,
            selected,
            viewport_offset,
            max_visible,
            bg_style: Style::new().bg(DEFAULT_POPUP_BG),
            selected_style: Style::new().bg(DEFAULT_POPUP_SELECTED_BG),
            unselected_style: Style::new().bg(DEFAULT_POPUP_BG),
            anchor_col: None,
        }
    }

    /// Switch to the minimal anchored (nvim-pmenu-style) presentation: the
    /// popup is sized to its longest visible item and starts at `col`, so a
    /// 1-cell pad puts item labels at `col + 1` — aligned with the word being
    /// completed when `col` is the trigger character's display column.
    #[must_use]
    pub fn anchored(mut self, col: u16) -> Self {
        self.anchor_col = Some(col);
        self
    }

    /// Set background color.
    pub fn bg_color(mut self, color: Color) -> Self {
        let bg_style = Style::new().bg(color);
        self.bg_style = bg_style;
        self.unselected_style = bg_style;
        self
    }

    /// Set selected item background color.
    pub fn selected_color(mut self, color: Color) -> Self {
        self.selected_style = Style::new().bg(color);
        self
    }

    /// Set all styles at once.
    pub fn styles(
        mut self,
        bg_style: Style,
        selected_style: Style,
        unselected_style: Style,
    ) -> Self {
        self.bg_style = bg_style;
        self.selected_style = selected_style;
        self.unselected_style = unselected_style;
        self
    }
}

impl PopupItemNode {
    pub fn desc(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}
