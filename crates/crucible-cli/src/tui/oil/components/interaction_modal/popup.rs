use super::{InteractionMode, InteractionModal, InteractionModalOutput};
use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::interaction::{InteractionResponse, PopupRequest, PopupResponse};
use crucible_oil::node::{col, row, styled, text, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_popup_key(
        &mut self,
        key: KeyEvent,
        popup: PopupRequest,
    ) -> InteractionModalOutput {
        let entries_count = popup.entries.len();
        let total_items = entries_count + if popup.allow_other { 1 } else { 0 };

        match self.mode {
            InteractionMode::Selecting => match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.selected = Self::wrap_selection(self.selected, -1, total_items.max(1));
                    InteractionModalOutput::None
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    self.selected = Self::wrap_selection(self.selected, 1, total_items.max(1));
                    InteractionModalOutput::None
                }
                KeyCode::Enter => {
                    if self.selected < entries_count {
                        let entry = popup.entries[self.selected].clone();
                        InteractionModalOutput::AskResponse {
                            request_id: self.request_id.clone(),
                            response: InteractionResponse::Popup(PopupResponse::selected(
                                self.selected,
                                entry,
                            )),
                        }
                    } else if popup.allow_other && self.selected == entries_count {
                        self.mode = InteractionMode::TextInput;
                        InteractionModalOutput::None
                    } else {
                        InteractionModalOutput::None
                    }
                }
                KeyCode::Esc => InteractionModalOutput::AskResponse {
                    request_id: self.request_id.clone(),
                    response: InteractionResponse::Cancelled,
                },
                KeyCode::Char('c') if Self::is_ctrl_c(key) => InteractionModalOutput::AskResponse {
                    request_id: self.request_id.clone(),
                    response: InteractionResponse::Cancelled,
                },
                _ => InteractionModalOutput::None,
            },
            InteractionMode::TextInput => match key.code {
                KeyCode::Enter => {
                    let response =
                        InteractionResponse::Popup(PopupResponse::other(self.other_text.clone()));
                    InteractionModalOutput::AskResponse {
                        request_id: self.request_id.clone(),
                        response,
                    }
                }
                KeyCode::Esc => {
                    self.mode = InteractionMode::Selecting;
                    InteractionModalOutput::None
                }
                KeyCode::Backspace => {
                    self.other_text.pop();
                    InteractionModalOutput::None
                }
                KeyCode::Char(c) => {
                    self.other_text.push(c);
                    InteractionModalOutput::None
                }
                _ => InteractionModalOutput::None,
            },
        }
    }

    pub(super) fn render_popup_interaction(&self, popup: &PopupRequest, term_width: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let panel_bg = t.resolve_color(t.colors.background);
        let border_fg = t.resolve_color(t.colors.background);

        let title_text = format!(" {} ", popup.title);
        let title_pad = " ".repeat(term_width.saturating_sub(title_text.len()));
        let title = styled(
            format!("{title_text}{title_pad}"),
            Style::new().bg(panel_bg).bold(),
        );

        let mut choice_nodes: Vec<Node> = Vec::new();
        for (i, entry) in popup.entries.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { " > " } else { "   " };
            let label_style = if is_selected {
                Style::new().fg(t.resolve_color(t.colors.primary)).bold()
            } else {
                Style::new().fg(t.resolve_color(t.colors.text))
            };
            if let Some(ref desc) = entry.description {
                choice_nodes.push(row([
                    styled(format!("{prefix}{}", entry.label), label_style),
                    styled(
                        format!("  {desc}"),
                        Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
                    ),
                ]));
            } else {
                choice_nodes.push(styled(format!("{prefix}{}", entry.label), label_style));
            }
        }

        if popup.allow_other {
            let other_idx = popup.entries.len();
            let is_selected = self.selected == other_idx;
            let prefix = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::new().fg(t.resolve_color(t.colors.primary)).bold()
            } else {
                Style::new()
                    .fg(t.resolve_color(t.colors.text_muted))
                    .italic()
            };
            choice_nodes.push(styled(format!("{prefix}Other..."), style));
        }

        if self.mode == InteractionMode::TextInput {
            choice_nodes.push(row([
                styled(
                    "   Enter text: ",
                    Style::new().fg(t.resolve_color(t.colors.text_muted)),
                ),
                styled(
                    &self.other_text,
                    Style::new().fg(t.resolve_color(t.colors.text)),
                ),
                styled("_", Style::new().fg(t.resolve_color(t.colors.primary))),
            ]));
        }

        let key_style = Style::new().fg(t.resolve_color(t.colors.primary));
        let hint_style = Style::new().fg(t.resolve_color(t.colors.text_muted)).dim();

        col([
            text(""),
            styled(
                t.decorations
                    .half_block_bottom
                    .to_string()
                    .repeat(term_width),
                Style::new().fg(border_fg),
            ),
            title,
            col(choice_nodes),
            styled(
                t.decorations.half_block_top.to_string().repeat(term_width),
                Style::new().fg(border_fg),
            ),
            row([
                styled(
                    " POPUP ",
                    Style::new()
                        .fg(t.resolve_color(t.colors.error))
                        .bold()
                        .reverse(),
                ),
                styled("  ↑/↓", key_style),
                styled(" navigate", hint_style),
                styled("  Enter", key_style),
                styled(" select", hint_style),
                styled("  Esc", key_style),
                styled(" cancel", hint_style),
            ]),
            text(""),
        ])
    }
}
