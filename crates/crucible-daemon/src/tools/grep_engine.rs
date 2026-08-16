//! Shared grep engine (ripgrep crates) for content search.
//!
//! The single engine behind the `grep_notes` MCP tool, the `search_grep`
//! RPC, and `POST /api/search/grep`. Lives in its own module because all
//! three surfaces share the wire contract defined here.

use anyhow::Context as _;
use globset::Glob;
use grep::matcher::Matcher;
use grep::regex::{RegexMatcher, RegexMatcherBuilder};
use grep::searcher::{sinks::UTF8, BinaryDetection, SearcherBuilder};
use ignore::WalkBuilder;
use std::path::Path;

/// A directory to walk, together with the rule for what it may yield.
///
/// The two travel together because separating them is the bug: `grep_notes`
/// validated its start directory and then let the walker recurse into whatever
/// lived beneath it, which under a kiln that encloses the sessions root is
/// every transcript on the machine. A caller cannot name a walk root here
/// without saying, in the same expression, what the walk is allowed to return.
pub(crate) struct WalkScope<'a> {
    root: &'a Path,
    admits: &'a dyn Fn(&Path) -> bool,
}

impl<'a> WalkScope<'a> {
    /// A walk contained by `admits` — the session's filesystem scope.
    pub(crate) fn contained(root: &'a Path, admits: &'a dyn Fn(&Path) -> bool) -> Self {
        Self { root, admits }
    }

    /// A walk with no containment, for the surfaces the USER drives directly
    /// (the `search_grep` RPC and `POST /api/search/grep`), where the path came
    /// from the person at the keyboard rather than from a model.
    pub(crate) fn user_driven(root: &'a Path) -> Self {
        Self {
            root,
            admits: &|_| true,
        }
    }
}

/// Max characters retained in a grep hit's `text` snippet (long lines are
/// truncated so a minified/generated line can't blow up a response).
const GREP_SNIPPET_CAP: usize = 300;

/// A single content-search hit.
///
/// `match_start`/`match_end` are **character** offsets into `text` (post-trim),
/// suitable for `<mark>` highlighting in the web UI. Only the first match on a
/// line is reported. Wire keys (`path`/`rel_path`/`line`/`text`/`match_start`/
/// `match_end`) are the `search_grep` RPC + `POST /api/search/grep` contract.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GrepHit {
    /// Absolute path to the matched file.
    pub path: String,
    /// Path relative to the search's `rel_base` (forward-slash separators).
    pub rel_path: String,
    /// 1-based line number.
    pub line: u64,
    /// The matched line, trimmed of surrounding whitespace and capped at
    /// [`GREP_SNIPPET_CAP`] characters.
    pub text: String,
    /// Character offset of the first match's start within `text`.
    pub match_start: usize,
    /// Character offset of the first match's end within `text`.
    pub match_end: usize,
}

/// Result of a `search_grep` call: the hits plus whether they were capped at
/// the requested limit. Matches the `POST /api/search/grep` response body.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GrepSearchResponse {
    pub hits: Vec<GrepHit>,
    pub truncated: bool,
}

