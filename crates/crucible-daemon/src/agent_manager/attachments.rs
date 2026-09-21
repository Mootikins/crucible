//! `@file` mentions in a user message, resolved to file contents.
//!
//! The TUI's `@` completion inserts the path as literal text and always did;
//! what was missing was anyone reading it. Resolution lives here — daemon-side
//! — rather than in the composer, so every client gets it from the same code:
//! the web composer sends the same text and needs no expansion logic of its
//! own, and a message replayed from history resolves the same way.
//!
//! A mention can end in a line suffix: `@a.rs:12` or `@a.rs:12-14`. Then only
//! those lines attach. The path resolves under the tool root of the session
//! and then under each root of its containment, so a kiln-relative path works.
//! The containment of the session judges each candidate, so a mention reads
//! no file that a `read_file` call could not read.
//!
//! Everything here is best-effort and silent. A mention that names no file,
//! escapes every root, or is binary is left alone: the agent still sees the
//! text the user typed and can call `read_file` itself. Failing loudly on
//! `user@example.com` would be worse than doing nothing.

use crate::tools::containment::{Access, Containment, RootSet};
use crate::tools::path_resolution::ResolvedPath;
use crucible_core::traits::ContextMessage;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// Metadata tag marking the attachment block, mirroring `PRECOGNITION_TAG`.
pub(crate) const ATTACHMENT_TAG: &str = "file_attachment";

/// Largest single file inlined into the prompt. Bigger files are truncated
/// with a note rather than dropped — a user who attaches a 2MB log still gets
/// its head, and the agent is told the tail is missing so it can read the rest.
const MAX_FILE_BYTES: usize = 64 * 1024;

/// Ceiling across all mentions in one message, so `@a @b @c @d` cannot blow
/// the context window open.
const MAX_TOTAL_BYTES: usize = 192 * 1024;

/// Build the system block carrying the contents of every `@file` mention in
/// `content`, or `None` when nothing resolved.
///
/// `tool_root` is where the tools of the session act. `roots` is the
/// containment of the session: it supplies the other roots to try, and it
/// judges every candidate.
pub(super) fn build_attachment_message(
    tool_root: &Path,
    roots: &RootSet,
    content: &str,
) -> Option<ContextMessage> {
    let mut block = String::new();
    let mut total = 0usize;
    let mut seen: Vec<(PathBuf, Option<RangeInclusive<usize>>)> = Vec::new();

    for mention in extract_mentions(content) {
        let (path_text, lines) = split_line_suffix(mention);
        let Some(path) = resolve_under_roots(tool_root, roots, path_text) else {
            continue;
        };
        let key = (path, lines.clone());
        if seen.contains(&key) {
            continue;
        }
        // Read before the dedup commit: an unreadable path should not shadow
        // a later readable mention of the same file.
        let Ok(text) = std::fs::read_to_string(&key.0) else {
            continue;
        };
        let text = match &lines {
            Some(range) => match select_lines(&text, range) {
                Some(selected) => selected,
                // A range past the end names no text. The agent still sees
                // the mention and can read the file itself.
                None => continue,
            },
            None => text,
        };
        seen.push(key);

        if total >= MAX_TOTAL_BYTES {
            block.push_str("\n(further attachments omitted: total size limit reached)\n");
            break;
        }

        let budget = MAX_FILE_BYTES.min(MAX_TOTAL_BYTES - total);
        let (body, truncated) = truncate_on_char_boundary(&text, budget);
        total += body.len();

        block.push_str(&format!(
            "\n### {mention}\n\n```\n{}\n```\n",
            body.trim_end()
        ));
        if truncated {
            block.push_str("(truncated: attach a smaller file or read it with a tool)\n");
        }
    }

    if block.is_empty() {
        return None;
    }

    let mut msg = ContextMessage::system(format!(
        "The user attached these files with `@` in their message. \
         Their contents are below — you do not need to read them again.\n{block}"
    ));
    msg.metadata.tags.push(ATTACHMENT_TAG.to_string());
    Some(msg)
}

/// Pull `@`-prefixed tokens that start a word.
///
/// The word-start rule is what keeps `user@example.com` and `crate@1.2.3` out:
/// only `@` at the beginning of the message or after whitespace/`(`/`[` counts.
fn extract_mentions(content: &str) -> Vec<&str> {
    let bytes = content.as_bytes();
    let mut mentions = Vec::new();

    for (i, _) in content.match_indices('@') {
        let starts_word = i == 0
            || matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b'[' | b'"' | b'\''
            );
        if !starts_word {
            continue;
        }
        let rest = &content[i + 1..];
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let token = trim_trailing_punctuation(&rest[..end]);
        if !token.is_empty() {
            mentions.push(token);
        }
    }

    mentions
}

