//! Anchored edits: change a few lines of a note without rewriting it.
//!
//! A whole-file write is the wrong unit for a note. It conflicts with ANY
//! concurrent edit, so a phone that moved one ticket loses to an agent that
//! reworded the body, and a plugin that rewrites one frontmatter line takes the
//! whole document with it. An anchored edit names the text it expects and the
//! text that replaces it, so it applies only where the file still says what the
//! caller thought, and a stale edit fails loudly instead of clobbering.
//!
//! This module is pure: bytes in, bytes out, no filesystem. Its callers are the
//! web route a browser uses and `cru.fs.edit`, which a plugin uses; one
//! operation so the two halves of the app cannot disagree about what a safe
//! write is.

use serde::{Deserialize, Serialize};

use crate::parser::BlockHash;

/// The hash of the bytes ON DISK, for conflict reporting.
///
/// NOT the hash the note index carries: that one is written asynchronously by
/// the file watcher, so it lags a save and cannot answer "did this file change
/// under me". This one is taken inside the read-modify-write that uses it.
pub fn disk_hash(text: &str) -> String {
    BlockHash::new(*blake3::hash(text.as_bytes()).as_bytes()).to_hex()
}

/// One change: the text expected, and what replaces it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AnchoredEdit {
    /// The text this edit expects to find. Matched against WHOLE LINES.
    pub expect: String,
    /// What replaces it. May carry newlines, so one line becomes several.
    pub replace: String,
    /// Which match to take when `expect` legitimately appears more than once,
    /// zero-based. Absent means the text must appear exactly once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<usize>,
}

/// Why one edit could not be applied. The index is the caller's edit index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum EditRefusal {
    /// The expected text is not in the file. It moved on, or was never there.
    NotFound { index: usize },
    /// The text appears more than once and the edit named no occurrence.
    Ambiguous { index: usize, matches: usize },
    /// `occurrence` named a match the file does not have.
    NoSuchOccurrence { index: usize, matches: usize },
    /// Two edits in one batch cover the same lines.
    Overlaps { index: usize, other: usize },
    /// `expect` was empty, which would match everywhere and nowhere.
    EmptyExpect { index: usize },
}

/// What applying a batch produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    /// The file with every edit applied.
    Applied(String),
    /// Nothing was applied: a batch is all or nothing.
    Refused(Vec<EditRefusal>),
}

/// The line ending a file uses, so a rewrite keeps it.
fn dominant_newline(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if crlf > lf {
        "\r\n"
    } else {
        "\n"
    }
}

/// Byte ranges of fenced code blocks, which anchors never match inside.
///
/// A note about `status: todo` in a fence is prose about a value, not the value.
/// Matching there would rewrite documentation and leave the real line alone.
fn fenced_ranges(lines: &[(usize, usize, &str)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for &(start, end, line) in lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            match open {
                Some(from) => {
                    out.push((from, end));
                    open = None;
                }
                None => open = Some(start),
            }
        }
    }
    // An unclosed fence runs to the end of the file: everything after it is
    // inside, which is what a renderer does too.
    if let Some(from) = open {
        if let Some(&(_, end, _)) = lines.last() {
            out.push((from, end));
        }
    }
    out
}

/// Line spans as `(start, end, text)`, `end` excluding the newline.
fn line_spans(text: &str) -> Vec<(usize, usize, &str)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            let mut end = i;
            if end > start && text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
            spans.push((start, end, &text[start..end]));
            start = i + 1;
        }
    }
    if start <= text.len() {
        spans.push((start, text.len(), &text[start..]));
    }
    spans
}

/// Every whole-line span where `expect` matches, outside fenced blocks.
fn matches_of(text: &str, expect: &str) -> Vec<(usize, usize)> {
    let spans = line_spans(text);
    let fences = fenced_ranges(&spans);
    let wanted: Vec<&str> = expect
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    let mut found = Vec::new();
    if wanted.is_empty() {
        return found;
    }
    for window_start in 0..spans.len() {
        if window_start + wanted.len() > spans.len() {
            break;
        }
        let hit = wanted
            .iter()
            .enumerate()
            .all(|(k, want)| spans[window_start + k].2 == *want);
        if !hit {
            continue;
        }
        let start = spans[window_start].0;
        let end = spans[window_start + wanted.len() - 1].1;
        if fences.iter().any(|&(fs, fe)| start >= fs && end <= fe) {
            continue;
        }
        found.push((start, end));
    }
    found
}

/// Apply a batch to `original`, or refuse it whole.
///
/// Every anchor resolves against the ORIGINAL text and the splices happen by
/// offset, so no edit can match text an earlier edit in the same batch wrote —
/// which would be a match against a document that never existed on disk. It is
/// also what makes the already-applied rule sound: `replace` already present at
/// an anchor can only mean another writer, never this batch.
pub fn apply_anchored_edits(original: &str, edits: &[AnchoredEdit]) -> EditOutcome {
    let newline = dominant_newline(original);
    let mut refusals = Vec::new();
    // The caller's edit index travels WITH the span. An edit that is already
    // applied contributes no span, so the two lists drift — and a refusal
    // that reported a span index named edits the caller never wrote.
    let mut spans: Vec<(usize, usize, String, usize)> = Vec::new();

    for (index, edit) in edits.iter().enumerate() {
        if edit.expect.is_empty() {
            refusals.push(EditRefusal::EmptyExpect { index });
            continue;
        }
        let found = matches_of(original, &edit.expect);
        let chosen = match (found.len(), edit.occurrence) {
            (0, _) => {
                // Already applied by someone else is a success, not a conflict:
                // a second device syncing the same change must not raise one.
                if !matches_of(original, &edit.replace).is_empty() {
                    continue;
                }
                refusals.push(EditRefusal::NotFound { index });
                continue;
            }
            (_, Some(n)) if n >= found.len() => {
                refusals.push(EditRefusal::NoSuchOccurrence {
                    index,
                    matches: found.len(),
                });
                continue;
            }
            (_, Some(n)) => found[n],
            (1, None) => found[0],
            (n, None) => {
                refusals.push(EditRefusal::Ambiguous { index, matches: n });
                continue;
            }
        };
        spans.push((
            chosen.0,
            chosen.1,
            edit.replace.replace('\n', newline),
            index,
        ));
    }

    // Overlap is checked across the whole batch, because two edits that cover
    // the same lines have no order that means anything.
    let mut ordered: Vec<usize> = (0..spans.len()).collect();
    ordered.sort_by_key(|&i| spans[i].0);
    for pair in ordered.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if spans[b].0 < spans[a].1 {
            let (first, second) = (spans[a].3, spans[b].3);
            refusals.push(EditRefusal::Overlaps {
                index: first.min(second),
                other: first.max(second),
            });
        }
    }

    if !refusals.is_empty() {
        return EditOutcome::Refused(refusals);
    }

    let mut out = String::with_capacity(original.len());
    let mut cursor = 0usize;
    for &i in &ordered {
        let (start, end, ref replacement, _) = spans[i];
        out.push_str(&original[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&original[cursor..]);
    EditOutcome::Applied(out)
}
