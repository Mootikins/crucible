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
                conn.execute("DELETE FROM notes_fts WHERE path = ?1", [&path])
                    .sql()?;

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

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute("DELETE FROM notes_fts WHERE path = ?1", [&path])
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
    pub async fn search(&self, query: &str, limit: usize) -> StorageResult<Vec<FtsResult>> {
        let pool = self.pool.clone();
        let query = query.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        r#"
                    SELECT
                        path,
                        title,
                        snippet(notes_fts, 2, '<mark>', '</mark>', '...', 32) as snippet,
                        bm25(notes_fts) as rank
                    FROM notes_fts
                    WHERE notes_fts MATCH ?1
                    ORDER BY rank
                    LIMIT ?2
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
                let mut stmt = conn
                    .prepare(
                        r#"
                    SELECT
                        path,
                        title,
                        snippet(notes_fts, 2, '<mark>', '</mark>', '...', 32) as snippet,
                        bm25(notes_fts, 0.0, ?2, ?3) as rank
                    FROM notes_fts
                    WHERE notes_fts MATCH ?1
                    ORDER BY rank
                    LIMIT ?4
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
