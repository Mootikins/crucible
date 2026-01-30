use crate::tui::oil::component::Component;
use crate::tui::oil::components::PopupOverlay;
use crate::tui::oil::node::Node;
use crate::tui::oil::ViewContext;
use crucible_oil::node::PopupItemNode;

/// View-only component that encapsulates popup overlay rendering.
///
/// All popup state (kind, filter, trigger position, etc.) remains on `OilChatApp`.
/// This component receives pre-computed props and renders the overlay.
pub struct PopupComponent {
    pub visible: bool,
    pub items: Vec<PopupItemNode>,
    pub selected: usize,
    pub input_height: usize,
    pub max_visible: usize,
}

impl PopupComponent {
    pub fn new(items: Vec<PopupItemNode>) -> Self {
        Self {
            visible: true,
            items,
            selected: 0,
            input_height: 3,
            max_visible: 10,
        }
    }

    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn input_height(mut self, height: usize) -> Self {
        self.input_height = height;
        self
    }

    #[must_use]
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max;
        self
    }
}

impl Component for PopupComponent {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        if !self.visible || self.items.is_empty() {
            return Node::Empty;
        }

        let status_height = 1;
        let offset_from_bottom = self.input_height + status_height;

        PopupOverlay::new(self.items.clone())
            .selected(self.selected)
            .visible(true)
            .offset_from_bottom(offset_from_bottom)
            .max_visible(self.max_visible)
            .view(_ctx.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::components::popup_item;
    use crate::tui::oil::render::render_to_plain_text;

    fn sample_items() -> Vec<PopupItemNode> {
        vec![popup_item("alpha"), popup_item("beta"), popup_item("gamma")]
    }

    #[test]
    fn popup_hidden_renders_empty() {
        let popup = PopupComponent::new(vec![]).visible(false);
        let mut h = ComponentHarness::new(80, 24);
        h.render_component(&popup);
        assert_eq!(h.viewport().trim(), "");
    }

    #[test]
    fn popup_visible_false_renders_empty() {
        let popup = PopupComponent::new(sample_items()).visible(false);
        let mut h = ComponentHarness::new(80, 24);
        h.render_component(&popup);
        assert_eq!(h.viewport().trim(), "");
    }

    #[test]
    fn popup_empty_items_renders_empty() {
        let popup = PopupComponent::new(vec![]).visible(true);
        let mut h = ComponentHarness::new(80, 24);
        h.render_component(&popup);
        assert_eq!(h.viewport().trim(), "");
    }

    #[test]
    fn popup_visible_with_items_renders_content() {
        let popup = PopupComponent::new(sample_items())
            .visible(true)
            .selected(0)
            .input_height(3);
        let h = ComponentHarness::new(80, 24);
        let node = popup.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("alpha"));
        assert!(plain.contains("beta"));
        assert!(plain.contains("gamma"));
    }

    #[test]
    fn popup_selection_index_respected() {
        let popup = PopupComponent::new(sample_items())
            .visible(true)
            .selected(1)
            .input_height(3);
        let h = ComponentHarness::new(80, 24);
        let node = popup.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("alpha"));
        assert!(plain.contains("beta"));
        assert!(plain.contains("gamma"));
    }

    #[test]
    fn popup_with_descriptions() {
        use crate::tui::oil::components::popup_item_with_desc;
        let items = vec![
            popup_item_with_desc("cmd1", "First command"),
            popup_item_with_desc("cmd2", "Second command"),
        ];
        let popup = PopupComponent::new(items)
            .visible(true)
            .selected(0)
            .input_height(3);
        let h = ComponentHarness::new(80, 24);
        let node = popup.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("cmd1"));
        assert!(plain.contains("cmd2"));
    }
}
