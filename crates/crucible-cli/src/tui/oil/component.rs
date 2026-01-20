use crate::tui::oil::app::ViewContext;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::node::Node;
use crate::tui::oil::planning::{FramePlanner, FrameSnapshot, FrameTrace};

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
        self.last_snapshot = Some(self.planner.plan(&tree));
        self.last_snapshot.as_ref().unwrap()
    }

    pub fn trace(&self) -> Option<&FrameTrace> {
        self.last_snapshot.as_ref().map(|s| s.trace())
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
    use crate::tui::oil::node::{scrollback, text};

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
    fn component_with_static_graduates() {
        let mut h = ComponentHarness::new(80, 24);

        struct StaticComponent;
        impl Component for StaticComponent {
            fn view(&self, _ctx: &ViewContext<'_>) -> Node {
                scrollback("static-1", [text("Graduated content")])
            }
        }

        h.render_component(&StaticComponent);

        let trace = h.trace().expect("should have trace");
        assert_eq!(trace.graduated_keys, vec!["static-1"]);
        assert!(h.stdout_delta().contains("Graduated content"));
    }

    #[test]
    fn screen_combines_stdout_and_viewport() {
        let mut h = ComponentHarness::new(80, 24);

        struct MixedComponent;
        impl Component for MixedComponent {
            fn view(&self, _ctx: &ViewContext<'_>) -> Node {
                use crate::tui::oil::node::col;
                col([
                    scrollback("msg-1", [text("Old message")]),
                    text("Current viewport"),
                ])
            }
        }

        h.render_component(&MixedComponent);

        let screen = h.screen();
        assert!(screen.contains("Old message"));
        assert!(screen.contains("Current viewport"));
    }
}
