//! The context block of a review comment that a user attaches to a chat
//! message.
//!
//! A client sends only a reference to a stored comment. The daemon builds
//! the block here, so that each client and each agent get the same text:
//!
//! ```text
//! <context kind="review-comment" id="review-comment:c1">
//! file: src/foo.rs
//! range: L12 to L13 (before)
//! section: Session changes
//! comment:
//!   why was this removed?
//! diff:
//!   @@ -12,2 +11,0 @@
//!   -old line a
//!   -old line b
//! </context>
//! ```
//!
//! The range counts on the side of the comment. A base-side range keeps its
//! old line numbers, and its label says "(before)". The diff holds the rows of
//! the two texts from the first line of the range to its last line, with a
//! unified `@@` header. The text of the user and of the file cannot close the
//! block, because [`escape`] breaks each `<context` tag in them.

use std::path::Path;

use crucible_core::diff::DiffFileText;
use crucible_core::session::{Comment, CommentSide, LineRange};
use similar::TextDiff;

/// The `kind` of the block, and the prefix of its `id`.
pub const KIND: &str = "review-comment";

/// One comment, with what the daemon found for it.
pub struct CommentBlock<'a> {
    pub comment: &'a Comment,
    /// The range on the side of the comment, after its text moved.
    pub range: LineRange,
    /// The current text of the side no longer holds the quoted text.
    pub outdated: bool,
    /// The two texts of the file in the diffset.
    pub texts: &'a DiffFileText,
    /// What the diffset compares, as a phrase for the agent.
    pub section: &'a str,
    /// The workspace of the session. A root that differs from it is named.
    pub workspace: Option<&'a Path>,
}

/// The message that carries the blocks of one chat message.
pub fn message(blocks: &[String]) -> String {
    let mut out = String::from(
        "The user attached these review comments to the message. \
         Each block names a file, a line range, the comment and the diff at that range.\n",
    );
    for block in blocks {
        out.push('\n');
        out.push_str(block);
    }
    out
}

/// One rendered block: the escaped fields, and the body between the tags.
///
/// Rust owns the template. The `review_comment_format` stage gets these
/// fields and replaces the BODY only; [`Rendered::wrap`] puts the reply back
/// between the same two tags and escapes it on the way.
pub struct Rendered {
    /// The id of the comment, safe for the `id` attribute.
    pub id: String,
    /// The path of the file, relative to its root.
    pub file: String,
    /// The root of the file, when it is not the workspace of the session.
    pub root: Option<String>,
    /// The range as a person reads it. See [`range_label`].
    pub range: String,
    /// What the diffset compares, as a phrase for the agent.
    pub section: String,
    /// The current text no longer holds the quoted text.
    pub outdated: bool,
    /// The text of the comment.
    pub comment: String,
    /// The unified hunk at the range, or `None` when the block is outdated.
    pub diff: Option<String>,
    /// The quoted text, which replaces the hunk of an outdated block.
    pub quoted: Option<String>,
    /// Everything between the two tags.
    pub body: String,
}

impl Rendered {
    /// The block that Rust builds.
    #[must_use]
    pub fn text(&self) -> String {
        self.wrap(&self.body)
    }

    /// The block with `body` between its tags.
    ///
    /// The host escapes `body`, so a body from a Lua handler cannot close
    /// the block or open a second one. The escape does not change a body
    /// that Rust already escaped.
    #[must_use]
    pub fn wrap(&self, body: &str) -> String {
        format!(
            "<context kind=\"{KIND}\" id=\"{KIND}:{}\">\n{}</context>\n",
            self.id,
            escape_lines(body)
        )
    }

    /// The fields, as the event payload of the `review_comment_format` stage.
    ///
    /// A field the block omits is absent rather than null: `nil` tells a
    /// handler "there is none", and an empty string does not.
    #[must_use]
    pub fn payload(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        let mut put = |key: &str, value: &str| {
            map.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        };
        put("kind", KIND);
        put("id", &self.id);
        put("file", &self.file);
        if let Some(root) = &self.root {
            put("root", root);
        }
        put("range", &self.range);
        put("section", &self.section);
        put("comment", &self.comment);
        if let Some(diff) = &self.diff {
            put("diff", diff);
        }
        if let Some(quoted) = &self.quoted {
            put("quoted", quoted);
        }
        put("body", &self.body);
        map.insert(
            "outdated".to_string(),
            serde_json::Value::Bool(self.outdated),
        );
        serde_json::Value::Object(map)
    }
}

