use crate::tui::oil::component::Component;
use crate::tui::oil::ViewContext;
use crucible_oil::components::Drawer as OilDrawer;
use crucible_oil::node::Node;

/// Type alias for Oil's Drawer
pub type Drawer = OilDrawer;

impl Component for Drawer {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        OilDrawer::view(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::{components::DrawerKind, focus::FocusContext, render::render_to_plain_text};

    #[test]
    fn component_adapter_preserves_the_oil_drawer() {
        let drawer = Drawer::new(DrawerKind::Messages)
            .width(60)
            .items(vec![("14:30".into(), "Session saved".into())]);
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let actual = render_to_plain_text(&Component::view(&drawer, &ctx), 60);
        assert_eq!(actual, render_to_plain_text(&OilDrawer::view(&drawer), 60));
        assert!(actual.contains("Session saved"));
    }
}
