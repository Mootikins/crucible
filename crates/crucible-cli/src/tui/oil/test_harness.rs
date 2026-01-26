use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::node::Node;
use crate::tui::oil::planning::{FramePlanner, FrameSnapshot, FrameTrace};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub struct AppHarness<A: App> {
    app: A,
    focus: FocusContext,
    planner: FramePlanner,
    last_snapshot: Option<FrameSnapshot>,
    stdout_buffer: String,
}

impl<A: App> AppHarness<A> {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            app: A::init(),
            focus: FocusContext::new(),
            planner: FramePlanner::new(width, height),
            last_snapshot: None,
            stdout_buffer: String::new(),
        }
    }

    pub fn send_key(&mut self, code: KeyCode) -> &mut Self {
        self.send_key_with_modifiers(code, KeyModifiers::NONE)
    }

    pub fn send_key_with_modifiers(&mut self, code: KeyCode, modifiers: KeyModifiers) -> &mut Self {
        let event = Event::Key(KeyEvent::new(code, modifiers));

        if code == KeyCode::Tab {
            if modifiers.contains(KeyModifiers::SHIFT) {
                self.focus.focus_prev();
            } else {
                self.focus.focus_next();
            }
        } else {
            let action = self.app.update(event);
            self.process_action(action);
        }

        self.render();
        self
    }

    pub fn send_text(&mut self, text: &str) -> &mut Self {
        for c in text.chars() {
            self.send_key(KeyCode::Char(c));
        }
        self
    }

    pub fn send_enter(&mut self) -> &mut Self {
        self.send_key(KeyCode::Enter)
    }

    pub fn send_escape(&mut self) -> &mut Self {
        self.send_key(KeyCode::Esc)
    }

    pub fn send_tab(&mut self) -> &mut Self {
        self.send_key(KeyCode::Tab)
    }

    pub fn send_shift_tab(&mut self) -> &mut Self {
        self.send_key_with_modifiers(KeyCode::Tab, KeyModifiers::SHIFT)
    }

    pub fn send_ctrl_c(&mut self) -> &mut Self {
        self.send_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    pub fn tick(&mut self) -> &mut Self {
        let action = self.app.update(Event::Tick);
        self.process_action(action);
        self.render();
        self
    }

    pub fn send_message(&mut self, msg: A::Msg) -> &mut Self {
        let action = self.app.on_message(msg);
        self.process_action(action);
        self.render();
        self
    }

    fn process_action(&mut self, action: Action<A::Msg>) {
        match action {
            Action::Quit => {}
            Action::Continue => {}
            Action::Send(msg) => {
                let action = self.app.on_message(msg);
                self.process_action(action);
            }
            Action::Batch(actions) => {
                for action in actions {
                    self.process_action(action);
                }
            }
        }
    }

    pub fn render(&mut self) -> &mut Self {
        let ctx = ViewContext::new(&self.focus);
        let tree = self.app.view(&ctx);
        let snapshot = self.planner.plan(&tree);
        self.stdout_buffer.push_str(&snapshot.stdout_delta);
        self.last_snapshot = Some(snapshot);
        self
    }

    pub fn tree(&self) -> Node {
        let ctx = ViewContext::new(&self.focus);
        self.app.view(&ctx)
    }

    pub fn viewport(&self) -> &str {
        self.last_snapshot
            .as_ref()
            .map(|s| s.viewport_content())
            .unwrap_or("")
    }

    pub fn stdout(&self) -> &str {
        &self.stdout_buffer
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

    pub fn trace(&self) -> Option<&FrameTrace> {
        self.last_snapshot.as_ref().map(|s| s.trace())
    }

    pub fn graduated_count(&self) -> usize {
        self.planner.graduation().graduated_count()
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focus.is_focused(id)
    }

    pub fn focused_id(&self) -> Option<&str> {
        self.focus.active_id().map(|id| id.0.as_str())
    }

    pub fn app(&self) -> &A {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::chat_app::OilChatApp;

    #[test]
    fn harness_initializes_and_renders() {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
        harness.render();

        assert!(!harness.viewport().is_empty());
    }

    #[test]
    fn harness_sends_keys() {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
        harness.render();

        harness.send_key(KeyCode::Char('h'));
        harness.send_key(KeyCode::Char('i'));
    }

    #[test]
    fn harness_sends_text() {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
        harness.render();

        harness.send_text("hello");
    }

    #[test]
    fn trace_available_after_render() {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);

        assert!(harness.trace().is_none());

        harness.render();

        let trace = harness.trace().expect("trace should exist after render");
        assert_eq!(trace.frame_no, 1);
    }

    #[test]
    fn screen_combines_stdout_and_viewport() {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
        harness.render();

        let screen = harness.screen();
        assert!(!screen.is_empty());
    }
}
