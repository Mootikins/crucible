use super::{InteractionMode, InteractionModal, InteractionModalOutput};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{EditRequest, EditResponse, InteractionResponse};
use crucible_oil::node::{col, row, styled, text, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_edit_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
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

    pub(super) fn render_edit_interaction(&self, edit: &EditRequest, term_width: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let panel_bg = t.resolve_color(t.colors.background);
        let border_fg = t.resolve_color(t.colors.background);

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
                        Style::new()
                            .bg(panel_bg)
                            .fg(t.resolve_color(t.colors.text_muted))
                            .dim(),
                    ),
                    styled(
                        before,
                        Style::new().bg(panel_bg).fg(t.resolve_color(t.colors.text)),
                    ),
                    styled(
                        cursor_char,
                        Style::new()
                            .bg(t.resolve_color(t.colors.primary))
                            .fg(panel_bg)
                            .bold(),
                    ),
                    styled(
                        after,
                        Style::new().bg(panel_bg).fg(t.resolve_color(t.colors.text)),
                    ),
                ]));
            } else {
                content_nodes.push(row([
                    styled(
                        line_num,
                        Style::new()
                            .bg(panel_bg)
                            .fg(t.resolve_color(t.colors.text_muted))
                            .dim(),
                    ),
                    styled(
                        line,
                        Style::new().bg(panel_bg).fg(t.resolve_color(t.colors.text)),
                    ),
                ]));
            }
        }

        let key_style = Style::new().fg(t.resolve_color(t.colors.primary));
        let hint_style = Style::new().fg(t.resolve_color(t.colors.text_muted)).dim();
        let mode_label = match self.mode {
            InteractionMode::Selecting => "NORMAL",
            InteractionMode::TextInput => "INSERT",
        };

        let footer = if self.mode == InteractionMode::TextInput {
            row([
                styled(
                    format!(" {mode_label} "),
                    Style::new()
                        .fg(t.resolve_color(t.colors.error))
                        .bold()
                        .reverse(),
                ),
                styled("  Ctrl+S", key_style),
                styled(" save", hint_style),
                styled("  Esc", key_style),
                styled(" normal", hint_style),
            ])
        } else {
            row([
                styled(
                    format!(" {mode_label} "),
                    Style::new()
                        .fg(t.resolve_color(t.colors.error))
                        .bold()
                        .reverse(),
                ),
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
            styled(
                t.decorations
                    .half_block_bottom
                    .to_string()
                    .repeat(term_width),
                Style::new().fg(border_fg),
            ),
            header,
            col(content_nodes),
            styled(
                t.decorations.half_block_top.to_string().repeat(term_width),
                Style::new().fg(border_fg),
            ),
            footer,
            text(""),
        ])
    }
}
