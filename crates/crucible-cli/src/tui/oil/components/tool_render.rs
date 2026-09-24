//! Tool call rendering component.
//!
//! Renders tool call states: pending (with static ● icon), complete (with ✓),
//! and error (with ✗). No animated spinners — animation lives in chrome only.

use crate::tui::oil::components::diff_view::{render_diff, DiffOptions};
use crate::tui::oil::theme::ThemeConfig;
use crate::tui::oil::utils::truncate_to_chars;
use crate::tui::oil::viewport_cache::CachedToolCall;
use crucible_oil::ansi::visible_width;
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::{AdaptiveColor, Style};
use crucible_oil::truncate_to_width;
use std::time::{Duration, Instant};

/// Foreground-only style from a theme-resolved adaptive color. Condenses the
/// pervasive `Style::new().fg(t.resolve_color(...))` call sites.
fn fg(t: &ThemeConfig, color: AdaptiveColor) -> Style {
    Style::new().fg(t.resolve_color(color))
}

impl CachedToolCall {
    /// Render a compact tool call with default spinner frame (0) and diffs visible.
    ///
    /// The frame clock is the call's own start, so the card shows no elapsed
    /// time. Only tests use this; the transcript passes the real frame clock.
    pub fn render_compact(&self, width: usize) -> Node {
        self.render_compact_with(self.started_at, 0, width, true)
    }

    /// Render a compact tool call with specified spinner frame; diffs visible.
    pub fn render_compact_with_frame(&self, spinner_frame: usize, width: usize) -> Node {
        self.render_compact_with(self.started_at, spinner_frame, width, true)
    }

    /// Render a compact tool call. `show_diffs` gates the diff body for
    /// Edit/Write tool calls; the rest of the result still renders.
    ///
    /// `now` is the frame clock. A running card reads its elapsed time from
    /// it, so the same state renders the same card on any machine.
    pub fn render_compact_with(
        &self,
        now: Instant,
        spinner_frame: usize,
        width: usize,
        show_diffs: bool,
    ) -> Node {
        if self.superseded {
            return Node::Empty;
        }

        let display_name = self.display_name();
        // One row: the line of the render collapses to one line.
        let line = (self.render.as_ref())
            .and_then(|r| r.line.as_deref())
            .unwrap_or_default();
        let one_line = line.replace('\n', " ").replace('\r', "");
        let primary_arg: &str = &one_line;
        let result_str = self.result();

        let inner = if let Some(ref error) = self.error {
            self.render_error(&display_name, primary_arg, error, width)
        } else if self.complete {
            self.render_complete(&display_name, primary_arg, &result_str, width, show_diffs)
        } else if self.backgrounded {
            self.render_backgrounded(&display_name, primary_arg, width)
        } else {
            self.render_running(
                now,
                &display_name,
                primary_arg,
                &result_str,
                spinner_frame,
                width,
            )
        };

        let fields = self.render_fields(width);
        let description_node = self.render_description();
        if matches!(description_node, Node::Empty) && fields.is_empty() {
            inner
        } else {
            col(std::iter::once(inner)
                .chain(fields)
                .chain(std::iter::once(description_node)))
        }
    }

    /// One dim row for each field of the render, cut to the width.
    fn render_fields(&self, width: usize) -> Vec<Node> {
        let t = crate::tui::oil::theme::active();
        let fields = self.render.as_ref().map_or(&[][..], |r| &r.fields[..]);
        fields
            .iter()
            .map(|f| {
                let value = match &f.value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let text = format!("    {}: {}", f.label, value.replace('\n', " "));
                styled(
                    fit_arg_to_width(&text, width),
                    fg(t, t.colors.text_muted).dim(),
                )
            })
            .collect()
    }

    fn display_name(&self) -> String {
        crucible_daemon::acp::streaming::humanize_tool_title(&self.name)
    }

    fn render_description(&self) -> Node {
        let desc = match self.description.as_deref() {
            Some(d) if !d.is_empty() => d,
            _ => return Node::Empty,
        };
        let t = crate::tui::oil::theme::active();
        styled(format!("    {}", desc), fg(t, t.colors.text_muted).dim())
    }

