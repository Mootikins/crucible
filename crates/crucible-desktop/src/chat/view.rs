//! Main chat view component
//!
//! Container for the chat interface with message list and input.

use gpui::prelude::*;
use gpui::{div, px, rgb, Context, FontWeight, Render};

use crate::{Message, MessageRole};

/// Main chat view state
pub struct ChatView {
    messages: Vec<Message>,
    input_text: String,
}

impl ChatView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            messages: vec![Message::assistant(
                "Hello! I'm your Crucible assistant. How can I help you today?",
            )],
            input_text: String::new(),
        }
    }

    #[allow(dead_code)]
    fn send_message(&mut self, cx: &mut Context<Self>) {
        if self.input_text.trim().is_empty() {
            return;
        }

        // Add user message
        let user_msg = Message::user(self.input_text.clone());
        self.messages.push(user_msg);

        // Clear input
        self.input_text.clear();

        // TODO: Send to backend and get response
        // For now, add a placeholder response
        self.messages
            .push(Message::assistant("I received your message!"));

        cx.notify();
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(self.render_header())
            .child(self.render_messages())
            .child(self.render_input())
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

    fn render_input(&self) -> impl IntoElement {
        let placeholder_text: &str = if self.input_text.is_empty() {
            "Type a message... (Cmd+Enter to send)"
        } else {
            &self.input_text
        };

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
                    .h(px(48.0))
                    .px_4()
                    .rounded_lg()
                    .bg(rgb(0x313244))
                    .flex()
                    .items_center()
                    .text_color(rgb(0x6c7086))
                    .child(placeholder_text.to_string()),
            )
            .child(
                div()
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
                    .child("Send"),
            )
    }
}
