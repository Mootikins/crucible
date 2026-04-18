use super::{InteractionMode, InteractionModal, InteractionModalOutput};
use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::interaction::{AskBatch, AskRequest, AskResponse, InteractionResponse};
use crucible_oil::node::{col, row, styled, text, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_ask_key(
        &mut self,
        key: KeyEvent,
        ask_request: AskRequest,
    ) -> InteractionModalOutput {
        let choices_count = ask_request.choices.as_ref().map(|c| c.len()).unwrap_or(0);
        let total_items = choices_count + if ask_request.allow_other { 1 } else { 0 };

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
                    if self.selected < choices_count {
                        let response = if ask_request.multi_select {
                            InteractionResponse::Ask(AskResponse::selected_many(
                                self.checked.iter().copied().collect::<Vec<_>>(),
                            ))
                        } else {
                            InteractionResponse::Ask(AskResponse::selected(self.selected))
                        };
                        InteractionModalOutput::AskResponse {
                            request_id: self.request_id.clone(),
                            response,
                        }
                    } else if ask_request.allow_other && self.selected == choices_count {
                        self.mode = InteractionMode::TextInput;
                        InteractionModalOutput::None
                    } else {
                        InteractionModalOutput::None
                    }
                }
                KeyCode::Tab if ask_request.allow_other => {
                    self.mode = InteractionMode::TextInput;
                    InteractionModalOutput::None
                }
                KeyCode::Char(' ') if ask_request.multi_select => {
                    Self::toggle_checked(&mut self.checked, self.selected);
                    InteractionModalOutput::None
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
                        InteractionResponse::Ask(AskResponse::other(self.other_text.clone()));
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

    pub(super) fn handle_ask_batch_key(
        &mut self,
        key: KeyEvent,
        batch: AskBatch,
    ) -> InteractionModalOutput {
        if self.current_question >= batch.questions.len() {
            return InteractionModalOutput::None;
        }

        let current_q = &batch.questions[self.current_question];
        let choices_count = current_q.choices.len();
        let total_items = choices_count + if current_q.allow_other { 1 } else { 0 };

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
                KeyCode::Char(' ') if current_q.multi_select => {
                    Self::toggle_checked(&mut self.checked, self.selected);
                    InteractionModalOutput::None
                }
                KeyCode::Tab => {
                    self.advance_batch_question(&batch);
                    InteractionModalOutput::None
                }
                KeyCode::BackTab => {
                    if self.current_question > 0 {
                        self.current_question -= 1;
                        self.selected = 0;
                        self.checked.clear();
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Enter => {
                    let is_last = self.current_question == batch.questions.len() - 1;
                    if is_last {
                        let response = InteractionResponse::AskBatch(
                            crucible_core::interaction::AskBatchResponse::new(batch.id),
                        );
                        InteractionModalOutput::AskResponse {
                            request_id: self.request_id.clone(),
                            response,
                        }
                    } else {
                        self.advance_batch_question(&batch);
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
            InteractionMode::TextInput => InteractionModalOutput::None,
        }
    }

    fn advance_batch_question(&mut self, batch: &AskBatch) {
        if self.current_question < batch.questions.len() - 1 {
            self.current_question += 1;
            self.selected = 0;
            self.checked.clear();
        }
    }

    pub(super) fn render_ask_interaction_single(
        &self,
        ask_request: &AskRequest,
        term_width: usize,
    ) -> Node {
        let question = &ask_request.question;
        let choices = ask_request.choices.as_deref().unwrap_or(&[]);
        let multi_select = ask_request.multi_select;
        let allow_other = ask_request.allow_other;

        self.render_ask_common(question, choices, multi_select, allow_other, 1, term_width)
    }

    pub(super) fn render_ask_interaction_batch(
        &self,
        batch: &AskBatch,
        term_width: usize,
    ) -> Node {
        if self.current_question >= batch.questions.len() {
            return Node::Empty;
        }

        let q = &batch.questions[self.current_question];
        self.render_ask_common(
            &q.question,
            &q.choices,
            q.multi_select,
            q.allow_other,
            batch.questions.len(),
            term_width,
        )
    }

    fn render_ask_common(
        &self,
        question: &str,
        choices: &[String],
        multi_select: bool,
        allow_other: bool,
        total_questions: usize,
        term_width: usize,
    ) -> Node {
        let t = crate::tui::oil::theme::active();
        let header_bg = t.resolve_color(t.colors.background);
        let footer_bg = t.resolve_color(t.colors.background);
        let top_border = styled(
            t.decorations
                .half_block_bottom
                .to_string()
                .repeat(term_width),
            Style::new().fg(t.resolve_color(t.colors.background)),
        );
        let bottom_border = styled(
            t.decorations.half_block_top.to_string().repeat(term_width),
            Style::new().fg(t.resolve_color(t.colors.background)),
        );

        let header_text = if total_questions > 1 {
            format!(
                " {} (Question {}/{}) ",
                question,
                self.current_question + 1,
                total_questions
            )
        } else {
            format!(" {} ", question)
        };
        let header_padding = " ".repeat(term_width.saturating_sub(header_text.len()));
        let header = styled(
            format!("{}{}", header_text, header_padding),
            Style::new().bg(header_bg).bold(),
        );

        let mut choice_nodes: Vec<Node> = Vec::new();

        for (i, choice) in choices.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if multi_select {
                let is_checked = self.checked.contains(&i);
                if is_checked {
                    "[x]"
                } else {
                    "[ ]"
                }
            } else if is_selected {
                " > "
            } else {
                "   "
            };
            let style = if is_selected {
                Style::new().fg(t.resolve_color(t.colors.primary)).bold()
            } else {
                Style::new().fg(t.resolve_color(t.colors.text))
            };
            choice_nodes.push(styled(format!("{}{}", prefix, choice), style));
        }

        if allow_other {
            let other_idx = choices.len();
            let is_selected = self.selected == other_idx;
            let prefix = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::new().fg(t.resolve_color(t.colors.primary)).bold()
            } else {
                Style::new()
                    .fg(t.resolve_color(t.colors.text_muted))
                    .italic()
            };
            choice_nodes.push(styled(format!("{}Other...", prefix), style));
        }

        let key_style = Style::new()
            .bg(footer_bg)
            .fg(t.resolve_color(t.colors.primary));
        let sep_style = Style::new()
            .bg(footer_bg)
            .fg(t.resolve_color(t.colors.text_muted));
        let text_style = Style::new()
            .bg(footer_bg)
            .fg(t.resolve_color(t.colors.text))
            .dim();

        let footer_content = row([
            styled(" ", text_style),
            styled("↑/↓", key_style),
            styled(" navigate ", text_style),
            styled(t.decorations.separator_char.clone(), sep_style),
            styled(" ", text_style),
            styled("Enter", key_style),
            styled(" select ", text_style),
            styled(t.decorations.separator_char.clone(), sep_style),
            styled(" ", text_style),
            styled("Esc", key_style),
            styled(" cancel ", text_style),
        ]);

        let footer_padding = styled(
            " ".repeat(term_width.saturating_sub(45)),
            Style::new().bg(footer_bg),
        );
        let footer = row([footer_content, footer_padding]);

        if self.mode == InteractionMode::TextInput {
            let input_line = row([
                styled(
                    "   Enter text: ",
                    Style::new().fg(t.resolve_color(t.colors.text_muted)),
                ),
                styled(
                    &self.other_text,
                    Style::new().fg(t.resolve_color(t.colors.text)),
                ),
                styled("_", Style::new().fg(t.resolve_color(t.colors.primary))),
            ]);
            choice_nodes.push(input_line);
        }

        let choices_col = col(choice_nodes);

        col([
            text(""),
            top_border,
            header,
            choices_col,
            bottom_border,
            footer,
            text(""),
        ])
    }
}
