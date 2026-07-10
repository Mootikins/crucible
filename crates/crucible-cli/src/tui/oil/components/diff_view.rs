//! Shared diff renderer used by tool-call scrollback and permission popups.
//!
//! Takes a `FileDiff` plus options and produces a `Node`. Picks side-by-side
//! when the width budget allows it, falls back to unified otherwise. The
//! caller is responsible for everything else (extracting the diff from a
//! tool call, deciding whether to render at all, etc).

use crate::formatting::SyntaxHighlighter;
use crate::tui::oil::theme;
use crate::tui::oil::utils::{truncate_to_chars, visible_width};
use crucible_core::types::acp::{FileDiff, MAX_DIFF_BYTES};
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::{Color, Style};
use similar::{ChangeTag, TextDiff};
use std::path::Path;

pub const SIDE_BY_SIDE_MIN_WIDTH: usize = 120;

fn count_changes(old: &str, new: &str) -> (usize, usize) {
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0;
    let mut removed = 0;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    SideBySide,
}

#[derive(Debug, Clone)]
pub struct DiffOptions {
    pub max_width: usize,
    pub max_lines: Option<usize>,
    pub context_lines: usize,
    pub collapsed: bool,
    /// `None` = auto-pick from `max_width`; `Some` = forced override (used by tests).
    pub layout: Option<DiffLayout>,
}

impl DiffOptions {
    pub fn for_width(max_width: usize) -> Self {
        Self {
            max_width,
            max_lines: Some(200),
            context_lines: 3,
            collapsed: false,
            layout: None,
        }
    }

    pub fn resolved_layout(&self) -> DiffLayout {
        self.layout
            .unwrap_or(if self.max_width >= SIDE_BY_SIDE_MIN_WIDTH {
                DiffLayout::SideBySide
            } else {
                DiffLayout::Unified
            })
    }
}

fn diff_action(diff: &FileDiff) -> &'static str {
    match (&diff.old_content, diff.new_content.as_str()) {
        (None, _) => "create",
        (Some(_), "") => "delete",
        (Some(_), _) => "edit",
    }
}

fn render_header(diff: &FileDiff, line_counts: Option<(usize, usize)>) -> Node {
    let t = theme::active();
    let action = diff_action(diff);
    let mut parts = vec![
        styled(
            format!("{} ", action),
            Style::new().fg(t.resolve_color(t.colors.info)),
        ),
        styled(
            diff.path.clone(),
            Style::new().fg(t.resolve_color(t.colors.text)),
        ),
    ];
    if let Some((added, removed)) = line_counts {
        parts.push(styled(
            format!("  +{} -{}", added, removed),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        ));
    }
    row(parts)
}

pub fn render_diff(diff: &FileDiff, opts: &DiffOptions) -> Node {
    let old = diff.old_content.as_deref().unwrap_or("");
    let new = diff.new_content.as_str();

    // Oversize guard runs *before* count_changes / TextDiff / highlighter so a
    // 10 MiB blob doesn't pin the UI thread just to render a one-line header.
    if old.len() > MAX_DIFF_BYTES || new.len() > MAX_DIFF_BYTES {
        let header = render_header(diff, None);
        if opts.collapsed {
            return header;
        }
        let t = theme::active();
        let msg = styled(
            "  diff suppressed (file too large)",
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        );
        return col([header, msg]);
    }

    let counts = count_changes(old, new);
    let header = render_header(diff, Some(counts));

    if opts.collapsed {
        return header;
    }

    let language = infer_language(&diff.path);
    let highlighter = SyntaxHighlighter::active();

    let body = match opts.resolved_layout() {
        DiffLayout::Unified => render_unified(
            old,
            new,
            opts.context_lines,
            opts.max_width,
            opts.max_lines,
            &highlighter,
            language.as_deref(),
        ),
        DiffLayout::SideBySide => render_side_by_side(
            old,
            new,
            opts.max_width,
            opts.max_lines,
            &highlighter,
            language.as_deref(),
        ),
    };

    col([header, body])
}

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "tiff"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "wasm"
            | "class"
            | "jar"
            | "mp3"
            | "mp4"
            | "wav"
            | "ogg"
            | "flac"
            | "mov"
            | "avi"
            | "mkv"
            | "webm"
    )
}

fn infer_language(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .filter(|e| !is_binary_extension(e))
}

