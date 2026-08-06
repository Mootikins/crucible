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
use std::time::Duration;

/// Foreground-only style from a theme-resolved adaptive color. Condenses the
/// pervasive `Style::new().fg(t.resolve_color(...))` call sites.
fn fg(t: &ThemeConfig, color: AdaptiveColor) -> Style {
    Style::new().fg(t.resolve_color(color))
}

impl CachedToolCall {
    /// Render a compact tool call with default spinner frame (0) and diffs visible.
    pub fn render_compact(&self, width: usize) -> Node {
        self.render_compact_with(0, width, true)
    }

    /// Render a compact tool call with specified spinner frame; diffs visible.
    pub fn render_compact_with_frame(&self, spinner_frame: usize, width: usize) -> Node {
        self.render_compact_with(spinner_frame, width, true)
    }

    /// Render a compact tool call. `show_diffs` gates the diff body for
    /// Edit/Write tool calls; the rest of the result still renders.
    pub fn render_compact_with(
        &self,
        spinner_frame: usize,
        width: usize,
        show_diffs: bool,
    ) -> Node {
        if self.superseded {
            return Node::Empty;
        }

        let display_name = self.display_name();
        let auto_primary = format_primary_arg_for(&self.name, &self.args);
        let primary_arg: &str = self
            .lua_primary_arg
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&auto_primary);
        let result_str = self.result();

        let inner = if let Some(ref error) = self.error {
            self.render_error(&display_name, primary_arg, error, width)
        } else if self.complete {
            self.render_complete(&display_name, primary_arg, &result_str, width, show_diffs)
        } else {
            self.render_running(
                &display_name,
                primary_arg,
                &result_str,
                spinner_frame,
                width,
            )
        };

        let description_node = self.render_description();
        if matches!(description_node, Node::Empty) {
            inner
        } else {
            col([inner, description_node])
        }
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
        let result_summary = if !result_str.is_empty() {
            summarize_tool_result(&self.name, result_str)
        } else {
            None
        };

        let collapsed = collapse_result(&self.name, result_str, result_summary.as_deref());
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
            format_tool_result(&self.name, result_str, width)
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

