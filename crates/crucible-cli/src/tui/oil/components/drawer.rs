use crate::tui::oil::component::Component;
use crucible_oil::node::Node;
use crate::tui::oil::ViewContext;
#[allow(unused_imports)] // WIP: DrawerKind not yet used
use crucible_oil::components::{Drawer as OilDrawer, DrawerKind};

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
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn drawer_renders_items() {
        let drawer = Drawer::new(DrawerKind::Messages).width(60).items(vec![
            ("14:30:12".to_string(), "INFO Session saved".to_string()),
            ("14:31:00".to_string(), "WARN Low memory".to_string()),
        ]);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("14:30:12"));
        assert!(plain.contains("INFO Session saved"));
        assert!(plain.contains("14:31:00"));
        assert!(plain.contains("WARN Low memory"));
    }

    #[test]
    fn drawer_has_borders() {
        let drawer = Drawer::new(DrawerKind::Messages).width(40);
        let plain = render_to_plain_text(&drawer.view(), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
    }

    #[test]
    fn drawer_has_footer_badge() {
        let drawer = Drawer::new(DrawerKind::Messages).width(60);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("MESSAGES"));
        assert!(plain.contains("ESC/q"));
        assert!(plain.contains("close"));
    }

    #[test]
    fn drawer_limits_items() {
        let items: Vec<(String, String)> = (0..20)
            .map(|i| (format!("label{}", i), format!("content{}", i)))
            .collect();
        let drawer = Drawer::new(DrawerKind::Messages)
            .width(60)
            .max_items(3)
            .items(items);
        let plain = render_to_plain_text(&drawer.view(), 60);
        assert!(plain.contains("label0"));
        assert!(plain.contains("label2"));
        assert!(!plain.contains("label3"));
    }

    #[test]
    fn drawer_empty_items() {
        let drawer = Drawer::new(DrawerKind::Messages).width(40);
        let plain = render_to_plain_text(&drawer.view(), 40);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
        assert!(plain.contains("MESSAGES"));
    }
}
