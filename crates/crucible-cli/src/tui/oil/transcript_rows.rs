//! The rows of finished transcript nodes, kept between frames.
//!
//! The native view prints the whole transcript on every frame, and the
//! terminal keeps the rows that scroll off the top. To lay every node out
//! again on every frame costs about 160 ms at 5,000 rows: the frame parses
//! the markdown of every answer and gives the whole transcript to taffy. A
//! finished node gives the same rows until an input of its render changes.
//! So this cache keeps those rows, and a frame lays out only the nodes that
//! can still change.
//!
//! Both modes read this one cache. The native view puts the kept rows back
//! in its frame tree ([`TranscriptRows::frame_nodes`]). The full-screen view
//! places the rows itself, and it also needs where the text of each row is
//! ([`TranscriptRows::frame_rows`]).

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::containers::ContainerList;
use crate::tui::oil::theme;
use crucible_oil::cell_grid::RowText;
use crucible_oil::node::{rows, Node};
use crucible_oil::render::{render_to_text_rows, TextRows};
use std::sync::Arc;

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

/// The rows of one transcript node at one width.
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeRows {
    /// Each row as `render_to_rows` gives it.
    pub(crate) rows: Arc<[String]>,
    /// Where the source text of each row is, for a full-screen selection.
    pub(crate) text: Arc<[RowText]>,
}

/// The rows of one transcript node in a full-screen frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameRows {
    /// No rows for a node that renders nothing.
    pub(crate) rows: NodeRows,
    /// Whether the node is finished. A node that can still change has no
    /// final rows yet.
    pub(crate) finished: bool,
}

#[derive(Debug, Default)]
pub(crate) struct TranscriptRows {
    /// The kept rows of each node, by position. `None` rows for a node that
    /// renders nothing.
    kept: Vec<Option<(RowsKey, Option<NodeRows>)>>,
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
        self.frame(
            list,
            ctx,
            // An empty node takes no place in the column; kept rows would
            // take one, with a gap around it.
            |kept| kept.map_or(Node::Empty, |kept| rows(Arc::clone(&kept.rows))),
            |tree| tree,
        )
    }

    /// The rows of each transcript node for a full-screen frame at `ctx`.
    ///
    /// These are the rows of [`TranscriptRows::frame_nodes`]: a finished
    /// node gives its kept rows, and a node that can still change is laid
    /// out to rows that are not kept.
    pub(crate) fn frame_rows(
        &mut self,
        list: &ContainerList,
        ctx: &ViewContext<'_>,
    ) -> Vec<FrameRows> {
        let width = ctx.terminal_size.0;
        self.frame(
            list,
            ctx,
            |kept| FrameRows {
                rows: kept.cloned().unwrap_or_default(),
                finished: true,
            },
            |tree| FrameRows {
                rows: lay_out(&tree, width).unwrap_or_default(),
                finished: false,
            },
        )
    }

    /// One `T` for each node: `from_kept` for a finished node, from its
    /// kept rows, and `live` for a node that can still change, from its tree.
    fn frame<T>(
        &mut self,
        list: &ContainerList,
        ctx: &ViewContext<'_>,
        from_kept: impl Fn(Option<&NodeRows>) -> T,
        live: impl Fn(Node) -> T,
    ) -> Vec<T> {
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
                    return live(node.render(prev, ctx));
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
                        return from_kept(rows.as_ref());
                    }
                }
                self.layouts += 1;
                let laid_out = lay_out(&node.render(prev, ctx), ctx.terminal_size.0);
                let out = from_kept(laid_out.as_ref());
                *kept = Some((key, laid_out));
                out
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

/// The rows of `tree` at `width`, or `None` for a node that renders nothing.
fn lay_out(tree: &Node, width: u16) -> Option<NodeRows> {
    match tree {
        Node::Empty => None,
        tree => {
            let TextRows { rows, text } = render_to_text_rows(tree, width);
            Some(NodeRows {
                rows: rows.into(),
                text: text.into(),
            })
        }
    }
}