/// Highlight a single diff line and recolor each span's `fg` to `diff_fg` so
/// red/green diff coloring wins over syntax tokens while preserving bold/italic
/// (and any future bg) bits emitted by the highlighter.
fn highlight_line(
    highlighter: &SyntaxHighlighter,
    language: Option<&str>,
    line: &str,
    diff_fg: Option<Color>,
) -> Vec<Node> {
    let lang = match language {
        Some(l) if SyntaxHighlighter::supports_language(l) => l,
        _ => {
            let mut style = Style::new();
            if let Some(fg) = diff_fg {
                style = style.fg(fg);
            }
            return vec![styled(line.to_string(), style)];
        }
    };

    if line.is_empty() {
        let mut style = Style::new();
        if let Some(fg) = diff_fg {
            style = style.fg(fg);
        }
        return vec![styled(String::new(), style)];
    }

    highlighter
        .highlight(line, lang)
        .into_iter()
        .flat_map(|hl| hl.spans)
        .map(|span| {
            let style = match diff_fg {
                Some(fg) => span.style.fg(fg),
                None => span.style,
            };
            styled(span.text, style)
        })
        .collect()
}

/// One row of the side-by-side view: optional left line, optional right line.
struct PairedRow {
    left: Option<(String, ChangeTag)>,
    right: Option<(String, ChangeTag)>,
}

fn pair_changes(old: &str, new: &str) -> Vec<PairedRow> {
    let diff = TextDiff::from_lines(old, new);
    let mut rows = Vec::new();
    let mut pending_deletes: Vec<String> = Vec::new();
    let mut pending_inserts: Vec<String> = Vec::new();

    fn flush(rows: &mut Vec<PairedRow>, deletes: &mut Vec<String>, inserts: &mut Vec<String>) {
        let n = deletes.len().max(inserts.len());
        for i in 0..n {
            rows.push(PairedRow {
                left: deletes.get(i).map(|s| (s.clone(), ChangeTag::Delete)),
                right: inserts.get(i).map(|s| (s.clone(), ChangeTag::Insert)),
            });
        }
        deletes.clear();
        inserts.clear();
    }

    for change in diff.iter_all_changes() {
        let line = change.value().trim_end_matches('\n').to_string();
        match change.tag() {
            ChangeTag::Delete => pending_deletes.push(line),
            ChangeTag::Insert => pending_inserts.push(line),
            ChangeTag::Equal => {
                flush(&mut rows, &mut pending_deletes, &mut pending_inserts);
                rows.push(PairedRow {
                    left: Some((line.clone(), ChangeTag::Equal)),
                    right: Some((line, ChangeTag::Equal)),
                });
            }
        }
    }
    flush(&mut rows, &mut pending_deletes, &mut pending_inserts);
    rows
}

/// One change inside a collected hunk, carrying enough info to emit later
/// once the full hunk is known and the budget can be balanced across
/// deletes/inserts.
struct HunkChange {
    tag: ChangeTag,
    line: String,
}

