//! Full-screen view of a plugin's surface.
//!
//! A surface is data a plugin declared — rows with a stable id, text, optional
//! detail and an optional [`Mark`]. This draws it; it never asks the plugin how.
//!
//! **A modal, not a pane.** A pane would need the window layer: a per-pane
//! scroll offset, focus routing and float anchoring inside a rectangle, none of
//! which exists. Nothing owns a transcript scroll offset today, by design, and
//! a pane is where that stops being true. A modal needs none of it — it owns the
//! screen for as long as it is open, exactly as the shell modal does — so this
//! is the first surface a user can reach without building the window registry.
//!
//! The glyph for a mark is chosen **here**, not by the plugin. The plugin states
//! `busy` or `blocked`; this file decides what that looks like in a terminal, and
//! the web decides separately. That split is the whole reason `Mark` is a stated
//! vocabulary instead of a character a plugin picks.

use crossterm::event::{KeyCode, KeyEvent};
use crucible_oil::node::{col, row, spacer, styled, Node};
use crucible_oil::style::{Color, Style};

/// One row, as the client received it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfaceModalRow {
    pub id: String,
    pub text: String,
    pub detail: Option<String>,
    /// The declared mark name, or `None`. Kept as the string the daemon sent so
    /// a mark this build has no glyph for still renders its row.
    pub mark: Option<String>,
}

/// What a key did, for the app to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceModalOutcome {
    /// The key moved the cursor or did nothing. Stay open.
    Handled,
    /// Close the modal.
    Close,
}

/// A surface, open full-screen.
#[derive(Debug, Clone, Default)]
pub struct SurfaceModal {
    title: String,
    rows: Vec<SurfaceModalRow>,
    cursor: usize,
    scroll_offset: usize,
    /// What the daemon said the version was, so a `surface_changed` for an older
    /// version can be ignored rather than causing a pointless refetch.
    version: u64,
}

impl SurfaceModal {
    #[must_use]
    pub fn new(title: String, rows: Vec<SurfaceModalRow>, version: u64) -> Self {
        Self {
            title,
            rows,
            cursor: 0,
            scroll_offset: 0,
            version,
        }
    }

