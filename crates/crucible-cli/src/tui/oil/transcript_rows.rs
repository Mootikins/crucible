//! The rows of finished transcript nodes, kept between frames.
//!
//! The native view prints the whole transcript on every frame, and the
//! terminal keeps the rows that scroll off the top. To lay every node out
//! again on every frame costs about 160 ms at 5,000 rows: the frame parses
//! the markdown of every answer and gives the whole transcript to taffy. A
//! finished node gives the same rows until an input of its render changes.
//! So this cache keeps those rows, and a frame lays out only the nodes that
//! can still change.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::containers::ContainerList;
use crate::tui::oil::theme;
use crucible_oil::node::{rows, Node};
use crucible_oil::render::render_to_rows;

/// Everything that the rows of a finished node depend on.
///
/// `ChatNode::render` reads the node, the kind of the node above it and the
/// frame context. The revision covers the first two: nodes are only
/// appended, and a node never changes its kind, so a node with the same
/// revision has the same node above it. A finished node does not read the
/// frame clock or the spinner frame, so neither is here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowsKey {
    /// Changes on each change to the node. See [`ContainerList::revisions`].
    revision: u64,
    width: u16,
    /// Changes when the theme, the highlight groups, the geometry or the
    /// bars change. The frame's theme is the installed one, and components
    /// also read the installed slots directly.
    style_generation: u64,
    show_thinking: bool,
    show_diffs: bool,
}

#[derive(Debug, Default)]
pub(crate) struct TranscriptRows {
    /// The kept rows of each node, by position: a rows node, or
    /// `Node::Empty` for a node that renders nothing.
    kept: Vec<Option<(RowsKey, Node)>>,
    /// How many node layouts the cache did, over its whole life.
    layouts: u64,
}

impl TranscriptRows {
    /// The transcript nodes for a frame at `ctx`.
    ///
    /// A finished node gives its kept rows, and it is laid out again only
    /// when its key changes. A node that can still change is laid out from
    /// source and is not kept. `ctx.terminal_size.0` must be the width that
    /// the frame is laid out at, because the kept rows have that width.
    pub(crate) fn frame_nodes(&mut self, list: &ContainerList, ctx: &ViewContext<'_>) -> Vec<Node> {
        let nodes = list.nodes();
        self.kept.resize(nodes.len(), None);
        let style_generation = theme::slot::generation();
        let before = self.layouts;

        let frame = nodes
            .iter()
            .zip(list.revisions())
            .zip(self.kept.iter_mut())
            .enumerate()
            .map(|(i, ((node, &revision), kept))| {
                let prev = i.checked_sub(1).map(|p| &nodes[p]);
                if !node.is_complete() {
                    *kept = None;
                    return node.render(prev, ctx);
                }
                let key = RowsKey {
                    revision,
                    width: ctx.terminal_size.0,
                    style_generation,
                    show_thinking: ctx.show_thinking,
                    show_diffs: ctx.show_diffs,
                };
                if let Some((kept_key, rows)) = kept {
                    if *kept_key == key {
                        return rows.clone();
                    }
                }
                self.layouts += 1;
                // An empty node takes no place in the column; kept rows
                // would take one, with a gap around it.
                let laid_out = match node.render(prev, ctx) {
                    Node::Empty => Node::Empty,
                    tree => rows(render_to_rows(&tree, ctx.terminal_size.0)),
                };
                *kept = Some((key, laid_out.clone()));
                laid_out
            })
            .collect();

        tracing::trace!(
            laid_out = self.layouts - before,
            nodes = nodes.len(),
            "transcript rows for a frame"
        );
        frame
    }

    /// How many node layouts the cache did, over its whole life.
    #[cfg(test)]
    pub(crate) fn layouts(&self) -> u64 {
        self.layouts
    }
}
