//! FTS5 full-text search for SQLite backend
//!
//! Provides full-text search over note titles and content using SQLite's FTS5 extension.
//!
//! ## Usage
//!
//! ```ignore
//! use crucible_daemon::storage::sqlite::{SqlitePool, FtsIndex};
//!
//! // `notes_fts` is created by the migration ladder when the pool opens the
//! // database, so there is no setup step here.
//! let pool = SqlitePool::new(config)?;
//! let fts = FtsIndex::new(pool.clone());
//!
//! // Index a note
//! fts.index("notes/example.md", "Example Note", "Some content here").await?;
//!
//! // Search
//! let results = fts.search("example").await?;
//! ```

use crate::storage::sqlite::connection::SqlitePool;
use crate::storage::sqlite::error_ext::SqliteResultExt;
use crucible_core::storage::{StorageError, StorageResult};

/// `notes_fts` DDL. Executed by the migration ladder
/// (`schema::apply_migrations`), which is the only DDL owner for the kiln
/// database; the constant lives here because this module owns the table's
/// shape. See `docs/Meta/Analysis/Storage Schema.md`.
pub(crate) const NOTES_FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    path,
    title,
    content,
    tokenize='porter unicode61'
);
"#;

/// A full-text search result
#[derive(Debug, Clone, PartialEq)]
pub struct FtsResult {
    /// Path to the note
    pub path: String,
    /// Note title
    pub title: String,
    /// Snippet of matching content (with highlights)
    pub snippet: String,
    /// BM25 relevance score (lower is better in FTS5)
    pub rank: f64,
}

/// FTS5 phrase literal that locates the row for `path` through the inverted
/// index, or `None` when the path cannot be phrase-matched.
///
/// `DELETE FROM notes_fts WHERE path = ?` is a full scan of the content
/// table — FTS5 has no b-tree on its columns, only the term index — so the
/// per-note delete cost grew linearly with the kiln and made every backfill
/// quadratic (measured: 8.1ms/update and 35s to build 12k notes, vs
/// 0.6ms/update and 3.1s via the phrase match). The same tokenizer runs on
/// the indexed path and on this phrase, so a path always matches itself; the
/// caller still adds an exact `path = ?` guard because two different paths
/// can tokenize identically (`x/y.md` and `x-y.md` are both `[x, y, md]`).
///
/// `None` when the path has no alphanumeric characters at all: unicode61
/// would tokenize it to nothing, the phrase would match nothing, and a stale
/// row would silently survive. Those paths take the scan instead.
fn path_phrase(path: &str) -> Option<String> {
    path.chars()
        .any(char::is_alphanumeric)
        .then(|| format!("path:\"{}\"", path.replace('"', "\"\"")))
}

/// Delete the row(s) for `path`, via the term index when possible.
fn delete_path(conn: &rusqlite::Connection, path: &str) -> StorageResult<()> {
    match path_phrase(path) {
        Some(phrase) => conn
            .execute(
                "DELETE FROM notes_fts WHERE rowid IN (
                    SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?1 AND path = ?2)",
                rusqlite::params![phrase, path],
            )
            .sql()?,
        None => conn
            .execute("DELETE FROM notes_fts WHERE path = ?1", [path])
            .sql()?,
    };
    Ok(())
}

