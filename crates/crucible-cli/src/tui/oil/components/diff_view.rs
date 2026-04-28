//! Shared diff renderer used by tool-call scrollback and permission popups.
//!
//! Takes a `FileDiff` plus options and produces a `Node`. Picks side-by-side
//! when the width budget allows it, falls back to unified otherwise. The
//! caller is responsible for everything else (extracting the diff from a
//! tool call, deciding whether to render at all, etc).

use crate::tui::oil::diff::{count_changes, diff_to_node_width};
use crate::tui::oil::theme;
use crate::tui::oil::utils::truncate_to_chars;
use crucible_core::types::acp::FileDiff;
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;
use similar::{ChangeTag, TextDiff};

pub const SIDE_BY_SIDE_MIN_WIDTH: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Auto,
    Unified,
    SideBySide,
}

/// Layout after `Auto` has been resolved against `max_width`.
///
/// Returning this from `resolved_layout()` lets the renderer match
/// exhaustively without an `unreachable!` arm for `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedLayout {
    Unified,
    SideBySide,
}

#[derive(Debug, Clone)]
pub struct DiffOptions {
    pub max_width: usize,
    pub max_lines: Option<usize>,
    pub context_lines: usize,
    pub collapsed: bool,
    pub layout: DiffLayout,
    pub language_hint: Option<String>,
}

impl DiffOptions {
    pub fn for_width(max_width: usize) -> Self {
        Self {
            max_width,
            max_lines: Some(200),
            context_lines: 3,
            collapsed: false,
            layout: DiffLayout::Auto,
            language_hint: None,
        }
    }

    pub fn resolved_layout(&self) -> ResolvedLayout {
        match self.layout {
            DiffLayout::Unified => ResolvedLayout::Unified,
            DiffLayout::SideBySide => ResolvedLayout::SideBySide,
            DiffLayout::Auto if self.max_width >= SIDE_BY_SIDE_MIN_WIDTH => {
                ResolvedLayout::SideBySide
            }
            DiffLayout::Auto => ResolvedLayout::Unified,
        }
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
    let counts = count_changes(old, new);
    let header = render_header(diff, Some(counts));

    if opts.collapsed {
        return header;
    }

    let body = match opts.resolved_layout() {
        ResolvedLayout::Unified => {
            diff_to_node_width(old, new, opts.context_lines, Some(opts.max_width))
        }
        ResolvedLayout::SideBySide => render_side_by_side(old, new, opts.max_width),
    };

    col([header, body])
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

    fn flush(
        rows: &mut Vec<PairedRow>,
        deletes: &mut Vec<String>,
        inserts: &mut Vec<String>,
    ) {
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

fn render_side_by_side(old: &str, new: &str, max_width: usize) -> Node {
    let t = theme::active();
    let pane_width = max_width.saturating_sub(3) / 2;
    // Defensive: Auto only picks SideBySide at max_width >= 120, but explicit
    // callers can request a narrow side-by-side. Fall back to unified rather
    // than render unreadable 3-column-wide panes.
    if pane_width < 10 {
        return diff_to_node_width(old, new, 3, Some(max_width));
    }
    let separator_style = Style::new().fg(t.resolve_color(t.colors.text_dim)).dim();
    let delete_style = Style::new().fg(t.resolve_color(t.colors.error));
    let insert_style = Style::new().fg(t.resolve_color(t.colors.success));
    let context_style = Style::new().fg(t.resolve_color(t.colors.text_dim));

    let cell = |content: Option<&(String, ChangeTag)>, width: usize| -> Node {
        match content {
            None => styled(" ".repeat(width), Style::new()),
            Some((text, tag)) => {
                let style = match tag {
                    ChangeTag::Delete => delete_style,
                    ChangeTag::Insert => insert_style,
                    ChangeTag::Equal => context_style,
                };
                let truncated = truncate_to_chars(text, width, true).into_owned();
                let padded = format!("{:width$}", truncated, width = width);
                styled(padded, style)
            }
        }
    };

    let mut rows = Vec::new();
    for pr in pair_changes(old, new) {
        rows.push(row([
            cell(pr.left.as_ref(), pane_width),
            styled(" │ ", separator_style),
            cell(pr.right.as_ref(), pane_width),
        ]));
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
        assert_eq!(opts.resolved_layout(), ResolvedLayout::Unified);
    }

    #[test]
    fn auto_layout_picks_side_by_side_at_threshold() {
        let opts = DiffOptions::for_width(120);
        assert_eq!(opts.resolved_layout(), ResolvedLayout::SideBySide);
    }

    #[test]
    fn explicit_layout_overrides_auto() {
        let mut opts = DiffOptions::for_width(200);
        opts.layout = DiffLayout::Unified;
        assert_eq!(opts.resolved_layout(), ResolvedLayout::Unified);
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
        assert!(!out.contains("CHANGED"), "collapsed must not show body: {out:?}");
    }

    #[test]
    fn unified_renders_added_and_removed_lines() {
        let d = FileDiff::from_contents(
            "x.rs",
            Some("alpha\nbeta\ngamma\n".into()),
            "alpha\nbeta-CHANGED\ngamma\n",
        );
        let mut opts = DiffOptions::for_width(80);
        opts.layout = DiffLayout::Unified;
        let out = render(&d, &opts);
        assert!(out.contains("-beta"), "got: {out:?}");
        assert!(out.contains("+beta-CHANGED"), "got: {out:?}");
    }

    #[test]
    fn header_shows_line_counts_when_expanded() {
        let d = FileDiff::from_contents("x.rs", Some("a\nb\n".into()), "a\nB1\nB2\n");
        let mut opts = DiffOptions::for_width(80);
        opts.layout = DiffLayout::Unified;
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
        opts.layout = DiffLayout::SideBySide;
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
        opts.layout = DiffLayout::SideBySide;
        let out = render(&d, &opts);
        let line_with_new = out.lines().find(|l| l.contains("NEW")).unwrap_or("");
        assert!(
            line_with_new.contains('│'),
            "expected pane separator on insert row: {line_with_new:?}"
        );
    }
}