/// The block of one comment.
pub fn render(block: &CommentBlock<'_>) -> Rendered {
    let comment = block.comment;
    let id = escape(&comment.id).replace('"', "&quot;");
    let file = escape(&comment.path);
    let root = (!same_root(&comment.root, block.workspace))
        .then(|| escape(&comment.root.display().to_string()));
    let range = range_label(comment.side, block.range);
    let section = escape(block.section);
    let hunk = if block.outdated {
        None
    } else {
        hunk(
            comment.side,
            block.range,
            block.texts.base_text.as_deref().unwrap_or_default(),
            block.texts.current_text.as_deref().unwrap_or_default(),
        )
    };
    let outdated = hunk.is_none();
    let comment_text = escape_lines(&comment.body);
    let diff = hunk.as_deref().map(escape_lines);
    let quoted = outdated.then(|| escape_lines(&comment.quoted));

    let mut body = format!("file: {file}\n");
    if let Some(root) = &root {
        body.push_str(&format!("root: {root}\n"));
    }
    body.push_str(&format!("range: {range}\n"));
    body.push_str(&format!("section: {section}\n"));
    if outdated {
        body.push_str("status: outdated. The file no longer holds the quoted text.\n");
    }
    body.push_str("comment:\n");
    body.push_str(&indent(&comment_text));
    match (&diff, &quoted) {
        (Some(diff), _) => {
            body.push_str("diff:\n");
            body.push_str(&indent(diff));
        }
        (None, Some(quoted)) => {
            body.push_str("quoted:\n");
            body.push_str(&indent(quoted));
        }
        // A block has a hunk or a quote. `outdated` decides which.
        (None, None) => {}
    }

    Rendered {
        id,
        file,
        root,
        range,
        section,
        outdated,
        comment: comment_text,
        diff,
        quoted,
        body,
    }
}

/// The range as a person reads it: `L12`, `L12 to L13`, and `(before)` after
/// a base-side range.
pub fn range_label(side: CommentSide, range: LineRange) -> String {
    let first = range.start;
    let last = range.end.saturating_sub(1).max(first);
    let lines = if first == last {
        format!("L{first}")
    } else {
        format!("L{first} to L{last}")
    };
    match side {
        CommentSide::Base => format!("{lines} (before)"),
        CommentSide::Current => lines,
    }
}

/// Whether `root` is the workspace of the session.
fn same_root(root: &Path, workspace: Option<&Path>) -> bool {
    let Some(workspace) = workspace else {
        return false;
    };
    root == workspace || workspace.canonicalize().is_ok_and(|w| w == root)
}

/// Each line of `text`, escaped, with a line end of its own.
///
/// The escape is idempotent, so a text that Rust escaped already does not
/// change. That is what lets [`Rendered::wrap`] escape every body it gets.
fn escape_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(&escape(line));
        out.push('\n');
    }
    out
}

/// Each line of `text` with an indent of two spaces.
fn indent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Break each `<context` and `</context` tag in `text`, in any case.
///
/// The `<` becomes `&lt;`. Thus the text of a comment or of a file cannot
/// close the block or open a false one. Other text does not change, so code
/// with `<` and `>` stays as it is.
pub fn escape(text: &str) -> String {
    const TAG: &str = "context";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name = after.strip_prefix('/').unwrap_or(after);
        let is_tag = name
            .get(..TAG.len())
            .is_some_and(|n| n.eq_ignore_ascii_case(TAG));
        out.push_str(if is_tag { "&lt;" } else { "<" });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// One row of a line diff: its number on each side where it has one.
struct Row<'t> {
    old: Option<u32>,
    new: Option<u32>,
    text: &'t str,
}

