//! Interaction modal component for permission and ask requests.
//!
//! Follows Elm-style architecture: Msg → update → Output.

use crate::tui::oil::node::{col, row, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::{colors, styles};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{
    AskBatch, AskRequest, AskResponse, InteractionRequest, InteractionResponse, PermAction,
    PermRequest, PermResponse, PermissionScope,
};
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
}

impl InteractionModal {
    /// Create a new interaction modal for the given request.
    pub fn new(request_id: String, request: InteractionRequest) -> Self {
        Self {
            request_id,
            request,
            selected: 0,
            filter: String::new(),
            other_text: String::new(),
            mode: InteractionMode::Selecting,
            checked: HashSet::new(),
            current_question: 0,
            other_text_preserved: false,
            batch_answers: Vec::new(),
            batch_other_texts: Vec::new(),
            diff_collapsed: false,
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
            _ => InteractionModalOutput::None,
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
        let has_pattern_option = !perm_request.tokens().is_empty();
        let total_options = if has_pattern_option { 3 } else { 2 };

        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                self.selected = Self::wrap_selection(self.selected, -1, total_options);
                InteractionModalOutput::None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                self.selected = Self::wrap_selection(self.selected, 1, total_options);
                InteractionModalOutput::None
            }
            KeyCode::Enter => self.handle_perm_enter_key(&perm_request, has_pattern_option),
            KeyCode::Char('y') | KeyCode::Char('Y') => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow(),
            },
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                InteractionModalOutput::PermissionResponse {
                    request_id: self.request_id.clone(),
                    response: PermResponse::deny(),
                }
            }
            KeyCode::Char('c') if Self::is_ctrl_c(key) => {
                InteractionModalOutput::PermissionResponse {
                    request_id: self.request_id.clone(),
                    response: PermResponse::deny(),
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => self.handle_perm_pattern_key(&perm_request),
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.diff_collapsed = !self.diff_collapsed;
                InteractionModalOutput::ToggleDiff
            }
            _ => InteractionModalOutput::None,
        }
    }

    fn handle_perm_enter_key(
        &self,
        perm_request: &PermRequest,
        has_pattern_option: bool,
    ) -> InteractionModalOutput {
        match self.selected {
            0 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow(),
            },
            1 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::deny(),
            },
            2 if has_pattern_option => {
                let pattern = perm_request.pattern_at(perm_request.tokens().len());
                InteractionModalOutput::PermissionResponse {
                    request_id: self.request_id.clone(),
                    response: PermResponse::allow_pattern(
                        pattern.clone(),
                        PermissionScope::Project,
                    ),
                }
            }
            _ => InteractionModalOutput::None,
        }
    }

    fn handle_perm_pattern_key(&self, perm_request: &PermRequest) -> InteractionModalOutput {
        let tokens = perm_request.tokens();
        if tokens.is_empty() {
            return InteractionModalOutput::Notify(
                "Cannot create pattern for this request type".to_string(),
            );
        }
        let pattern = perm_request.pattern_at(tokens.len());
        InteractionModalOutput::PermissionResponse {
            request_id: self.request_id.clone(),
            response: PermResponse::allow_pattern(pattern.clone(), PermissionScope::Project),
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

    /// Render the modal view.
    pub fn view(&self, term_width: usize, queue_size: usize) -> Node {
        match &self.request {
            InteractionRequest::Permission(perm) => {
                self.render_perm_interaction(perm, term_width, queue_size)
            }
            InteractionRequest::Ask(ask) => self.render_ask_interaction_single(ask, term_width),
            InteractionRequest::AskBatch(batch) => {
                self.render_ask_interaction_batch(batch, term_width)
            }
            _ => Node::Empty,
        }
    }

    fn render_perm_interaction(
        &self,
        perm_request: &PermRequest,
        term_width: usize,
        queue_size: usize,
    ) -> Node {
        let panel_bg = colors::INPUT_BG;
        let border_fg = colors::BORDER;

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
        let has_pattern_option = !perm_request.tokens().is_empty();

        // Helper: pad a content string to full width with INPUT_BG background
        let pad_line = |content: &str, visible_len: usize| -> Node {
            let pad = " ".repeat(term_width.saturating_sub(visible_len));
            styled(
                format!("{}{}", content, pad),
                Style::new().bg(panel_bg).fg(colors::OVERLAY_BRIGHT),
            )
        };

        let mut lines: Vec<Node> = Vec::new();

        // ── Top border: ▄ repeated ──
        lines.push(styled(
            "\u{2584}".repeat(term_width),
            Style::new().fg(border_fg),
        ));

        // ── Action detail line ──
        let action_text = if queue_total > 1 {
            format!("  [{}/{}] {}", 1, queue_total, action_detail)
        } else {
            format!("  {}", action_detail)
        };
        let action_visible_len = action_text.len();
        lines.push(pad_line(&action_text, action_visible_len));

        // ── Blank line ──
        lines.push(styled(" ".repeat(term_width), Style::new().bg(panel_bg)));

        // ── Options ──
        let options: Vec<(&str, &str)> = if has_pattern_option {
            vec![
                ("y", "Allow once"),
                ("n", "Deny"),
                ("p", "Allow + Save pattern"),
            ]
        } else {
            vec![("y", "Allow once"), ("n", "Deny")]
        };

        for (i, (key, label)) in options.iter().enumerate() {
            let is_selected = i == self.selected;
            if is_selected {
                let content = format!("    > [{}] {}", key, label);
                let visible_len = content.len();
                let pad = " ".repeat(term_width.saturating_sub(visible_len));
                lines.push(styled(
                    format!("{}{}", content, pad),
                    Style::new().bg(panel_bg).fg(colors::TEXT_ACCENT).bold(),
                ));
            } else {
                let key_part = format!("      [{}]", key);
                let label_part = format!(" {}", label);
                let visible_len = key_part.len() + label_part.len();
                let pad = " ".repeat(term_width.saturating_sub(visible_len));
                lines.push(row([
                    styled(key_part, Style::new().bg(panel_bg).fg(colors::OVERLAY_TEXT)),
                    styled(
                        label_part,
                        Style::new().bg(panel_bg).fg(colors::OVERLAY_BRIGHT),
                    ),
                    styled(pad, Style::new().bg(panel_bg)),
                ]));
            }
        }

        // ── Bottom border: ▀ repeated ──
        lines.push(styled(
            "\u{2580}".repeat(term_width),
            Style::new().fg(border_fg),
        ));

        // ── Footer: PERMISSION badge + TYPE badge + key hints ──
        let key_style = styles::overlay_key(colors::ERROR);
        let hint_style = styles::overlay_hint();

        let shortcut_keys = options
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>()
            .join("/");

        let mut footer_nodes: Vec<Node> = vec![
            styled(" PERMISSION ", styles::permission_badge()),
            styled(format!(" {} ", type_label), styles::permission_type()),
            styled(" ↑/↓", key_style),
            styled(" navigate", hint_style),
            styled("  Enter", key_style),
            styled(" select", hint_style),
            styled(format!("  {}", shortcut_keys), key_style),
            styled(" shortcuts", hint_style),
        ];

        if is_write {
            footer_nodes.push(styled("  h", key_style));
            footer_nodes.push(styled(" diff", hint_style));
        }

        footer_nodes.push(styled("  Esc", key_style));
        footer_nodes.push(styled(" cancel", hint_style));

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
        let header_bg = colors::INPUT_BG;
        let footer_bg = colors::INPUT_BG;
        let top_border = styled("▄".repeat(term_width), Style::new().fg(colors::INPUT_BG));
        let bottom_border = styled("▀".repeat(term_width), Style::new().fg(colors::INPUT_BG));

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
                Style::new().fg(colors::TEXT_ACCENT).bold()
            } else {
                Style::new().fg(colors::TEXT_PRIMARY)
            };
            choice_nodes.push(styled(format!("{}{}", prefix, choice), style));
        }

        if allow_other {
            let other_idx = choices.len();
            let is_selected = self.selected == other_idx;
            let prefix = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::new().fg(colors::TEXT_ACCENT).bold()
            } else {
                Style::new().fg(colors::TEXT_MUTED).italic()
            };
            choice_nodes.push(styled(format!("{}Other...", prefix), style));
        }

        let key_style = Style::new().bg(footer_bg).fg(colors::TEXT_ACCENT);
        let sep_style = Style::new().bg(footer_bg).fg(colors::TEXT_MUTED);
        let text_style = Style::new().bg(footer_bg).fg(colors::TEXT_PRIMARY).dim();

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
                styled("   Enter text: ", Style::new().fg(colors::TEXT_MUTED)),
                styled(&self.other_text, Style::new().fg(colors::TEXT_PRIMARY)),
                styled("_", Style::new().fg(colors::TEXT_ACCENT)),
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
                                    format!("\"{}...\"", &s[..27])
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
        let mut modal =
            InteractionModal::new("req-1".to_string(), InteractionRequest::Permission(perm));

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
        let mut modal =
            InteractionModal::new("req-1".to_string(), InteractionRequest::Permission(perm));

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
        let mut modal =
            InteractionModal::new("req-1".to_string(), InteractionRequest::Permission(perm));

        assert_eq!(modal.selected, 0);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
        assert_eq!(modal.selected, 1);

        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
        assert_eq!(modal.selected, 0);
    }

    #[test]
    fn test_ask_modal_selection() {
        let ask = AskRequest::new("Choose one").choices(["A", "B", "C"]);
        let mut modal = InteractionModal::new("req-2".to_string(), InteractionRequest::Ask(ask));

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
        let mut modal = InteractionModal::new("req-3".to_string(), InteractionRequest::Ask(ask));

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
        let mut modal = InteractionModal::new("req-4".to_string(), InteractionRequest::Ask(ask));

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
}