    /// Replace the rows after a refetch, keeping the cursor on the same row id.
    ///
    /// By id, not by index. A row that arrives or leaves above the cursor would
    /// otherwise move the selection under the reader's hands, which is the
    /// failure a list that refreshes itself makes most often.
    pub fn update(&mut self, rows: Vec<SurfaceModalRow>, version: u64) {
        let anchor = self.selected_id().map(str::to_string);
        self.rows = rows;
        self.version = version;
        self.cursor = anchor
            .and_then(|id| self.rows.iter().position(|r| r.id == id))
            .unwrap_or_else(|| self.cursor.min(self.rows.len().saturating_sub(1)));
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The id under the cursor, for an action to name later.
    #[must_use]
    pub fn selected_id(&self) -> Option<&str> {
        self.rows.get(self.cursor).map(|r| r.id.as_str())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SurfaceModalOutcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => SurfaceModalOutcome::Close,
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.rows.is_empty() {
                    self.cursor = (self.cursor + 1).min(self.rows.len() - 1);
                }
                SurfaceModalOutcome::Handled
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                SurfaceModalOutcome::Handled
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.cursor = 0;
                SurfaceModalOutcome::Handled
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.cursor = self.rows.len().saturating_sub(1);
                SurfaceModalOutcome::Handled
            }
            _ => SurfaceModalOutcome::Handled,
        }
    }

    /// The glyph for a declared mark.
    ///
    /// An unknown mark draws a space, never a placeholder like `?`: a build that
    /// does not know a status should say nothing about it rather than assert that
    /// something is wrong.
    fn mark_glyph(mark: Option<&str>) -> &'static str {
        match mark {
            Some("busy") => "●",
            Some("blocked") => "⏸",
            Some("ok") => "○",
            Some("failed") => "✗",
            _ => " ",
        }
    }

    fn mark_color(mark: Option<&str>, t: &crate::tui::oil::theme::ThemeConfig) -> Color {
        match mark {
            Some("busy") => t.resolve_color(t.colors.primary),
            Some("blocked") => t.resolve_color(t.colors.warning),
            Some("failed") => t.resolve_color(t.colors.error),
            _ => t.resolve_color(t.colors.text_muted),
        }
    }

    /// Keep the cursor inside the visible window.
    fn visible_range(&self, height: usize) -> (usize, usize) {
        if height == 0 || self.rows.is_empty() {
            return (0, 0);
        }
        let mut start = self.scroll_offset.min(self.cursor);
        if self.cursor >= start + height {
            start = self.cursor + 1 - height;
        }
        let end = (start + height).min(self.rows.len());
        (start, end)
    }

    pub fn view(&self, term_width: usize, term_height: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let header_bg = t.resolve_color(t.colors.popup_bg);
        let footer_bg = t.resolve_color(t.colors.background);

        let header_text = format!(" {} ", self.title);
        let header_padding = " ".repeat(term_width.saturating_sub(header_text.chars().count()));
        let header = styled(
            format!("{header_text}{header_padding}"),
            Style::new().bg(header_bg).bold(),
        );

        // Two rows of chrome: the header and the footer.
        let body_height = term_height.saturating_sub(2);
        let (start, end) = self.visible_range(body_height);

        let body_lines: Vec<Node> = if self.rows.is_empty() {
            vec![styled(
                " nothing here yet ".to_string(),
                Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
            )]
        } else {
            self.rows[start..end]
                .iter()
                .enumerate()
                .map(|(offset, r)| self.row_view(r, start + offset == self.cursor, t))
                .collect()
        };

        let footer = self.footer(term_width, footer_bg, t);
        col([header, col(body_lines), spacer(), footer])
    }

    fn row_view(
        &self,
        r: &SurfaceModalRow,
        selected: bool,
        t: &crate::tui::oil::theme::ThemeConfig,
    ) -> Node {
        let mark = styled(
            format!(" {} ", Self::mark_glyph(r.mark.as_deref())),
            Style::new().fg(Self::mark_color(r.mark.as_deref(), t)),
        );
        let label_style = if selected {
            Style::new().fg(t.resolve_color(t.colors.primary)).bold()
        } else {
            Style::new().fg(t.resolve_color(t.colors.text))
        };
        let cursor = styled(if selected { "› " } else { "  " }.to_string(), label_style);
        let mut parts = vec![cursor, mark, styled(r.text.clone(), label_style)];
        if let Some(detail) = &r.detail {
            parts.push(styled(
                format!("  {detail}"),
                Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
            ));
        }
        row(parts)
    }

    fn footer(&self, width: usize, bg: Color, t: &crate::tui::oil::theme::ThemeConfig) -> Node {
        let key_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.primary));
        let text_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.text)).dim();
        let count = format!("({} rows)", self.rows.len());
        let left = row([
            styled(" j/k".to_string(), key_style),
            styled(" move  ".to_string(), text_style),
            styled("esc".to_string(), key_style),
            styled(" close  ".to_string(), text_style),
        ]);
        let pad = " ".repeat(width.saturating_sub(count.chars().count() + 26).max(1));
        row([left, styled(pad, text_style), styled(count, text_style)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn rows(ids: &[&str]) -> Vec<SurfaceModalRow> {
        ids.iter()
            .map(|id| SurfaceModalRow {
                id: (*id).to_string(),
                text: (*id).to_string(),
                detail: None,
                mark: None,
            })
            .collect()
    }

    fn modal(ids: &[&str]) -> SurfaceModal {
        SurfaceModal::new("Sessions".into(), rows(ids), 1)
    }

    #[test]
    fn the_cursor_moves_and_stops_at_both_ends() {
        let mut m = modal(&["a", "b", "c"]);
        assert_eq!(m.selected_id(), Some("a"));

        m.handle_key(key(KeyCode::Char('k')));
        assert_eq!(m.selected_id(), Some("a"), "up at the top stays");

        m.handle_key(key(KeyCode::Char('j')));
        m.handle_key(key(KeyCode::Char('j')));
        assert_eq!(m.selected_id(), Some("c"));
        m.handle_key(key(KeyCode::Char('j')));
        assert_eq!(m.selected_id(), Some("c"), "down at the bottom stays");

        m.handle_key(key(KeyCode::Char('g')));
        assert_eq!(m.selected_id(), Some("a"));
        m.handle_key(key(KeyCode::Char('G')));
        assert_eq!(m.selected_id(), Some("c"));
    }

    #[test]
    fn escape_and_q_close_and_nothing_else_does() {
        assert_eq!(
            modal(&["a"]).handle_key(key(KeyCode::Esc)),
            SurfaceModalOutcome::Close
        );
        assert_eq!(
            modal(&["a"]).handle_key(key(KeyCode::Char('q'))),
            SurfaceModalOutcome::Close
        );
        assert_eq!(
            modal(&["a"]).handle_key(key(KeyCode::Char('x'))),
            SurfaceModalOutcome::Handled
        );
    }

    /// A refetch keeps the selection on the same **row**, not the same index.
    /// A row arriving above the cursor would otherwise move the selection under
    /// the reader's hands, which is how a self-refreshing list goes wrong.
    #[test]
    fn a_refetch_keeps_the_cursor_on_the_same_row() {
        let mut m = modal(&["a", "b", "c"]);
        m.handle_key(key(KeyCode::Char('j')));
        assert_eq!(m.selected_id(), Some("b"));

        m.update(rows(&["new", "a", "b", "c"]), 2);

        assert_eq!(m.selected_id(), Some("b"), "still on b, now at index 2");
        assert_eq!(m.version(), 2);
        assert_eq!(m.row_count(), 4);
    }

    /// When the selected row is gone the cursor clamps rather than pointing past
    /// the end, which would make `selected_id` `None` on a non-empty list.
    #[test]
    fn a_refetch_that_drops_the_selected_row_clamps() {
        let mut m = modal(&["a", "b", "c"]);
        m.handle_key(key(KeyCode::Char('G')));
        assert_eq!(m.selected_id(), Some("c"));

        m.update(rows(&["a"]), 2);

        assert_eq!(m.selected_id(), Some("a"));
    }

    #[test]
    fn an_empty_surface_has_no_selection_and_does_not_panic() {
        let mut m = SurfaceModal::new("Empty".into(), vec![], 1);
        assert_eq!(m.selected_id(), None);
        m.handle_key(key(KeyCode::Char('j')));
        m.handle_key(key(KeyCode::Char('G')));
        assert_eq!(m.selected_id(), None);
        assert_eq!(m.row_count(), 0);
    }

    /// The plugin states a status; this file owns the glyph. An unknown mark
    /// draws a space, never a placeholder that asserts something is wrong.
    #[test]
    fn every_declared_mark_has_a_glyph_and_an_unknown_one_is_blank() {
        assert_eq!(SurfaceModal::mark_glyph(Some("busy")), "●");
        assert_eq!(SurfaceModal::mark_glyph(Some("blocked")), "⏸");
        assert_eq!(SurfaceModal::mark_glyph(Some("ok")), "○");
        assert_eq!(SurfaceModal::mark_glyph(Some("failed")), "✗");
        assert_eq!(SurfaceModal::mark_glyph(Some("sideways")), " ");
        assert_eq!(SurfaceModal::mark_glyph(None), " ");
    }

    /// The window follows the cursor, so a selection below the fold scrolls into
    /// view rather than being drawn off-screen.
    #[test]
    fn the_visible_window_follows_the_cursor() {
        let ids: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let mut m = modal(&refs);

        assert_eq!(m.visible_range(5), (0, 5));

        m.handle_key(key(KeyCode::Char('G')));
        let (start, end) = m.visible_range(5);
        assert_eq!(end, 20, "the last row is visible");
        assert_eq!(start, 15);
    }
}