/// Strip sentence punctuation a mention picked up from its surroundings.
///
/// A trailing `.` is left alone: `@notes/todo.md` must not become
/// `@notes/todo.m`, and a path ending in a bare `.` is not a thing people
/// write. Everything else here cannot end a filename in practice.
fn trim_trailing_punctuation(token: &str) -> &str {
    token.trim_end_matches([',', ';', ':', '!', '?', ')', ']', '}', '"', '\''])
}

/// Split a trailing `:N` or `:N-M` from a mention.
///
/// The lines are 1-based and the end is inclusive, as in the reference form
/// `path:start-end`. A token with no digit suffix is all path. A digit suffix
/// that names no line (`:0`, `:3-2`) still splits, so [`select_lines`]
/// refuses it and the mention attaches nothing rather than the whole file.
fn split_line_suffix(mention: &str) -> (&str, Option<RangeInclusive<usize>>) {
    // A sentence can end right after the range: `see @a.rs:2-3.`
    let token = mention.strip_suffix('.').unwrap_or(mention);
    let Some((path, suffix)) = token.rsplit_once(':') else {
        return (mention, None);
    };
    let (start, end) = suffix.split_once('-').unwrap_or((suffix, suffix));
    let parse = |part: &str| {
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<usize>().ok())
            .flatten()
    };
    match (parse(start), parse(end)) {
        (Some(start), Some(end)) if !path.is_empty() => (path, Some(start..=end)),
        _ => (mention, None),
    }
}

/// The lines of `text` in `range`, or `None` when the range names no line.
fn select_lines(text: &str, range: &RangeInclusive<usize>) -> Option<String> {
    let (start, end) = (*range.start(), *range.end());
    if start == 0 || end < start {
        return None;
    }
    let selected: Vec<&str> = text.lines().skip(start - 1).take(end - start + 1).collect();
    (!selected.is_empty()).then(|| selected.join("\n"))
}

/// Resolve a mention under the tool root, then under each containment root.
///
/// Absolute paths are refused outright rather than checked: a message is
/// user-authored text reaching a daemon that may be serving several clients,
/// and "the file I meant" is always relative to a root. The containment of
/// the session judges each candidate on both of its resolved forms, so a
/// `..` or a link out of every root attaches nothing.
fn resolve_under_roots(tool_root: &Path, roots: &RootSet, mention: &str) -> Option<PathBuf> {
    let candidate = Path::new(mention);
    if mention.is_empty() || candidate.is_absolute() {
        return None;
    }

    std::iter::once(tool_root)
        .chain(roots.allowed_roots())
        .find_map(|root| {
            let resolved = ResolvedPath::resolve(&root.join(candidate));
            match roots.judge_resolved(&resolved, Access::Read) {
                Containment::Permitted(path) if path.is_file() => Some(path),
                _ => None,
            }
        })
}