    /// Raw badge text (with leading space and brackets) for width math.
    /// Empty string when no badge should be shown.
    fn source_badge_text(&self) -> String {
        let source = self
            .source
            .as_ref()
            .and_then(|s| s.badge_label())
            .map(|label| format!(" [{}]", label))
            .unwrap_or_default();
        // A permission granted without asking must leave a trace: otherwise
        // an auto-approved call looks exactly like one that never needed
        // permission, and auto mode has no audit trail at all.
        match self.auto_approved {
            Some(_) => format!("{source} [auto]"),
            None => source,
        }
    }

    fn render_source_badge(&self) -> Node {
        let text = self.source_badge_text();
        if text.is_empty() {
            return Node::Empty;
        }
        let t = crate::tui::oil::theme::active();
        styled(text, fg(t, t.colors.text_muted).dim())
    }

    fn render_error(
        &self,
        display_name: &str,
        primary_arg: &str,
        error: &str,
        width: usize,
    ) -> Node {
        let t = crate::tui::oil::theme::active();
        let icon = format!(" {} ", t.decorations.tool_error_icon);
        let badge_text = self.source_badge_text();
        let source_badge = self.render_source_badge();
        // Budget for primary_arg: terminal width minus icon, name, badge, and
        // the surrounding spaces in `arg_part` (` {} `, =2 cols).
        let arg_budget = width.saturating_sub(
            visible_width(&icon) + visible_width(display_name) + visible_width(&badge_text) + 2,
        );
        let fitted_arg = fit_arg_to_width(primary_arg, arg_budget);
        let arg_part = if fitted_arg.is_empty() {
            " ".to_string()
        } else {
            format!(" {} ", fitted_arg)
        };
        let prefix_width =
            visible_width(&icon) + visible_width(display_name) + visible_width(&arg_part);
        let remaining = width.saturating_sub(prefix_width + 2).max(10);
        let error_first_line = error.lines().next().unwrap_or(error);
        let error_visible = visible_width(error_first_line);
        if error_visible <= remaining {
            row([
                styled(icon, fg(t, t.colors.error)),
                styled(display_name, fg(t, t.colors.text_dim)),
                source_badge,
                styled(arg_part, fg(t, t.colors.text_dim).dim()),
                styled(
                    format!("\u{2192} {}", error_first_line),
                    fg(t, t.colors.error).bold(),
                ),
            ])
        } else {
            let header = row([
                styled(icon, fg(t, t.colors.error)),
                styled(display_name, fg(t, t.colors.text_dim)),
                source_badge,
                styled(arg_part, fg(t, t.colors.text_dim).dim()),
            ]);
            let error_node = styled(
                format!("  \u{2192} {}", error_first_line),
                fg(t, t.colors.error).bold(),
            );
            col([header, error_node])
        }
    }

