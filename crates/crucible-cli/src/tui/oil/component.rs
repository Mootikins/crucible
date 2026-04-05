use crate::tui::oil::app::ViewContext;
use crucible_oil::focus::FocusContext;
use crucible_oil::node::Node;
use crucible_oil::planning::{FramePlanner, FrameSnapshot};

pub trait Component {
    fn view(&self, ctx: &ViewContext<'_>) -> Node;
}

impl<F> Component for F
where
    F: Fn(&ViewContext<'_>) -> Node,
{
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        (self)(ctx)
    }
}

pub struct ComponentHarness {
    focus: FocusContext,
    planner: FramePlanner,
    last_snapshot: Option<FrameSnapshot>,
}

impl ComponentHarness {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            focus: FocusContext::new(),
            planner: FramePlanner::new(width, height),
            last_snapshot: None,
        }
    }

    pub fn render_component(&mut self, c: &impl Component) -> &FrameSnapshot {
        let ctx = ViewContext::new(&self.focus);
        let tree = c.view(&ctx);
        self.last_snapshot.insert(self.planner.plan(&tree))
    }

    pub fn viewport(&self) -> &str {
        self.last_snapshot
            .as_ref()
            .map(|s| s.viewport_content())
            .unwrap_or("")
    }

    pub fn stdout_delta(&self) -> &str {
        self.last_snapshot
            .as_ref()
            .map(|s| s.stdout_delta.as_str())
            .unwrap_or("")
    }

    pub fn screen(&self) -> String {
        self.last_snapshot
            .as_ref()
            .map(|s| s.screen())
            .unwrap_or_default()
    }

    pub fn focus(&self) -> &FocusContext {
        &self.focus
    }

    pub fn focus_mut(&mut self) -> &mut FocusContext {
        &mut self.focus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::node::{col, text};

    struct SimpleComponent {
        message: String,
    }

    impl Component for SimpleComponent {
        fn view(&self, _ctx: &ViewContext<'_>) -> Node {
            text(&self.message)
        }
    }

    #[test]
    fn component_renders_to_viewport() {
        let mut h = ComponentHarness::new(80, 24);
        let component = SimpleComponent {
            message: "Hello".to_string(),
        };

        h.render_component(&component);

        assert!(h.viewport().contains("Hello"));
    }

    #[test]
    fn closure_implements_component() {
        let mut h = ComponentHarness::new(80, 24);
        let closure = |_ctx: &ViewContext<'_>| text("From closure");

        h.render_component(&closure);

        assert!(h.viewport().contains("From closure"));
    }

    #[test]
    fn component_renders_content_to_viewport() {
        let mut h = ComponentHarness::new(80, 24);

        struct ContentComponent;
        impl Component for ContentComponent {
            fn view(&self, _ctx: &ViewContext<'_>) -> Node {
                col([text("Some content")])
            }
        }

        h.render_component(&ContentComponent);

        assert!(h.viewport().contains("Some content"));
    }

    #[test]
    fn screen_combines_stdout_and_viewport() {
        let mut h = ComponentHarness::new(80, 24);

        struct MixedComponent;
        impl Component for MixedComponent {
            fn view(&self, _ctx: &ViewContext<'_>) -> Node {
                col([text("Old message"), text("Current viewport")])
            }
        }

        h.render_component(&MixedComponent);

        let screen = h.screen();
        assert!(screen.contains("Old message"));
        assert!(screen.contains("Current viewport"));
    }
}
