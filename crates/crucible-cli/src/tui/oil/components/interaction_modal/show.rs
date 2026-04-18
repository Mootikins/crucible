use super::{InteractionModal, InteractionModalOutput};
use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::interaction::{InteractionRequest, InteractionResponse, ShowRequest};
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_show_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
        let content_lines = if let InteractionRequest::Show(show) = &self.request {
            show.content.lines().count()
        } else {
            0
        };

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if self.scroll_offset < content_lines.saturating_sub(1) {
                    self.scroll_offset += 1;
                }
                InteractionModalOutput::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                InteractionModalOutput::None
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll_offset = (self.scroll_offset + 20).min(content_lines.saturating_sub(1));
                InteractionModalOutput::None
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(20);
                InteractionModalOutput::None
            }
            KeyCode::Char('g') => {
                self.scroll_offset = 0;
                InteractionModalOutput::None
            }
            KeyCode::Char('G') => {
                self.scroll_offset = content_lines.saturating_sub(1);
                InteractionModalOutput::None
            }
            KeyCode::Esc | KeyCode::Char('q') => InteractionModalOutput::AskResponse {
                request_id: self.request_id.clone(),
                response: InteractionResponse::Cancelled,
            },
            KeyCode::Char('c') if Self::is_ctrl_c(key) => InteractionModalOutput::AskResponse {
                request_id: self.request_id.clone(),
                response: InteractionResponse::Cancelled,
            },
            _ => InteractionModalOutput::None,
        }
    }

    pub(super) fn render_show_interaction(&self, show: &ShowRequest, term_width: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let panel_bg = t.resolve_color(t.colors.background);
        let border_fg = t.resolve_color(t.colors.background);

        let title_text = show
            .title
            .as_deref()
            .map(|t| format!(" {} ", t))
            .unwrap_or_else(|| " View ".to_string());
        let title_pad = " ".repeat(term_width.saturating_sub(title_text.len()));
        let title = styled(
            format!("{title_text}{title_pad}"),
            Style::new().bg(panel_bg).bold(),
        );

        let content_lines: Vec<&str> = show.content.lines().collect();
        let visible_count = 20.min(content_lines.len());
        let end = (self.scroll_offset + visible_count).min(content_lines.len());
        let visible = &content_lines[self.scroll_offset..end];

        let mut lines: Vec<Node> = Vec::with_capacity(visible_count + 6);
        lines.push(styled(
            t.decorations
                .half_block_bottom
                .to_string()
                .repeat(term_width),
            Style::new().fg(border_fg),
        ));
        lines.push(title);

        for line in visible {
            let pad = " ".repeat(term_width.saturating_sub(line.len() + 2));
            lines.push(styled(
                format!("  {line}{pad}"),
                Style::new().bg(panel_bg).fg(t.resolve_color(t.colors.text)),
            ));
        }

        lines.push(styled(
            t.decorations.half_block_top.to_string().repeat(term_width),
            Style::new().fg(border_fg),
        ));

        let key_style = Style::new().fg(t.resolve_color(t.colors.primary));
        let hint_style = Style::new().fg(t.resolve_color(t.colors.text_muted)).dim();
        let scroll_info = format!(" {}/{}", self.scroll_offset + 1, content_lines.len().max(1));

        lines.push(row([
            styled(
                " SHOW ",
                Style::new()
                    .fg(t.resolve_color(t.colors.error))
                    .bold()
                    .reverse(),
            ),
            styled(scroll_info, hint_style),
            styled("  j/k", key_style),
            styled(" scroll", hint_style),
            styled("  PgUp/PgDn", key_style),
            styled(" page", hint_style),
            styled("  q/Esc", key_style),
            styled(" close", hint_style),
        ]));

        col(lines)
    }
}