    fn render_complete(
        &self,
        display_name: &str,
        primary_arg: &str,
        result_str: &str,
        width: usize,
        show_diffs: bool,
    ) -> Node {
        let summary = self.render.as_ref().and_then(|r| r.summary.as_deref());
        let collapsed = collapse_result(result_str, summary);
        let has_arrow_suffix = collapsed.is_some();

        let t = crate::tui::oil::theme::active();
        let arrow_suffix = if let Some(ref s) = collapsed {
            styled(format!("→ {}", s), fg(t, t.colors.text_muted))
        } else {
            Node::Empty
        };

        let badge_text = self.source_badge_text();
        let source_badge = self.render_source_badge();
        let icon_str = format!(" {} ", t.decorations.tool_success_icon);
        let arrow_suffix_text = collapsed
            .as_ref()
            .map(|s| format!("→ {}", s))
            .unwrap_or_default();
        // Budget for primary_arg: total width minus icon, display name, badge,
        // arrow suffix, and the surrounding spaces in arg_node (1 or 2 cols).
        let arg_spacing = if has_arrow_suffix { 2 } else { 1 };
        let arg_budget = width.saturating_sub(
            visible_width(&icon_str)
                + visible_width(display_name)
                + visible_width(&badge_text)
                + visible_width(&arrow_suffix_text)
                + arg_spacing,
        );
        let fitted_arg = fit_arg_to_width(primary_arg, arg_budget);
        let arg_node = if fitted_arg.is_empty() {
            if has_arrow_suffix {
                styled(" ", Style::new())
            } else {
                Node::Empty
            }
        } else if has_arrow_suffix {
            styled(format!(" {} ", fitted_arg), fg(t, t.colors.text_dim).dim())
        } else {
            styled(format!(" {}", fitted_arg), fg(t, t.colors.text_dim).dim())
        };
        let header = row([
            styled(icon_str, fg(t, t.colors.success)),
            styled(display_name, fg(t, t.colors.text_dim)),
            source_badge,
            arg_node,
            arrow_suffix,
        ]);

        let result_node = if has_arrow_suffix || result_str.is_empty() {
            Node::Empty
        } else {
            format_tool_result(result_str, width)
        };

        let diff_node = if show_diffs && !self.diffs.is_empty() {
            let opts = DiffOptions::for_width(width);
            let nodes: Vec<Node> = self.diffs.iter().map(|d| render_diff(d, &opts)).collect();
            col(nodes)
        } else {
            Node::Empty
        };

        let mut children = vec![header];
        if !matches!(diff_node, Node::Empty) {
            children.push(diff_node);
        }
        if !matches!(result_node, Node::Empty) {
            children.push(result_node);
        }
        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            col(children)
        }
    }

    /// A call that outran the split threshold, drawn where it was made.
    ///
    /// The card is frozen: no spinner, no elapsed time, no streamed output.
    /// Nothing here reads a clock or changes again, so the row survives a
    /// scroll out of the repaintable window. The finish node lands below.
    fn render_backgrounded(&self, display_name: &str, primary_arg: &str, width: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let icon = styled("\u{25B8}", fg(t, t.colors.text_dim));
        let badge_text = self.source_badge_text();
        let source_badge = self.render_source_badge();
        const SUFFIX: &str = "  started in the background";
        let arg_budget = width.saturating_sub(
            3 + visible_width(display_name)
                + visible_width(&badge_text)
                + 1
                + visible_width(SUFFIX),
        );
        let fitted_arg = fit_arg_to_width(primary_arg, arg_budget);
        let arg_node = if fitted_arg.is_empty() {
            Node::Empty
        } else {
            styled(format!(" {}", fitted_arg), fg(t, t.colors.text_dim).dim())
        };
        row([
            styled(" ", Style::new()),
            icon,
            styled(" ", Style::new()),
            styled(display_name.to_string(), fg(t, t.colors.text_dim)),
            source_badge,
            arg_node,
            styled(SUFFIX, fg(t, t.colors.text_muted).dim()),
        ])
    }

    fn render_running(
        &self,
        now: Instant,
        display_name: &str,
        primary_arg: &str,
        result_str: &str,
        spinner_frame: usize,
        width: usize,
    ) -> Node {
        let elapsed = self.elapsed_at(now);
        let show_elapsed = elapsed >= Duration::from_secs(2);

        let t = crate::tui::oil::theme::active();
        // No animated spinner in container content — spinners are chrome only.
        // Pending tools show a static ● indicator instead.
        let _ = spinner_frame; // unused — animation is in turn indicator
        let pending_icon = styled("\u{25CF}", fg(t, t.colors.text_dim));
        let badge_text = self.source_badge_text();
        let source_badge = self.render_source_badge();
        let elapsed_text = if show_elapsed {
            format!("  {}", format_elapsed(elapsed))
        } else {
            String::new()
        };
        // Header layout: " ● " (3 cols) + display_name + badge + " " + arg + elapsed
        let arg_budget = width.saturating_sub(
            3 + visible_width(display_name)
                + visible_width(&badge_text)
                + 1
                + visible_width(&elapsed_text),
        );
        let fitted_arg = fit_arg_to_width(primary_arg, arg_budget);
        let arg_node = if fitted_arg.is_empty() {
            Node::Empty
        } else {
            styled(format!(" {}", fitted_arg), fg(t, t.colors.text_dim).dim())
        };
        let header = row([
            styled(" ", Style::new()),
            pending_icon,
            styled(" ", Style::new()),
            styled(display_name, fg(t, t.colors.text_dim)),
            source_badge,
            arg_node,
            if show_elapsed {
                styled(
                    format!("  {}", format_elapsed(elapsed)),
                    fg(t, t.colors.text_dim).dim(),
                )
            } else {
                Node::Empty
            },
        ]);

        let result_node = if result_str.is_empty() {
            Node::Empty
        } else {
            format_streaming_output(result_str, width)
        };

        if matches!(result_node, Node::Empty) {
            header
        } else {
            col([header, result_node])
        }
    }
}

// --- Pure string/format utilities ---

pub(crate) fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{}s", secs / 60, secs % 60)
    }
}

