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
        let state = match &mut self.panel_state {
            Some(s) => s,
            None => return InteractionModalOutput::None,
        };

        match self.mode {
            InteractionMode::Selecting => match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    } else if !state.visible.is_empty() {
                        state.cursor = state.visible.len() - 1;
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    if !state.visible.is_empty() {
                        state.cursor = (state.cursor + 1) % state.visible.len();
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char(' ') if panel.hints.multi_select => {
                    if let Some(&orig_idx) = state.visible.get(state.cursor) {
                        if state.selected.contains(&orig_idx) {
                            state.selected.retain(|&i| i != orig_idx);
                            self.checked.remove(&orig_idx);
                        } else {
                            state.selected.push(orig_idx);
                            self.checked.insert(orig_idx);
                        }
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Enter => {
                    if panel.hints.multi_select {
                        let result = PanelResult::selected(state.selected.iter().copied());
                        InteractionModalOutput::AskResponse {
                            request_id: self.request_id.clone(),
                            response: InteractionResponse::Panel(result),
                        }
                    } else if let Some(&orig_idx) = state.visible.get(state.cursor) {
                        if panel.hints.allow_other && orig_idx == panel.items.len() {
                            self.mode = InteractionMode::TextInput;
                            InteractionModalOutput::None
                        } else {
                            let result = PanelResult::selected(std::iter::once(orig_idx));
                            InteractionModalOutput::AskResponse {
                                request_id: self.request_id.clone(),
                                response: InteractionResponse::Panel(result),
                            }
                        }
                    } else {
                        InteractionModalOutput::None
                    }
                }
                KeyCode::Char('/') if panel.hints.filterable => {
                    self.mode = InteractionMode::TextInput;
                    InteractionModalOutput::None
                }
                KeyCode::Esc => InteractionModalOutput::AskResponse {
                    request_id: self.request_id.clone(),
                    response: InteractionResponse::Panel(PanelResult::cancelled()),
                },
                KeyCode::Char('c') if Self::is_ctrl_c(key) => InteractionModalOutput::AskResponse {
                    request_id: self.request_id.clone(),
                    response: InteractionResponse::Panel(PanelResult::cancelled()),
                },
                _ => InteractionModalOutput::None,
            },
            InteractionMode::TextInput => {
                if panel.hints.filterable {
                    self.handle_panel_filter_key(key, &panel)
                } else if panel.hints.allow_other {
                    match key.code {
                        KeyCode::Enter => {
                            let result = PanelResult::other(self.other_text.clone());
                            InteractionModalOutput::AskResponse {
                                request_id: self.request_id.clone(),
                                response: InteractionResponse::Panel(result),
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
                    }
                } else {
                    InteractionModalOutput::None
                }
            }
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
                InteractionModalOutput::None
            }
            KeyCode::Backspace => {
                state.filter.pop();
                Self::update_panel_visible(state, panel);
                InteractionModalOutput::None
            }
            KeyCode::Char(c) => {
                state.filter.push(c);
                Self::update_panel_visible(state, panel);
                InteractionModalOutput::None
            }
            _ => InteractionModalOutput::None,
        }
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
        if state.cursor >= state.visible.len() {
            state.cursor = state.visible.len().saturating_sub(1);
        }
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
            let filter_style = if self.mode == InteractionMode::TextInput {
                Style::new().fg(t.resolve_color(t.colors.text))
            } else {
                Style::new().fg(t.resolve_color(t.colors.text_muted))
            };
            let cursor_mark = if self.mode == InteractionMode::TextInput {
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
            let other_cursor = visible_indices.len();
            let is_cursor = cursor == other_cursor;
            let prefix = if is_cursor { " >  " } else { "    " };
            let style = if is_cursor {
                Style::new().fg(t.resolve_color(t.colors.primary)).bold()
            } else {
                Style::new()
                    .fg(t.resolve_color(t.colors.text_muted))
                    .italic()
            };
            item_nodes.push(styled(format!("{prefix}Other..."), style));

            if self.mode == InteractionMode::TextInput && !panel.hints.filterable {
                item_nodes.push(row([
                    styled(
                        "     Enter text: ",
                        Style::new().fg(t.resolve_color(t.colors.text_muted)),
                    ),
                    styled(
                        &self.other_text,
                        Style::new().fg(t.resolve_color(t.colors.text)),
                    ),
                    styled("_", Style::new().fg(t.resolve_color(t.colors.primary))),
                ]));
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