/// Cut `text` to at most `limit` bytes without splitting a character.
fn truncate_on_char_boundary(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, body) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, body).unwrap();
        }
        dir
    }

    /// Attach with the workspace as the only root, as a session with no kiln.
    fn attach(workspace: &Path, content: &str) -> Option<ContextMessage> {
        let roots = RootSet::scoped([workspace.to_path_buf()], []);
        build_attachment_message(workspace, &roots, content)
    }

    const FIVE_LINES: &str = "line-one\nline-two\nline-three\nline-four\nline-five\n";

    #[test]
    fn a_line_suffix_attaches_that_line() {
        let ws = workspace_with(&[("a.rs", FIVE_LINES)]);
        let msg = attach(ws.path(), "look at @a.rs:2").expect("a line suffix must resolve");
        assert!(msg.content.contains("line-two"), "got: {}", msg.content);
        for other in ["line-one", "line-three"] {
            assert!(
                !msg.content.contains(other),
                "only line 2 attaches, got: {}",
                msg.content
            );
        }
    }

    #[test]
    fn a_range_suffix_attaches_those_lines() {
        let ws = workspace_with(&[("a.rs", FIVE_LINES)]);
        let msg = attach(ws.path(), "look at @a.rs:2-3.").expect("a range suffix must resolve");
        assert!(msg.content.contains("line-two"), "got: {}", msg.content);
        assert!(msg.content.contains("line-three"), "got: {}", msg.content);
        for other in ["line-one", "line-four"] {
            assert!(
                !msg.content.contains(other),
                "only lines 2-3 attach, got: {}",
                msg.content
            );
        }
    }

    #[test]
    fn a_range_that_is_not_valid_attaches_nothing() {
        let ws = workspace_with(&[("a.rs", FIVE_LINES)]);
        for bad in ["@a.rs:0", "@a.rs:3-2", "@a.rs:9"] {
            assert!(
                attach(ws.path(), bad).is_none(),
                "{bad} must attach nothing"
            );
        }
    }

    #[test]
    fn a_mention_resolves_under_an_attached_kiln() {
        let ws = workspace_with(&[]);
        let kiln = workspace_with(&[("notes/idea.md", "KILN-BODY")]);
        let roots = RootSet::scoped([kiln.path().to_path_buf(), ws.path().to_path_buf()], []);
        let msg = build_attachment_message(ws.path(), &roots, "see @notes/idea.md")
            .expect("a kiln-relative mention must resolve");
        assert!(msg.content.contains("KILN-BODY"));
    }

    #[test]
    fn a_mention_outside_every_root_is_refused() {
        let ws = workspace_with(&[]);
        let kiln = workspace_with(&[]);
        let outside = workspace_with(&[("secret.txt", "LEAKED")]);
        let roots = RootSet::scoped([kiln.path().to_path_buf(), ws.path().to_path_buf()], []);

        let escape = format!(
            "read @../{}/secret.txt",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        assert!(
            build_attachment_message(ws.path(), &roots, &escape).is_none(),
            "a `..` mention must not leave every root"
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("secret.txt"),
                kiln.path().join("link.txt"),
            )
            .unwrap();
            assert!(
                build_attachment_message(ws.path(), &roots, "read @link.txt").is_none(),
                "a link out of a root must not attach its target"
            );
        }
    }

    #[test]
    fn an_email_address_is_not_an_attachment() {
        // The mention rule has to survive ordinary prose, or every message
        // mentioning a colleague tries to open a file.
        let ws = workspace_with(&[("example.com", "NOT-THIS")]);
        assert!(
            attach(ws.path(), "ask user@example.com about it").is_none(),
            "an `@` mid-word is not a file mention"
        );
    }

    #[test]
    fn a_mention_with_no_matching_file_attaches_nothing() {
        let ws = workspace_with(&[]);
        assert!(attach(ws.path(), "see @nope.md").is_none());
    }

    #[test]
    fn mentions_outside_the_workspace_are_refused() {
        let ws = workspace_with(&[]);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "LEAKED").unwrap();

        let escape = format!(
            "read @../{}/secret.txt",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        assert!(
            attach(ws.path(), &escape).is_none(),
            "a mention must not escape the workspace"
        );

        let absolute = format!("read @{}", outside.path().join("secret.txt").display());
        assert!(
            attach(ws.path(), &absolute).is_none(),
            "an absolute path is not a workspace mention"
        );
    }

    #[test]
    fn trailing_sentence_punctuation_does_not_break_a_mention() {
        let ws = workspace_with(&[("notes.md", "BODY")]);
        let msg = attach(ws.path(), "look at @notes.md, then stop")
            .expect("mention followed by a comma should resolve");
        assert!(msg.content.contains("BODY"));
    }

    #[test]
    fn the_same_file_mentioned_twice_is_attached_once() {
        let ws = workspace_with(&[("notes.md", "BODY-ONCE")]);
        let msg = attach(ws.path(), "@notes.md vs @notes.md").unwrap();
        assert_eq!(msg.content.matches("BODY-ONCE").count(), 1);
    }

    #[test]
    fn an_oversized_file_is_truncated_rather_than_dropped() {
        let big = "x".repeat(MAX_FILE_BYTES * 2);
        let ws = workspace_with(&[("big.log", big.as_str())]);
        let msg = attach(ws.path(), "@big.log").expect("still attached");
        assert!(msg.content.contains("truncated"), "and says so");
        assert!(
            msg.content.len() < MAX_FILE_BYTES + 4096,
            "the whole file must not land in the prompt"
        );
    }

    #[test]
    fn a_directory_mention_attaches_nothing() {
        let ws = workspace_with(&[("dir/file.md", "BODY")]);
        assert!(attach(ws.path(), "@dir").is_none());
    }
}
