//! The transcript as rows at one width, with a row cache per node.
//!
//! The native view lays the whole transcript out with taffy on every frame.
//! Here each `ChatNode` is laid out on its own, and a finished node keeps its
//! rows until the width or its content changes. A frame then lays out only
//! the nodes that still change, which is usually the one that streams.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::containers::ChatNode;
use crucible_oil::cell_grid::{CellGrid, RowJoin};
use crucible_oil::render::{render_tree_to_grid, NATURAL_HEIGHT};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A row of a laid-out node: its cells, and how it continues the row above.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptRow {
    /// The row as styled text, trailing padding dropped.
    pub ansi: String,
    /// How the row continues the row above, when a wrap split them.
    pub join: Option<RowJoin>,
}

impl TranscriptRow {
    fn from_grid(grid: &CellGrid, y: usize) -> Self {
        let mut ansi = grid.row_ansi(y);
        // `row_ansi` ends a short row with an erase for the screen diff; a
        // stored row is blitted into a fresh grid, so it needs none.
        if let Some(stripped) = ansi.strip_suffix("\x1b[K") {
            ansi.truncate(stripped.len());
        }
        Self {
            ansi,
            join: grid.join(y).cloned(),
        }
    }

    pub fn as_ref(&self) -> super::selection::RowRef<'_> {
        super::selection::RowRef {
            ansi: &self.ansi,
            join: self.join.as_ref(),
        }
    }
}

/// What a cached node was laid out for. A finished node whose key is the
/// same keeps its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EntryKey {
    width: u16,
    fingerprint: u64,
}

#[derive(Debug, Default)]
struct Entry {
    /// `None` for a node that still changes: it is laid out every frame.
    key: Option<EntryKey>,
    rows: Vec<TranscriptRow>,
}

/// A position in the transcript that survives a reflow: a row inside a node,
/// as a share of that node's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    entry: usize,
    row: usize,
    rows: usize,
}

/// How much work one [`Transcript::sync`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncStats {
    pub laid_out: usize,
    pub reused: usize,
}

#[derive(Debug, Default)]
pub struct Transcript {
    width: u16,
    entries: Vec<Entry>,
    /// The first row of each entry, separator rows included.
    starts: Vec<usize>,
    total: usize,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    /// Rows in the transcript, blank separator rows included.
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Forget every cached row, as after a theme change.
    pub fn invalidate(&mut self) {
        for entry in &mut self.entries {
            entry.key = None;
        }
    }

