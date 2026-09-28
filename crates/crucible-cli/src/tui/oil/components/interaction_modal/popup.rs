use super::choice::{self, ChoiceList, ChoiceStep};
use super::{InteractionModal, InteractionModalOutput, InteractionMode};
use crossterm::event::KeyEvent;
use crucible_core::interaction::{InteractionResponse, PopupRequest, PopupResponse};
use crucible_oil::node::{col, row, styled, text, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_popup_key(
        &mut self,
        key: KeyEvent,
        popup: PopupRequest,
    ) -> InteractionModalOutput {
        let list = ChoiceList::new(popup.entries.len()).allow_other(popup.allow_other);
        let response = match list.handle_key(key, self.choice_input()) {
            ChoiceStep::Pick(index) => InteractionResponse::Popup(PopupResponse::selected(
                index,
                popup.entries[index].clone(),
            )),
            ChoiceStep::Other(text) => InteractionResponse::Popup(PopupResponse::other(text)),
            ChoiceStep::Cancel => InteractionResponse::Cancelled,
            // A popup is single-select, so the flow sends no `PickMany`.
            ChoiceStep::PickMany(_) | ChoiceStep::Handled | ChoiceStep::Ignored => {
                return InteractionModalOutput::None
            }
        };
        InteractionModalOutput::AskResponse {
            request_id: self.request_id.clone(),
            response,
        }
    }

    pub(super) fn render_popup_interaction(&self, popup: &PopupRequest, term_width: usize) -> Node {
        use crate::tui::oil::theme::groups;
        let t = crate::tui::oil::theme::active();
        let panel_bg = groups::bg_or("Modal", t.resolve_color(t.colors.background));
        let border_fg = groups::fg_or("ModalBorder", t.resolve_color(t.colors.background));

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
            choice_nodes.push(choice::other_row(
                self.selected == popup.entries.len(),
                " > ",
            ));
        }

        if self.mode == InteractionMode::TextInput {
            choice_nodes.push(choice::other_text_row("   ", &self.other_text));
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
