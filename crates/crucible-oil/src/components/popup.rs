use crate::focus::FocusContext;
use crate::node::{focusable, overlay_from_bottom, popup, Node, PopupItemNode};

pub const POPUP_MAX_VISIBLE: usize = 10;
pub const FOCUS_POPUP: &str = "popup";

pub struct PopupOverlay {
    items: Vec<PopupItemNode>,
    selected: usize,
    visible: bool,
    offset_from_bottom: usize,
    max_visible: usize,
}

impl PopupOverlay {
    pub fn new(items: Vec<PopupItemNode>) -> Self {
        Self {
            items,
            selected: 0,
            visible: true,
            offset_from_bottom: 3,
            max_visible: POPUP_MAX_VISIBLE,
        }
    }

    #[must_use]
    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[must_use]
    pub fn offset_from_bottom(mut self, offset: usize) -> Self {
        self.offset_from_bottom = offset;
        self
    }

    #[must_use]
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn move_selection_up(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn move_selection_down(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len().saturating_sub(1));
        }
    }

    pub fn move_selection_up_wrap(&mut self) {
        if !self.items.is_empty() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn move_selection_down_wrap(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn selected_item(&self) -> Option<&PopupItemNode> {
        self.items.get(self.selected)
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.selected_item().map(|item| item.label.as_str())
    }

    pub fn view(&self, _focus: &FocusContext) -> Node {
        if !self.visible || self.items.is_empty() {
            return Node::Empty;
        }

        let popup_node = focusable(
            FOCUS_POPUP,
            popup(self.items.clone(), self.selected, self.max_visible),
        );
        overlay_from_bottom(popup_node, self.offset_from_bottom)
    }
}

pub fn popup_item(label: impl Into<String>) -> PopupItemNode {
    PopupItemNode {
        label: label.into(),
        description: None,
        kind: None,
    }
}

pub fn popup_item_with_desc(
    label: impl Into<String>,
    description: impl Into<String>,
) -> PopupItemNode {
    PopupItemNode {
        label: label.into(),
        description: Some(description.into()),
        kind: None,
    }
}

pub fn popup_item_full(
    label: impl Into<String>,
    description: Option<String>,
    kind: Option<String>,
) -> PopupItemNode {
    PopupItemNode {
        label: label.into(),
        description,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_to_plain_text;

    fn sample_items() -> Vec<PopupItemNode> {
        vec![
            popup_item("item1"),
            popup_item("item2"),
            popup_item("item3"),
        ]
    }

    #[test]
    fn empty_popup_returns_empty_node() {
        let overlay = PopupOverlay::new(vec![]);
        let focus = FocusContext::new();
        let node = overlay.view(&focus);
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn hidden_popup_returns_empty_node() {
        let overlay = PopupOverlay::new(sample_items()).visible(false);
        let focus = FocusContext::new();
        let node = overlay.view(&focus);
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn visible_popup_renders_items() {
        let overlay = PopupOverlay::new(sample_items());
        let focus = FocusContext::new();
        let node = overlay.view(&focus);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("item1"));
        assert!(plain.contains("item2"));
        assert!(plain.contains("item3"));
    }

    #[test]
    fn selection_navigation() {
        let mut overlay = PopupOverlay::new(sample_items());

        assert_eq!(overlay.selected, 0);
        assert_eq!(overlay.selected_label(), Some("item1"));

        overlay.move_selection_down();
        assert_eq!(overlay.selected, 1);
        assert_eq!(overlay.selected_label(), Some("item2"));

        overlay.move_selection_down();
        assert_eq!(overlay.selected, 2);
        assert_eq!(overlay.selected_label(), Some("item3"));

        overlay.move_selection_down();
        assert_eq!(overlay.selected, 2);

        overlay.move_selection_up();
        assert_eq!(overlay.selected, 1);

        overlay.move_selection_up();
        overlay.move_selection_up();
        assert_eq!(overlay.selected, 0);
    }

    #[test]
    fn selection_navigation_wrap() {
        let mut overlay = PopupOverlay::new(sample_items());

        assert_eq!(overlay.selected, 0);

        overlay.move_selection_up_wrap();
        assert_eq!(overlay.selected, 2);

        overlay.move_selection_up_wrap();
        assert_eq!(overlay.selected, 1);

        overlay.move_selection_down_wrap();
        assert_eq!(overlay.selected, 2);

        overlay.move_selection_down_wrap();
        assert_eq!(overlay.selected, 0);
    }

    #[test]
    fn popup_item_constructors() {
        let simple = popup_item("test");
        assert_eq!(simple.label, "test");
        assert!(simple.description.is_none());
        assert!(simple.kind.is_none());

        let with_desc = popup_item_with_desc("test", "description");
        assert_eq!(with_desc.label, "test");
        assert_eq!(with_desc.description, Some("description".to_string()));
        assert!(with_desc.kind.is_none());

        let full = popup_item_full("test", Some("desc".to_string()), Some("kind".to_string()));
        assert_eq!(full.label, "test");
        assert_eq!(full.description, Some("desc".to_string()));
        assert_eq!(full.kind, Some("kind".to_string()));
    }

    #[test]
    fn len_and_is_empty() {
        let empty = PopupOverlay::new(vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let with_items = PopupOverlay::new(sample_items());
        assert!(!with_items.is_empty());
        assert_eq!(with_items.len(), 3);
    }

    #[test]
    fn builder_methods() {
        let overlay = PopupOverlay::new(sample_items())
            .selected(1)
            .visible(true)
            .offset_from_bottom(5)
            .max_visible(8);

        assert_eq!(overlay.selected, 1);
        assert!(overlay.visible);
        assert_eq!(overlay.offset_from_bottom, 5);
        assert_eq!(overlay.max_visible, 8);
    }
}
