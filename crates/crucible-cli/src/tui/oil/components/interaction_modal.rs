//! Interaction modal component for permission and ask requests.
//!
//! Follows Elm-style architecture: Msg → update → Output.

use crate::tui::oil::node::{col, row, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{
    AskBatch, AskRequest, AskResponse, EditRequest, EditResponse, InteractionRequest,
    InteractionResponse, InteractivePanel, PanelResult, PanelState, PermAction, PermRequest,
    PermResponse, PermissionScope, PopupRequest, PopupResponse, ShowRequest,
};
use crucible_core::types::PopupEntry;
use std::collections::HashSet;

/// Mode for interaction modal input handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    /// Navigating/selecting from choices.
    #[default]
    Selecting,
    /// Free-text input (for "Other" option).
    TextInput,
}

/// Messages that can be sent to the interaction modal.
#[derive(Debug, Clone)]
pub enum InteractionModalMsg {
    Key(KeyEvent),
}

/// Output from the interaction modal's update function.
#[derive(Debug, Clone)]
pub enum InteractionModalOutput {
    /// No action needed, continue.
    None,
    /// Close the modal (cancelled).
    Close,
    /// Permission response ready to send.
    PermissionResponse {
        request_id: String,
        response: PermResponse,
    },
    /// Ask response ready to send.
    AskResponse {
        request_id: String,
        response: InteractionResponse,
    },
    /// Toggle diff preview visibility.
    ToggleDiff,
    /// Show a notification toast.
    Notify(String),
}

/// State for the interaction modal (Ask, AskBatch, Permission, etc.).
pub struct InteractionModal {
    /// Correlates with response sent back to daemon.
    pub request_id: String,
    /// The request being displayed.
    pub request: InteractionRequest,
    /// Current selection index for choice-based requests.
    pub selected: usize,
    /// Filter text for filterable panels (future use).
    pub filter: String,
    /// Free-text input buffer for "Other" option.
    pub other_text: String,
    /// Current input mode.
    pub mode: InteractionMode,
    /// Checked items for multi-select mode.
    pub checked: HashSet<usize>,
    /// Current question index for multi-question batches.
    pub current_question: usize,
    /// Track if "Other" text was previously entered (for dim rendering when deselected).
    pub other_text_preserved: bool,
    /// Answers per question for AskBatch (Vec of selected indices per question).
    pub batch_answers: Vec<HashSet<usize>>,
    /// Other text per question for AskBatch.
    pub batch_other_texts: Vec<String>,
    /// Whether the diff preview is collapsed (for permission requests with file changes).
    pub diff_collapsed: bool,
    /// Scroll offset for Show and Panel views.
    pub scroll_offset: usize,
    /// Lines of content for Edit interaction.
    pub edit_lines: Vec<String>,
    /// Cursor line position in Edit interaction.
    pub edit_cursor_line: usize,
    /// Cursor column position in Edit interaction.
    pub edit_cursor_col: usize,
    /// Panel tracking state for Panel interaction.
    pub panel_state: Option<PanelState>,
}

impl InteractionModal {
    pub fn new(request_id: String, request: InteractionRequest, show_diff: bool) -> Self {
        let edit_lines = if let InteractionRequest::Edit(ref edit) = request {
            edit.content.lines().map(String::from).collect()
        } else {
            Vec::new()
        };
        let panel_state = if let InteractionRequest::Panel(ref panel) = request {
            Some(PanelState::initial(panel))
        } else {
            None
        };
        let checked = if let InteractionRequest::Panel(ref panel) = request {
            panel.hints.initial_selection.iter().copied().collect()
        } else {
            HashSet::new()
        };
        Self {
            request_id,
            request,
            selected: 0,
            filter: String::new(),
            other_text: String::new(),
            mode: InteractionMode::Selecting,
            checked,
            current_question: 0,
            other_text_preserved: false,
            batch_answers: Vec::new(),
            batch_other_texts: Vec::new(),
            diff_collapsed: !show_diff,
            scroll_offset: 0,
            edit_lines,
            edit_cursor_line: 0,
            edit_cursor_col: 0,
            panel_state,
        }
    }

