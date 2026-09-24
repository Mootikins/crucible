use super::helpers::{prettify_tool_args, prettify_tool_args_full};
use super::{InteractionModal, InteractionModalOutput, InteractionMode};
use crate::tui::oil::components::diff_view::{render_diff, DiffOptions};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{PermAction, PermRequest, PermResponse, PermissionScope};
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;
use unicode_width::UnicodeWidthStr;

impl InteractionModal {
    pub(super) fn handle_perm_key(
        &mut self,
        key: KeyEvent,
        perm_request: PermRequest,
    ) -> InteractionModalOutput {
        // "Allowlist" is the third option. With no grant that can name the
        // call it is not offered: a click would save nothing.
        let grant = perm_request.suggested_pattern();
        let total_options = 2 + usize::from(grant.is_some());

        match self.mode {
            InteractionMode::Selecting => match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.selected = Self::wrap_selection(self.selected, -1, total_options);
                    InteractionModalOutput::None
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    self.selected = Self::wrap_selection(self.selected, 1, total_options);
                    InteractionModalOutput::None
                }
                KeyCode::Enter
                    if key.modifiers.contains(KeyModifiers::SHIFT) && self.selected == 2 =>
                {
                    self.allowlist(grant, PermissionScope::User)
                }
                KeyCode::Enter => self.handle_perm_confirm(grant),
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
                    self.allowlist(grant, PermissionScope::Project)
                }
                KeyCode::Tab => {
                    self.mode = InteractionMode::TextInput;
                    if self.selected == 2 {
                        self.other_text = grant.unwrap_or_default();
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

    /// "Allowlist": allow the call and save `grant` at `scope`. With no
    /// grant the option is not offered, so the key does nothing.
    fn allowlist(&self, grant: Option<String>, scope: PermissionScope) -> InteractionModalOutput {
        match grant {
            Some(pattern) => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow_pattern(pattern, scope),
            },
            None => InteractionModalOutput::None,
        }
    }

    fn handle_perm_confirm(&self, grant: Option<String>) -> InteractionModalOutput {
        match self.selected {
            0 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::allow(),
            },
            1 => InteractionModalOutput::PermissionResponse {
                request_id: self.request_id.clone(),
                response: PermResponse::deny(),
            },
            2 => self.allowlist(grant, PermissionScope::Project),
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

        let mut rows: Vec<String> = Vec::new();
        let (type_label, action_detail, is_write) = match &perm_request.action {
            PermAction::Bash { tokens } => ("BASH", tokens.join(" "), false),
            PermAction::Read { segments } => ("READ", format!("/{}", segments.join("/")), false),
            PermAction::Write { segments } => ("WRITE", format!("/{}", segments.join("/")), true),
            // The daemon sent the call with its render, so the card, the
            // web and the deny messages show the same line. The label is
            // the canonical kind, not a guess from the tool name.
            PermAction::Tool { name, args } => match perm_request.call.as_deref() {
                Some(call) => {
                    // The fields of the render stand in place of the
                    // arguments, as on the web card.
                    let fields = call.render.as_ref().map_or(&[][..], |r| &r.fields[..]);
                    rows = fields
                        .iter()
                        .map(|f| match &f.value {
                            serde_json::Value::String(v) => format!("{}: {v}", f.label),
                            v => format!("{}: {v}", f.label),
                        })
                        .collect();
                    if rows.is_empty() && call.kind != "command" {
                        rows.push(match self.full_commands {
                            true => prettify_tool_args_full(args),
                            false => prettify_tool_args(args),
                        });
                    }
                    let line = match self.full_commands {
                        true => call.render.as_ref().and_then(|r| r.line.clone()),
                        false => call.summary(60),
                    }
                    .unwrap_or_default();
                    match call.kind.as_str() {
                        "command" => ("BASH", line, false),
                        _ => ("TOOL", format!("{} {line}", call.tool), false),
                    }
                }
                None => {
                    let args = match self.full_commands {
                        true => prettify_tool_args_full(args),
                        false => prettify_tool_args(args),
                    };
                    ("TOOL", format!("{name} {args}"), false)
                }
            },
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

        let queue_prefix = if queue_total > 1 {
            format!("[{}/{}] ", 1, queue_total)
        } else {
            String::new()
        };

        let texts = std::iter::once(format!("{queue_prefix}{action_detail}"))
            .chain(rows.into_iter().filter(|r| !r.is_empty()));
        for text in texts {
            if self.full_commands {
                // Show the complete command/args: wrap to the panel width so
                // nothing is clipped at the terminal edge. Each wrapped line
                // is its own padded row (single overlong text nodes get
                // clipped by the renderer, not wrapped).
                let wrap_width = term_width.saturating_sub(4).max(20);
                for line in crate::tui::oil::utils::wrap::wrap_words(&text, wrap_width) {
                    let text = format!("  {}", line);
                    let width = UnicodeWidthStr::width(text.as_str());
                    lines.push(pad_line(&text, width));
                }
            } else {
                // Compact: first line only, ellipsized to the terminal width.
                let text = format!("  {text}");
                let truncated =
                    crate::tui::oil::utils::truncate::truncate_first_line(&text, term_width, true);
                let width = UnicodeWidthStr::width(truncated.as_ref());
                lines.push(pad_line(&truncated, width));
            }
        }

        // Everything else that is known: the agent, the tool name on the
        // wire and the layer that asked.
        let call = perm_request.call.as_deref();
        let about: Vec<String> = [
            call.and_then(|c| c.agent.clone())
                .map(|a| format!("agent {a}")),
            call.and_then(|c| c.raw.as_ref()?.name.clone())
                .map(|n| format!("wire name {n}")),
            perm_request.layer.clone().map(|l| format!("asked by {l}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !about.is_empty() {
            let text = format!("  {}", about.join(" · "));
            let pad = " ".repeat(term_width.saturating_sub(UnicodeWidthStr::width(text.as_str())));
            lines.push(styled(
                format!("{text}{pad}"),
                Style::new()
                    .bg(panel_bg)
                    .fg(t.resolve_color(t.colors.text_dim)),
            ));
        }

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

        let options = [("y", "Yes"), ("n", "No"), ("a", "Allowlist")];
        let offered = 2 + usize::from(perm_request.suggested_pattern().is_some());

        for (i, (key, label)) in options[..offered].iter().enumerate() {
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
                styled(if offered > 2 { "  y/n/a" } else { "  y/n" }, key_style),
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
