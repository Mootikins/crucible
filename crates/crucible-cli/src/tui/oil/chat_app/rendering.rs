//! Rendering methods for OilChatApp.
//!
//! All `render_*` methods, layout calculations, and view helpers.

use chrono::Local;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::{PopupComponent, StatusComponent};
use crate::tui::oil::node::*;
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::style::{Padding, Style};
use crate::tui::oil::utils::wrap_chars;

use super::{OilChatApp, FOCUS_INPUT, INPUT_MAX_CONTENT_LINES, POPUP_HEIGHT};

impl OilChatApp {
    pub(super) fn render_messages_drawer(&self, ctx: &ViewContext<'_>) -> Node {
        use crate::tui::oil::components::status_bar::NotificationToastKind;
        use crate::tui::oil::components::{NotificationComponent, NotificationEntry};

        let term_width = ctx.terminal_size.0 as usize;

        let entries: Vec<NotificationEntry> = self
            .notification_area
            .history()
            .iter()
            .map(|(notif, instant)| {
                let kind = match &notif.kind {
                    crucible_core::types::NotificationKind::Toast => NotificationToastKind::Info,
                    crucible_core::types::NotificationKind::Progress { .. } => {
                        NotificationToastKind::Info
                    }
                    crucible_core::types::NotificationKind::Warning => {
                        NotificationToastKind::Warning
                    }
                };

                // Compute wall-clock timestamp
                let elapsed = instant.elapsed();
                let created =
                    Local::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
                let timestamp = created.format("%H:%M:%S").to_string();

                // Pass full message (wrapping happens in NotificationComponent)
                let message = notif.message.trim_end();

                NotificationEntry::new(message, kind, timestamp)
            })
            .collect();

        let comp = NotificationComponent::new(entries)
            .visible(true)
            .width(term_width);

        comp.view(ctx)
    }

    pub(super) fn render_shell_modal(&self) -> Node {
        let (term_width, term_height) = self.terminal_size.get();
        let term_width = term_width as usize;
        let term_height = term_height as usize;

        self.shell_modal
            .as_ref()
            .map(|m| m.view(term_width, term_height))
            .unwrap_or(Node::Empty)
    }

    /// Render chat content using the container-based architecture.
    ///
    /// This renders all live containers (graduated ones are already dropped).
    /// Each container is wrapped in scrollback with its stable ID.
    pub(super) fn render_containers(&self) -> Node {
        let term_width = self.terminal_size.get().0 as usize;
        let containers = self.container_list.containers();

        let mut nodes: Vec<Node> = containers
            .iter()
            .enumerate()
            .map(|(i, c)| {
                use crate::tui::oil::chat_container::{ChatContainer, ViewParams};
                let render_state = RenderState {
                    terminal_width: term_width as u16,
                    spinner_frame: self.spinner_frame,
                    show_thinking: self.show_thinking,
                };
                let is_continuation = match c {
                    ChatContainer::AssistantResponse {
                        is_continuation, ..
                    } => *is_continuation,
                    _ => false,
                };
                let params = ViewParams {
                    render_state,
                    is_continuation,
                    is_complete: self.container_list.is_response_complete(i),
                };
                c.view_with_params(&params)
            })
            .collect();

        // Turn-level spinner: shown when the turn is active but no container
        // is currently displaying a spinner (e.g. after tools complete, before
        // next TextDelta or StreamComplete).
        if self.container_list.needs_turn_spinner() {
            let t = crate::tui::oil::theme::active();
            nodes.push(
                row([
                    text(" "),
                    spinner(None, self.spinner_frame)
                        .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
                ])
                .with_margin(Padding {
                    top: 1,
                    ..Default::default()
                }),
            );
        }

        // Only return empty if both viewport AND spinner produced nothing
        if nodes.is_empty() {
            return Node::Empty;
        }

        // When graduated content exists above in stdout, insert a spacer line
        // so the viewport starts with a blank line (visual separation from
        // the graduated user prompt / previous content).
        if self.container_list.has_graduated() {
            nodes.insert(0, text(" "));
        }

        col(nodes)
    }

    pub(super) fn render_status(&self) -> Node {
        let mut comp = StatusComponent::new()
            .mode(self.mode)
            .model(&self.model)
            .context(self.context_used, self.context_total)
            .status(&self.status);

        if let Some(ref cfg) = self.statusline_config {
            comp = comp.config(cfg);
        }

        if let Some((text, kind)) = self.notification_area.active_toast() {
            comp = comp.toast(text, kind);
        }
        let counts = self.notification_area.warning_counts();
        if !counts.is_empty() {
            comp = comp.counts(counts);
        }

        let focus = crate::tui::oil::focus::FocusContext::default();
        let ctx = ViewContext::new(&focus);
        comp.view(&ctx)
    }

    pub(super) fn render_input(&self, ctx: &ViewContext<'_>) -> Node {
        use crate::tui::oil::components::{InputComponent, InputMode as ComponentInputMode};

        let input_mode = ComponentInputMode::from_content(self.input.content());
        let is_focused = !self.popup.show || ctx.is_focused(FOCUS_INPUT);

        InputComponent::new(
            self.input.content(),
            self.input.cursor(),
            ctx.terminal_size.0 as usize,
        )
        .mode(input_mode)
        .focused(is_focused)
        .show_popup(self.popup.show)
        .view(ctx)
    }

    pub(super) fn render_popup_overlay(&self, ctx: &ViewContext<'_>) -> Node {
        use super::state::AutocompleteKind;

        let show = self.popup.show && self.popup.kind != AutocompleteKind::None;
        let items = if show { self.get_popup_items() } else { vec![] };

        let popup = PopupComponent::new(items)
            .visible(show)
            .selected(self.popup.selected)
            .input_height(self.calculate_input_height())
            .max_visible(POPUP_HEIGHT);

        popup.view(ctx)
    }

    pub(super) fn calculate_input_height(&self) -> usize {
        let width = self.terminal_size.get().0 as usize;
        let content = self.input.content();
        let display_content = if content.starts_with(':') || content.starts_with('!') {
            &content[1..]
        } else {
            content
        };
        let content_width = width.saturating_sub(4);
        let lines = wrap_chars(display_content, content_width);
        let visible_lines = lines.len().min(INPUT_MAX_CONTENT_LINES);
        visible_lines + 2
    }
}
