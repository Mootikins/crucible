//! The transcript as rows at one width.
//!
//! The rows come from the app's kept rows (`transcript_rows`), the cache
//! that the native view uses too: a finished node keeps its rows, and a
//! frame lays out only the nodes that can still change. This module does
//! not keep rows. It numbers the rows of the frame, finds a row, and maps a
//! place in the text across a reflow.
//!
//! After a width change, most nodes have no rows at the new width. Such a
//! node has an estimate of its height, so the rows are still numbered, and
//! the view lays the node out when it comes on screen, when a copy or a dump
//! reaches it, or at idle time ([`Transcript::lay_out`]). A layout changes
//! the height of the node, so a place in the transcript is a node and a row
//! in it ([`Anchor`]), not a row number.

use super::selection::RowRef;
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::theme;
use crate::tui::oil::transcript_rows::{FrameRows, Slot};
use crucible_oil::focus::FocusContext;
use std::ops::Range;

/// A position in the transcript that survives a reflow and the layout of
/// the nodes above it: a row inside a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    entry: usize,
    row: usize,
    /// The rows of the node and the width when the anchor was taken. At
    /// another width, the row keeps its share of the node's rows.
    rows: usize,
    width: u16,
}

impl Anchor {
    /// The entry of the node that holds the anchor.
    pub fn entry(&self) -> usize {
        self.entry
    }
}

/// The row numbers of the entries at one moment, to find a row again
/// after a layout moved it. See [`Transcript::relocate`].
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// The first row of each entry, separator rows included.
    starts: Vec<usize>,
    total: usize,
}

impl Index {
    /// The entry that holds row `index`, and the offset of the row in it.
    /// A separator row is at the offset one past the last row of the entry
    /// above it.
    fn locate(&self, index: usize) -> Option<(usize, usize)> {
        if index >= self.total {
            return None;
        }
        let entry = self.starts.partition_point(|&start| start <= index) - 1;
        Some((entry, index - self.starts[entry]))
    }
}

#[derive(Debug, Default)]
pub struct Transcript {
    /// The terminal size of the last frame. Layouts outside a frame use it.
    size: (u16, u16),
    /// Each node, in the order of the nodes.
    entries: Vec<Slot>,
    index: Index,
    /// Entries that have only an estimate.
    estimated: usize,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn width(&self) -> u16 {
        self.size.0
    }

    /// Rows in the transcript, blank separator rows included. Entries
    /// without rows count with their estimate.
    pub fn len(&self) -> usize {
        self.index.total
    }

    pub fn is_empty(&self) -> bool {
        self.index.total == 0
    }

    /// Entries that have only an estimate.
    pub fn estimated(&self) -> usize {
        self.estimated
    }

    /// Take the entries of `app`'s transcript for a frame. `ctx` must come
    /// from `OilChatApp::frame_context`; its width is the transcript width.
    /// No finished node is laid out here.
    pub fn sync(&mut self, app: &mut OilChatApp, ctx: &ViewContext<'_>) {
        self.size = ctx.terminal_size;
        self.entries = app.transcript_frame_slots(ctx);
        self.reindex();
    }