    /// Process a message and return the output action.
    pub fn update(&mut self, msg: InteractionModalMsg) -> InteractionModalOutput {
        match msg {
            InteractionModalMsg::Key(key) => self.handle_key(key),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
        match &self.request {
            InteractionRequest::Ask(ask) => self.handle_ask_key(key, ask.clone()),
            InteractionRequest::AskBatch(batch) => self.handle_ask_batch_key(key, batch.clone()),
            InteractionRequest::Permission(perm) => self.handle_perm_key(key, perm.clone()),
            InteractionRequest::Show(_) => self.handle_show_key(key),
            InteractionRequest::Popup(popup) => self.handle_popup_key(key, popup.clone()),
            InteractionRequest::Edit(_) => self.handle_edit_key(key),
            InteractionRequest::Panel(panel) => self.handle_panel_key(key, panel.clone()),
        }
    }

    fn handle_ask_key(&mut self, key: KeyEvent, ask_request: AskRequest) -> InteractionModalOutput {
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

    fn handle_ask_batch_key(&mut self, key: KeyEvent, batch: AskBatch) -> InteractionModalOutput {
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

    fn handle_perm_key(
        &mut self,
        key: KeyEvent,
        perm_request: PermRequest,
    ) -> InteractionModalOutput {
        const TOTAL_OPTIONS: usize = 3;

        match self.mode {
            InteractionMode::Selecting => match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.selected = Self::wrap_selection(self.selected, -1, TOTAL_OPTIONS);
                    InteractionModalOutput::None
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    self.selected = Self::wrap_selection(self.selected, 1, TOTAL_OPTIONS);
                    InteractionModalOutput::None
                }
                KeyCode::Enter => self.handle_perm_confirm(&perm_request),
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    InteractionModalOutput::PermissionResponse {
                        request_id: self.request_id.clone(),
                        response: PermResponse::allow(),
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    InteractionModalOutput::PermissionResponse {
                        request_id: self.request_id.clone(),
                        response: PermResponse::deny(),
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    InteractionModalOutput::PermissionResponse {
                        request_id: self.request_id.clone(),
                        response: PermResponse::allow_pattern(
                            perm_request.suggested_pattern(),
                            PermissionScope::Project,
                        ),
                    }
                }
                KeyCode::Tab => {
                    self.mode = InteractionMode::TextInput;
                    if self.selected == 2 {
                        self.other_text = perm_request.suggested_pattern();
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.diff_collapsed = !self.diff_collapsed;
                    InteractionModalOutput::ToggleDiff
                }
                KeyCode::Esc | KeyCode::Char('c')
                    if key.code == KeyCode::Esc || Self::is_ctrl_c(key) =>
                {
                    InteractionModalOutput::PermissionResponse {
                        request_id: self.request_id.clone(),
                        response: PermResponse::deny(),
                    }
                }
                _ => InteractionModalOutput::None,
            },
            InteractionMode::TextInput => match key.code {
                KeyCode::Enter => self.handle_perm_text_confirm(&perm_request),
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

    fn handle_perm_confirm(&self, perm_request: &PermRequest) -> InteractionModalOutput {
        match self.selected {
            0 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow(),
            },
            1 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::deny(),
            },
            2 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow_pattern(
                    perm_request.suggested_pattern(),
                    PermissionScope::Project,
                ),
            },
            _ => InteractionModalOutput::None,
        }
    }

    fn handle_perm_text_confirm(&self, _perm_request: &PermRequest) -> InteractionModalOutput {
        let text = self.other_text.trim().to_string();
        match self.selected {
            0 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow(),
            },
            1 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: if text.is_empty() {
                    PermResponse::deny()
                } else {
                    PermResponse::deny_with_reason(text)
                },
            },
            2 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: if text.is_empty() {
                    PermResponse::deny()
                } else {
                    PermResponse::allow_pattern(text, PermissionScope::Project)
                },
            },
            _ => InteractionModalOutput::None,
        }
    }

    fn wrap_selection(selected: usize, delta: isize, total: usize) -> usize {
        if delta < 0 && selected == 0 {
            total - 1
        } else if delta < 0 {
            selected - 1
        } else {
            (selected + 1) % total
        }
    }

    fn toggle_checked(set: &mut HashSet<usize>, value: usize) {
        if set.contains(&value) {
            set.remove(&value);
        } else {
            set.insert(value);
        }
    }

    fn is_ctrl_c(key: KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }

    pub fn view(&self, term_width: usize, queue_size: usize) -> Node {
        match &self.request {
            InteractionRequest::Permission(perm) => {
                self.render_perm_interaction(perm, term_width, queue_size)
            }
            InteractionRequest::Ask(ask) => self.render_ask_interaction_single(ask, term_width),
            InteractionRequest::AskBatch(batch) => {
                self.render_ask_interaction_batch(batch, term_width)
            }
            InteractionRequest::Show(show) => self.render_show_interaction(show, term_width),
            InteractionRequest::Popup(popup) => self.render_popup_interaction(popup, term_width),
            InteractionRequest::Edit(edit) => self.render_edit_interaction(edit, term_width),
            InteractionRequest::Panel(panel) => self.render_panel_interaction(panel, term_width),
        }
    }

