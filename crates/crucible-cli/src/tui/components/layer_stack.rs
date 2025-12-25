//! Layer-based rendering system for composable UI layers
//!
//! Provides `LayerStack` to manage event propagation for layered UI
//! elements (base layer → popup → modal).
//!
//! # Architecture
//!
//! The `LayerStack` manages event routing for three rendering layers:
//!
//! 1. **Base Layer** - Main UI (conversation history, input, status bar)
//! 2. **Popup Layer** - Transient overlays (autocomplete, command palette)
//! 3. **Modal Layer** - Dialogs that capture focus and block interaction
//!
//! Note: LayerStack is primarily for **event routing**, not rendering.
//! The actual rendering is done by the caller, who controls the exact
//! z-order and layout of widgets.
//!
//! # Event Propagation
//!
//! Events flow top-down through the stack:
//!
//! - Modal active → modal handles all events (blocks lower layers)
//! - Popup focused → popup handles events (blocks base layer)
//! - Otherwise → base layer receives events (if interactive)
//!
//! # Example
//!
//! ```ignore
//! // Render widgets manually (caller controls z-order)
//! frame.render_widget(base_widget, area);
//! if popup_active {
//!     frame.render_widget(popup_widget, popup_area);
//! }
//! if dialog_active {
//!     frame.render_widget(dialog_widget, dialog_area);
//! }
//!
//! // Route events through LayerStack
//! let mut stack = LayerStack::new(focus_target);
//! if let Some(popup) = popup_widget {
//!     stack.set_popup(popup);
//! }
//! if let Some(dialog) = dialog_widget {
//!     stack.set_modal(dialog);
//! }
//! let result = stack.route_event(&event);
//! ```

use super::{EventResult, FocusTarget, InteractiveWidget};
use crossterm::event::Event;

/// Event routing coordinator for layered UI
///
/// Routes events to the appropriate layer based on focus and layer priority.
/// Does NOT handle rendering - that's done by the caller who controls exact
/// widget placement and z-order.
pub struct LayerStack<'a> {
    /// Popup layer reference (if active)
    popup: Option<&'a mut dyn InteractiveWidget>,
    /// Modal layer reference (if active)
    modal: Option<&'a mut dyn InteractiveWidget>,
    /// Current focus target
    focus: FocusTarget,
}

impl<'a> LayerStack<'a> {
    /// Create a new layer stack with the given focus
    pub fn new(focus: FocusTarget) -> Self {
        Self {
            popup: None,
            modal: None,
            focus,
        }
    }

    /// Set the popup layer (transient overlays)
    ///
    /// Popups can receive focus and handle events.
    pub fn set_popup(&mut self, widget: &'a mut dyn InteractiveWidget) {
        self.popup = Some(widget);
    }

    /// Set the modal layer (dialogs)
    ///
    /// Modals capture all events when present.
    pub fn set_modal(&mut self, widget: &'a mut dyn InteractiveWidget) {
        self.modal = Some(widget);
    }

    /// Set the current focus target
    pub fn set_focus(&mut self, target: FocusTarget) {
        self.focus = target;
    }

    /// Route event to the appropriate layer
    ///
    /// Event propagation rules:
    /// 1. If modal exists → modal handles ALL events (captures focus)
    /// 2. Else if popup exists and focused → popup handles events
    /// 3. Otherwise → return Ignored (caller handles base layer)
    ///
    /// This ensures modals block all interaction and popups only
    /// handle events when explicitly focused.
    pub fn route_event(&mut self, event: &Event) -> EventResult {
        // Modal captures all events when present
        if let Some(modal) = self.modal.as_mut() {
            return modal.handle_event(event);
        }

        // Popup handles events when focused
        if self.focus == FocusTarget::Popup {
            if let Some(popup) = self.popup.as_mut() {
                return popup.handle_event(event);
            }
        }

        // Base layer is not handled here (caller's responsibility)
        EventResult::Ignored
    }

    /// Check if any layer is active that can receive focus
    pub fn has_focusable_layer(&self) -> bool {
        self.modal.is_some() || self.popup.is_some()
    }

    /// Check if modal is active (blocks all other interaction)
    pub fn has_modal(&self) -> bool {
        self.modal.is_some()
    }

