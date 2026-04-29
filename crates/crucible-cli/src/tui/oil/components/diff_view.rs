//! Shared diff renderer used by tool-call scrollback and permission popups.
//!
//! Takes a `FileDiff` plus options and produces a `Node`. Picks side-by-side
//! when the width budget allows it, falls back to unified otherwise. The
//! caller is responsible for everything else (extracting the diff from a
//! tool call, deciding whether to render at all, etc).

use crate::formatting::SyntaxHighlighter;
use crate::tui::oil::theme;
use crate::tui::oil::utils::{truncate_to_chars, visible_width};
use crucible_core::types::acp::FileDiff;
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

/// Suppress diff body when either side exceeds this byte budget. The full file
/// content is still in the FileDiff, but materializing 1 MiB+ through
/// `TextDiff::from_lines` and the highlighter blows up frame time.
const MAX_DIFF_BYTES: usize = 1024 * 1024;

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
    pub language_hint: Option<String>,
}

impl DiffOptions {
    pub fn for_width(max_width: usize) -> Self {
        Self {
            max_width,
            max_lines: Some(200),
            context_lines: 3,
            collapsed: false,
            layout: None,
            language_hint: None,
        }
    }

    pub fn resolved_layout(&self) -> DiffLayout {
        self.layout.unwrap_or_else(|| {
            if self.max_width >= SIDE_BY_SIDE_MIN_WIDTH {
                DiffLayout::SideBySide
            } else {
                DiffLayout::Unified
            }
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

    let language = infer_language(&diff.path, opts.language_hint.as_deref());
    let highlighter = SyntaxHighlighter::new();

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

fn infer_language(path: &str, hint: Option<&str>) -> Option<String> {
    if let Some(h) = hint {
        return Some(h.to_string());
    }
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
    let mut nodes: Vec<Node> = Vec::new();

    let t = theme::active();
    let delete_color = t.resolve_color(t.colors.error);
    let insert_color = t.resolve_color(t.colors.success);
    let context_color = t.resolve_color(t.colors.text_dim);

    let mut in_hunk = false;
    let mut hunk_lines: Vec<Node> = Vec::new();
    let mut context_buffer: Vec<String> = Vec::new();
    let mut pending_context: Vec<String> = Vec::new();

    // Once we cross `max_lines`, stop emitting and just tally the remaining
    // changes for the footer. We prefer to cut between hunks (after a flush),
    // but a single massive hunk also flushes at a line boundary so file-create
    // diffs don't blow past the budget entirely.
    let mut truncated = false;
    let mut remaining_lines = 0usize;
    let mut remaining_added = 0usize;
    let mut remaining_removed = 0usize;

    let line_budget = max_width.saturating_sub(1);

    let push_context = |hunk_lines: &mut Vec<Node>, ctx_line: &str| {
        let trunc = truncate_to_chars(ctx_line, line_budget, true).into_owned();
        let mut spans = vec![styled(" ", Style::new().fg(context_color))];
        spans.extend(highlight_line(
            highlighter,
            language,
            &trunc,
            Some(context_color),
        ));
        hunk_lines.push(row(spans));
    };

    for change in diff.iter_all_changes() {
        let tag = change.tag();
        let line_content = change.value().trim_end_matches('\n');

        if truncated {
            match tag {
                ChangeTag::Insert => {
                    remaining_added += 1;
                    remaining_lines += 1;
                }
                ChangeTag::Delete => {
                    remaining_removed += 1;
                    remaining_lines += 1;
                }
                ChangeTag::Equal => {}
            }
            continue;
        }

        match tag {
            ChangeTag::Equal => {
                if in_hunk {
                    if context_lines > 0 && pending_context.len() < context_lines {
                        pending_context.push(line_content.to_string());
                    } else {
                        flush_hunk(&mut nodes, &mut hunk_lines);
                        in_hunk = false;
                        pending_context.clear();
                        if let Some(max) = max_lines {
                            if nodes.len() >= max {
                                truncated = true;
                            }
                        }
                    }
                }
                if context_lines > 0 {
                    context_buffer.push(line_content.to_string());
                    if context_buffer.len() > context_lines {
                        context_buffer.remove(0);
                    }
                }
            }
            ChangeTag::Delete | ChangeTag::Insert => {
                if !in_hunk {
                    in_hunk = true;
                    for ctx_line in &context_buffer {
                        push_context(&mut hunk_lines, ctx_line);
                    }
                } else {
                    let ctx: Vec<String> = pending_context.drain(..).collect();
                    for ctx_line in &ctx {
                        push_context(&mut hunk_lines, ctx_line);
                    }
                }

                let (prefix, color) = match tag {
                    ChangeTag::Delete => ("-", delete_color),
                    ChangeTag::Insert => ("+", insert_color),
                    ChangeTag::Equal => unreachable!(),
                };
                let trunc = truncate_to_chars(line_content, line_budget, true).into_owned();
                let mut spans = vec![styled(prefix, Style::new().fg(color))];
                spans.extend(highlight_line(highlighter, language, &trunc, Some(color)));
                hunk_lines.push(row(spans));

                // Mid-hunk budget check: a single giant hunk (e.g. file-create
                // with hundreds of inserts) would otherwise blow past max_lines
                // entirely. Flush at the line boundary; row-level rendering
                // means each line is self-contained, so this is a clean cut.
                if let Some(max) = max_lines {
                    if nodes.len() + hunk_lines.len() >= max {
                        flush_hunk(&mut nodes, &mut hunk_lines);
                        in_hunk = false;
                        pending_context.clear();
                        truncated = true;
                    }
                }
            }
        }
    }

    if in_hunk && !truncated {
        flush_hunk(&mut nodes, &mut hunk_lines);
    }

    if truncated && remaining_lines > 0 {
        let footer = styled(
            format!(
                "  … {} more lines (+{} -{})",
                remaining_lines, remaining_added, remaining_removed
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

fn flush_hunk(nodes: &mut Vec<Node>, hunk_lines: &mut Vec<Node>) {
    if hunk_lines.is_empty() {
        return;
    }
    nodes.append(hunk_lines);
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
}
