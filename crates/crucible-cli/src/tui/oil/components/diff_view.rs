//! Shared diff renderer used by tool-call scrollback and permission popups.
//!
//! Takes a `FileDiff` plus options and produces a `Node`. Picks side-by-side
//! when the width budget allows it, falls back to unified otherwise. The
//! caller is responsible for everything else (extracting the diff from a
//! tool call, deciding whether to render at all, etc).

use crate::tui::oil::theme;
use crucible_core::types::acp::FileDiff;
use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::Style;

pub const SIDE_BY_SIDE_MIN_WIDTH: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Auto,
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

    pub fn resolved_layout(&self) -> DiffLayout {
        match self.layout {
            DiffLayout::Auto if self.max_width >= SIDE_BY_SIDE_MIN_WIDTH => DiffLayout::SideBySide,
            DiffLayout::Auto => DiffLayout::Unified,
            other => other,
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
    let header = render_header(diff, None);
    if opts.collapsed {
        return header;
    }
    col([header])
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
        opts.layout = DiffLayout::Unified;
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
        assert!(!out.contains("CHANGED"), "collapsed must not show body: {out:?}");
    }
}