    fn render_perm_interaction(
        &self,
        perm_request: &PermRequest,
        term_width: usize,
        queue_size: usize,
    ) -> Node {
        let theme = ThemeTokens::default_ref();
        let panel_bg = theme.input_bg;
        let border_fg = theme.border;

        let (type_label, action_detail, is_write) = match &perm_request.action {
            PermAction::Bash { tokens } => ("BASH", tokens.join(" "), false),
            PermAction::Read { segments } => ("READ", format!("/{}", segments.join("/")), false),
            PermAction::Write { segments } => ("WRITE", format!("/{}", segments.join("/")), true),
            PermAction::Tool { name, args } => {
                let args_str = Self::prettify_tool_args(args);
                ("TOOL", format!("{} {}", name, args_str), false)
            }
        };

        let queue_total = 1 + queue_size;

        let pad_line = |content: &str, visible_len: usize| -> Node {
            let pad = " ".repeat(term_width.saturating_sub(visible_len));
            styled(
                format!("{content}{pad}"),
                Style::new().bg(panel_bg).fg(theme.overlay_bright),
            )
        };

        let mut lines: Vec<Node> = Vec::new();

        lines.push(styled(
            "\u{2584}".repeat(term_width),
            Style::new().fg(border_fg),
        ));

        let action_text = if queue_total > 1 {
            format!("  [{}/{}] {}", 1, queue_total, action_detail)
        } else {
            format!("  {}", action_detail)
        };
        lines.push(pad_line(&action_text, action_text.len()));

        lines.push(styled(" ".repeat(term_width), Style::new().bg(panel_bg)));

        let options: [(&str, &str); 3] = [("y", "Yes"), ("n", "No"), ("a", "Allowlist")];

        for (i, (key, label)) in options.iter().enumerate() {
            let is_selected = i == self.selected;
            if is_selected {
                let content = format!("  > [{}] {}", key, label);
                let pad = " ".repeat(term_width.saturating_sub(content.len()));
                lines.push(styled(
                    format!("{content}{pad}"),
                    Style::new().bg(panel_bg).fg(theme.text_accent).bold(),
                ));
            } else {
                let key_part = format!("    [{}]", key);
                let label_part = format!(" {}", label);
                let pad = " ".repeat(term_width.saturating_sub(key_part.len() + label_part.len()));
                lines.push(row([
                    styled(key_part, Style::new().bg(panel_bg).fg(theme.overlay_text)),
                    styled(
                        label_part,
                        Style::new().bg(panel_bg).fg(theme.overlay_bright),
                    ),
                    styled(pad, Style::new().bg(panel_bg)),
                ]));
            }

            if is_selected && self.mode == InteractionMode::TextInput {
                let prompt = format!("      > {}_", self.other_text);
                let pad = " ".repeat(term_width.saturating_sub(prompt.len()));
                lines.push(styled(
                    format!("{prompt}{pad}"),
                    Style::new().bg(panel_bg).fg(theme.text_primary),
                ));
            }
        }

        lines.push(styled(
            "\u{2580}".repeat(term_width),
            Style::new().fg(border_fg),
        ));

        let key_style = theme.overlay_key(theme.error);
        let hint_style = theme.overlay_hint();

        let footer_nodes: Vec<Node> = if self.mode == InteractionMode::TextInput {
            vec![
                styled(" PERMISSION ", theme.permission_badge()),
                styled(format!(" {} ", type_label), theme.permission_type()),
                styled("  Enter", key_style),
                styled(" send", hint_style),
                styled("  Esc", key_style),
                styled(" back", hint_style),
            ]
        } else {
            let mut nodes = vec![
                styled(" PERMISSION ", theme.permission_badge()),
                styled(format!(" {} ", type_label), theme.permission_type()),
                styled("  y/n/a", key_style),
                styled(" options", hint_style),
                styled("  ↑↓", key_style),
                styled(" move", hint_style),
                styled("  Tab", key_style),
                styled(" entry", hint_style),
            ];
            if is_write {
                nodes.push(styled("  h", key_style));
                nodes.push(styled(" diff", hint_style));
            }
            nodes.push(styled("  Esc", key_style));
            nodes.push(styled(" cancel", hint_style));
            nodes
        };

        lines.push(row(footer_nodes));

        col(lines)
    }

    fn render_ask_interaction_single(&self, ask_request: &AskRequest, term_width: usize) -> Node {
        let question = &ask_request.question;
        let choices = ask_request.choices.as_deref().unwrap_or(&[]);
        let multi_select = ask_request.multi_select;
        let allow_other = ask_request.allow_other;

        self.render_ask_common(question, choices, multi_select, allow_other, 1, term_width)
    }

