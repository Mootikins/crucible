//! `@file` mentions in a user message, resolved to file contents.
//!
//! The TUI's `@` completion inserts the path as literal text and always did;
//! what was missing was anyone reading it. Resolution lives here — daemon-side
//! — rather than in the composer, so every client gets it from the same code:
//! the web composer sends the same text and needs no expansion logic of its
//! own, and a message replayed from history resolves the same way.
//!
//! Everything here is best-effort and silent. A mention that names no file,
//! escapes the workspace, or is binary is left alone: the agent still sees the
//! text the user typed and can call `read_file` itself. Failing loudly on
//! `user@example.com` would be worse than doing nothing.

use crucible_core::traits::ContextMessage;
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
pub(super) fn build_attachment_message(workspace: &Path, content: &str) -> Option<ContextMessage> {
    let mut block = String::new();
    let mut total = 0usize;
    let mut seen: Vec<PathBuf> = Vec::new();

    for mention in extract_mentions(content) {
        let Some(path) = resolve_in_workspace(workspace, mention) else {
            continue;
        };
        if seen.contains(&path) {
            continue;
        }
        // Read before the dedup commit: an unreadable path should not shadow
        // a later readable mention of the same file.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        seen.push(path);

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

/// Resolve a mention against the workspace, refusing anything that leaves it.
///
/// Absolute paths are refused outright rather than checked: a message is
/// user-authored text reaching a daemon that may be serving several clients,
/// and "the file I meant" is always workspace-relative. Symlinks are followed
/// by `canonicalize`, so the containment check sees where a link really points.
fn resolve_in_workspace(workspace: &Path, mention: &str) -> Option<PathBuf> {
    let candidate = Path::new(mention);
    if candidate.is_absolute() {
        return None;
    }

    let joined = workspace.join(candidate);
    let resolved = joined.canonicalize().ok()?;
    if !resolved.is_file() {
        return None;
    }

    // Canonicalize the workspace too: a workspace reached through a symlink
    // (macOS `/tmp` → `/private/tmp`, which is every tempdir-based test on
    // that platform) would fail the prefix check against an un-resolved root.
    let root = workspace.canonicalize().ok()?;
    resolved.starts_with(&root).then_some(resolved)
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

    #[test]
    fn an_email_address_is_not_an_attachment() {
        // The mention rule has to survive ordinary prose, or every message
        // mentioning a colleague tries to open a file.
        let ws = workspace_with(&[("example.com", "NOT-THIS")]);
        assert!(
            build_attachment_message(ws.path(), "ask user@example.com about it").is_none(),
            "an `@` mid-word is not a file mention"
        );
    }

    #[test]
    fn a_mention_with_no_matching_file_attaches_nothing() {
        let ws = workspace_with(&[]);
        assert!(build_attachment_message(ws.path(), "see @nope.md").is_none());
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
            build_attachment_message(ws.path(), &escape).is_none(),
            "a mention must not escape the workspace"
        );

        let absolute = format!("read @{}", outside.path().join("secret.txt").display());
        assert!(
            build_attachment_message(ws.path(), &absolute).is_none(),
            "an absolute path is not a workspace mention"
        );
    }

    #[test]
    fn trailing_sentence_punctuation_does_not_break_a_mention() {
        let ws = workspace_with(&[("notes.md", "BODY")]);
        let msg = build_attachment_message(ws.path(), "look at @notes.md, then stop")
            .expect("mention followed by a comma should resolve");
        assert!(msg.content.contains("BODY"));
    }

    #[test]
    fn the_same_file_mentioned_twice_is_attached_once() {
        let ws = workspace_with(&[("notes.md", "BODY-ONCE")]);
        let msg = build_attachment_message(ws.path(), "@notes.md vs @notes.md").unwrap();
        assert_eq!(msg.content.matches("BODY-ONCE").count(), 1);
    }

    #[test]
    fn an_oversized_file_is_truncated_rather_than_dropped() {
        let big = "x".repeat(MAX_FILE_BYTES * 2);
        let ws = workspace_with(&[("big.log", big.as_str())]);
        let msg = build_attachment_message(ws.path(), "@big.log").expect("still attached");
        assert!(msg.content.contains("truncated"), "and says so");
        assert!(
            msg.content.len() < MAX_FILE_BYTES + 4096,
            "the whole file must not land in the prompt"
        );
    }

    #[test]
    fn a_directory_mention_attaches_nothing() {
        let ws = workspace_with(&[("dir/file.md", "BODY")]);
        assert!(build_attachment_message(ws.path(), "@dir").is_none());
    }
}