    /// Check if popup is active
    pub fn has_popup(&self) -> bool {
        self.popup.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::{Paragraph, Widget};

    // =========================================================================
    // Test Widgets
    // =========================================================================

    /// Interactive popup widget
    struct TestPopup {
        handled: bool,
    }

    impl TestPopup {
        fn new() -> Self {
            Self { handled: false }
        }
    }

    impl Widget for TestPopup {
        fn render(self, area: Rect, buf: &mut Buffer) {
            let para = Paragraph::new("Popup Layer");
            para.render(area, buf);
        }
    }

    impl InteractiveWidget for TestPopup {
        fn handle_event(&mut self, event: &Event) -> EventResult {
            if let Event::Key(KeyEvent {
                code: KeyCode::Char('p'),
                ..
            }) = event
            {
                self.handled = true;
                EventResult::Consumed
            } else {
                EventResult::Ignored
            }
        }

        fn focusable(&self) -> bool {
            true
        }
    }

    /// Interactive modal widget
    struct TestModal {
        handled: bool,
    }

    impl TestModal {
        fn new() -> Self {
            Self { handled: false }
        }
    }

    impl Widget for TestModal {
        fn render(self, area: Rect, buf: &mut Buffer) {
            let para = Paragraph::new("Modal Layer");
            para.render(area, buf);
        }
    }

    impl InteractiveWidget for TestModal {
        fn handle_event(&mut self, _event: &Event) -> EventResult {
            // Modal captures ALL events
            self.handled = true;
            EventResult::Consumed
        }

        fn focusable(&self) -> bool {
            true
        }
    }

    // =========================================================================
    // Tests
    // =========================================================================

    #[test]
    fn layer_stack_new() {
        let stack = LayerStack::new(FocusTarget::Input);
        assert_eq!(stack.focus, FocusTarget::Input);
        assert!(!stack.has_popup());
        assert!(!stack.has_modal());
    }

    #[test]
    fn layer_stack_set_popup() {
        let mut popup = TestPopup::new();
        let mut stack = LayerStack::new(FocusTarget::Input);
        stack.set_popup(&mut popup);
        assert!(stack.has_popup());
    }

    #[test]
    fn layer_stack_set_modal() {
        let mut modal = TestModal::new();
        let mut stack = LayerStack::new(FocusTarget::Input);
        stack.set_modal(&mut modal);
        assert!(stack.has_modal());
    }

    #[test]
    fn layer_stack_set_focus() {
        let mut stack = LayerStack::new(FocusTarget::Input);
        stack.set_focus(FocusTarget::Popup);
        assert_eq!(stack.focus, FocusTarget::Popup);
    }

    #[test]
    fn event_routing_modal_captures_all() {
        let mut popup = TestPopup::new();
        let mut modal = TestModal::new();
        let mut stack = LayerStack::new(FocusTarget::Popup);
        stack.set_popup(&mut popup);
        stack.set_modal(&mut modal);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        // Modal should capture the event, popup should not receive it
        assert_eq!(result, EventResult::Consumed);
        assert!(modal.handled);
        assert!(!popup.handled); // Popup never saw the event
    }

    #[test]
    fn event_routing_popup_when_focused() {
        let mut popup = TestPopup::new();
        let mut stack = LayerStack::new(FocusTarget::Popup);
        stack.set_popup(&mut popup);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        assert_eq!(result, EventResult::Consumed);
        assert!(popup.handled);
    }

    #[test]
    fn event_routing_popup_not_focused() {
        let mut popup = TestPopup::new();
        let mut stack = LayerStack::new(FocusTarget::Input); // Not focused on popup
        stack.set_popup(&mut popup);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        // Popup should not handle event when not focused
        assert_eq!(result, EventResult::Ignored);
        assert!(!popup.handled);
    }

    #[test]
    fn event_routing_base_ignored() {
        // No popup or modal active
        let mut stack = LayerStack::new(FocusTarget::Input);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        // Should return Ignored since no layers handle it
        assert_eq!(result, EventResult::Ignored);
    }

    #[test]
    fn has_focusable_layer_with_modal() {
        let mut modal = TestModal::new();
        let mut stack = LayerStack::new(FocusTarget::Input);
        stack.set_modal(&mut modal);
        assert!(stack.has_focusable_layer());
    }

    #[test]
    fn has_focusable_layer_with_popup() {
        let mut popup = TestPopup::new();
        let mut stack = LayerStack::new(FocusTarget::Input);
        stack.set_popup(&mut popup);
        assert!(stack.has_focusable_layer());
    }

    #[test]
    fn has_focusable_layer_empty() {
        let stack = LayerStack::new(FocusTarget::Input);
        assert!(!stack.has_focusable_layer());
    }

    #[test]
    fn modal_blocks_popup() {
        // Even with popup focused, modal should capture events
        let mut popup = TestPopup::new();
        let mut modal = TestModal::new();
        let mut stack = LayerStack::new(FocusTarget::Popup);
        stack.set_popup(&mut popup);
        stack.set_modal(&mut modal);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        // Modal captures event, popup never sees it
        assert_eq!(result, EventResult::Consumed);
        assert!(modal.handled);
        assert!(!popup.handled);
    }

    #[test]
    fn layer_priority_modal_over_popup() {
        // Test that modal always takes priority even if both are present
        let mut popup = TestPopup::new();
        let mut modal = TestModal::new();
        let mut stack = LayerStack::new(FocusTarget::Dialog);
        stack.set_popup(&mut popup);
        stack.set_modal(&mut modal);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        let result = stack.route_event(&event);

        assert_eq!(result, EventResult::Consumed);
        assert!(modal.handled);
        assert!(!popup.handled);
    }
}