    fn render_running(
        &self,
        display_name: &str,
        primary_arg: &str,
        result_str: &str,
        spinner_frame: usize,
        width: usize,
    ) -> Node {
        let elapsed = self.elapsed();
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

/// The identity a result summary is keyed on.
///
/// **Divergence A4.** These tables used to match `self.name` literally, which
/// only ever named the internal agent's tools. ACP carries no tool name on the
/// wire — only a prose `title` — so a delegated card's `name` is
/// `humanize_tool_title(title)`: `Read File`, `Read`. `"read_file"` equals
/// neither, so a delegated read painted the file body into the transcript
/// while the internal read of the same file collapsed to a one-line summary.
///
/// Routing the match through the humanizer keys the summary on exactly the
/// identity [`CachedToolCall::display_name`] already puts in the card header,
/// so the two can no longer disagree. The function is idempotent on an
/// already-clean title, which is what lets one arm serve `read_file`,
/// `mcp_read`, `mcp__crucible__read_file` and a bare prose `Read`.
///
/// Membership is preserved exactly: every name each arm used to list maps into
/// the arm it now lists (`read_file` → `Read File`, `mcp_read` → `Read`,
/// `edit`/`mcp_edit` → `Edit`, …). `edit_file` and `write_file` stay outside
/// the `Edit`/`Write` arms, as they were — and that costs no parity, because
/// both tools answer with one short line (`Replaced N occurrence(s)`), which
/// [`collapse_result`] returns verbatim before it ever reaches the table.
///
/// # Namespaced names are not keys
///
/// The humanizer exists to build a *display name*, so it strips whatever
/// namespace a tool arrived under: `mcp__crucible__write`, `mcp__fs__write` and
/// `plugin_foo__write` all show as `Write`. That is right in a header and wrong
/// as a summary key — [`collapse_result`]'s `Write` arm is unconditional and
/// replaces the entire result with the word `written`, so keying on the
/// stripped name would silently destroy the output of any third-party MCP or
/// plugin tool whose trailing segment happened to be `write` or `edit`.
///
/// So a `__` in the name — the separator every namespacing scheme here uses —
/// disqualifies it. That costs nothing on the ACP path this normalization was
/// added for: a delegated card's name has *already* been through the humanizer
/// (`acp_handle/translate.rs`), and its title-caser splits on `_`, so a
/// humanized name never contains `__`. The rule is what keeps the change
/// additive — it admits the prose spellings ACP needs and no name that did not
/// already reach these tables before.
///
/// # A title is prose, so it carries its subject
///
/// The first version of this stopped at the humanizer, which covers a title
/// that happens to *look* like an internal tool name and nothing else. The
/// repo's richest recording of a real Claude Code session that ran tools
/// (`assets/fixtures/malformed-acp-recording.jsonl`) shows that is not what
/// agents send: its
/// titles are `Find`, `Terminal`, `Read File` and `Read tools/hello.rn`. A
/// resolved title appends the thing being acted on, so the key is the leading
/// Title-Cased run — everything up to the first word that does not start
/// uppercase.
///
/// That cut cannot pull an internal tool into a new arm: `title_case`
/// uppercases *every* word it produces from a snake_case or kebab-case name,
/// so `read_notes` → `Read Notes` has no lowercase tail to lose and stays out
/// of the `Read` arm exactly as it was. It only bites on agent-authored prose,
/// which is the only thing that has a subject appended. A title whose *first*
/// word is not Title-Cased (a bare shell command, say) keeps its whole
/// humanized form and, as before, matches nothing.
///
/// The one synonym the arms below list — `Find` for glob — is read off that
/// same recording rather than guessed. ACP has no tool name on the wire, so
/// there is no closed set here and never will be until schema 1.6.0's
/// `unstable_tool_call_name` lands; see `acp_tool_name`
/// (`agent_manager/messaging/permission.rs`), which is the other place paying
/// for the same missing field. Keying on ACP's `kind` instead would not close
/// it either: `Glob` and `Grep` are both `ToolKind::Search`, so the two arms
/// below could not be told apart without the title anyway.
fn summary_key(name: &str) -> Option<String> {
    if name.contains("__") {
        return None;
    }
    let humanized = crucible_daemon::acp::streaming::humanize_tool_title(name);
    let leading_run: Vec<&str> = humanized
        .split_whitespace()
        .take_while(|word| word.starts_with(char::is_uppercase))
        .collect();
    if leading_run.is_empty() {
        return Some(humanized);
    }
    Some(leading_run.join(" "))
}

fn collapse_result(name: &str, result: &str, summary: Option<&str>) -> Option<String> {
    if let Some(s) = summary {
        return Some(s.to_string());
    }

    if result.is_empty() {
        return None;
    }

    let inner = unwrap_json_result(result);
    let lines: Vec<&str> = inner.lines().collect();
    if lines.len() == 1 && inner.len() <= 60 {
        return Some(inner.trim().to_string());
    }

    match summary_key(name).as_deref() {
        Some("Write") => Some("written".to_string()),
        Some("Edit") => Some("applied".to_string()),
        _ => None,
    }
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

/// Extracts the primary argument from a JSON arg blob and normalizes it to a
/// single line. Does NOT truncate — callers fit it to available width via
/// [`fit_arg_to_width`].
/// The argument worth showing on a tool's status row.
///
/// Delegates to the shared projection so the TUI, the web and the daemon's
/// deny messages all name the same argument. Newlines collapse because this
/// is one row: `ToolDisplay::summary` keeps the first line, and the full text
/// is in the expanded view.
pub fn format_primary_arg_for(tool_name: &str, args: &str) -> String {
    if args.is_empty() || args == "{}" {
        return String::new();
    }
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) else {
        return String::new();
    };
    crucible_core::types::ToolDisplay::of(tool_name, &parsed)
        .primary
        .map(|p| p.replace('\n', " ").replace('\r', ""))
        .unwrap_or_default()
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
pub fn format_tool_result(name: &str, result: &str, width: usize) -> Node {
    if let Some(summary) = summarize_tool_result(name, result) {
        let t = crate::tui::oil::theme::active();
        return styled(format!("   {}", summary), fg(t, t.colors.text_muted));
    }
    let inner = unwrap_json_result(result);
    format_output_tail(&inner, "   ", width)
}

/// Summarize tool result into a short string.
pub fn summarize_tool_result(name: &str, result: &str) -> Option<String> {
    let inner = unwrap_json_result(result);
    match summary_key(name).as_deref() {
        Some("Read File" | "Read") => {
            // Extract short bracketed metadata (e.g., "[Directory Context: ...]") if present,
            // but not spill references or long content
            let bracket_summary = inner.rfind('[').and_then(|i| {
                let bracket = &inner[i..];
                if bracket.len() <= 60 && !bracket.contains("$CRU_SESSION_DIR") {
                    Some(bracket.to_string())
                } else {
                    None
                }
            });
            bracket_summary.or_else(|| Some(format!("{} lines", inner.lines().count())))
        }
        // `Find` is Claude Code's title for its glob (malformed-acp-recording.jsonl).
        Some("Glob" | "Find") => count_newline_items(&inner).map(|n| format!("{} files", n)),
        Some("Grep") => count_grep_matches(&inner).map(|n| format!("{} matches", n)),
        Some("Edit") if inner.contains("success") || inner.contains("applied") => {
            Some("applied".to_string())
        }
        Some("Write") if inner.contains("success") || inner.contains("written") => {
            Some("written".to_string())
        }
        // `Terminal` — Claude Code's title for its bash (malformed-acp-recording.jsonl) — is
        // deliberately *not* listed here. This arm answers only when the
        // result is one line under 60 characters, which is exactly when
        // `collapse_result`'s name-independent short-result branch answers with
        // the same string; adding the synonym would be an arm with no
        // observable effect, and a test for it would pass either way. Bash
        // parity holds by that route instead — pinned by
        // `a_delegated_shell_command_needs_no_synonym_to_match_the_internal_one`.
        Some("Bash") => {
            let lines: Vec<&str> = inner.lines().collect();
            if lines.len() <= 1 && inner.len() < 60 {
                Some(inner.trim().to_string())
            } else {
                None
            }
        }
        _ => None,
    }
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

fn count_newline_items(result: &str) -> Option<usize> {
    let newline_count = result.matches('\n').count();
    let escaped_newline_count = result.matches("\\n").count();
    let count = newline_count.max(escaped_newline_count) + 1;
    (count > 1).then_some(count)
}

fn count_grep_matches(result: &str) -> Option<usize> {
    let count = result
        .lines()
        .filter(|l| l.contains(':') && !l.trim().is_empty())
        .count();
    (count > 0).then_some(count)
}

#[cfg(test)]
#[path = "tool_render_tests.rs"]
mod tests;
