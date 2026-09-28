use super::choice::{self, ChoiceInput, ChoiceList, ChoiceStep};
use super::{InteractionModal, InteractionModalOutput, InteractionMode};
use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::interaction::{InteractionResponse, InteractivePanel, PanelResult, PanelState};
use crucible_oil::node::{col, row, styled, text, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_panel_key(
        &mut self,
        key: KeyEvent,
        panel: InteractivePanel,
    ) -> InteractionModalOutput {
        if self.mode == InteractionMode::Filter {
            return self.handle_panel_filter_key(key, &panel);
        }
        if self.mode == InteractionMode::Selecting
            && key.code == KeyCode::Char('/')
            && panel.hints.filterable
        {
            self.mode = InteractionMode::Filter;
            return InteractionModalOutput::None;
        }

        let Some(state) = self.panel_state.as_mut() else {
            return InteractionModalOutput::None;
        };
        let list = ChoiceList::filtered(&state.visible)
            .allow_other(panel.hints.allow_other)
            .multi_select(panel.hints.multi_select);
        let input = ChoiceInput {
            cursor: &mut state.cursor,
            checked: &mut self.checked,
            text: &mut self.other_text,
            mode: &mut self.mode,
        };
        let result = match list.handle_key(key, input) {
            ChoiceStep::Pick(index) => PanelResult::selected(std::iter::once(index)),
            ChoiceStep::PickMany(indices) => PanelResult::selected(indices),
            ChoiceStep::Other(text) => PanelResult::other(text),
            ChoiceStep::Cancel => PanelResult::cancelled(),
            ChoiceStep::Handled | ChoiceStep::Ignored => return InteractionModalOutput::None,
        };
        InteractionModalOutput::AskResponse {
            request_id: self.request_id.clone(),
            response: InteractionResponse::Panel(result),
        }
    }

    fn handle_panel_filter_key(
        &mut self,
        key: KeyEvent,
        panel: &InteractivePanel,
    ) -> InteractionModalOutput {
        let state = match &mut self.panel_state {
            Some(s) => s,
            None => return InteractionModalOutput::None,
        };

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.mode = InteractionMode::Selecting;
            }
            KeyCode::Backspace => {
                state.filter.pop();
                Self::update_panel_visible(state, panel);
            }
            KeyCode::Char(c) => {
                state.filter.push(c);
                Self::update_panel_visible(state, panel);
            }
            _ => {}
        }
        InteractionModalOutput::None
    }

    fn update_panel_visible(state: &mut PanelState, panel: &InteractivePanel) {
        let filter_lower = state.filter.to_lowercase();
        state.visible = panel
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                filter_lower.is_empty() || item.label.to_lowercase().contains(&filter_lower)
            })
            .map(|(i, _)| i)
            .collect();
        state.cursor = ChoiceList::filtered(&state.visible)
            .allow_other(panel.hints.allow_other)
            .clamp(state.cursor);
    }

    pub(super) fn render_panel_interaction(
        &self,
        panel: &InteractivePanel,
        term_width: usize,
    ) -> Node {
        let t = crate::tui::oil::theme::active();
        let panel_bg = t.resolve_color(t.colors.background);
        let border_fg = t.resolve_color(t.colors.background);

        let header_text = format!(" {} ", panel.header);
        let header_pad = " ".repeat(term_width.saturating_sub(header_text.len()));
        let header = styled(
            format!("{header_text}{header_pad}"),
            Style::new().bg(panel_bg).bold(),
        );

        let state = self.panel_state.as_ref();
        let visible_indices: &[usize] = state.map(|s| s.visible.as_slice()).unwrap_or(&[]);
        let cursor = state.map(|s| s.cursor).unwrap_or(0);

        let mut item_nodes: Vec<Node> = Vec::new();

        if panel.hints.filterable {
            let filter_text = state.map(|s| s.filter.as_str()).unwrap_or("");
            let filter_style = if self.mode == InteractionMode::Filter {
                Style::new().fg(t.resolve_color(t.colors.text))
            } else {
                Style::new().fg(t.resolve_color(t.colors.text_muted))
            };
            let cursor_mark = if self.mode == InteractionMode::Filter {
                "_"
            } else {
                ""
            };
            item_nodes.push(row([
                styled("  / ", Style::new().fg(t.resolve_color(t.colors.primary))),
                styled(filter_text, filter_style),
                styled(
                    cursor_mark,
                    Style::new().fg(t.resolve_color(t.colors.primary)),
                ),
            ]));
        }

        for (vi, &orig_idx) in visible_indices.iter().enumerate() {
            if let Some(item) = panel.items.get(orig_idx) {
                let is_cursor = vi == cursor;
                let is_checked = self.checked.contains(&orig_idx);

                let prefix = if panel.hints.multi_select {
                    if is_checked {
                        "[x] "
                    } else {
                        "[ ] "
                    }
                } else if is_cursor {
                    " >  "
                } else {
                    "    "
                };

                let label_style = if is_cursor {
                    Style::new().fg(t.resolve_color(t.colors.primary)).bold()
                } else {
                    Style::new().fg(t.resolve_color(t.colors.text))
                };

                if let Some(ref desc) = item.description {
                    item_nodes.push(row([
                        styled(format!("{prefix}{}", item.label), label_style),
                        styled(
                            format!("  {desc}"),
                            Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
                        ),
                    ]));
                } else {
                    item_nodes.push(styled(format!("{prefix}{}", item.label), label_style));
                }
            }
        }

        if panel.hints.allow_other {
            item_nodes.push(choice::other_row(cursor == visible_indices.len(), " >  "));
            if self.mode == InteractionMode::TextInput {
                item_nodes.push(choice::other_text_row("     ", &self.other_text));
            }
        }

        let key_style = Style::new().fg(t.resolve_color(t.colors.primary));
        let hint_style = Style::new().fg(t.resolve_color(t.colors.text_muted)).dim();

        let mut footer_nodes = vec![styled(
            " PANEL ",
            Style::new()
                .fg(t.resolve_color(t.colors.error))
                .bold()
                .reverse(),
        )];
        footer_nodes.extend([styled("  ↑/↓", key_style), styled(" move", hint_style)]);
        if panel.hints.multi_select {
            footer_nodes.extend([styled("  Space", key_style), styled(" toggle", hint_style)]);
        }
        if panel.hints.filterable {
            footer_nodes.extend([styled("  /", key_style), styled(" filter", hint_style)]);
        }
        footer_nodes.extend([
            styled("  Enter", key_style),
            styled(" accept", hint_style),
            styled("  Esc", key_style),
            styled(" cancel", hint_style),
        ]);

        col([
            text(""),
            styled(
                t.decorations
                    .half_block_bottom
                    .to_string()
                    .repeat(term_width),
                Style::new().fg(border_fg),
            ),
            header,
            col(item_nodes),
            styled(
                t.decorations.half_block_top.to_string().repeat(term_width),
                Style::new().fg(border_fg),
            ),
            row(footer_nodes),
            text(""),
        ])
    }
}