    fn render_ask_interaction_batch(&self, batch: &AskBatch, term_width: usize) -> Node {
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
        let theme = ThemeTokens::default_ref();
        let header_bg = theme.input_bg;
        let footer_bg = theme.input_bg;
        let top_border = styled("▄".repeat(term_width), Style::new().fg(theme.input_bg));
        let bottom_border = styled("▀".repeat(term_width), Style::new().fg(theme.input_bg));

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
                Style::new().fg(theme.text_accent).bold()
            } else {
                Style::new().fg(theme.text_primary)
            };
            choice_nodes.push(styled(format!("{}{}", prefix, choice), style));
        }

        if allow_other {
            let other_idx = choices.len();
            let is_selected = self.selected == other_idx;
            let prefix = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::new().fg(theme.text_accent).bold()
            } else {
                Style::new().fg(theme.text_muted).italic()
            };
            choice_nodes.push(styled(format!("{}Other...", prefix), style));
        }

        let key_style = Style::new().bg(footer_bg).fg(theme.text_accent);
        let sep_style = Style::new().bg(footer_bg).fg(theme.text_muted);
        let text_style = Style::new().bg(footer_bg).fg(theme.text_primary).dim();

        let footer_content = row([
            styled(" ", text_style),
            styled("↑/↓", key_style),
            styled(" navigate ", text_style),
            styled("│", sep_style),
            styled(" ", text_style),
            styled("Enter", key_style),
            styled(" select ", text_style),
            styled("│", sep_style),
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
                styled("   Enter text: ", Style::new().fg(theme.text_muted)),
                styled(&self.other_text, Style::new().fg(theme.text_primary)),
                styled("_", Style::new().fg(theme.text_accent)),
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

    // ─── Show handler ──────────────────────────────────────────────────

    fn handle_show_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
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

    fn render_show_interaction(&self, show: &ShowRequest, term_width: usize) -> Node {
        let theme = ThemeTokens::default_ref();
        let panel_bg = theme.input_bg;
        let border_fg = theme.input_bg;

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
        lines.push(styled("▄".repeat(term_width), Style::new().fg(border_fg)));
        lines.push(title);

        for line in visible {
            let pad = " ".repeat(term_width.saturating_sub(line.len() + 2));
            lines.push(styled(
                format!("  {line}{pad}"),
                Style::new().bg(panel_bg).fg(theme.text_primary),
            ));
        }

        lines.push(styled("▀".repeat(term_width), Style::new().fg(border_fg)));

        let key_style = Style::new().fg(theme.text_accent);
        let hint_style = Style::new().fg(theme.text_muted).dim();
        let scroll_info = format!(" {}/{}", self.scroll_offset + 1, content_lines.len().max(1));

        lines.push(row([
            styled(" SHOW ", theme.permission_badge()),
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

    // ─── Popup handler ─────────────────────────────────────────────────

    fn handle_popup_key(&mut self, key: KeyEvent, popup: PopupRequest) -> InteractionModalOutput {
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

    fn render_popup_interaction(&self, popup: &PopupRequest, term_width: usize) -> Node {
        let theme = ThemeTokens::default_ref();
        let panel_bg = theme.input_bg;
        let border_fg = theme.input_bg;

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
                Style::new().fg(theme.text_accent).bold()
            } else {
                Style::new().fg(theme.text_primary)
            };
            if let Some(ref desc) = entry.description {
                choice_nodes.push(row([
                    styled(format!("{prefix}{}", entry.label), label_style),
                    styled(format!("  {desc}"), Style::new().fg(theme.text_muted).dim()),
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
                Style::new().fg(theme.text_accent).bold()
            } else {
                Style::new().fg(theme.text_muted).italic()
            };
            choice_nodes.push(styled(format!("{prefix}Other..."), style));
        }

        if self.mode == InteractionMode::TextInput {
            choice_nodes.push(row([
                styled("   Enter text: ", Style::new().fg(theme.text_muted)),
                styled(&self.other_text, Style::new().fg(theme.text_primary)),
                styled("_", Style::new().fg(theme.text_accent)),
            ]));
        }

        let key_style = Style::new().fg(theme.text_accent);
        let hint_style = Style::new().fg(theme.text_muted).dim();

        col([
            text(""),
            styled("▄".repeat(term_width), Style::new().fg(border_fg)),
            title,
            col(choice_nodes),
            styled("▀".repeat(term_width), Style::new().fg(border_fg)),
            row([
                styled(" POPUP ", theme.permission_badge()),
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

    // ─── Edit handler ──────────────────────────────────────────────────

    fn handle_edit_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            let content = self.edit_lines.join("\n");
            return InteractionModalOutput::AskResponse {
                request_id: self.request_id.clone(),
                response: InteractionResponse::Edit(EditResponse::new(content)),
            };
        }

        match self.mode {
            InteractionMode::Selecting => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.edit_cursor_line < self.edit_lines.len().saturating_sub(1) {
                        self.edit_cursor_line += 1;
                        self.edit_cursor_col =
                            self.edit_cursor_col.min(self.current_edit_line_len());
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.edit_cursor_line = self.edit_cursor_line.saturating_sub(1);
                    self.edit_cursor_col = self.edit_cursor_col.min(self.current_edit_line_len());
                    InteractionModalOutput::None
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.edit_cursor_col = self.edit_cursor_col.saturating_sub(1);
                    InteractionModalOutput::None
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    if self.edit_cursor_col < self.current_edit_line_len() {
                        self.edit_cursor_col += 1;
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char('i') => {
                    self.mode = InteractionMode::TextInput;
                    InteractionModalOutput::None
                }
                KeyCode::Char('a') => {
                    self.mode = InteractionMode::TextInput;
                    if self.edit_cursor_col < self.current_edit_line_len() {
                        self.edit_cursor_col += 1;
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char('o') => {
                    self.edit_cursor_line += 1;
                    self.edit_lines.insert(self.edit_cursor_line, String::new());
                    self.edit_cursor_col = 0;
                    self.mode = InteractionMode::TextInput;
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
                KeyCode::Esc => {
                    self.mode = InteractionMode::Selecting;
                    InteractionModalOutput::None
                }
                KeyCode::Enter => {
                    let line = &self.edit_lines[self.edit_cursor_line];
                    let remainder = line[self.edit_cursor_col..].to_string();
                    self.edit_lines[self.edit_cursor_line] =
                        line[..self.edit_cursor_col].to_string();
                    self.edit_cursor_line += 1;
                    self.edit_lines.insert(self.edit_cursor_line, remainder);
                    self.edit_cursor_col = 0;
                    InteractionModalOutput::None
                }
                KeyCode::Backspace => {
                    if self.edit_cursor_col > 0 {
                        self.edit_lines[self.edit_cursor_line].remove(self.edit_cursor_col - 1);
                        self.edit_cursor_col -= 1;
                    } else if self.edit_cursor_line > 0 {
                        let removed = self.edit_lines.remove(self.edit_cursor_line);
                        self.edit_cursor_line -= 1;
                        self.edit_cursor_col = self.edit_lines[self.edit_cursor_line].len();
                        self.edit_lines[self.edit_cursor_line].push_str(&removed);
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Left => {
                    self.edit_cursor_col = self.edit_cursor_col.saturating_sub(1);
                    InteractionModalOutput::None
                }
                KeyCode::Right => {
                    if self.edit_cursor_col < self.current_edit_line_len() {
                        self.edit_cursor_col += 1;
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Up => {
                    if self.edit_cursor_line > 0 {
                        self.edit_cursor_line -= 1;
                        self.edit_cursor_col =
                            self.edit_cursor_col.min(self.current_edit_line_len());
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Down => {
                    if self.edit_cursor_line < self.edit_lines.len().saturating_sub(1) {
                        self.edit_cursor_line += 1;
                        self.edit_cursor_col =
                            self.edit_cursor_col.min(self.current_edit_line_len());
                    }
                    InteractionModalOutput::None
                }
                KeyCode::Char(c) => {
                    if self.edit_cursor_line < self.edit_lines.len() {
                        self.edit_lines[self.edit_cursor_line].insert(self.edit_cursor_col, c);
                        self.edit_cursor_col += 1;
                    }
                    InteractionModalOutput::None
                }
                _ => InteractionModalOutput::None,
            },
        }
    }

    fn current_edit_line_len(&self) -> usize {
        self.edit_lines
            .get(self.edit_cursor_line)
            .map(|l| l.len())
            .unwrap_or(0)
    }

    fn render_edit_interaction(&self, edit: &EditRequest, term_width: usize) -> Node {
        let theme = ThemeTokens::default_ref();
        let panel_bg = theme.input_bg;
        let border_fg = theme.input_bg;

        let format_label = match edit.format {
            crucible_core::interaction::ArtifactFormat::Markdown => "MD",
            crucible_core::interaction::ArtifactFormat::Code => "CODE",
            crucible_core::interaction::ArtifactFormat::Json => "JSON",
            crucible_core::interaction::ArtifactFormat::Plain => "TEXT",
        };

        let hint_text = edit
            .hint
            .as_deref()
            .map(|h| format!("  {h}"))
            .unwrap_or_default();
        let header_text = format!(" EDIT [{format_label}]{hint_text} ");
        let header_pad = " ".repeat(term_width.saturating_sub(header_text.len()));
        let header = styled(
            format!("{header_text}{header_pad}"),
            Style::new().bg(panel_bg).bold(),
        );

        let visible_start = if self.edit_cursor_line >= 20 {
            self.edit_cursor_line - 19
        } else {
            0
        };
        let visible_end = (visible_start + 20).min(self.edit_lines.len());

        let mut content_nodes: Vec<Node> = Vec::new();
        for i in visible_start..visible_end {
            let line = &self.edit_lines[i];
            let line_num = format!("{:>4} ", i + 1);
            let is_cursor_line = i == self.edit_cursor_line;

            if is_cursor_line {
                let (before, cursor_char, after) = if self.edit_cursor_col < line.len() {
                    let before = &line[..self.edit_cursor_col];
                    let cursor = &line[self.edit_cursor_col..self.edit_cursor_col + 1];
                    let after = &line[self.edit_cursor_col + 1..];
                    (before.to_string(), cursor.to_string(), after.to_string())
                } else {
                    (line.clone(), " ".to_string(), String::new())
                };
                content_nodes.push(row([
                    styled(
                        line_num,
                        Style::new().bg(panel_bg).fg(theme.text_muted).dim(),
                    ),
                    styled(before, Style::new().bg(panel_bg).fg(theme.text_primary)),
                    styled(
                        cursor_char,
                        Style::new().bg(theme.text_accent).fg(theme.input_bg).bold(),
                    ),
                    styled(after, Style::new().bg(panel_bg).fg(theme.text_primary)),
                ]));
            } else {
                content_nodes.push(row([
                    styled(
                        line_num,
                        Style::new().bg(panel_bg).fg(theme.text_muted).dim(),
                    ),
                    styled(line, Style::new().bg(panel_bg).fg(theme.text_primary)),
                ]));
            }
        }

        let key_style = Style::new().fg(theme.text_accent);
        let hint_style = Style::new().fg(theme.text_muted).dim();
        let mode_label = match self.mode {
            InteractionMode::Selecting => "NORMAL",
            InteractionMode::TextInput => "INSERT",
        };

        let footer = if self.mode == InteractionMode::TextInput {
            row([
                styled(format!(" {mode_label} "), theme.permission_badge()),
                styled("  Ctrl+S", key_style),
                styled(" save", hint_style),
                styled("  Esc", key_style),
                styled(" normal", hint_style),
            ])
        } else {
            row([
                styled(format!(" {mode_label} "), theme.permission_badge()),
                styled("  i", key_style),
                styled(" insert", hint_style),
                styled("  Ctrl+S", key_style),
                styled(" save", hint_style),
                styled("  Esc", key_style),
                styled(" cancel", hint_style),
            ])
        };

        col([
            text(""),
            styled("▄".repeat(term_width), Style::new().fg(border_fg)),
            header,
            col(content_nodes),
            styled("▀".repeat(term_width), Style::new().fg(border_fg)),
            footer,
            text(""),
        ])
    }

    // ─── Panel handler ─────────────────────────────────────────────────

    fn handle_panel_key(
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

    fn render_panel_interaction(&self, panel: &InteractivePanel, term_width: usize) -> Node {
        let theme = ThemeTokens::default_ref();
        let panel_bg = theme.input_bg;
        let border_fg = theme.input_bg;

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
                Style::new().fg(theme.text_primary)
            } else {
                Style::new().fg(theme.text_muted)
            };
            let cursor_mark = if self.mode == InteractionMode::TextInput {
                "_"
            } else {
                ""
            };
            item_nodes.push(row([
                styled("  / ", Style::new().fg(theme.text_accent)),
                styled(filter_text, filter_style),
                styled(cursor_mark, Style::new().fg(theme.text_accent)),
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
                    Style::new().fg(theme.text_accent).bold()
                } else {
                    Style::new().fg(theme.text_primary)
                };

                if let Some(ref desc) = item.description {
                    item_nodes.push(row([
                        styled(format!("{prefix}{}", item.label), label_style),
                        styled(format!("  {desc}"), Style::new().fg(theme.text_muted).dim()),
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
                Style::new().fg(theme.text_accent).bold()
            } else {
                Style::new().fg(theme.text_muted).italic()
            };
            item_nodes.push(styled(format!("{prefix}Other..."), style));

            if self.mode == InteractionMode::TextInput && !panel.hints.filterable {
                item_nodes.push(row([
                    styled("     Enter text: ", Style::new().fg(theme.text_muted)),
                    styled(&self.other_text, Style::new().fg(theme.text_primary)),
                    styled("_", Style::new().fg(theme.text_accent)),
                ]));
            }
        }

        let key_style = Style::new().fg(theme.text_accent);
        let hint_style = Style::new().fg(theme.text_muted).dim();

        let mut footer_nodes = vec![styled(" PANEL ", theme.permission_badge())];
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
            styled("▄".repeat(term_width), Style::new().fg(border_fg)),
            header,
            col(item_nodes),
            styled("▀".repeat(term_width), Style::new().fg(border_fg)),
            row(footer_nodes),
            text(""),
        ])
    }

    fn prettify_tool_args(args: &serde_json::Value) -> String {
        match args {
            serde_json::Value::Object(map) => {
                let pairs: Vec<String> = map
                    .iter()
                    .take(3)
                    .map(|(k, v)| {
                        let v_str = match v {
                            serde_json::Value::String(s) => {
                                if s.len() > 30 {
                                    let truncated: String = s.chars().take(27).collect();
                                    format!("\"{}...\"", truncated)
                                } else {
                                    format!("\"{}\"", s)
                                }
                            }
                            _ => v.to_string(),
                        };
                        format!("{}={}", k, v_str)
                    })
                    .collect();
                if map.len() > 3 {
                    format!("({}, ...)", pairs.join(", "))
                } else {
                    format!("({})", pairs.join(", "))
                }
            }
            _ => args.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_perm_modal_allow() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('y'))));
        match output {
            InteractionModalOutput::PermissionResponse {
                request_id,
                response,
            } => {
                assert_eq!(request_id, "req-1");
                assert!(response.allowed);
            }
            _ => panic!("Expected PermissionResponse"),
        }
    }

    #[test]
    fn test_perm_modal_deny() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('n'))));
        match output {
            InteractionModalOutput::PermissionResponse {
                request_id,
                response,
            } => {
                assert_eq!(request_id, "req-1");
                assert!(!response.allowed);
            }
            _ => panic!("Expected PermissionResponse"),
        }
    }

    #[test]
    fn test_perm_modal_navigation() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        assert_eq!(modal.selected, 0);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
        assert_eq!(modal.selected, 0);
    }

    #[test]
    fn test_ask_modal_selection() {
        let ask = AskRequest::new("Choose one").choices(["A", "B", "C"]);
        let mut modal =
            InteractionModal::new("req-2".to_string(), InteractionRequest::Ask(ask), true);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        match output {
            InteractionModalOutput::AskResponse {
                request_id,
                response,
            } => {
                assert_eq!(request_id, "req-2");
                match response {
                    InteractionResponse::Ask(ask_resp) => {
                        assert_eq!(ask_resp.selected, vec![1]);
                    }
                    _ => panic!("Expected Ask response"),
                }
            }
            _ => panic!("Expected AskResponse"),
        }
    }

    #[test]
    fn test_ask_modal_cancel_esc() {
        let ask = AskRequest::new("Choose one").choices(["A", "B"]);
        let mut modal =
            InteractionModal::new("req-3".to_string(), InteractionRequest::Ask(ask), true);

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => {
                assert!(matches!(response, InteractionResponse::Cancelled));
            }
            _ => panic!("Expected AskResponse with Cancelled"),
        }
    }

    #[test]
    fn test_ask_modal_cancel_ctrl_c() {
        let ask = AskRequest::new("Choose one").choices(["A", "B"]);
        let mut modal =
            InteractionModal::new("req-4".to_string(), InteractionRequest::Ask(ask), true);

        let output = modal.update(InteractionModalMsg::Key(ctrl_c()));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => {
                assert!(matches!(response, InteractionResponse::Cancelled));
            }
            _ => panic!("Expected AskResponse with Cancelled"),
        }
    }

    #[test]
    fn test_wrap_selection() {
        assert_eq!(InteractionModal::wrap_selection(0, -1, 3), 2);
        assert_eq!(InteractionModal::wrap_selection(2, 1, 3), 0);
        assert_eq!(InteractionModal::wrap_selection(1, -1, 3), 0);
        assert_eq!(InteractionModal::wrap_selection(1, 1, 3), 2);
    }

    #[test]
    fn test_toggle_checked() {
        let mut set = HashSet::new();
        InteractionModal::toggle_checked(&mut set, 1);
        assert!(set.contains(&1));
        InteractionModal::toggle_checked(&mut set, 1);
        assert!(!set.contains(&1));
    }

    #[test]
    fn test_perm_modal_allowlist_shortcut() {
        let perm = PermRequest::bash(["cargo", "build"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('a'))));
        match output {
            InteractionModalOutput::PermissionResponse { response, .. } => {
                assert!(response.allowed);
                assert!(response.pattern.is_some());
                assert_eq!(response.pattern.unwrap(), "cargo *");
            }
            _ => panic!("Expected PermissionResponse with pattern"),
        }
    }

    #[test]
    fn test_perm_modal_tab_opens_text_input() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        assert_eq!(modal.mode, InteractionMode::Selecting);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
        assert_eq!(modal.mode, InteractionMode::TextInput);
    }

    #[test]
    fn test_perm_modal_tab_on_allowlist_prefills_pattern() {
        let perm = PermRequest::bash(["cargo", "test"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 2);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
        assert_eq!(modal.mode, InteractionMode::TextInput);
        assert_eq!(modal.other_text, "cargo *");
    }

    #[test]
    fn test_perm_modal_deny_with_reason() {
        let perm = PermRequest::bash(["rm", "-rf", "/"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
        assert_eq!(modal.mode, InteractionMode::TextInput);

        for c in "too dangerous".chars() {
            modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
        }

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        match output {
            InteractionModalOutput::PermissionResponse { response, .. } => {
                assert!(!response.allowed);
                assert_eq!(response.reason.as_deref(), Some("too dangerous"));
            }
            _ => panic!("Expected PermissionResponse with reason"),
        }
    }

    #[test]
    fn test_perm_modal_esc_from_text_returns_to_selecting() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
        assert_eq!(modal.mode, InteractionMode::TextInput);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        assert_eq!(modal.mode, InteractionMode::Selecting);
    }

    #[test]
    fn test_perm_modal_navigation_wraps_at_3() {
        let perm = PermRequest::bash(["npm", "install"]);
        let mut modal = InteractionModal::new(
            "req-1".to_string(),
            InteractionRequest::Permission(perm),
            true,
        );

        assert_eq!(modal.selected, 0);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
        assert_eq!(modal.selected, 2);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 0);
    }

    // ─── Show tests ────────────────────────────────────────────────────

    fn make_show_modal(content: &str) -> InteractionModal {
        let show = ShowRequest::new(content);
        InteractionModal::new("show-1".into(), InteractionRequest::Show(show), false)
    }

    #[test]
    fn show_scroll_down_and_up() {
        let content = (0..30)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut modal = make_show_modal(&content);

        assert_eq!(modal.scroll_offset, 0);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
        assert_eq!(modal.scroll_offset, 1);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
        assert_eq!(modal.scroll_offset, 0);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
        assert_eq!(modal.scroll_offset, 0);
    }

    #[test]
    fn show_dismiss_with_q() {
        let mut modal = make_show_modal("hello");
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('q'))));
        assert!(matches!(
            output,
            InteractionModalOutput::AskResponse {
                response: InteractionResponse::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn show_dismiss_with_esc() {
        let mut modal = make_show_modal("hello");
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        assert!(matches!(
            output,
            InteractionModalOutput::AskResponse {
                response: InteractionResponse::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn show_page_down() {
        let content = (0..50)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut modal = make_show_modal(&content);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::PageDown)));
        assert_eq!(modal.scroll_offset, 20);
    }

    // ─── Popup tests ───────────────────────────────────────────────────

    fn make_popup_modal(entries: Vec<&str>, allow_other: bool) -> InteractionModal {
        let mut popup = PopupRequest::new("Pick one");
        for e in entries {
            popup = popup.entry(PopupEntry::new(e));
        }
        if allow_other {
            popup = popup.allow_other();
        }
        InteractionModal::new("popup-1".into(), InteractionRequest::Popup(popup), false)
    }

    #[test]
    fn popup_navigation_and_select() {
        let mut modal = make_popup_modal(vec!["Alpha", "Beta", "Gamma"], false);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => match response {
                InteractionResponse::Popup(pr) => {
                    assert_eq!(pr.selected_index, Some(1));
                }
                _ => panic!("Expected Popup response"),
            },
            _ => panic!("Expected AskResponse"),
        }
    }

    #[test]
    fn popup_cancel() {
        let mut modal = make_popup_modal(vec!["A", "B"], false);
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        assert!(matches!(
            output,
            InteractionModalOutput::AskResponse {
                response: InteractionResponse::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn popup_other_text_input() {
        let mut modal = make_popup_modal(vec!["A"], true);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        assert_eq!(modal.mode, InteractionMode::TextInput);

        for c in "custom".chars() {
            modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
        }
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => match response {
                InteractionResponse::Popup(pr) => {
                    assert_eq!(pr.other, Some("custom".into()));
                }
                _ => panic!("Expected Popup response"),
            },
            _ => panic!("Expected AskResponse"),
        }
    }

    // ─── Edit tests ────────────────────────────────────────────────────

    fn make_edit_modal(content: &str) -> InteractionModal {
        let edit = EditRequest::new(content);
        InteractionModal::new("edit-1".into(), InteractionRequest::Edit(edit), false)
    }

    #[test]
    fn edit_initializes_lines() {
        let modal = make_edit_modal("line one\nline two\nline three");
        assert_eq!(modal.edit_lines.len(), 3);
        assert_eq!(modal.edit_lines[0], "line one");
    }

    #[test]
    fn edit_normal_mode_navigation() {
        let mut modal = make_edit_modal("abc\ndef\nghi");

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
        assert_eq!(modal.edit_cursor_line, 1);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('l'))));
        assert_eq!(modal.edit_cursor_col, 1);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
        assert_eq!(modal.edit_cursor_line, 0);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('h'))));
        assert_eq!(modal.edit_cursor_col, 0);
    }

    #[test]
    fn edit_insert_mode_typing() {
        let mut modal = make_edit_modal("hello");

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
        assert_eq!(modal.mode, InteractionMode::TextInput);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('X'))));
        assert_eq!(modal.edit_lines[0], "Xhello");
        assert_eq!(modal.edit_cursor_col, 1);
    }

    #[test]
    fn edit_ctrl_s_saves() {
        let mut modal = make_edit_modal("original");

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('!'))));

        let save_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        let output = modal.update(InteractionModalMsg::Key(save_key));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => match response {
                InteractionResponse::Edit(er) => {
                    assert_eq!(er.modified, "!original");
                }
                _ => panic!("Expected Edit response"),
            },
            _ => panic!("Expected AskResponse"),
        }
    }

    #[test]
    fn edit_cancel_in_normal_mode() {
        let mut modal = make_edit_modal("text");
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        assert!(matches!(
            output,
            InteractionModalOutput::AskResponse {
                response: InteractionResponse::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn edit_enter_splits_line() {
        let mut modal = make_edit_modal("abcdef");

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));

        assert_eq!(modal.edit_lines.len(), 2);
        assert_eq!(modal.edit_lines[0], "abc");
        assert_eq!(modal.edit_lines[1], "def");
        assert_eq!(modal.edit_cursor_line, 1);
        assert_eq!(modal.edit_cursor_col, 0);
    }

    #[test]
    fn edit_backspace_joins_lines() {
        let mut modal = make_edit_modal("abc\ndef");

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Backspace)));

        assert_eq!(modal.edit_lines.len(), 1);
        assert_eq!(modal.edit_lines[0], "abcdef");
        assert_eq!(modal.edit_cursor_line, 0);
        assert_eq!(modal.edit_cursor_col, 3);
    }

    // ─── Panel tests ───────────────────────────────────────────────────

    use crucible_core::interaction::{PanelHints, PanelItem};

    fn make_panel_modal(items: Vec<&str>, hints: PanelHints) -> InteractionModal {
        let panel = InteractivePanel::new("Select")
            .items(items.into_iter().map(PanelItem::new))
            .hints(hints);
        InteractionModal::new("panel-1".into(), InteractionRequest::Panel(panel), false)
    }

    #[test]
    fn panel_navigation_wraps() {
        let mut modal = make_panel_modal(vec!["A", "B", "C"], PanelHints::new());

        assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 0);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 1);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 2);
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 0);
    }

    #[test]
    fn panel_single_select() {
        let mut modal = make_panel_modal(vec!["X", "Y"], PanelHints::new());
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));

        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => match response {
                InteractionResponse::Panel(result) => {
                    assert!(!result.cancelled);
                    assert_eq!(result.selected, vec![1]);
                }
                _ => panic!("Expected Panel response"),
            },
            _ => panic!("Expected AskResponse"),
        }
    }

    #[test]
    fn panel_multi_select_toggle() {
        let mut modal = make_panel_modal(vec!["A", "B", "C"], PanelHints::new().multi_select());

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(' '))));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(' '))));

        let state = modal.panel_state.as_ref().unwrap();
        assert!(state.selected.contains(&0));
        assert!(state.selected.contains(&2));
        assert!(!state.selected.contains(&1));
    }

    #[test]
    fn panel_cancel() {
        let mut modal = make_panel_modal(vec!["A"], PanelHints::new());
        let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
        match output {
            InteractionModalOutput::AskResponse { response, .. } => match response {
                InteractionResponse::Panel(result) => assert!(result.cancelled),
                _ => panic!("Expected Panel response"),
            },
            _ => panic!("Expected AskResponse"),
        }
    }

    #[test]
    fn panel_filter_narrows_visible() {
        let mut modal = make_panel_modal(
            vec!["Apple", "Banana", "Avocado"],
            PanelHints::new().filterable(),
        );

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('/'))));
        assert_eq!(modal.mode, InteractionMode::TextInput);

        for c in "a".chars() {
            modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
        }

        let state = modal.panel_state.as_ref().unwrap();
        assert_eq!(state.visible.len(), 3);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('p'))));
        let state = modal.panel_state.as_ref().unwrap();
        assert_eq!(state.visible.len(), 1);
        assert_eq!(state.visible[0], 0);
    }

    #[test]
    fn panel_initial_selection_applied() {
        let modal = make_panel_modal(
            vec!["A", "B", "C"],
            PanelHints::new().multi_select().initial_selection([1, 2]),
        );
        assert!(modal.checked.contains(&1));
        assert!(modal.checked.contains(&2));
        assert!(!modal.checked.contains(&0));
    }
}
