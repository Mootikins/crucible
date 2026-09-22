//! Full-screen view of one diffset, for `:diff`.
//!
//! The view shows one file at a time and pages through it. It asks for the
//! text of a file when the user moves to that file, as the web pane does when
//! the user expands a file. Thus a branch with many files does not send all
//! its texts at once.
//!
//! Like the surface modal, this is a modal and not a pane: it owns the screen
//! while it is open, and it needs no window layer.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::diff::{DiffFileText, Diffset, DiffsetId, DiffsetSource};
use crucible_oil::node::{col, row, spacer, styled, Node};
use crucible_oil::style::{Color, Style};

use super::diff_view::{diff_row_count, diffset_file_diff, render_diffset_file, DiffOptions};

/// The rows that are not diff rows: the header, the footer, the file header
/// of the body and the "more lines" row below a page.
const CHROME_ROWS: usize = 4;

/// The body height before the first frame gives the real one.
const DEFAULT_PAGE: usize = 20;

/// A request for the text of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFileRequest {
    pub source: DiffsetSource,
    pub id: DiffsetId,
    pub index: usize,
    pub path: String,
    /// The old path of a renamed file.
    pub from: Option<String>,
}

/// What a key did, for the app to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffModalOutcome {
    /// The key moved the view or did nothing. Stay open.
    Handled,
    /// Close the modal.
    Close,
    /// The view moved to a file with no text. Ask the daemon for it.
    Load(DiffFileRequest),
}

/// The text of one file, as the view knows it.
#[derive(Debug, Clone, Default)]
enum FileSlot {
    /// Nobody asked for the text yet.
    #[default]
    Absent,
    /// The client asked for the text and waits for it.
    Asked,
    Loaded(DiffFileText),
}

/// A diffset, open full-screen.
#[derive(Debug, Clone)]
pub struct DiffModal {
    diffset: Diffset,
    /// The header text. `None` names the source of the diffset.
    title: Option<String>,
    texts: Vec<FileSlot>,
    /// The index of the file on screen.
    file: usize,
    /// The first body row of the page.
    first_line: usize,
    /// The body rows of the last frame. A page key moves by this many rows.
    page: Cell<usize>,
    /// The width of the last frame. The row count depends on the layout.
    width: Cell<usize>,
}

impl DiffModal {
    #[must_use]
    pub fn new(diffset: Diffset) -> Self {
        let texts = vec![FileSlot::Absent; diffset.files.len()];
        Self {
            diffset,
            title: None,
            texts,
            file: 0,
            first_line: 0,
            page: Cell::new(DEFAULT_PAGE),
            width: Cell::new(80),
        }
    }

    /// A diffset whose texts the caller holds already, such as a proposal.
    ///
    /// A file with `None` has no text yet, and the view asks for it as
    /// [`Self::new`] does.
    #[must_use]
    pub fn with_texts(diffset: Diffset, texts: Vec<Option<DiffFileText>>) -> Self {
        let mut modal = Self::new(diffset);
        for (slot, text) in modal.texts.iter_mut().zip(texts) {
            if let Some(text) = text {
                *slot = FileSlot::Loaded(text);
            }
        }
        modal
    }

    /// Show `title` in the header in place of the source.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The id of the diffset on screen.
    #[must_use]
    pub fn id(&self) -> &DiffsetId {
        &self.diffset.id
    }

    /// The index of the file on screen.
    #[must_use]
    pub fn file(&self) -> usize {
        self.file
    }

    /// The first body row of the page.
    #[must_use]
    pub fn first_line(&self) -> usize {
        self.first_line
    }

    /// The request for the text of the file on screen, when nobody asked yet.
    ///
    /// The request marks the file as asked, so one file gets one request.
    pub fn request_text(&mut self) -> Option<DiffFileRequest> {
        let entry = self.diffset.files.get(self.file)?;
        let slot = self.texts.get_mut(self.file)?;
        if entry.binary || entry.too_large || !matches!(slot, FileSlot::Absent) {
            return None;
        }
        *slot = FileSlot::Asked;
        Some(DiffFileRequest {
            source: self.diffset.source.clone(),
            id: self.diffset.id.clone(),
            index: self.file,
            path: entry.path.clone(),
            from: crate::commands::diff::renamed_from(&entry.status).map(str::to_string),
        })
    }

