//! Main chat view component
//!
//! Container for the chat interface with message list and input.

use gpui::prelude::*;
use gpui::{
    actions, div, px, rgb, Context, Entity, FocusHandle, Focusable, FontWeight, KeyBinding,
    MouseButton, Render, SharedString, Subscription, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::text::TextView;

use crate::backend::MockAgent;
use crate::{Message, MessageRole};

// Define actions for keyboard shortcuts
actions!(crucible_chat, [ClearChat]);

/// Main chat view state
///
/// Contains the message history, input field state, and agent for generating responses.
/// Uses GPUI's Entity and Subscription systems for reactive updates.
pub struct ChatView {
    messages: Vec<Message>,
    input_state: Entity<InputState>,
    agent: MockAgent,
    focus_handle: FocusHandle,
    /// Held to keep input event subscription alive
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    /// Creates a new chat view with default welcome message.
    ///
    /// Initializes the input field, mock agent, and keyboard shortcuts.
    /// The view subscribes to input events for Cmd+Enter message sending.
    ///
    /// # Arguments
    /// * `window` - GPUI window handle
    /// * `cx` - GPUI context for view creation
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Register key bindings
        cx.bind_keys([KeyBinding::new("cmd-k", ClearChat, None)]);

        // Create focus handle
        let focus_handle = cx.focus_handle();

        // Create input state for the text field
        let input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Type a message... (Cmd+Enter to send)")
        });

        // Subscribe to input events
        let subscription = cx.subscribe_in(&input_state, window, Self::on_input_event);

        // Create mock agent
        let agent = MockAgent::new();

        Self {
            messages: vec![Message::assistant(
                "Hello! I'm your Crucible assistant. How can I help you today?\n\n\
                Try saying **hello**, asking for **help**, or testing **markdown** rendering!\n\n\
                **Shortcuts:** `Cmd+Enter` to send, `Cmd+K` to clear chat",
            )],
            input_state,
            agent,
            focus_handle,
            _subscriptions: vec![subscription],
        }
    }

    /// Clear all messages from the chat
    fn clear_chat(&mut self, cx: &mut Context<Self>) {
        self.messages.clear();
        self.messages
            .push(Message::assistant("Chat cleared. How can I help you?"));
        cx.notify();
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
        self.messages.push(Message::user(text.clone()));

        // Clear input
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });

        // Get response from mock agent (synchronous for now)
        let response = self.agent.send_message_sync(&text);
        self.messages.push(Message::assistant(response));

        cx.notify();
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("chat-view")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ClearChat, _window, cx| {
                this.clear_chat(cx);
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .child(self.render_header())
            .child(self.render_messages(window, cx))
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

    fn render_messages(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut messages_div = div().flex_1().p_4().gap_3().flex().flex_col();

        for (i, msg) in self.messages.iter().enumerate() {
            messages_div = messages_div.child(self.render_message(i, msg, window, cx));
        }

        messages_div
    }

    fn render_message(
        &self,
        index: usize,
        message: &Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

        let bubble = div()
            .max_w(px(600.0))
            .px_4()
            .py_2()
            .rounded_lg()
            .bg(bg_color);

        let bubble = if is_user {
            // User messages: plain text
            bubble.text_color(text_color).child(message.content.clone())
        } else {
            // Assistant messages: markdown rendering
            let id: SharedString = format!("msg-{}", index).into();
            bubble.child(TextView::markdown(id, &message.content, window, cx))
        };

        container.child(bubble)
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
            .child(div().flex_1().child(Input::new(&input_state).h(px(48.0))))
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.send_message(window, cx);
                        }),
                    )
                    .child("Send"),
            )
    }
}