    /// Lay out the nodes that changed since the last call. `ctx` must come
    /// from `OilChatApp::frame_context`; its width is the transcript width.
    pub fn sync(&mut self, nodes: &[ChatNode], ctx: &ViewContext<'_>) -> SyncStats {
        let width = ctx.terminal_size.0;
        self.width = width;
        self.entries.truncate(nodes.len());
        let mut stats = SyncStats::default();
        for (i, node) in nodes.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| &nodes[p]);
            let key = node.is_complete().then(|| EntryKey {
                width,
                fingerprint: fingerprint(node, prev, ctx),
            });
            if key.is_some() && self.entries.get(i).is_some_and(|e| e.key == key) {
                stats.reused += 1;
                continue;
            }
            let rendered = render_tree_to_grid(&node.render(prev, ctx), width, NATURAL_HEIGHT);
            let rows = (0..rendered.grid.height())
                .map(|y| TranscriptRow::from_grid(&rendered.grid, y))
                .collect();
            let entry = Entry { key, rows };
            if i < self.entries.len() {
                self.entries[i] = entry;
            } else {
                self.entries.push(entry);
            }
            stats.laid_out += 1;
        }
        self.reindex();
        stats
    }

    fn reindex(&mut self) {
        self.starts.clear();
        let mut row = 0;
        for entry in &self.entries {
            // One blank row between nodes, as the native view's `gap(1)`.
            if !entry.rows.is_empty() && row > 0 {
                row += 1;
            }
            self.starts.push(row);
            row += entry.rows.len();
        }
        self.total = row;
    }

    /// Row `index`, or `None` for a separator row or a row past the end.
    pub fn row(&self, index: usize) -> Option<&TranscriptRow> {
        let (entry, offset) = self.locate(index)?;
        self.entries[entry].rows.get(offset)
    }

    /// The entry that holds row `index`, and the offset of the row in it.
    /// A separator row belongs to the entry below it, at a negative offset,
    /// so it returns `None`.
    fn locate(&self, index: usize) -> Option<(usize, usize)> {
        if index >= self.total {
            return None;
        }
        let entry = self.starts.partition_point(|&start| start <= index) - 1;
        Some((entry, index - self.starts[entry]))
    }

    /// The anchor for row `index`, to find the same text after a reflow.
    pub fn anchor_at(&self, index: usize) -> Option<Anchor> {
        let (entry, row) = self.locate(index)?;
        Some(Anchor {
            entry,
            row,
            rows: self.entries[entry].rows.len(),
        })
    }

    /// The row where `anchor` is now. The row keeps its share of the node's
    /// rows, so a node that wraps to twice the rows maps row 3 to row 6.
    pub fn resolve(&self, anchor: Anchor) -> usize {
        let Some(&start) = self.starts.get(anchor.entry) else {
            return self.total;
        };
        let rows = self.entries[anchor.entry].rows.len();
        let row = (anchor.row * rows).checked_div(anchor.rows).unwrap_or(0);
        start + row.min(rows.saturating_sub(1))
    }

    /// Every row, as the styled text that the exit dump prints. Separator
    /// rows are empty strings.
    pub fn styled_rows(&self, entries: std::ops::Range<usize>) -> Vec<String> {
        let mut out = Vec::new();
        let leading_gap = entries.start > 0;
        for entry in &self.entries[entries] {
            if entry.rows.is_empty() {
                continue;
            }
            if leading_gap || !out.is_empty() {
                out.push(String::new());
            }
            out.extend(entry.rows.iter().map(|row| row.ansi.clone()));
        }
        out
    }

    /// Entries that are cached as finished, counted from the start. The
    /// exit dump prints only these, because a node that still changes has
    /// no final rows.
    pub fn finished_prefix(&self) -> usize {
        self.entries
            .iter()
            .position(|e| e.key.is_none())
            .unwrap_or(self.entries.len())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// What a finished node's rows depend on, apart from the width.
///
/// A finished node rarely changes, but it can: a tool group gains a tool,
/// a backgrounded tool finishes. The fingerprint covers those fields and
/// the flags that change what a node draws. It is not a hash of the whole
/// node, because hashing every text on every frame costs more than it saves.
fn fingerprint(node: &ChatNode, prev: Option<&ChatNode>, ctx: &ViewContext<'_>) -> u64 {
    let mut h = DefaultHasher::new();
    std::mem::discriminant(node).hash(&mut h);
    prev.map(std::mem::discriminant).hash(&mut h);
    (ctx.show_thinking, ctx.show_diffs).hash(&mut h);
    match node {
        ChatNode::UserMessage { text } | ChatNode::SystemMessage { text } => text.hash(&mut h),
        ChatNode::AssistantResponse {
            text,
            thinking,
            complete,
        } => {
            text.len().hash(&mut h);
            thinking.len().hash(&mut h);
            complete.hash(&mut h);
        }
        ChatNode::ToolGroup { tools } => {
            for tool in tools {
                tool.id.hash(&mut h);
                tool.complete.hash(&mut h);
                tool.backgrounded.hash(&mut h);
                tool.output_total_bytes.hash(&mut h);
                tool.error.is_some().hash(&mut h);
            }
        }
        ChatNode::BackgroundToolFinished { tool, .. } => tool.id.hash(&mut h),
        ChatNode::SubagentTask { .. } | ChatNode::ShellExecution { .. } => {}
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::fullscreen::fixtures;
    use crate::tui::oil::theme;
    use crucible_oil::ansi::strip_ansi;
    use crucible_oil::focus::FocusContext;

    fn sync(
        transcript: &mut Transcript,
        app: &crate::tui::oil::OilChatApp,
        width: u16,
    ) -> SyncStats {
        let focus = FocusContext::new();
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, 40));
        let ctx = app.frame_context(&ctx);
        transcript.sync(app.container_list().nodes(), &ctx)
    }

    fn text_of(transcript: &Transcript, index: usize) -> String {
        transcript
            .row(index)
            .map(|row| strip_ansi(&row.ansi))
            .unwrap_or_default()
    }

    #[test]
    fn finished_nodes_are_laid_out_once() {
        let app = fixtures::app_with_exchanges(3);
        let mut transcript = Transcript::new();
        let first = sync(&mut transcript, &app, 100);
        assert_eq!(first.laid_out, 6);

        let second = sync(&mut transcript, &app, 100);
        assert_eq!(
            second,
            SyncStats {
                laid_out: 0,
                reused: 6
            }
        );
    }

    #[test]
    fn a_streaming_node_is_laid_out_every_frame() {
        let mut app = fixtures::app_with_exchanges(2);
        app.on_message(crate::tui::oil::ChatAppMsg::UserMessage("next".into()));
        app.on_message(crate::tui::oil::ChatAppMsg::TextDelta("partial".into()));
        let mut transcript = Transcript::new();
        sync(&mut transcript, &app, 100);

        let stats = sync(&mut transcript, &app, 100);
        assert_eq!(stats.laid_out, 1, "only the streaming answer");
        assert_eq!(stats.reused, 5);
    }

    #[test]
    fn a_width_change_lays_every_node_out_again() {
        let app = fixtures::app_with_exchanges(2);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &app, 100);
        let narrow = sync(&mut transcript, &app, 60);
        assert_eq!(narrow.laid_out, 4);
    }

    #[test]
    fn nodes_are_separated_by_one_blank_row() {
        let mut app = crate::tui::oil::OilChatApp::default();
        app.add_system_message("first".into());
        app.add_system_message("second".into());
        let mut transcript = Transcript::new();
        sync(&mut transcript, &app, 40);

        assert_eq!(transcript.len(), 3);
        assert!(text_of(&transcript, 0).contains("first"));
        assert!(transcript.row(1).is_none(), "a separator row");
        assert!(text_of(&transcript, 2).contains("second"));
    }

    #[test]
    fn a_node_taller_than_the_layout_cap_keeps_every_row() {
        // `NATURAL_HEIGHT` gives taffy a finite ceiling; a long answer must
        // still keep all of its rows.
        let mut app = crate::tui::oil::OilChatApp::default();
        let long: String = (0..800).map(|i| format!("line {i}\n\n")).collect();
        app.on_message(crate::tui::oil::ChatAppMsg::TextDelta(long));
        app.on_message(crate::tui::oil::ChatAppMsg::StreamComplete);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &app, 80);

        let last = (0..transcript.len())
            .rev()
            .map(|i| text_of(&transcript, i))
            .find(|t| !t.trim().is_empty())
            .unwrap();
        assert!(last.contains("line 799"), "last row: {last:?}");
    }

    #[test]
    fn an_anchor_finds_the_same_node_after_a_reflow() {
        let app = fixtures::app_with_exchanges(4);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &app, 160);
        // The first row of the third answer.
        let start = transcript.starts[5];
        let before = text_of(&transcript, start);
        let anchor = transcript.anchor_at(start).unwrap();

        sync(&mut transcript, &app, 70);
        let after = text_of(&transcript, transcript.resolve(anchor));
        assert_eq!(before.trim(), after.trim());
    }
}