    /// Lay out `entries` that have only an estimate, at the size of the
    /// last frame. Entries that have rows stay as they are.
    pub fn lay_out(&mut self, app: &mut OilChatApp, entries: &[usize]) {
        let focus = FocusContext::new();
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), self.size);
        // The rows of a transcript node do not read the focus: the kept
        // rows have no focus in their key, and a frame reuses them. So a
        // new focus context gives the rows that a frame gives.
        let ctx = app.frame_context(&ctx);
        for &entry in entries {
            if !matches!(self.entries.get(entry), Some(Slot::Estimate(_))) {
                continue;
            }
            // A node gone since the last frame shows nothing until the next
            // frame takes the entries again.
            let rows = app.transcript_node_rows(entry, &ctx).unwrap_or(FrameRows {
                finished: true,
                ..FrameRows::default()
            });
            self.entries[entry] = Slot::Rows(rows);
        }
        self.reindex();
    }

    fn reindex(&mut self) {
        let index = &mut self.index;
        index.starts.clear();
        self.estimated = 0;
        let mut row = 0;
        for entry in &self.entries {
            let height = height(entry);
            self.estimated += usize::from(matches!(entry, Slot::Estimate(_)));
            // One blank row between nodes, as the native view's `gap(1)`.
            if height > 0 && row > 0 {
                row += 1;
            }
            index.starts.push(row);
            row += height;
        }
        index.total = row;
    }

    /// The row numbers now. Keep them before a layout to find a row again
    /// with [`Transcript::relocate`].
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// The row where row `row` of `before` is now. A layout changes the
    /// height of its entries and moves the rows below them.
    pub fn relocate(&self, before: &Index, row: usize) -> usize {
        match before.locate(row) {
            Some((entry, offset)) => match self.index.starts.get(entry) {
                Some(&start) => start + offset.min(self.entry_height(entry)),
                None => self.index.total,
            },
            None => (row + self.index.total).saturating_sub(before.total),
        }
    }

    fn entry_height(&self, entry: usize) -> usize {
        self.entries.get(entry).map_or(0, height)
    }

    /// Row `index`, or `None` for a separator row, a row past the end or a
    /// row of an entry that has only an estimate.
    pub fn row(&self, index: usize) -> Option<RowRef<'_>> {
        let (entry, offset) = self.index.locate(index)?;
        let Slot::Rows(FrameRows { rows, .. }) = &self.entries[entry] else {
            return None;
        };
        Some(RowRef {
            ansi: rows.rows.get(offset)?,
            text: rows.text.get(offset),
        })
    }

    /// The entries that hold a row in `rows`.
    pub fn entries_in(&self, rows: Range<usize>) -> Range<usize> {
        let starts = &self.index.starts;
        if rows.is_empty() || rows.start >= self.index.total {
            return 0..0;
        }
        let first = starts.partition_point(|&start| start <= rows.start) - 1;
        let end = starts.partition_point(|&start| start < rows.end);
        first..end
    }

    /// The entries that hold a row in `rows` and have only an estimate.
    pub fn estimated_in(&self, rows: Range<usize>) -> Vec<usize> {
        self.entries_in(rows)
            .filter(|&entry| matches!(self.entries[entry], Slot::Estimate(_)))
            .collect()
    }

    /// The entry with only an estimate that is nearest to `entry`, the one
    /// above first at the same distance: a reader scrolls up more often.
    pub fn nearest_estimated(&self, entry: usize) -> Option<usize> {
        if self.estimated == 0 {
            return None;
        }
        let is_estimate = |e: usize| matches!(self.entries.get(e), Some(Slot::Estimate(_)));
        (0..self.entries.len()).find_map(|distance| {
            let above = entry.checked_sub(distance).filter(|&e| is_estimate(e));
            above.or_else(|| Some(entry + distance).filter(|&e| is_estimate(e)))
        })
    }

    /// The anchor for row `index`, to find the same text after a reflow.
    pub fn anchor_at(&self, index: usize) -> Option<Anchor> {
        let (entry, row) = self.index.locate(index)?;
        Some(Anchor {
            entry,
            row,
            rows: self.entry_height(entry),
            width: self.width(),
        })
    }

    /// The row where `anchor` is now. At the width of the anchor, it is the
    /// same row of the node, even when the node grew. At another width, the
    /// row keeps its share of the node's rows, so a node that wraps to
    /// twice the rows maps row 3 to row 6.
    pub fn resolve(&self, anchor: Anchor) -> usize {
        let Some(&start) = self.index.starts.get(anchor.entry) else {
            return self.index.total;
        };
        let rows = self.entry_height(anchor.entry);
        let row = if anchor.width == self.width() {
            anchor.row
        } else {
            (anchor.row * rows).checked_div(anchor.rows).unwrap_or(0)
        };
        start + row.min(rows)
    }

    /// The rows of `entries`, as the styled text that the exit dump prints.
    /// Separator rows are empty strings. Lay the entries out first: an
    /// entry that has only an estimate has no rows to print.
    pub fn styled_rows(&self, entries: Range<usize>) -> Vec<String> {
        let mut out = Vec::new();
        let leading_gap = entries.start > 0;
        for entry in &self.entries[entries] {
            let Slot::Rows(FrameRows { rows, .. }) = entry else {
                continue;
            };
            if rows.rows.is_empty() {
                continue;
            }
            if leading_gap || !out.is_empty() {
                out.push(String::new());
            }
            out.extend(rows.rows.iter().cloned());
        }
        out
    }

    /// Entries that are finished, counted from the start. The exit dump
    /// prints only these, because a node that still changes has no final
    /// rows. An entry with an estimate is finished: only a finished node
    /// can wait for its layout.
    pub fn finished_prefix(&self) -> usize {
        self.entries
            .iter()
            .position(|e| {
                matches!(
                    e,
                    Slot::Rows(FrameRows {
                        finished: false,
                        ..
                    })
                )
            })
            .unwrap_or(self.entries.len())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// The rows of an entry, or its estimate.
fn height(entry: &Slot) -> usize {
    match entry {
        Slot::Rows(FrameRows { rows, .. }) => rows.rows.len(),
        Slot::Estimate(rows) => *rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::fullscreen::fixtures;
    use crate::tui::oil::theme;
    use crucible_oil::ansi::strip_ansi;
    use crucible_oil::focus::FocusContext;

    /// Take the entries at `width`, without a layout of a finished node.
    fn sync_only(transcript: &mut Transcript, app: &mut OilChatApp, width: u16) {
        let focus = FocusContext::new();
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, 40));
        let ctx = app.frame_context(&ctx);
        transcript.sync(app, &ctx);
    }

    /// Take the entries at `width`, and lay out every one.
    fn sync(transcript: &mut Transcript, app: &mut OilChatApp, width: u16) {
        sync_only(transcript, app, width);
        let all: Vec<usize> = (0..transcript.entry_count()).collect();
        transcript.lay_out(app, &all);
    }

    fn text_of(transcript: &Transcript, index: usize) -> String {
        transcript
            .row(index)
            .map(|row| strip_ansi(row.ansi))
            .unwrap_or_default()
    }

    /// The full-screen view keeps no rows of its own: it reads the kept rows
    /// of the native view, so a frame lays out no finished node again.
    #[test]
    fn a_frame_reuses_the_kept_rows_of_the_app() {
        let mut app = fixtures::app_with_exchanges(3);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 100);
        let first = app.transcript_layouts();
        assert_eq!(first, 6, "each finished node is laid out once");

        sync_only(&mut transcript, &mut app, 100);
        assert_eq!(app.transcript_layouts(), first);
        assert_eq!(transcript.estimated(), 0, "every node has its kept rows");
    }

    /// A width change lays out nothing by itself. Each node gets an
    /// estimate from its rows at the old width, until its layout.
    #[test]
    fn a_width_change_gives_estimates_until_a_layout() {
        let mut app = fixtures::app_with_exchanges(3);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 100);
        let first = app.transcript_layouts();
        let rows_at_100 = transcript.entry_height(1);

        sync_only(&mut transcript, &mut app, 50);
        assert_eq!(app.transcript_layouts(), first, "no layout");
        assert_eq!(transcript.estimated(), 6);
        assert_eq!(transcript.entry_height(1), rows_at_100 * 2);
        assert!(transcript.row(transcript.index.starts[1]).is_none());

        transcript.lay_out(&mut app, &[1]);
        assert_eq!(app.transcript_layouts(), first + 1);
        assert_eq!(transcript.estimated(), 5);
        assert!(transcript.row(transcript.index.starts[1]).is_some());
    }

    /// A layout moves the rows below the node; `relocate` finds a row
    /// again, and an anchor needs no help.
    #[test]
    fn a_layout_above_a_row_moves_it_and_relocate_finds_it() {
        let mut app = fixtures::app_with_exchanges(3);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 100);
        sync_only(&mut transcript, &mut app, 50);
        let last = transcript.entry_count() - 1;
        transcript.lay_out(&mut app, &[last]);
        let row = transcript.index.starts[last] + 2;
        let text = text_of(&transcript, row);
        let anchor = transcript.anchor_at(row).unwrap();
        let before = transcript.index().clone();

        transcript.lay_out(&mut app, &[0, 1, 2]);
        let moved = transcript.relocate(&before, row);
        assert_ne!(moved, row, "the estimates were not exact");
        assert_eq!(text_of(&transcript, moved), text);
        assert_eq!(transcript.resolve(anchor), moved);
    }

    #[test]
    fn a_streaming_node_is_not_finished() {
        let mut app = fixtures::app_with_exchanges(2);
        app.on_message(crate::tui::oil::ChatAppMsg::UserMessage("next".into()));
        app.on_message(crate::tui::oil::ChatAppMsg::TextDelta("partial".into()));
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 100);
        assert_eq!(transcript.entry_count(), 6);
        assert_eq!(transcript.finished_prefix(), 5);
    }

    #[test]
    fn nodes_are_separated_by_one_blank_row() {
        let mut app = OilChatApp::default();
        app.add_system_message("first".into());
        app.add_system_message("second".into());
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 40);

        let rows: Vec<String> = (0..transcript.len())
            .map(|i| text_of(&transcript, i))
            .collect();
        let first = rows.iter().position(|r| r.contains("first")).unwrap();
        let second = rows.iter().position(|r| r.contains("second")).unwrap();
        assert!(
            transcript.row(second - 1).is_none(),
            "a separator row: {rows:#?}"
        );
        assert!(first < second - 1);
    }

    #[test]
    fn a_node_taller_than_the_layout_cap_keeps_every_row() {
        // `NATURAL_HEIGHT` gives taffy a finite ceiling; a long answer must
        // still keep all of its rows.
        let mut app = OilChatApp::default();
        let long: String = (0..800).map(|i| format!("line {i}\n\n")).collect();
        app.on_message(crate::tui::oil::ChatAppMsg::TextDelta(long));
        app.on_message(crate::tui::oil::ChatAppMsg::StreamComplete);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 80);

        let last = (0..transcript.len())
            .rev()
            .map(|i| text_of(&transcript, i))
            .find(|t| !t.trim().is_empty())
            .unwrap();
        assert!(last.contains("line 799"), "last row: {last:?}");
    }

    #[test]
    fn an_anchor_finds_the_same_node_after_a_reflow() {
        let mut app = fixtures::app_with_exchanges(4);
        let mut transcript = Transcript::new();
        sync(&mut transcript, &mut app, 160);
        // The first text row of the third answer.
        let start = (transcript.index.starts[5]..transcript.len())
            .find(|&i| !text_of(&transcript, i).trim().is_empty())
            .unwrap();
        let before = text_of(&transcript, start);
        let anchor = transcript.anchor_at(start).unwrap();

        sync(&mut transcript, &mut app, 70);
        let after = text_of(&transcript, transcript.resolve(anchor));
        assert_eq!(before.trim(), after.trim());
    }
}