/// Errors from [`grep_search`], split so callers can surface a bad user
/// pattern as an invalid-params rejection rather than an internal failure.
#[derive(Debug, thiserror::Error)]
pub enum GrepSearchError {
    /// The query failed to compile in regex mode. The message names the syntax
    /// problem (straight from the regex parser). Literal mode can't hit this —
    /// escaped queries always compile.
    #[error("Invalid regex: {0}")]
    InvalidRegex(String),
    /// Any other failure (bad glob, matcher internals).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Content search (ripgrep engine: `grep-regex` + `grep-searcher`) over files
/// under the walk root, honoring `.gitignore` and skipping binary files.
///
/// This is the single grep engine shared by the `grep_notes` MCP tool and the
/// `search_grep` RPC/HTTP endpoint.
///
/// * `walk` — directory tree to walk, paired with the rule for what it may
///   yield ([`WalkScope`]).
/// * `rel_base` — paths in `GrepHit::rel_path` are reported relative to this
///   (usually the same as the walk root, but `grep_notes` reports relative to
///   the kiln root while walking a subfolder).
/// * `query` — a **literal** substring by default (regex-escaped); a regular
///   expression (Rust regex syntax) when `regex` is set.
/// * `regex` — compile `query` as a regex instead of escaping it. A pattern
///   that doesn't compile returns [`GrepSearchError::InvalidRegex`].
/// * `glob` — optional name filter (e.g. `*.md`); `None`/empty/`"null"` searches
///   all files. Matched against the file's base name.
/// * `limit` — max hits; when reached, `truncated` is `true`.
/// * `case_insensitive` — ASCII/Unicode-insensitive matching when set;
///   orthogonal to `regex`.
///
/// Returns `(hits, truncated)`.
pub(crate) fn grep_search(
    walk: WalkScope<'_>,
    rel_base: &Path,
    query: &str,
    regex: bool,
    glob: Option<&str>,
    limit: usize,
    case_insensitive: bool,
) -> Result<(Vec<GrepHit>, bool), GrepSearchError> {
    if query.is_empty() || limit == 0 {
        return Ok((Vec::new(), false));
    }

    let pattern = if regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    // `line_terminator` mirrors `RegexMatcher::new_line_matcher`; the builder
    // form is used so `case_insensitive` stays orthogonal to the pattern
    // (no `(?i)` prefix that a user regex could interact with).
    let matcher = RegexMatcherBuilder::new()
        .line_terminator(Some(b'\n'))
        .case_insensitive(case_insensitive)
        .build(&pattern)
        .map_err(|e| {
            if regex {
                GrepSearchError::InvalidRegex(e.to_string())
            } else {
                GrepSearchError::Other(anyhow::Error::new(e).context("compiling literal matcher"))
            }
        })?;

    // Name filter. An explicit empty/"null" glob (LLMs sometimes send the
    // literal string) is treated as "no filter".
    let glob_matcher = match glob {
        Some(g) if !g.is_empty() && g != "null" => {
            Some(Glob::new(g).context("compiling glob")?.compile_matcher())
        }
        _ => None,
    };

    // Quit scanning a file at the first NUL byte so binary files can't leak
    // garbage lines into results.
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();

    let mut hits: Vec<GrepHit> = Vec::new();
    let mut truncated = false;

    for entry in WalkBuilder::new(walk.root)
        .standard_filters(true)
        .build()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();

        // The containment rule applies to what the walk YIELDS. `standard_filters`
        // skips hidden entries BELOW the root but never the root itself, so a
        // caller who names a hidden directory is handed its contents — and no
        // filter of ripgrep's knows anything about the session's roots anyway.
        if !(walk.admits)(path) {
            continue;
        }

        if let Some(ref gm) = glob_matcher {
            let name = path.file_name().unwrap_or_default();
            if !gm.is_match(name) {
                continue;
            }
        }

        let abs = path.to_string_lossy().to_string();
        let rel = path
            .strip_prefix(rel_base)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let search_result = searcher.search_path(
            &matcher,
            path,
            UTF8(|lnum, line| {
                let (text, match_start, match_end) = format_grep_hit(&matcher, line);
                hits.push(GrepHit {
                    path: abs.clone(),
                    rel_path: rel.clone(),
                    line: lnum,
                    text,
                    match_start,
                    match_end,
                });
                // Stop this file (and, below, the whole walk) once capped.
                Ok(hits.len() < limit)
            }),
        );
        // A per-file read error (e.g. permissions) shouldn't abort the search.
        let _ = search_result;

        if hits.len() >= limit {
            truncated = true;
            break;
        }
    }

    Ok((hits, truncated))
}

/// Trim a matched line, cap it, and re-express the first match's byte span as
/// character offsets into the trimmed+capped snippet. Leading-whitespace
/// trimming shifts the offsets accordingly.
fn format_grep_hit(matcher: &RegexMatcher, line: &str) -> (String, usize, usize) {
    // Byte span of the first match in the raw line (matcher operates on the
    // untrimmed line, so offsets are relative to it).
    let raw_match = matcher.find(line.as_bytes()).ok().flatten();

    let lead_bytes = line.len() - line.trim_start().len();
    let trimmed = line.trim();

    // Match byte offsets relative to the trimmed line (clamped into range).
    let (mb_start, mb_end) = match raw_match {
        Some(m) => (
            m.start().saturating_sub(lead_bytes).min(trimmed.len()),
            m.end().saturating_sub(lead_bytes).min(trimmed.len()),
        ),
        None => (0, 0),
    };

    // Byte → char offset (boundaries are valid: regex matches and whitespace
    // trims both fall on char boundaries of a `&str`).
    let byte_to_char = |b: usize| trimmed.get(..b).map_or(0, |s| s.chars().count());
    let mut cs = byte_to_char(mb_start);
    let mut ce = byte_to_char(mb_end);

    let capped: String = trimmed.chars().take(GREP_SNIPPET_CAP).collect();
    let cap_len = capped.chars().count();
    cs = cs.min(cap_len);
    ce = ce.min(cap_len);

    (capped, cs, ce)
}
