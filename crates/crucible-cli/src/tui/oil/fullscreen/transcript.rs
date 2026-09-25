//! The transcript as rows at one width.
//!
//! The rows come from the app's kept rows (`transcript_rows`), the cache
//! that the native view uses too: a finished node keeps its rows, and a
//! frame lays out only the nodes that can still change. This module does
//! not keep rows. It numbers the rows of the frame, finds a row, and maps a
//! place in the text across a reflow.

use super::selection::RowRef;
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::transcript_rows::FrameRows;

/// A position in the transcript that survives a reflow: a row inside a node,
/// as a share of that node's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    entry: usize,
    row: usize,
    rows: usize,
}

#[derive(Debug, Default)]
pub struct Transcript {
    width: u16,
    /// The rows of each node, in the order of the nodes.
    entries: Vec<FrameRows>,
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

    /// Take the rows of `app`'s transcript for a frame. `ctx` must come from
    /// `OilChatApp::frame_context`; its width is the transcript width.
    pub fn sync(&mut self, app: &mut OilChatApp, ctx: &ViewContext<'_>) {
        self.width = ctx.terminal_size.0;
        self.entries = app.transcript_frame_rows(ctx);
        self.reindex();
    }

    fn reindex(&mut self) {
        self.starts.clear();
        let mut row = 0;
        for entry in &self.entries {
            // One blank row between nodes, as the native view's `gap(1)`.
            if !entry.rows.rows.is_empty() && row > 0 {
                row += 1;
            }
            self.starts.push(row);
            row += entry.rows.rows.len();
        }
        self.total = row;
    }

    /// Row `index`, or `None` for a separator row or a row past the end.
    pub fn row(&self, index: usize) -> Option<RowRef<'_>> {
        let (entry, offset) = self.locate(index)?;
        let rows = &self.entries[entry].rows;
        Some(RowRef {
            ansi: rows.rows.get(offset)?,
            text: rows.text.get(offset),
        })
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
            rows: self.entries[entry].rows.rows.len(),
        })
    }

    /// The row where `anchor` is now. The row keeps its share of the node's
    /// rows, so a node that wraps to twice the rows maps row 3 to row 6.
    pub fn resolve(&self, anchor: Anchor) -> usize {
        let Some(&start) = self.starts.get(anchor.entry) else {
            return self.total;
        };
        let rows = self.entries[anchor.entry].rows.rows.len();
        let row = (anchor.row * rows).checked_div(anchor.rows).unwrap_or(0);
        start + row.min(rows.saturating_sub(1))
    }

    /// The rows of `entries`, as the styled text that the exit dump prints.
    /// Separator rows are empty strings.
    pub fn styled_rows(&self, entries: std::ops::Range<usize>) -> Vec<String> {
        let mut out = Vec::new();
        let leading_gap = entries.start > 0;
        for entry in &self.entries[entries] {
            if entry.rows.rows.is_empty() {
                continue;
            }
            if leading_gap || !out.is_empty() {
                out.push(String::new());
            }
            out.extend(entry.rows.rows.iter().cloned());
        }
        out
    }

    /// Entries that are finished, counted from the start. The exit dump
    /// prints only these, because a node that still changes has no final
    /// rows.
    pub fn finished_prefix(&self) -> usize {
        self.entries
            .iter()
            .position(|e| !e.finished)
            .unwrap_or(self.entries.len())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::fullscreen::fixtures;
    use crate::tui::oil::theme;
    use crucible_oil::ansi::strip_ansi;
    use crucible_oil::focus::FocusContext;

    fn sync(transcript: &mut Transcript, app: &mut OilChatApp, width: u16) {
        let focus = FocusContext::new();
        let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, 40));
        let ctx = app.frame_context(&ctx);
        transcript.sync(app, &ctx);
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

        sync(&mut transcript, &mut app, 100);
        assert_eq!(app.transcript_layouts(), first);

        sync(&mut transcript, &mut app, 60);
        assert_eq!(app.transcript_layouts(), first + 6, "a width change");
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
        let start = (transcript.starts[5]..transcript.len())
            .find(|&i| !text_of(&transcript, i).trim().is_empty())
            .unwrap();
        let before = text_of(&transcript, start);
        let anchor = transcript.anchor_at(start).unwrap();

        sync(&mut transcript, &mut app, 70);
        let after = text_of(&transcript, transcript.resolve(anchor));
        assert_eq!(before.trim(), after.trim());
    }
}