/// Build an FTS5 MATCH query from raw user input: implicit AND over words.
///
/// FTS5 MATCH takes a query *syntax*, not a literal — a bare `foo-bar` or a
/// stray quote is a syntax error that would surface as "search failed" for
/// input that is obviously just words. The old fix quoted the whole input as
/// one phrase, which made every multi-word query adjacency-only. This keeps
/// the no-syntax-errors property while giving multi-word queries "all words
/// somewhere in the note" semantics:
///
/// - whitespace-separated terms are each double-quoted (so FTS5 operators
///   like `AND`/`OR`/`NOT`/`*` typed by the user stay literal words) and
///   joined with `AND`;
/// - a user-supplied `"double-quoted span"` survives as one adjacency phrase;
/// - an unbalanced quote degrades gracefully: the stray quote is kept as term
///   text (doubled inside the emitted phrase), never passed through as syntax;
/// - terms with no alphanumeric characters are dropped — unicode61 tokenizes
///   them to nothing, and a zero-token phrase ANDed in would null the whole
///   query instead of being ignored;
/// - input with no usable terms emits `""`, a zero-token phrase that FTS5
///   accepts and matches nothing — same "empty results, no error" contract
///   the whole-phrase quoting gave empty input.
pub(crate) fn build_match_query(user_input: &str) -> String {
    // A term only matters if the tokenizer will get at least one token out of
    // it; unicode61's token characters are the alphanumerics.
    fn push_bare_terms(text: &str, terms: &mut Vec<String>) {
        terms.extend(
            text.split_whitespace()
                .filter(|t| t.chars().any(char::is_alphanumeric))
                .map(str::to_string),
        );
    }

    let mut terms: Vec<String> = Vec::new();
    let mut rest = user_input;
    while let Some(open) = rest.find('"') {
        push_bare_terms(&rest[..open], &mut terms);
        let after = &rest[open + 1..];
        match after.find('"') {
            Some(close) => {
                let phrase = &after[..close];
                if phrase.chars().any(char::is_alphanumeric) {
                    terms.push(phrase.to_string());
                }
                rest = &after[close + 1..];
            }
            None => {
                // Unbalanced: glue the stray quote onto what follows and
                // treat the result as ordinary term text.
                push_bare_terms(&format!("\"{after}"), &mut terms);
                rest = "";
            }
        }
    }
    push_bare_terms(rest, &mut terms);

    if terms.is_empty() {
        return "\"\"".to_string();
    }
    terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// FTS5 full-text search index
#[derive(Clone)]
pub struct FtsIndex {
    pool: SqlitePool,
}

impl FtsIndex {
    /// Create a new FTS index backed by the given pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Index a note's content for full-text search
    ///
    /// This updates the FTS index with the note's content. Call this when
    /// processing notes to enable content search.
    pub async fn index(&self, path: &str, title: &str, content: &str) -> StorageResult<()> {
        let pool = self.pool.clone();
        let path = path.to_string();
        let title = title.to_string();
        let content = content.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Delete existing entry if present
                delete_path(conn, &path)?;

                // Insert new entry
                conn.execute(
                    "INSERT INTO notes_fts(path, title, content) VALUES (?1, ?2, ?3)",
                    rusqlite::params![path, title, content],
                )
                .sql()?;

                Ok(())
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// Remove a note from the FTS index
    pub async fn remove(&self, path: &str) -> StorageResult<()> {
        let pool = self.pool.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || pool.with_connection(|conn| delete_path(conn, &path)))
            .await
            .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// Merge the index's segment b-trees into one (FTS5 `optimize`).
    ///
    /// Incremental delete+insert churn leaves the term index spread across
    /// segments that every query must consult; automerge keeps the count
    /// bounded but not minimal. Measured at 12k notes after 6k re-index
    /// updates: phrase-query latency 24.7ms → 16.1ms, cost 0.25s. On an
    /// already-merged index this is close to a no-op, so callers run it after
    /// batch processing rather than deciding whether it is needed.
    pub async fn optimize(&self) -> StorageResult<()> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute("INSERT INTO notes_fts(notes_fts) VALUES ('optimize')", [])
                    .sql()?;
                Ok(())
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// How many notes the index holds.
    ///
    /// The backfill decision is a comparison against the note count, not an
    /// emptiness check: a daemon killed part-way through the first backfill,
    /// or one note that was transiently unreadable, left a non-empty index
    /// that would never be completed — `cru search` silently missing those
    /// notes until each happened to change, which is the bug the backfill
    /// exists to prevent.
    pub async fn count(&self) -> StorageResult<i64> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let count: i64 = conn
                    .query_row("SELECT count(*) FROM notes_fts", [], |row| row.get(0))
                    .sql()?;
                Ok(count)
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// Whether the index holds no rows.
    ///
    /// Used to decide whether a kiln needs the one-time backfill: a kiln
    /// processed before this index existed has notes in SQLite and nothing
    /// here, and would search as though it were empty.
    pub async fn is_empty(&self) -> StorageResult<bool> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let count: i64 = conn
                    .query_row("SELECT count(*) FROM notes_fts", [], |row| row.get(0))
                    .sql()?;
                Ok(count == 0)
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// Search for notes matching a query
    ///
    /// Uses FTS5's default ranking (BM25). The query supports FTS5 syntax:
    /// - `word` - match word
    /// - `word*` - prefix match
    /// - `"phrase search"` - exact phrase
    /// - `word1 AND word2` - both words
    /// - `word1 OR word2` - either word
    /// - `word1 NOT word2` - word1 but not word2
    ///
    /// Note: the `search_text` RPC (behind `cru search`) runs user input
    /// through [`build_match_query`] before calling this, so the operator
    /// syntax above is reachable only by direct callers of this method, not
    /// from the CLI.
    pub async fn search(&self, query: &str, limit: usize) -> StorageResult<Vec<FtsResult>> {
        let pool = self.pool.clone();
        let query = query.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // The inner query ranks and limits on the term index alone;
                // snippet() runs only for the surviving rows. Computed in the
                // top-level SELECT it runs for *every* matching row before
                // LIMIT (measured 16.6ms vs 10.0ms for a query matching most
                // of a 12k-note kiln). The `MATCH` in the outer join is what
                // gives snippet() its match positions.
                let mut stmt = conn
                    .prepare(
                        r#"
                    SELECT
                        f.path,
                        f.title,
                        snippet(f.notes_fts, 2, '<mark>', '</mark>', '...', 32) as snippet,
                        r.rank
                    FROM (
                        SELECT rowid AS id, bm25(notes_fts) AS rank
                        FROM notes_fts
                        WHERE notes_fts MATCH ?1
                        ORDER BY rank
                        LIMIT ?2
                    ) r
                    JOIN notes_fts f ON f.rowid = r.id AND f.notes_fts MATCH ?1
                    ORDER BY r.rank
                    "#,
                    )
                    .sql()?;

                let results = stmt
                    .query_map(rusqlite::params![query, limit as i64], |row| {
                        Ok(FtsResult {
                            path: row.get(0)?,
                            title: row.get(1)?,
                            snippet: row.get(2)?,
                            rank: row.get(3)?,
                        })
                    })
                    .sql()?
                    .collect::<Result<Vec<_>, _>>()
                    .sql()?;

                Ok(results)
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }

    /// Search with a custom column boost
    ///
    /// Allows boosting title matches over content matches.
    pub async fn search_boosted(
        &self,
        query: &str,
        title_boost: f64,
        content_boost: f64,
        limit: usize,
    ) -> StorageResult<Vec<FtsResult>> {
        let pool = self.pool.clone();
        let query = query.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // FTS5 bm25 takes column weights as arguments
                // Column order: path (0), title (1), content (2)
                // Same rank-then-snippet split as `search` — see the comment
                // there.
                let mut stmt = conn
                    .prepare(
                        r#"
                    SELECT
                        f.path,
                        f.title,
                        snippet(f.notes_fts, 2, '<mark>', '</mark>', '...', 32) as snippet,
                        r.rank
                    FROM (
                        SELECT rowid AS id, bm25(notes_fts, 0.0, ?2, ?3) AS rank
                        FROM notes_fts
                        WHERE notes_fts MATCH ?1
                        ORDER BY rank
                        LIMIT ?4
                    ) r
                    JOIN notes_fts f ON f.rowid = r.id AND f.notes_fts MATCH ?1
                    ORDER BY r.rank
                    "#,
                    )
                    .sql()?;

                let results = stmt
                    .query_map(
                        rusqlite::params![query, title_boost, content_boost, limit as i64],
                        |row| {
                            Ok(FtsResult {
                                path: row.get(0)?,
                                title: row.get(1)?,
                                snippet: row.get(2)?,
                                rank: row.get(3)?,
                            })
                        },
                    )
                    .sql()?
                    .collect::<Result<Vec<_>, _>>()
                    .sql()?;

                Ok(results)
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| StorageError::Backend(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::config::SqliteConfig;

    /// `notes_fts` comes from the migration ladder, which `SqlitePool::new`
    /// runs — so there is nothing for this fixture to set up beyond the pool.
    async fn setup_test_fts() -> StorageResult<FtsIndex> {
        let pool = SqlitePool::new(SqliteConfig::memory())?;
        Ok(FtsIndex::new(pool))
    }

    #[tokio::test]
    async fn the_ladder_creates_the_fts_table_before_any_index_call() {
        let fts = setup_test_fts().await.unwrap();

        // Should be able to search (empty results)
        let results = fts.search("test", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_fts_index_and_search() {
        let fts = setup_test_fts().await.unwrap();

        // Index some notes
        fts.index(
            "notes/rust.md",
            "Rust Programming",
            "Rust is a systems programming language",
        )
        .await
        .unwrap();
        fts.index(
            "notes/python.md",
            "Python Guide",
            "Python is great for scripting",
        )
        .await
        .unwrap();

        // Search for rust
        let results = fts.search("rust", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "notes/rust.md");
        assert_eq!(results[0].title, "Rust Programming");
    }

    #[tokio::test]
    async fn test_fts_phrase_search() {
        let fts = setup_test_fts().await.unwrap();

        fts.index(
            "a.md",
            "Note A",
            "the quick brown fox jumps over the lazy dog",
        )
        .await
        .unwrap();
        fts.index("b.md", "Note B", "quick fox runs away")
            .await
            .unwrap();

        // Phrase search
        let results = fts.search("\"quick brown\"", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "a.md");
    }

    #[tokio::test]
    async fn test_fts_prefix_search() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Programming", "content").await.unwrap();
        fts.index("b.md", "Problem Solving", "content")
            .await
            .unwrap();
        fts.index("c.md", "Other", "content").await.unwrap();

        // Prefix search
        let results = fts.search("pro*", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_fts_remove() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Test", "content").await.unwrap();
        let results = fts.search("test", 10).await.unwrap();
        assert_eq!(results.len(), 1);

        fts.remove("a.md").await.unwrap();
        let results = fts.search("test", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_fts_boosted_search() {
        let fts = setup_test_fts().await.unwrap();

        // Title has "rust", content doesn't
        fts.index("title_match.md", "Rust Guide", "A guide to programming")
            .await
            .unwrap();
        // Content has "rust", title doesn't
        fts.index(
            "content_match.md",
            "Programming Guide",
            "Learn about rust and go",
        )
        .await
        .unwrap();

        // With high title boost, title match should rank better
        let results = fts.search_boosted("rust", 10.0, 1.0, 10).await.unwrap();
        assert_eq!(results.len(), 2);
        // Note: BM25 returns negative scores where lower (more negative) is better
        // The title match should have a more negative (better) score
    }

    #[tokio::test]
    async fn reindexing_a_path_with_no_word_characters_still_replaces_the_old_row() {
        let fts = setup_test_fts().await.unwrap();

        // "...///" tokenizes to nothing, so the fast MATCH-driven delete
        // cannot find it and must fall back to the scan.
        fts.index("...///", "Old", "stale words").await.unwrap();
        fts.index("...///", "New", "fresh words").await.unwrap();

        assert!(fts.search("stale", 10).await.unwrap().is_empty());
        let results = fts.search("fresh", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "New");
    }

    #[tokio::test]
    async fn reindexing_unicode_and_quoted_paths_replaces_rather_than_duplicates() {
        let fts = setup_test_fts().await.unwrap();

        for path in ["héllo wörld.md", "a\"b.md", "notes/a/b.md"] {
            fts.index(path, "Old", "outdated").await.unwrap();
            fts.index(path, "New", "replaced").await.unwrap();
        }

        assert!(fts.search("outdated", 10).await.unwrap().is_empty());
        assert_eq!(fts.search("replaced", 10).await.unwrap().len(), 3);
        assert_eq!(fts.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn deleting_a_path_leaves_similarly_tokenized_paths_alone() {
        let fts = setup_test_fts().await.unwrap();

        // "x/y.md" and "x-y.md" tokenize to the same phrase [x, y, md]; the
        // delete must not take the neighbor out with the target.
        fts.index("x/y.md", "Slash", "slash content").await.unwrap();
        fts.index("x-y.md", "Dash", "dash content").await.unwrap();

        fts.remove("x/y.md").await.unwrap();

        assert!(fts.search("slash", 10).await.unwrap().is_empty());
        assert_eq!(fts.search("dash", 10).await.unwrap().len(), 1);

        fts.index("x/y.md", "Slash", "slash content").await.unwrap();
        fts.index("x/y.md", "Slash2", "slash again").await.unwrap();
        assert_eq!(fts.search("dash", 10).await.unwrap().len(), 1);
        assert_eq!(fts.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn snippet_highlights_the_matched_content() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note", "some text about ferrous metallurgy here")
            .await
            .unwrap();

        let results = fts.search("metallurgy", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0].snippet.contains("<mark>metallurgy</mark>"),
            "snippet was: {}",
            results[0].snippet
        );
    }

    #[tokio::test]
    async fn optimize_runs_and_search_still_works() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note", "alpha beta").await.unwrap();
        fts.optimize().await.unwrap();

        assert_eq!(fts.search("alpha", 10).await.unwrap().len(), 1);
    }

    // --- build_match_query: implicit-AND semantics over user input ---

    #[test]
    fn multi_word_input_becomes_anded_quoted_terms() {
        assert_eq!(build_match_query("foo bar"), r#""foo" AND "bar""#);
        assert_eq!(build_match_query("rust"), r#""rust""#);
    }

    #[test]
    fn user_quoted_span_is_preserved_as_one_phrase() {
        assert_eq!(build_match_query(r#""foo bar""#), r#""foo bar""#);
        assert_eq!(
            build_match_query(r#"say "foo bar" now"#),
            r#""say" AND "foo bar" AND "now""#
        );
    }

    #[test]
    fn operators_and_punctuation_stay_literal() {
        // FTS5 operators typed bare become quoted literals, never syntax.
        assert_eq!(build_match_query("a OR b"), r#""a" AND "OR" AND "b""#);
        assert_eq!(build_match_query("NOT done"), r#""NOT" AND "done""#);
        assert_eq!(build_match_query("pro*"), r#""pro*""#);
        assert_eq!(build_match_query("foo-bar"), r#""foo-bar""#);
    }

    #[test]
    fn stray_quote_is_term_text_not_syntax() {
        // Unbalanced quote: the stray quote rides along inside a quoted term
        // (doubled for FTS5), so the output is still a valid query.
        assert_eq!(build_match_query(r#"foo "bar"#), r#""foo" AND """bar""#);
        assert_eq!(
            build_match_query(r#"what"s this"#),
            r#""what" AND """s" AND "this""#
        );
    }

    #[test]
    fn empty_and_token_free_input_yield_a_match_nothing_query() {
        // A phrase with zero tokens is valid FTS5 and matches no rows — the
        // same "empty results, no error" contract the old whole-phrase
        // quoting gave empty input.
        assert_eq!(build_match_query(""), r#""""#);
        assert_eq!(build_match_query("   "), r#""""#);
        assert_eq!(build_match_query("* -"), r#""""#);
    }

    #[tokio::test]
    async fn multi_word_query_matches_non_adjacent_words() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note A", "alpha something in between beta")
            .await
            .unwrap();
        fts.index("b.md", "Note B", "alpha only, no second word")
            .await
            .unwrap();

        let results = fts
            .search(&build_match_query("alpha beta"), 10)
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "both words anywhere in the note must match"
        );
        assert_eq!(results[0].path, "a.md");
    }

    #[tokio::test]
    async fn user_quoted_phrase_still_requires_adjacency() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("adjacent.md", "A", "the quick brown fox")
            .await
            .unwrap();
        fts.index("spread.md", "B", "quick as a wink, brown as mud")
            .await
            .unwrap();

        let results = fts
            .search(&build_match_query("\"quick brown\""), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "adjacent.md");
    }

    #[tokio::test]
    async fn bare_operators_search_for_themselves() {
        let fts = setup_test_fts().await.unwrap();

        // Paths are indexed too, so neither fixture path may tokenize to
        // "and" — that would satisfy the query from the path column.
        fts.index("first.md", "A", "milk and cookies for foo bar")
            .await
            .unwrap();
        fts.index("second.md", "B", "foo bar plain").await.unwrap();

        // "foo AND bar" demands the literal word "and", not the operator.
        let results = fts
            .search(&build_match_query("foo AND bar"), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "first.md");

        // "pro*" is the literal word "pro", not a prefix query.
        fts.index("prefix.md", "C", "programming languages")
            .await
            .unwrap();
        fts.index("word.md", "D", "a pro tip").await.unwrap();
        let results = fts.search(&build_match_query("pro*"), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "word.md");
    }

    #[tokio::test]
    async fn stray_quote_input_does_not_error() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note", "foo then later bar")
            .await
            .unwrap();

        let results = fts
            .search(&build_match_query("foo \"bar"), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn empty_input_returns_no_results_without_error() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note", "some content").await.unwrap();

        for input in ["", "   ", "*"] {
            let results = fts.search(&build_match_query(input), 10).await.unwrap();
            assert!(results.is_empty(), "input {input:?} must match nothing");
        }
    }

    #[tokio::test]
    async fn snippet_and_rank_survive_the_and_query() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Note", "ferrous words then metallurgy topics")
            .await
            .unwrap();

        let results = fts
            .search(&build_match_query("ferrous metallurgy"), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0].snippet.contains("<mark>"),
            "snippet was: {}",
            results[0].snippet
        );
        assert!(results[0].rank.is_finite());
    }

    #[tokio::test]
    async fn test_fts_update_existing() {
        let fts = setup_test_fts().await.unwrap();

        fts.index("a.md", "Old Title", "old content").await.unwrap();
        let results = fts.search("old", 10).await.unwrap();
        assert_eq!(results.len(), 1);

        // Update with new content
        fts.index("a.md", "New Title", "new content").await.unwrap();

        // Old content should not be found
        let results = fts.search("old", 10).await.unwrap();
        assert!(results.is_empty());

        // New content should be found
        let results = fts.search("new", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "New Title");
    }
}