/// The one-line form of a result: the summary of the render, else a short
/// result itself.
fn collapse_result(result: &str, summary: Option<&str>) -> Option<String> {
    if let Some(s) = summary {
        return Some(s.to_string());
    }

    if result.is_empty() {
        return None;
    }

    let inner = unwrap_json_result(result);
    (inner.lines().count() == 1 && inner.len() <= 60).then(|| inner.trim().to_string())
}

/// Format tool arguments for display.
pub fn format_tool_args(args: &str) -> String {
    if args.is_empty() || args == "{}" {
        return String::new();
    }

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(obj) = parsed.as_object() {
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => {
                            let collapsed = s.replace('\n', "↵").replace('\r', "");
                            if collapsed.chars().count() > 30 {
                                format!("\"{}…\"", truncate_to_chars(&collapsed, 27, false))
                            } else {
                                format!("\"{}\"", collapsed)
                            }
                        }
                        other => {
                            let s = other.to_string();
                            if s.chars().count() > 30 {
                                format!("{}…", truncate_to_chars(&s, 27, false))
                            } else {
                                s
                            }
                        }
                    };
                    format!("{}={}", k, val)
                })
                .collect();
            return pairs.join(", ");
        }
    }

    let oneline = args.replace('\n', " ").replace("  ", " ");
    if oneline.chars().count() <= 60 {
        oneline
    } else {
        format!("{}…", truncate_to_chars(&oneline, 57, false))
    }
}

/// Truncates `arg` to fit within `available` visible columns, appending "…"
/// when truncated. Returns empty when the budget is too small to convey any
/// information — the caller should drop the arg from the line entirely.
///
/// Strict width contract: the returned string's visible width is always
/// `<= available`. Callers like the tool-call header pass a budget computed
/// after the icon/name/badge/separator are accounted for, so undershooting
/// the budget is the only safe direction on narrow terminals.
fn fit_arg_to_width(arg: &str, available: usize) -> String {
    if arg.is_empty() || available == 0 {
        return String::new();
    }
    if visible_width(arg) <= available {
        arg.to_string()
    } else if available == 1 {
        "…".to_string()
    } else {
        format!("{}…", truncate_to_width(arg, available - 1, false))
    }
}

/// Format tool result for display.
pub fn format_tool_result(result: &str, width: usize) -> Node {
    let inner = unwrap_json_result(result);
    format_output_tail(&inner, "   ", width)
}

/// Format streaming output from a running tool.
pub fn format_streaming_output(output: &str, width: usize) -> Node {
    let unwrapped = unwrap_json_result(output);
    format_output_tail(&unwrapped, "     ", width)
}

/// Format the tail of output with a prefix and optional "more lines" indicator.
pub fn format_output_tail(output: &str, prefix: &str, width: usize) -> Node {
    const MAX_TAIL: usize = 3;
    let all_lines: Vec<&str> = output.lines().collect();
    let t = crate::tui::oil::theme::active();
    let bar_prefix = format!("{}{} ", prefix, t.decorations.separator_char);
    let truncate_at = width.saturating_sub(visible_width(&bar_prefix) + 1);
    let dim_style = fg(t, t.colors.text_dim);

    let hidden_count = all_lines.len().saturating_sub(MAX_TAIL);
    let visible_lines = &all_lines[hidden_count..];

    let indicator = if hidden_count > 0 {
        styled(
            format!("{}({} more lines)", bar_prefix, hidden_count),
            dim_style,
        )
    } else {
        Node::Empty
    };

    let line_nodes = visible_lines.iter().map(|line| {
        let display = if visible_width(line) > truncate_at {
            format!(
                "{}{}…",
                bar_prefix,
                truncate_to_width(line, truncate_at, false)
            )
        } else {
            format!("{}{}", bar_prefix, line)
        };
        styled(display, dim_style)
    });

    col(std::iter::once(indicator).chain(line_nodes))
}

/// Unwraps JSON-encoded strings and `{"result": "..."}` objects.
///
/// This is defense-in-depth: the daemon-client should already unwrap,
/// but we handle it here too in case of:
/// - Direct tool execution (bypassing daemon)
/// - Future format changes
/// - Data from cached/persisted sources
pub(crate) fn unwrap_json_result(result: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(result) {
        // Handle plain JSON string: "content with \n newlines"
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        // Handle wrapped result: {"result": "content"}
        if let Some(inner) = v.get("result").and_then(|r| r.as_str()) {
            return inner.to_string();
        }
    }
    result.to_string()
}

#[cfg(test)]
#[path = "tool_render_tests.rs"]
mod tests;
