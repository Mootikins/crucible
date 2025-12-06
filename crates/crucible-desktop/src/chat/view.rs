//! Main chat view component
//!
//! Container for the chat interface with message list and input.

use gpui::prelude::*;
use gpui::{div, px, rgb, Context, Entity, FontWeight, MouseButton, Render, Subscription, Window};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::{Message, MessageRole};

/// Main chat view state
pub struct ChatView {
    messages: Vec<Message>,
    input_state: Entity<InputState>,
    #[allow(dead_code)]
    subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create input state for the text field
        let input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Type a message... (Cmd+Enter to send)")
        });

        // Subscribe to input events
        let subscription = cx.subscribe_in(&input_state, window, Self::on_input_event);

        Self {
            messages: vec![Message::assistant(
                "Hello! I'm your Crucible assistant. How can I help you today?",
            )],
            input_state,
            subscriptions: vec![subscription],
        }
    }

    fn on_input_event(
        &mut self,
        _input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::PressEnter { secondary } = event {
            // Cmd+Enter or Ctrl+Enter sends message
            if *secondary {
                self.send_message(window, cx);
            }
        }
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).value().to_string();

        if text.trim().is_empty() {
            return;
        }

        // Add user message
        self.messages.push(Message::user(text));

        // Clear input
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        // TODO: Send to backend and get response
        // For now, add a placeholder response
        self.messages
            .push(Message::assistant("I received your message!"));

        cx.notify();
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(self.render_header())
            .child(self.render_messages())
            .child(self.render_input(cx))
    }
}

impl ChatView {
    fn render_header(&self) -> impl IntoElement {
        div()
            .h(px(48.0))
            .w_full()
            .flex()
            .items_center()
            .px_4()
            .border_b_1()
            .border_color(rgb(0x313244))
            .bg(rgb(0x181825))
            .child(
                div()
                    .text_color(rgb(0xcdd6f4))
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Crucible Chat"),
            )
    }

    fn render_messages(&self) -> impl IntoElement {
        // Note: overflow methods not available in gpui 0.2, use flex layout instead
        div()
            .flex_1()
            .p_4()
            .gap_3()
            .flex()
            .flex_col()
            .children(self.messages.iter().map(|msg| self.render_message(msg)))
    }

    fn render_message(&self, message: &Message) -> impl IntoElement {
        let (bg_color, text_color, is_user) = match message.role {
            MessageRole::User => (rgb(0x89b4fa), rgb(0x1e1e2e), true),
            MessageRole::Assistant => (rgb(0x313244), rgb(0xcdd6f4), false),
        };

        let container = div().w_full().flex();

        let container = if is_user {
            container.justify_end()
        } else {
            container
        };

        container.child(
            div()
                .max_w(px(600.0))
                .px_4()
                .py_2()
                .rounded_lg()
                .bg(bg_color)
                .text_color(text_color)
                .child(message.content.clone()),
        )
    }

    fn render_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let input_state = self.input_state.clone();

        div()
            .h(px(80.0))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .p_4()
            .border_t_1()
            .border_color(rgb(0x313244))
            .bg(rgb(0x181825))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&input_state).h(px(48.0))),
            )
            .child(
                div()
                    .id("send-button")
                    .h(px(48.0))
                    .px_4()
                    .rounded_lg()
                    .bg(rgb(0x89b4fa))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x1e1e2e))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.send_message(window, cx);
                    }))
                    .child("Send"),
            )
    }
}
