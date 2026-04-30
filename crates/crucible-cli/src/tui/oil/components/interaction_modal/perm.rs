use super::helpers::prettify_tool_args;
use super::{InteractionModal, InteractionModalOutput, InteractionMode};
use crate::tui::oil::components::diff_view::{render_diff, DiffOptions};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{PermAction, PermRequest, PermResponse, PermissionScope};
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;

impl InteractionModal {
    pub(super) fn handle_perm_key(
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
                KeyCode::Enter
                    if key.modifiers.contains(KeyModifiers::SHIFT) && self.selected == 2 =>
                {
                    InteractionModalOutput::PermissionResponse {
                        request_id: self.request_id.clone(),
                        response: PermResponse::allow_pattern(
                            perm_request.suggested_pattern(),
                            PermissionScope::User,
                        ),
                    }
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
                KeyCode::Enter => {
                    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                    self.handle_perm_text_confirm(&perm_request, shift)
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

    fn handle_perm_text_confirm(
        &self,
        _perm_request: &PermRequest,
        shift: bool,
    ) -> InteractionModalOutput {
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
            2 => {
                let scope = if shift {
                    PermissionScope::User
                } else {
                    PermissionScope::Project
                };
                InteractionModalOutput::PermissionResponse {
                    request_id: self.request_id.clone(),
                    response: if text.is_empty() {
                        PermResponse::deny()
                    } else {
                        PermResponse::allow_pattern(text, scope)
                    },
                }
            }
            _ => InteractionModalOutput::None,
        }
    }

    pub(super) fn render_perm_interaction(
        &self,
        perm_request: &PermRequest,
        term_width: usize,
        queue_size: usize,
    ) -> Node {
        let t = crate::tui::oil::theme::active();
        let panel_bg = t.resolve_color(t.colors.background);
        let border_fg = t.resolve_color(t.colors.border);

        let (type_label, action_detail, is_write) = match &perm_request.action {
            PermAction::Bash { tokens } => ("BASH", tokens.join(" "), false),
            PermAction::Read { segments } => ("READ", format!("/{}", segments.join("/")), false),
            PermAction::Write { segments } => ("WRITE", format!("/{}", segments.join("/")), true),
            PermAction::Tool { name, args } => {
                let args_str = prettify_tool_args(args);
                ("TOOL", format!("{} {}", name, args_str), false)
            }
        };

        let queue_total = 1 + queue_size;

        let pad_line = |content: &str, visible_len: usize| -> Node {
            let pad = " ".repeat(term_width.saturating_sub(visible_len));
            styled(
                format!("{content}{pad}"),
                Style::new()
                    .bg(panel_bg)
                    .fg(t.resolve_color(t.colors.overlay_bright)),
            )
        };

        let mut lines: Vec<Node> = Vec::new();

        lines.push(styled(
            t.decorations
                .half_block_bottom
                .to_string()
                .repeat(term_width),
            Style::new().fg(border_fg),
        ));

        let action_text = if queue_total > 1 {
            format!("  [{}/{}] {}", 1, queue_total, action_detail)
        } else {
            format!("  {}", action_detail)
        };
        lines.push(pad_line(&action_text, action_text.len()));

        lines.push(styled(" ".repeat(term_width), Style::new().bg(panel_bg)));

        if !perm_request.diffs.is_empty() {
            for fd in &perm_request.diffs {
                let mut opts = DiffOptions::for_width(term_width);
                opts.max_lines = Some(500);
                opts.collapsed = self.diff_collapsed;
                // Wrap the rendered diff in a Box with `style.bg = panel_bg`
                // so the panel background paints behind the diff body. The
                // renderer fills the box rect with bg-styled spaces, and
                // CellGrid composition preserves the bg under any child
                // text spans (which only set fg). See
                // `crucible_oil::layout::tree_render::render_box_content`.
                lines.push(col([render_diff(fd, &opts)]).with_style(Style::new().bg(panel_bg)));
            }
            lines.push(styled(
                "  press h to expand/collapse diff",
                Style::new()
                    .bg(panel_bg)
                    .fg(t.resolve_color(t.colors.text_dim))
                    .dim(),
            ));
            lines.push(styled(" ".repeat(term_width), Style::new().bg(panel_bg)));
        }

        let options: [(&str, &str); 3] = [("y", "Yes"), ("n", "No"), ("a", "Allowlist")];

        for (i, (key, label)) in options.iter().enumerate() {
            let is_selected = i == self.selected;
            if is_selected {
                let content = format!("  > [{}] {}", key, label);
                let pad = " ".repeat(term_width.saturating_sub(content.len()));
                lines.push(styled(
                    format!("{content}{pad}"),
                    Style::new()
                        .bg(panel_bg)
                        .fg(t.resolve_color(t.colors.primary))
                        .bold(),
                ));
            } else {
                let key_part = format!("    [{}]", key);
                let label_part = format!(" {}", label);
                let pad = " ".repeat(term_width.saturating_sub(key_part.len() + label_part.len()));
                lines.push(row([
                    styled(
                        key_part,
                        Style::new()
                            .bg(panel_bg)
                            .fg(t.resolve_color(t.colors.overlay_text)),
                    ),
                    styled(
                        label_part,
                        Style::new()
                            .bg(panel_bg)
                            .fg(t.resolve_color(t.colors.overlay_bright)),
                    ),
                    styled(pad, Style::new().bg(panel_bg)),
                ]));
            }

            if is_selected && self.mode == InteractionMode::TextInput {
                let prompt = format!("      > {}_", self.other_text);
                let pad = " ".repeat(term_width.saturating_sub(prompt.len()));
                lines.push(styled(
                    format!("{prompt}{pad}"),
                    Style::new().bg(panel_bg).fg(t.resolve_color(t.colors.text)),
                ));
            }
        }

        lines.push(styled(
            t.decorations.half_block_top.to_string().repeat(term_width),
            Style::new().fg(border_fg),
        ));

        let key_style = Style::new().fg(t.resolve_color(t.colors.error));
        let hint_style = Style::new().fg(t.resolve_color(t.colors.diff_context));

        let footer_nodes: Vec<Node> = if self.mode == InteractionMode::TextInput {
            let mut nodes = vec![
                styled(
                    " PERMISSION ",
                    Style::new()
                        .fg(t.resolve_color(t.colors.error))
                        .bold()
                        .reverse(),
                ),
                styled(
                    format!(" {} ", type_label),
                    Style::new().fg(t.resolve_color(t.colors.error)).bold(),
                ),
                styled("  Enter", key_style),
                styled(" send", hint_style),
            ];
            if self.selected == 2 {
                nodes.push(styled("  S-Enter", key_style));
                nodes.push(styled(" global", hint_style));
            }
            nodes.push(styled("  Esc", key_style));
            nodes.push(styled(" back", hint_style));
            nodes
        } else {
            let mut nodes = vec![
                styled(
                    " PERMISSION ",
                    Style::new()
                        .fg(t.resolve_color(t.colors.error))
                        .bold()
                        .reverse(),
                ),
                styled(
                    format!(" {} ", type_label),
                    Style::new().fg(t.resolve_color(t.colors.error)).bold(),
                ),
                styled("  y/n/a", key_style),
                styled(" options", hint_style),
                styled("  ↑↓", key_style),
                styled(" move", hint_style),
                styled("  Tab", key_style),
                styled(" entry", hint_style),
            ];
            if self.selected == 2 {
                nodes.push(styled("  S-Enter", key_style));
                nodes.push(styled(" global", hint_style));
            }
            if is_write || !perm_request.diffs.is_empty() {
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
}