    /// Keep the text of one file. A text for another diffset is ignored.
    pub fn set_text(&mut self, id: &DiffsetId, index: usize, text: DiffFileText) {
        if id != &self.diffset.id {
            return;
        }
        if let Some(slot) = self.texts.get_mut(index) {
            *slot = FileSlot::Loaded(text);
        }
    }

    fn loaded_text(&self, index: usize) -> Option<&DiffFileText> {
        match self.texts.get(index)? {
            FileSlot::Loaded(text) => Some(text),
            FileSlot::Absent | FileSlot::Asked => None,
        }
    }

    fn options(&self) -> DiffOptions {
        DiffOptions::for_width(self.width.get())
    }

    /// The body rows of the file on screen.
    fn row_count(&self) -> usize {
        match (
            self.diffset.files.get(self.file),
            self.loaded_text(self.file),
        ) {
            (Some(entry), Some(text)) => {
                diff_row_count(&diffset_file_diff(entry, text), &self.options())
            }
            _ => 0,
        }
    }

    /// Move to the file at `index`, and ask for its text when needed.
    fn show_file(&mut self, index: usize) -> DiffModalOutcome {
        if index != self.file {
            self.file = index;
            self.first_line = 0;
        }
        self.request_text()
            .map_or(DiffModalOutcome::Handled, DiffModalOutcome::Load)
    }