fn render_unified(
    old: &str,
    new: &str,
    context_lines: usize,
    max_width: usize,
    max_lines: Option<usize>,
    highlighter: &SyntaxHighlighter,
    language: Option<&str>,
) -> Node {
    if old == new {
        return Node::Empty;
    }

    let diff = TextDiff::from_lines(old, new);

    // Two-pass approach: collect hunks first, then emit with a budget that
    // balances deletes vs inserts. The previous single-pass design cut at
    // `max_lines` mid-hunk; because `similar` emits all deletes before any
    // inserts within a contiguous changed region, a 30D-then-30I hunk with
    // budget=8 produced 8 minus-lines and zero plus-lines — the user saw
    // what was removed but never what replaced it. Collecting first lets
    // us split the budget proportionally per hunk.
    let mut hunks: Vec<Vec<HunkChange>> = Vec::new();
    let mut current_hunk: Vec<HunkChange> = Vec::new();
    let mut in_hunk = false;
    let mut context_buffer: Vec<String> = Vec::new();
    let mut pending_context: Vec<String> = Vec::new();

    for change in diff.iter_all_changes() {
        let tag = change.tag();
        let line_content = change.value().trim_end_matches('\n').to_string();

        match tag {
            ChangeTag::Equal => {
                if in_hunk {
                    if context_lines > 0 && pending_context.len() < context_lines {
                        pending_context.push(line_content.clone());
                    } else {
                        // Hunk ends here — append the trailing context lines
                        // we deferred and close it out.
                        for ctx_line in pending_context.drain(..) {
                            current_hunk.push(HunkChange {
                                tag: ChangeTag::Equal,
                                line: ctx_line,
                            });
                        }
                        hunks.push(std::mem::take(&mut current_hunk));
                        in_hunk = false;
                    }
                }
                if context_lines > 0 {
                    context_buffer.push(line_content);
                    if context_buffer.len() > context_lines {
                        context_buffer.remove(0);
                    }
                }
            }
            ChangeTag::Delete | ChangeTag::Insert => {
                if !in_hunk {
                    in_hunk = true;
                    for ctx_line in context_buffer.drain(..) {
                        current_hunk.push(HunkChange {
                            tag: ChangeTag::Equal,
                            line: ctx_line,
                        });
                    }
                } else {
                    for ctx_line in pending_context.drain(..) {
                        current_hunk.push(HunkChange {
                            tag: ChangeTag::Equal,
                            line: ctx_line,
                        });
                    }
                }
                current_hunk.push(HunkChange {
                    tag,
                    line: line_content,
                });
            }
        }
    }
    if in_hunk {
        for ctx_line in pending_context.drain(..) {
            current_hunk.push(HunkChange {
                tag: ChangeTag::Equal,
                line: ctx_line,
            });
        }
        hunks.push(current_hunk);
    }

    // Emit pass: walk hunks within the line budget. Each hunk that exceeds
    // its share of the remaining budget gets a balanced trim.
    let t = theme::active();
    let delete_color = t.resolve_color(t.colors.error);
    let insert_color = t.resolve_color(t.colors.success);
    let context_color = t.resolve_color(t.colors.text_dim);
    let line_budget = max_width.saturating_sub(1);

    let render_change = |change: &HunkChange| -> Node {
        let (prefix, color) = match change.tag {
            ChangeTag::Delete => ("-", delete_color),
            ChangeTag::Insert => ("+", insert_color),
            ChangeTag::Equal => (" ", context_color),
        };
        let trunc = truncate_to_chars(&change.line, line_budget, true).into_owned();
        let mut spans = vec![styled(prefix, Style::new().fg(color))];
        spans.extend(highlight_line(highlighter, language, &trunc, Some(color)));
        row(spans)
    };

    let mut nodes: Vec<Node> = Vec::new();
    let mut remaining_added = 0usize;
    let mut remaining_removed = 0usize;
    let max = max_lines.unwrap_or(usize::MAX);

    for hunk in &hunks {
        let remaining_budget = max.saturating_sub(nodes.len());
        if remaining_budget == 0 {
            for c in hunk {
                match c.tag {
                    ChangeTag::Insert => remaining_added += 1,
                    ChangeTag::Delete => remaining_removed += 1,
                    ChangeTag::Equal => {}
                }
            }
            continue;
        }

        let (deletes, inserts, contexts) = count_kinds(hunk);
        if hunk.len() <= remaining_budget {
            for c in hunk {
                nodes.push(render_change(c));
            }
        } else {
            // Allocate the budget proportionally between deletes and inserts,
            // reserving the smaller of (remaining_budget, contexts) for the
            // surrounding context lines.
            //
            // Invariant: `delete_cap + insert_cap <= change_budget`. The two
            // adjustments below keep both sides visible when possible
            // without overshooting:
            //   1. If proportional dcap rounded to 0 but deletes exist,
            //      give them at least one slot (consuming budget).
            //   2. If dcap consumed the whole budget but inserts exist and
            //      the budget can spare it (>=2), reduce dcap by one so
            //      inserts get a slot. At budget=1 with both sides present,
            //      the tie-breaker is deletes (similar's natural emit order).
            let ctx_share = contexts.min(remaining_budget / 4);
            let change_budget = remaining_budget.saturating_sub(ctx_share);
            let total_changes = deletes + inserts;
            let (delete_cap, insert_cap) = if total_changes == 0 {
                (0, 0)
            } else {
                let mut dcap = (change_budget * deletes / total_changes).min(deletes);
                if deletes > 0 && dcap == 0 && change_budget >= 1 {
                    dcap = 1;
                }
                if inserts > 0 && dcap == change_budget && change_budget >= 2 {
                    dcap = change_budget - 1;
                }
                let icap = change_budget.saturating_sub(dcap).min(inserts);
                (dcap, icap)
            };

            let mut emitted_deletes = 0usize;
            let mut emitted_inserts = 0usize;
            let mut emitted_contexts = 0usize;
            for c in hunk {
                match c.tag {
                    ChangeTag::Delete => {
                        if emitted_deletes < delete_cap {
                            nodes.push(render_change(c));
                            emitted_deletes += 1;
                        } else {
                            remaining_removed += 1;
                        }
                    }
                    ChangeTag::Insert => {
                        if emitted_inserts < insert_cap {
                            nodes.push(render_change(c));
                            emitted_inserts += 1;
                        } else {
                            remaining_added += 1;
                        }
                    }
                    ChangeTag::Equal => {
                        if emitted_contexts < ctx_share {
                            nodes.push(render_change(c));
                            emitted_contexts += 1;
                        }
                    }
                }
            }
        }
    }

    if remaining_added + remaining_removed > 0 {
        let footer = styled(
            format!(
                "  … {} more lines (+{} -{})",
                remaining_added + remaining_removed,
                remaining_added,
                remaining_removed
            ),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        );
        nodes.push(footer);
    }

    if nodes.is_empty() {
        Node::Empty
    } else {
        col(nodes)
    }
}

