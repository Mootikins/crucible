//! Key event handling for OilChatApp.
//!
//! Dispatches keyboard input to the appropriate handler based on app state
//! (streaming, popup visible, interaction modal, etc.)

use crossterm::event::KeyCode;

use crate::tui::oil::app::Action;
use crate::tui::oil::event::InputAction;

use super::messages::ChatAppMsg;
use super::state::AutocompleteKind;
use super::OilChatApp;

impl OilChatApp {
    pub(super) fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Action<ChatAppMsg> {
        if self.notification_area.is_visible() {
            self.notification_area.hide();
            return Action::Continue;
        }

        if self.shell_modal.is_some() {
            return self.handle_shell_modal_key(key);
        }
        if self.interaction_modal.is_some() {
            return self.handle_interaction_key(key);
        }
        if self.is_streaming() {
            return self.handle_streaming_key(key);
        }

        if key.code == KeyCode::F(1) {
            self.toggle_command_palette();
            return Action::Continue;
        }

        if self.popup.show {
            return self.handle_popup_key(key);
        }

        if self.is_ctrl_c(key) {
            return self.handle_ctrl_c();
        }
        self.message_queue.last_ctrl_c = None;

        // Handle Ctrl+T to toggle thinking display (works anytime, not just during streaming)
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        if key.code == KeyCode::Char('t') && ctrl {
            self.toggle_thinking_with_toast();
            return Action::Continue;
        }

        if key.code == KeyCode::BackTab {
            return self.set_mode_with_status(self.mode.cycle());
        }

        let action = InputAction::from(key);
        if let Some(submitted) = self.input.handle(action) {
            return self.handle_submit(submitted);
        }

        self.check_autocomplete_trigger()
            .unwrap_or(Action::Continue)
    }

    pub(super) fn is_ctrl_c(&self, key: crossterm::event::KeyEvent) -> bool {
        key.code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
    }

    pub(super) fn toggle_thinking_with_toast(&mut self) {
        self.show_thinking = !self.show_thinking;
        let state = if self.show_thinking { "on" } else { "off" };
        self.notification_area
            .add(crucible_core::types::Notification::toast(format!(
                "Thinking display: {}",
                state
            )));
    }

    pub(super) fn handle_ctrl_c(&mut self) -> Action<ChatAppMsg> {
        if !self.input.content().is_empty() {
            self.input.handle(InputAction::Clear);
            self.message_queue.last_ctrl_c = None;
            return Action::Continue;
        }

        let now = std::time::Instant::now();
        if let Some(last) = self.message_queue.last_ctrl_c {
            if now.duration_since(last) < std::time::Duration::from_millis(300) {
                return Action::Quit;
            }
        }
        self.message_queue.last_ctrl_c = Some(now);
        self.notification_area
            .add(crucible_core::types::Notification::toast(
                "Ctrl+C again to quit",
            ));
        Action::Continue
    }

    pub(super) fn handle_streaming_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Action<ChatAppMsg> {
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => {
                tracing::info!("Stream cancel requested via Esc");
                Action::Send(ChatAppMsg::StreamCancelled)
            }
            KeyCode::Char('c') if ctrl => {
                tracing::info!("Stream cancel requested via Ctrl+C");
                Action::Send(ChatAppMsg::StreamCancelled)
            }
            KeyCode::Char('t') if ctrl => {
                self.toggle_thinking_with_toast();
                Action::Continue
            }
            KeyCode::BackTab => self.set_mode_with_status(self.mode.cycle()),
            KeyCode::Enter if ctrl => {
                let content = self.input.content().to_string();
                if !content.trim().is_empty() {
                    self.input.handle(InputAction::Clear);
                    tracing::info!("Force-send during streaming");
                    Action::Send(ChatAppMsg::StreamCancelled)
                } else {
                    Action::Continue
                }
            }
            KeyCode::Enter => {
                let content = self.input.content().to_string();
                let trimmed = content.trim();
                if trimmed.starts_with(':') || trimmed.starts_with('/') {
                    self.input.handle(InputAction::Clear);
                    return self.handle_submit(content);
                }
                if !trimmed.is_empty() {
                    self.input.handle(InputAction::Clear);
                    Action::Send(ChatAppMsg::QueueMessage(content))
                } else {
                    Action::Continue
                }
            }
            _ => {
                let action = InputAction::from(key);
                self.input.handle(action);
                Action::Continue
            }
        }
    }

    pub(super) fn handle_popup_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Action<ChatAppMsg> {
        match key.code {
            KeyCode::Esc => {
                self.close_popup();
            }
            KeyCode::Up => {
                self.popup.selected = self.popup.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.get_popup_items().len().saturating_sub(1);
                self.popup.selected = (self.popup.selected + 1).min(max);
            }
            KeyCode::Enter | KeyCode::Tab => {
                return self.select_popup_item();
            }
            KeyCode::Backspace => {
                let before_len = self.input.content().len();
                self.input.handle(InputAction::Backspace);
                if self.input.content().len() == before_len {
                    self.close_popup();
                    return Action::Continue;
                }
                self.check_autocomplete_trigger();
            }
            KeyCode::Char(_c) if self.is_ctrl_c(key) => {
                self.input.handle(InputAction::Clear);
                self.close_popup();
            }
            KeyCode::Char(c) => {
                self.input.handle(InputAction::Insert(c));
                self.check_autocomplete_trigger();
            }
            _ => {}
        }
        Action::Continue
    }

    pub(super) fn select_popup_item(&mut self) -> Action<ChatAppMsg> {
        let items = self.get_popup_items();
        let Some(item) = items.get(self.popup.selected) else {
            return Action::Continue;
        };

        let label = item.label.clone();
        let kind = self.popup.kind.clone();
        self.insert_autocomplete_selection(&label);

        match kind {
            AutocompleteKind::SlashCommand => {
                self.input.handle(InputAction::Clear);
                self.handle_slash_command(&label)
            }
            AutocompleteKind::ReplCommand => {
                self.input.handle(InputAction::Clear);
                self.handle_repl_command(&label)
            }
            AutocompleteKind::Command => {
                self.close_popup();
                self.input.handle(InputAction::Clear);
                if label.starts_with('/') {
                    self.handle_slash_command(&label)
                } else if label.starts_with(':') {
                    self.handle_repl_command(&label)
                } else {
                    // Tool or other — can't execute directly, show in status
                    self.status = format!("Tool: {}", label);
                    Action::Continue
                }
            }
            _ => Action::Continue,
        }
    }

    pub(super) fn handle_submit(&mut self, content: String) -> Action<ChatAppMsg> {
        let content = content.trim().to_string();
        if content.is_empty() {
            return Action::Continue;
        }

        if content.starts_with('/') {
            return self.handle_slash_command(&content);
        }

        if content.starts_with(':') {
            return self.handle_repl_command(&content);
        }

        if content.starts_with('!') {
            return self.handle_shell_command(&content);
        }

        self.submit_user_message(content.clone());
        Action::Send(ChatAppMsg::UserMessage(content))
    }
}