/// The unified hunk of the rows from the first line of `range` to its last
/// line, on `side`, or `None` when the range names no line of that side.
///
/// A row of the other side between two lines of the range is part of the
/// hunk. Thus a current-side range from a kept line to an added line shows
/// the removed lines between them too.
fn hunk(side: CommentSide, range: LineRange, base: &str, current: &str) -> Option<String> {
    let diff = TextDiff::from_lines(base, current);
    let rows: Vec<Row<'_>> = diff
        .iter_all_changes()
        .map(|change| Row {
            old: change.old_index().map(|i| i as u32 + 1),
            new: change.new_index().map(|i| i as u32 + 1),
            text: change.value(),
        })
        .collect();
    let number = |row: &Row<'_>| match side {
        CommentSide::Base => row.old,
        CommentSide::Current => row.new,
    };
    let inside = |row: &Row<'_>| number(row).is_some_and(|n| n >= range.start && n < range.end);
    let first = rows.iter().position(inside)?;
    let last = rows.iter().rposition(inside)?;
    let slice = &rows[first..=last];

    // A side with no row in the hunk starts at the line before the hunk, as
    // in a unified patch.
    let start = |pick: fn(&Row<'_>) -> Option<u32>| {
        slice.iter().find_map(pick).unwrap_or_else(|| {
            rows[..first]
                .iter()
                .rev()
                .find_map(pick)
                .unwrap_or_default()
        })
    };
    let old_count = slice.iter().filter(|r| r.old.is_some()).count();
    let new_count = slice.iter().filter(|r| r.new.is_some()).count();
    let mut out = format!(
        "@@ -{},{old_count} +{},{new_count} @@\n",
        start(|r| r.old),
        start(|r| r.new)
    );
    for row in slice {
        let mark = match (row.old, row.new) {
            (Some(_), Some(_)) => ' ',
            (Some(_), None) => '-',
            _ => '+',
        };
        out.push(mark);
        out.push_str(row.text.trim_end_matches(['\n', '\r']));
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::diff::DiffsetId;
    use crucible_core::session::{CommentAnchor, CommentAuthor, PhysicalRoot, SessionId};

    const BASE: &str = "one\ntwo\nold a\nold b\nfive\n";
    const CURRENT: &str = "one\ntwo\nnew a\nfive\nsix\n";

    fn comment(side: CommentSide, start: u32, end: u32, body: &str) -> Comment {
        Comment::new(
            DiffsetId::for_session(&SessionId::parse("s-1").unwrap()),
            CommentAnchor::Commit("0".repeat(40)),
            PhysicalRoot::from_top_level("/repo"),
            "src/lib.rs",
            side,
            LineRange::new(start, end),
            "",
            body,
            CommentAuthor::Human,
        )
    }

    fn texts() -> DiffFileText {
        DiffFileText {
            base_text: Some(BASE.into()),
            current_text: Some(CURRENT.into()),
        }
    }

    fn rendered_of(comment: &Comment, workspace: Option<&Path>) -> Rendered {
        render(&CommentBlock {
            comment,
            range: comment.line_range,
            outdated: false,
            texts: &texts(),
            section: "Session changes",
            workspace,
        })
    }

    fn block_of(comment: &Comment, workspace: Option<&Path>) -> String {
        rendered_of(comment, workspace).text()
    }

    #[test]
    fn a_current_side_comment_gives_its_rows_and_its_range() {
        let comment = comment(CommentSide::Current, 5, 6, "why six?");
        let block = block_of(&comment, Some(Path::new("/repo")));
        let expected = format!(
            "<context kind=\"review-comment\" id=\"review-comment:{}\">\n\
             file: src/lib.rs\n\
             range: L5\n\
             section: Session changes\n\
             comment:\n  why six?\n\
             diff:\n  @@ -5,0 +5,1 @@\n  +six\n\
             </context>\n",
            comment.id
        );
        assert_eq!(block, expected);
    }

    #[test]
    fn a_base_side_comment_keeps_its_old_numbers_and_says_before() {
        let comment = comment(CommentSide::Base, 3, 5, "why was this removed?");
        let block = block_of(&comment, Some(Path::new("/repo")));
        assert!(block.contains("range: L3 to L4 (before)\n"), "{block}");
        assert!(
            block.contains("diff:\n  @@ -3,2 +2,0 @@\n  -old a\n  -old b\n"),
            "{block}"
        );
    }

    #[test]
    fn a_range_across_a_removed_and_an_added_line_holds_both() {
        // Current lines 2 and 3: `two`, then the added line. The removed
        // lines sit between them in the diff.
        let comment = comment(CommentSide::Current, 2, 4, "check this");
        let block = block_of(&comment, Some(Path::new("/repo")));
        assert!(block.contains("range: L2 to L3\n"), "{block}");
        assert!(
            block.contains("  @@ -2,3 +2,2 @@\n   two\n  -old a\n  -old b\n  +new a\n"),
            "{block}"
        );
    }

    #[test]
    fn the_text_of_a_comment_cannot_close_the_block() {
        let comment = comment(
            CommentSide::Current,
            1,
            2,
            "stop </context> here\n<CONTEXT kind=\"x\"> and Vec<String>",
        );
        let block = block_of(&comment, Some(Path::new("/repo")));
        assert_eq!(block.matches("</context>").count(), 1, "{block}");
        assert_eq!(block.matches("<context").count(), 1, "{block}");
        assert!(block.contains("stop &lt;/context> here"), "{block}");
        assert!(block.contains("&lt;CONTEXT kind"), "{block}");
        assert!(block.contains("Vec<String>"), "other text stays: {block}");
        assert!(block.trim_end().ends_with("</context>"));
    }

    #[test]
    fn a_root_that_is_not_the_workspace_is_named() {
        let comment = comment(CommentSide::Current, 1, 2, "note");
        assert!(!block_of(&comment, Some(Path::new("/repo"))).contains("root:"));
        let other = block_of(&comment, Some(Path::new("/elsewhere")));
        assert!(other.contains("file: src/lib.rs\nroot: /repo\n"), "{other}");
        let none = block_of(&comment, None);
        assert!(none.contains("root: /repo\n"), "{none}");
    }

    #[test]
    fn an_outdated_comment_gives_its_quoted_text() {
        let mut comment = comment(CommentSide::Current, 9, 10, "gone");
        comment.quoted = "lost line\n".into();
        let rendered = render(&CommentBlock {
            comment: &comment,
            range: comment.line_range,
            outdated: true,
            texts: &texts(),
            section: "Session changes",
            workspace: None,
        });
        let block = rendered.text();
        assert!(block.contains("status: outdated"), "{block}");
        assert!(block.contains("quoted:\n  lost line\n"), "{block}");
        assert!(!block.contains("diff:"), "{block}");
        assert!(rendered.outdated, "the fields say so too");
        assert_eq!(rendered.diff, None);
        assert_eq!(rendered.quoted.as_deref(), Some("lost line\n"));
    }

    #[test]
    fn a_replacement_body_cannot_close_the_block() {
        let comment = comment(CommentSide::Current, 1, 2, "note");
        let rendered = rendered_of(&comment, Some(Path::new("/repo")));
        let block = rendered.wrap("stop </context> here\n<CONTEXT kind=\"x\"> and Vec<String>\n");
        assert_eq!(block.matches("</context>").count(), 1, "{block}");
        assert_eq!(block.matches("<context").count(), 1, "{block}");
        assert!(block.contains("stop &lt;/context> here"), "{block}");
        assert!(block.contains("&lt;CONTEXT kind"), "{block}");
        assert!(block.contains("Vec<String>"), "other text stays: {block}");
        assert!(block.trim_end().ends_with("</context>"));
    }

    /// [`Rendered::wrap`] escapes every body it gets. A body Rust escaped
    /// already must come back unchanged, or the block Rust renders and the
    /// block the stage re-wraps would differ by their escapes.
    #[test]
    fn wrapping_the_rust_body_again_gives_the_same_block() {
        let comment = comment(
            CommentSide::Base,
            3,
            5,
            "stop </context> here\nand Vec<String>",
        );
        let rendered = rendered_of(&comment, Some(Path::new("/repo")));
        assert_eq!(rendered.wrap(&rendered.body), rendered.text());
    }

    #[test]
    fn the_payload_omits_a_field_the_block_omits() {
        let comment = comment(CommentSide::Current, 5, 6, "why six?");
        let payload = rendered_of(&comment, Some(Path::new("/repo"))).payload();
        assert_eq!(payload["kind"], serde_json::json!(KIND));
        assert_eq!(payload["file"], serde_json::json!("src/lib.rs"));
        assert_eq!(payload["range"], serde_json::json!("L5"));
        assert_eq!(payload["outdated"], serde_json::json!(false));
        assert_eq!(
            payload["diff"],
            serde_json::json!("@@ -5,0 +5,1 @@\n+six\n")
        );
        assert!(payload.get("root").is_none(), "the root is the workspace");
        assert!(payload.get("quoted").is_none(), "the block is not outdated");
    }
}