fn count_kinds(hunk: &[HunkChange]) -> (usize, usize, usize) {
    let mut d = 0;
    let mut i = 0;
    let mut e = 0;
    for c in hunk {
        match c.tag {
            ChangeTag::Delete => d += 1,
            ChangeTag::Insert => i += 1,
            ChangeTag::Equal => e += 1,
        }
    }
    (d, i, e)
}

fn render_side_by_side(
    old: &str,
    new: &str,
    max_width: usize,
    max_lines: Option<usize>,
    highlighter: &SyntaxHighlighter,
    language: Option<&str>,
) -> Node {
    let t = theme::active();
    let pane_width = max_width.saturating_sub(3) / 2;
    // Defensive: Auto only picks SideBySide at max_width >= 120, but explicit
    // callers can request a narrow side-by-side. Fall back to unified rather
    // than render unreadable 3-column-wide panes.
    if pane_width < 10 {
        return render_unified(old, new, 3, max_width, max_lines, highlighter, language);
    }
    let separator_style = Style::new().fg(t.resolve_color(t.colors.text_dim)).dim();
    let delete_color = t.resolve_color(t.colors.error);
    let insert_color = t.resolve_color(t.colors.success);
    let context_color = t.resolve_color(t.colors.text_dim);

    let cell = |content: Option<&(String, ChangeTag)>, width: usize| -> Node {
        match content {
            None => styled(" ".repeat(width), Style::new()),
            Some((text, tag)) => {
                let diff_fg = match tag {
                    ChangeTag::Delete => delete_color,
                    ChangeTag::Insert => insert_color,
                    ChangeTag::Equal => context_color,
                };
                let trunc = truncate_to_chars(text, width, true).into_owned();
                let used = visible_width(&trunc);
                let mut spans = highlight_line(highlighter, language, &trunc, Some(diff_fg));
                if used < width {
                    spans.push(styled(" ".repeat(width - used), Style::new()));
                }
                row(spans)
            }
        }
    };

    let all_rows = pair_changes(old, new);
    let (visible, dropped): (&[PairedRow], &[PairedRow]) = match max_lines {
        Some(max) if all_rows.len() > max => all_rows.split_at(max),
        _ => (all_rows.as_slice(), &[]),
    };

    let mut rows = Vec::with_capacity(visible.len() + 1);
    for pr in visible {
        rows.push(row([
            cell(pr.left.as_ref(), pane_width),
            styled(" │ ", separator_style),
            cell(pr.right.as_ref(), pane_width),
        ]));
    }

    if !dropped.is_empty() {
        let mut remaining_added = 0usize;
        let mut remaining_removed = 0usize;
        for pr in dropped {
            if let Some((_, ChangeTag::Insert)) = &pr.right {
                remaining_added += 1;
            }
            if let Some((_, ChangeTag::Delete)) = &pr.left {
                remaining_removed += 1;
            }
        }
        let footer = styled(
            format!(
                "  … {} more lines (+{} -{})",
                dropped.len(),
                remaining_added,
                remaining_removed
            ),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        );
        rows.push(footer);
    }

    col(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_string;

    fn render(diff: &FileDiff, opts: &DiffOptions) -> String {
        render_to_string(&render_diff(diff, opts), opts.max_width)
    }

    #[test]
    fn auto_layout_picks_unified_under_threshold() {
        let opts = DiffOptions::for_width(119);
        assert_eq!(opts.resolved_layout(), DiffLayout::Unified);
    }

    #[test]
    fn auto_layout_picks_side_by_side_at_threshold() {
        let opts = DiffOptions::for_width(120);
        assert_eq!(opts.resolved_layout(), DiffLayout::SideBySide);
    }

    #[test]
    fn explicit_layout_overrides_auto() {
        let mut opts = DiffOptions::for_width(200);
        opts.layout = Some(DiffLayout::Unified);
        assert_eq!(opts.resolved_layout(), DiffLayout::Unified);
    }

    #[test]
    fn header_shows_path_and_action_for_create() {
        let d = FileDiff::new("src/foo.rs", "fn new() {}\n");
        let mut opts = DiffOptions::for_width(80);
        opts.collapsed = true;
        let out = render(&d, &opts);
        assert!(out.contains("src/foo.rs"), "got: {out:?}");
        assert!(out.to_lowercase().contains("create"), "got: {out:?}");
    }

    #[test]
    fn header_shows_path_and_action_for_modify() {
        let d = FileDiff::from_contents("src/foo.rs", Some("a\n".into()), "b\n");
        let mut opts = DiffOptions::for_width(80);
        opts.collapsed = true;
        let out = render(&d, &opts);
        assert!(out.contains("src/foo.rs"));
        assert!(out.to_lowercase().contains("edit") || out.to_lowercase().contains("modify"));
    }

    #[test]
    fn collapsed_renders_only_header() {
        let d = FileDiff::from_contents(
            "src/foo.rs",
            Some("line1\nline2\n".into()),
            "line1\nCHANGED\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.collapsed = true;
        let out = render(&d, &opts);
        assert!(
            !out.contains("CHANGED"),
            "collapsed must not show body: {out:?}"
        );
    }

    #[test]
    fn unified_renders_added_and_removed_lines() {
        let d = FileDiff::from_contents(
            "x.rs",
            Some("alpha\nbeta\ngamma\n".into()),
            "alpha\nbeta-CHANGED\ngamma\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = render(&d, &opts);
        assert!(out.contains("-beta"), "got: {out:?}");
        assert!(out.contains("+beta-CHANGED"), "got: {out:?}");
    }

    #[test]
    fn header_shows_line_counts_when_expanded() {
        let d = FileDiff::from_contents("x.rs", Some("a\nb\n".into()), "a\nB1\nB2\n");
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        opts.context_lines = 0;
        let out = render(&d, &opts);
        assert!(out.contains("+2"), "expected +2 in: {out:?}");
        assert!(out.contains("-1"), "expected -1 in: {out:?}");
    }

    #[test]
    fn side_by_side_pairs_lines_in_two_columns() {
        let d = FileDiff::from_contents(
            "x.rs",
            Some("kept\nold\nkept2\n".into()),
            "kept\nnew\nkept2\n",
        );
        let mut opts = DiffOptions::for_width(140);
        opts.layout = Some(DiffLayout::SideBySide);
        opts.context_lines = 1;
        let out = render(&d, &opts);
        let line_with_old = out.lines().find(|l| l.contains("old")).unwrap_or("");
        assert!(
            line_with_old.contains("new"),
            "expected old and new on same row, got line: {line_with_old:?}\nfull:\n{out}"
        );
    }

    #[test]
    fn side_by_side_pads_unmatched_insert() {
        let d = FileDiff::from_contents("x.rs", Some("a\nb\n".into()), "a\nNEW\nb\n");
        let mut opts = DiffOptions::for_width(140);
        opts.layout = Some(DiffLayout::SideBySide);
        let out = render(&d, &opts);
        let line_with_new = out.lines().find(|l| l.contains("NEW")).unwrap_or("");
        assert!(
            line_with_new.contains('│'),
            "expected pane separator on insert row: {line_with_new:?}"
        );
    }

    #[test]
    fn syntax_highlighted_diff_for_rust_extension() {
        let d = FileDiff::from_contents(
            "src/lib.rs",
            Some("fn main() {\n    let x = 1;\n}\n".into()),
            "fn main() {\n    let x = 2;\n}\n",
        );
        let mut opts = DiffOptions::for_width(140);
        opts.layout = Some(DiffLayout::SideBySide);
        let out = render(&d, &opts);
        assert!(
            out.contains("fn"),
            "expected `fn` keyword to survive highlighting: {out:?}"
        );
        assert!(
            out.contains("let x = 1"),
            "expected old line content to be present: {out:?}"
        );
        assert!(
            out.contains("let x = 2"),
            "expected new line content to be present: {out:?}"
        );
    }

    #[test]
    fn binary_extension_skips_highlighting_no_panic() {
        let d = FileDiff::from_contents(
            "assets/logo.png",
            Some("\u{89}PNG\r\n\u{1a}\nbinaryjunk\n".into()),
            "\u{89}PNG\r\n\u{1a}\nDIFFERENT\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let _ = render(&d, &opts);
    }

    /// When a hunk is delete-heavy followed by insert-heavy (similar's
    /// natural emit order), truncation must show some of BOTH sides — not
    /// fill the budget with deletes and drop every insert. Regression for
    /// the unified-diff hide-all-inserts bug.
    #[test]
    fn truncation_balances_deletes_and_inserts() {
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..30 {
            old.push_str(&format!("old_line_{i}\n"));
            new.push_str(&format!("new_line_{i}\n"));
        }
        let d = FileDiff::from_contents("big.rs", Some(old), new);
        let mut opts = DiffOptions::for_width(80);
        opts.max_lines = Some(8);
        opts.layout = Some(DiffLayout::Unified);
        let out = render(&d, &opts);
        assert!(
            out.contains("-old_line_"),
            "expected at least one delete line: {out}"
        );
        assert!(
            out.contains("+new_line_"),
            "expected at least one insert line: {out}"
        );
    }

    /// `max_lines` is a hard cap. Earlier proportional split applied
    /// `.max(1)` to both delete- and insert-caps, which could emit two
    /// rows for a budget of one. Verify the body row count never exceeds
    /// `max_lines` regardless of the delete/insert ratio.
    #[test]
    fn truncation_never_overshoots_max_lines() {
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..20 {
            old.push_str(&format!("old_{i}\n"));
            new.push_str(&format!("new_{i}\n"));
        }
        let d = FileDiff::from_contents("x.rs", Some(old), new);
        for max in 1usize..=6 {
            let mut opts = DiffOptions::for_width(80);
            opts.max_lines = Some(max);
            opts.layout = Some(DiffLayout::Unified);
            let out = render(&d, &opts);
            // Body lines are the +/- prefixed rows; the header and footer
            // are separate. Count those two prefixes only.
            let body_lines = out
                .lines()
                .filter(|l| {
                    let stripped = crucible_oil::ansi::strip_ansi(l);
                    stripped.starts_with('+') || stripped.starts_with('-')
                })
                .count();
            assert!(
                body_lines <= max,
                "max={max} produced {body_lines} body lines: {out}"
            );
        }
    }

    #[test]
    fn truncation_footer_appears_when_max_lines_exceeded() {
        let new = (0..300).map(|i| format!("line{i}\n")).collect::<String>();
        let d = FileDiff::from_contents("x.rs", Some(String::new()), new);
        let mut opts = DiffOptions::for_width(80);
        opts.max_lines = Some(50);
        opts.layout = Some(DiffLayout::Unified);
        let out = render(&d, &opts);
        assert!(
            out.contains("more lines"),
            "expected truncation footer: {out:?}"
        );
    }

    #[test]
    fn oversize_diff_shows_suppressed_message() {
        // Either side > 1 MiB → suppress
        let big = "x".repeat(1024 * 1024 + 1);
        let d = FileDiff::from_contents("x.rs", Some(String::new()), big);
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = render(&d, &opts);
        assert!(
            out.to_lowercase().contains("suppressed") || out.to_lowercase().contains("too large")
        );
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crucible_oil::render::render_to_string;

    fn snap(diff: &FileDiff, opts: &DiffOptions) -> String {
        render_to_string(&render_diff(diff, opts), opts.max_width)
    }

    /// Modify a small Rust snippet at width 80 with explicit unified layout.
    #[test]
    fn snap_unified_modify_rust() {
        let d = FileDiff::from_contents(
            "src/lib.rs",
            Some("pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n".into()),
            "pub fn add(a: u32, b: u32) -> u32 {\n    a.saturating_add(b)\n}\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Same diff at width 140 with explicit side-by-side layout.
    #[test]
    fn snap_side_by_side_modify_rust() {
        let d = FileDiff::from_contents(
            "src/lib.rs",
            Some("pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n".into()),
            "pub fn add(a: u32, b: u32) -> u32 {\n    a.saturating_add(b)\n}\n",
        );
        let mut opts = DiffOptions::for_width(140);
        opts.layout = Some(DiffLayout::SideBySide);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// File-create diff (no `old_content`); auto layout at width 80 → unified.
    #[test]
    fn snap_create_new_file() {
        let d = FileDiff::new("README.md", "# Hello\n\nA new file.\n");
        let opts = DiffOptions::for_width(80);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Collapsed view yields header only — no diff body.
    #[test]
    fn snap_collapsed_header_only() {
        let d = FileDiff::from_contents(
            "x.rs",
            Some("alpha\nbeta\ngamma\n".into()),
            "alpha\nbeta-CHANGED\ngamma\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.collapsed = true;
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// At width 60 (< SIDE_BY_SIDE_MIN_WIDTH = 120), Auto must pick unified.
    #[test]
    fn snap_narrow_terminal_forces_unified() {
        let d = FileDiff::from_contents(
            "src/lib.rs",
            Some("pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n".into()),
            "pub fn add(a: u32, b: u32) -> u32 {\n    a.saturating_add(b)\n}\n",
        );
        let opts = DiffOptions::for_width(60);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Delete-file diff (`new_content == ""`): every body row must be a
    /// minus-prefixed line; header should say "delete".
    #[test]
    fn snap_delete_file_shows_all_red() {
        let d = FileDiff::from_contents(
            "src/old.rs",
            Some("fn doomed() {}\nfn also_doomed() {}\n".into()),
            "",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Truncation: with max_lines smaller than the change set, the body
    /// must be cut and a "more lines" footer must appear.
    #[test]
    fn snap_truncation_footer() {
        // 30 changed lines, max_lines=8 → must truncate.
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..30 {
            old.push_str(&format!("line_{i}_old\n"));
            new.push_str(&format!("line_{i}_NEW\n"));
        }
        let d = FileDiff::from_contents("big.rs", Some(old), new);
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        opts.max_lines = Some(8);
        let out = snap(&d, &opts);
        assert!(out.contains("more"), "expected truncation footer: {out:?}");
        insta::assert_snapshot!(out);
    }

    /// Oversize fallback: either side > MAX_DIFF_BYTES suppresses the body.
    #[test]
    fn snap_oversize_fallback_suppresses_body() {
        let big = "x".repeat(MAX_DIFF_BYTES + 1);
        let d = FileDiff::from_contents("huge.bin", Some("a\n".to_string()), big);
        let opts = DiffOptions::for_width(80);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Side-by-side row with an unmatched insert must pad the left pane and
    /// preserve the central pane separator on every row.
    #[test]
    fn snap_side_by_side_unmatched_insert_padding() {
        let d = FileDiff::from_contents(
            "x.rs",
            Some("alpha\nbeta\n".into()),
            "alpha\nINSERTED\nbeta\n",
        );
        let mut opts = DiffOptions::for_width(140);
        opts.layout = Some(DiffLayout::SideBySide);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Non-Rust language (Python) — snapshot ensures the highlighter
    /// dispatches on extension rather than hardcoding Rust.
    #[test]
    fn snap_python_syntax_highlighting() {
        let d = FileDiff::from_contents(
            "script.py",
            Some("def hello():\n    return 1\n".into()),
            "def hello():\n    return 42\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }

    /// Binary extension: should NOT be syntax-highlighted (the binary-skip
    /// path in `infer_language` returns None for these). Use a printable
    /// .png-named payload — we only care that the .png extension blocks
    /// syntax highlighting, not that the bytes are real PNG magic.
    #[test]
    fn snap_binary_extension_no_highlight() {
        let d = FileDiff::from_contents(
            "image.png",
            Some("placeholder bytes\n".to_string()),
            "different placeholder\n".to_string(),
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = Some(DiffLayout::Unified);
        let out = snap(&d, &opts);
        insta::assert_snapshot!(out);
    }
}
