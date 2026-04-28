//! Shared diff renderer used by tool-call scrollback and permission popups.
//!
//! Takes a `FileDiff` plus options and produces a `Node`. Picks side-by-side
//! when the width budget allows it, falls back to unified otherwise. The
//! caller is responsible for everything else (extracting the diff from a
//! tool call, deciding whether to render at all, etc).

use crucible_core::types::acp::FileDiff;
use crucible_oil::node::Node;

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

pub fn render_diff(_diff: &FileDiff, _opts: &DiffOptions) -> Node {
    Node::Empty
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