    fn scroll_to(&mut self, first_line: usize) -> DiffModalOutcome {
        let last = self.row_count().saturating_sub(self.page.get());
        self.first_line = first_line.min(last);
        DiffModalOutcome::Handled
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DiffModalOutcome {
        let files = self.diffset.files.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => DiffModalOutcome::Close,
            KeyCode::Char('n') | KeyCode::Right | KeyCode::Char('l') => {
                self.show_file((self.file + 1).min(files.saturating_sub(1)))
            }
            KeyCode::Char('p') | KeyCode::Left | KeyCode::Char('h') => {
                self.show_file(self.file.saturating_sub(1))
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll_to(self.first_line + self.page.get())
            }
            KeyCode::PageUp => self.scroll_to(self.first_line.saturating_sub(self.page.get())),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_to(self.first_line + 1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_to(self.first_line.saturating_sub(1)),
            KeyCode::Home | KeyCode::Char('g') => self.scroll_to(0),
            KeyCode::End | KeyCode::Char('G') => self.scroll_to(usize::MAX),
            _ => DiffModalOutcome::Handled,
        }
    }

    pub fn view(&self, term_width: usize, term_height: usize) -> Node {
        let t = crate::tui::oil::theme::active();
        let page = term_height.saturating_sub(CHROME_ROWS).max(1);
        self.page.set(page);
        self.width.set(term_width);

        let header_bg = t.resolve_color(t.colors.popup_bg);
        let files = self.diffset.files.len();
        let header_text = format!(
            " Diff: {}  file {}/{} ",
            self.title(),
            (self.file + 1).min(files),
            files
        );
        let header_padding = " ".repeat(term_width.saturating_sub(header_text.chars().count()));
        let header = styled(
            format!("{header_text}{header_padding}"),
            Style::new().bg(header_bg).bold(),
        );

        let body = match self.diffset.files.get(self.file) {
            None => styled(
                " no changes ".to_string(),
                Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
            ),
            Some(entry) => {
                let mut opts = self.options();
                opts.max_lines = Some(page);
                opts.first_line = Some(self.first_line);
                render_diffset_file(entry, self.loaded_text(self.file), &opts)
            }
        };

        let footer = self.footer(term_width, t.resolve_color(t.colors.background), t);
        col([header, body, spacer(), footer])
    }

    /// The base that the diffset compares against.
    fn title(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        match &self.diffset.source {
            DiffsetSource::Branch { base, head, .. } => match head {
                Some(head) => format!("{base}...{head}"),
                None => format!("{base}...working tree"),
            },
            DiffsetSource::SessionRecord { session } => format!("session {session}"),
            DiffsetSource::Proposal { id } => format!("proposal {id}"),
        }
    }

    fn footer(&self, width: usize, bg: Color, t: &crate::tui::oil::theme::ThemeConfig) -> Node {
        let key_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.primary));
        let text_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.text)).dim();
        let rows = self.row_count();
        let position = if rows == 0 {
            String::new()
        } else {
            let last = (self.first_line + self.page.get()).min(rows);
            format!("(rows {}-{last} of {rows})", self.first_line + 1)
        };
        let hints = [
            (" n/p", " file  "),
            ("PgUp/PgDn", " page  "),
            ("esc", " close  "),
        ];
        let used: usize = hints
            .iter()
            .map(|(k, v)| k.chars().count() + v.chars().count())
            .sum();
        let mut parts: Vec<Node> = hints
            .iter()
            .flat_map(|(k, v)| {
                [
                    styled((*k).to_string(), key_style),
                    styled((*v).to_string(), text_style),
                ]
            })
            .collect();
        let pad = " ".repeat(width.saturating_sub(used + position.chars().count()).max(1));
        parts.push(styled(pad, text_style));
        parts.push(styled(position, text_style));
        row(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use crucible_core::diff::{DiffFileEntry, FileStatus};
    use crucible_core::session::PhysicalRoot;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn entry(path: &str, status: FileStatus) -> DiffFileEntry {
        DiffFileEntry {
            root: PhysicalRoot::from_top_level("/repo"),
            path: path.into(),
            status,
            added: 1,
            removed: 1,
            binary: false,
            too_large: false,
        }
    }

    fn diffset() -> Diffset {
        let source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/repo"),
            base: "main".into(),
            head: None,
        };
        Diffset {
            id: source.id(),
            source,
            files: vec![
                entry("a.rs", FileStatus::Modified),
                entry(
                    "b.rs",
                    FileStatus::Renamed {
                        from: "old_b.rs".into(),
                    },
                ),
            ],
        }
    }

    fn long_text() -> DiffFileText {
        let base: String = (0..50).map(|i| format!("line_{i}\n")).collect();
        DiffFileText {
            base_text: Some(base),
            current_text: Some(String::new()),
        }
    }

    #[test]
    fn a_new_file_asks_for_its_text_once() {
        let mut modal = DiffModal::new(diffset());
        let request = modal.request_text().expect("the first file needs its text");
        assert_eq!(request.path, "a.rs");
        assert_eq!(request.index, 0);
        assert_eq!(modal.request_text(), None, "one request for one file");

        match modal.handle_key(key(KeyCode::Char('n'))) {
            DiffModalOutcome::Load(request) => {
                assert_eq!(request.path, "b.rs");
                assert_eq!(request.from.as_deref(), Some("old_b.rs"), "the old path");
            }
            other => panic!("expected a load, got {other:?}"),
        }
        assert_eq!(modal.file(), 1);
        assert_eq!(
            modal.handle_key(key(KeyCode::Char('n'))),
            DiffModalOutcome::Handled,
            "the last file stays, and it has its request"
        );
    }

    #[test]
    fn page_down_and_page_up_change_the_first_line() {
        let mut modal = DiffModal::new(diffset());
        let id = modal.id().clone();
        modal.set_text(&id, 0, long_text());
        // A frame of 14 rows leaves a page of 10 diff rows.
        let _ = modal.view(80, 14);

        modal.handle_key(key(KeyCode::PageDown));
        assert_eq!(modal.first_line(), 10);
        modal.handle_key(key(KeyCode::PageDown));
        assert_eq!(modal.first_line(), 20);
        modal.handle_key(key(KeyCode::PageUp));
        assert_eq!(modal.first_line(), 10);

        modal.handle_key(key(KeyCode::Char('G')));
        assert_eq!(modal.first_line(), 40, "the last page ends at the last row");
        modal.handle_key(key(KeyCode::PageDown));
        assert_eq!(modal.first_line(), 40, "no page after the last one");
    }

    #[test]
    fn a_text_for_another_diffset_is_ignored() {
        let mut modal = DiffModal::new(diffset());
        let other = DiffsetId::for_branch(&PhysicalRoot::from_top_level("/other"), "main", None);
        modal.set_text(&other, 0, long_text());
        assert_eq!(modal.row_count(), 0, "the text of another diffset");
    }

    #[test]
    fn escape_and_q_close() {
        assert_eq!(
            DiffModal::new(diffset()).handle_key(key(KeyCode::Esc)),
            DiffModalOutcome::Close
        );
        assert_eq!(
            DiffModal::new(diffset()).handle_key(key(KeyCode::Char('q'))),
            DiffModalOutcome::Close
        );
    }
}
