//! Kiln (workspace) constants and utilities

use std::path::Path;

/// Directories to exclude from file discovery and watching
pub const EXCLUDED_DIRS: &[&str] = &[".crucible", ".git", ".obsidian", "node_modules", ".trash"];

/// What a file inside a kiln *is*, for discovery, watching, and indexing.
///
/// Before this existed, "is this a file the kiln cares about" was spelled
/// `extension == "md"` at a dozen call sites across the daemon — the watcher
/// filter, discovery, the indexer, trash cleanup, rename, the agent-facing
/// listing and search tools. Adding `.canvas` by teaching each of those about a
/// second extension would have left twelve places to forget on the next format,
/// and they were already subtly inconsistent (some accepted `.markdown`, most
/// did not).
///
/// One predicate, used everywhere, is the point. Match on the variant when
/// behaviour genuinely differs; call [`is_indexable`](Self::is_indexable) when
/// it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KilnFileKind {
    /// A markdown note — the primary content type.
    Note,
    /// A JSON Canvas document. Indexed like a note (it contributes links to the
    /// graph) but parsed completely differently.
    Canvas,
    /// Anything else: images, PDFs, attachments. Present in the kiln and
    /// referenceable, but never parsed or indexed.
    Asset,
}

impl KilnFileKind {
    /// Classify a path by extension.
    ///
    /// Extension matching is case-insensitive: a vault synced from a
    /// case-preserving filesystem can contain `Note.MD`, and treating that as an
    /// asset would silently drop it from the index.
    pub fn of(path: &Path) -> Self {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return Self::Asset;
        };
        match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Self::Note,
            "canvas" => Self::Canvas,
            _ => Self::Asset,
        }
    }

    /// Whether this file participates in the note index and link graph.
    ///
    /// Both notes and canvases do: a canvas contributes its `file` nodes and
    /// the wikilinks in its text cards as links, so leaving it out would mean
    /// backlinks that silently omit the canvas referencing them. (Canvas
    /// *edges* are not yet stored — see `docs/Meta/Analysis/Canvas.md`.)
    pub fn is_indexable(self) -> bool {
        matches!(self, Self::Note | Self::Canvas)
    }

    /// The kiln-facing extensions, for watcher filters that take a list.
    pub const INDEXABLE_EXTENSIONS: &'static [&'static str] = &["md", "markdown", "canvas"];

    /// The [`Note`](Self::Note) extensions alone, for parsers that *advertise* a
    /// list rather than test a path. An advertisement that disagrees with
    /// [`is_note_file`] is how the next hand-rolled copy gets written, so it is
    /// sourced from here too.
    pub const NOTE_EXTENSIONS: &'static [&'static str] = &["md", "markdown"];
}

/// Whether `path` is a markdown note.
pub fn is_note_file(path: &Path) -> bool {
    KilnFileKind::of(path) == KilnFileKind::Note
}

/// Whether `path` is a JSON Canvas document.
pub fn is_canvas_file(path: &Path) -> bool {
    KilnFileKind::of(path) == KilnFileKind::Canvas
}

/// Whether `path` participates in the note index and link graph.
pub fn is_indexable_file(path: &Path) -> bool {
    KilnFileKind::of(path).is_indexable()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_by_extension() {
        assert_eq!(KilnFileKind::of(Path::new("a.md")), KilnFileKind::Note);
        assert_eq!(
            KilnFileKind::of(Path::new("a.markdown")),
            KilnFileKind::Note
        );
        assert_eq!(
            KilnFileKind::of(Path::new("board.canvas")),
            KilnFileKind::Canvas
        );
        assert_eq!(KilnFileKind::of(Path::new("img.png")), KilnFileKind::Asset);
        assert_eq!(KilnFileKind::of(Path::new("noext")), KilnFileKind::Asset);
    }

    /// A vault synced from a case-preserving filesystem really does contain
    /// `Note.MD`; classifying it as an asset drops it from the index silently.
    #[test]
    fn extension_matching_is_case_insensitive() {
        assert_eq!(KilnFileKind::of(Path::new("A.MD")), KilnFileKind::Note);
        assert_eq!(
            KilnFileKind::of(Path::new("B.Canvas")),
            KilnFileKind::Canvas
        );
    }

    #[test]
    fn notes_and_canvases_are_indexable_assets_are_not() {
        assert!(KilnFileKind::Note.is_indexable());
        assert!(KilnFileKind::Canvas.is_indexable());
        assert!(!KilnFileKind::Asset.is_indexable());
    }

    #[test]
    fn a_dotfile_without_an_extension_is_an_asset() {
        // `.canvas` as a whole filename has no extension — Path treats a leading
        // dot as the stem — so it must not be mistaken for a canvas document.
        assert_eq!(KilnFileKind::of(Path::new(".canvas")), KilnFileKind::Asset);
    }

    #[test]
    fn helper_predicates_agree_with_the_enum() {
        let canvas = PathBuf::from("x/y/board.canvas");
        assert!(is_canvas_file(&canvas));
        assert!(is_indexable_file(&canvas));
        assert!(!is_note_file(&canvas));

        let note = PathBuf::from("x/y/note.md");
        assert!(is_note_file(&note));
        assert!(is_indexable_file(&note));
        assert!(!is_canvas_file(&note));
    }

    #[test]
    fn indexable_extensions_match_the_predicate() {
        for ext in KilnFileKind::INDEXABLE_EXTENSIONS {
            let path = PathBuf::from(format!("f.{ext}"));
            assert!(
                is_indexable_file(&path),
                "{ext} is advertised as indexable but does not classify as such"
            );
        }
    }

    #[test]
    fn note_extensions_are_the_note_arm_of_the_classifier() {
        for ext in KilnFileKind::NOTE_EXTENSIONS {
            let path = PathBuf::from(format!("f.{ext}"));
            assert!(
                is_note_file(&path),
                "{ext} is advertised as a note extension but does not classify as one"
            );
            assert!(KilnFileKind::INDEXABLE_EXTENSIONS.contains(ext));
        }
    }

    /// The edge cases the fourteen hand-rolled copies of this predicate
    /// disagreed on. `.mdx`/`.mdc` embed JSX and are deliberately *not* notes;
    /// `.txt` is deliberately not a note either.
    #[test]
    fn classifies_the_edge_cases_the_hand_rolled_copies_disagreed_on() {
        let cases: &[(&str, KilnFileKind)] = &[
            ("a.md", KilnFileKind::Note),
            ("a.MD", KilnFileKind::Note),
            ("a.markdown", KilnFileKind::Note),
            ("a.MARKDOWN", KilnFileKind::Note),
            ("a.mdx", KilnFileKind::Asset),
            ("a.mdc", KilnFileKind::Asset),
            ("a.txt", KilnFileKind::Asset),
            ("noext", KilnFileKind::Asset),
            // A dotfile with a real extension is still a note.
            (".hidden.md", KilnFileKind::Note),
            // Only the last extension counts: a backup is not a note.
            ("notes.md.bak", KilnFileKind::Asset),
            ("board.canvas", KilnFileKind::Canvas),
        ];
        for (name, expected) in cases {
            assert_eq!(
                KilnFileKind::of(Path::new(name)),
                *expected,
                "{name} should classify as {expected:?}"
            );
        }
    }
}
