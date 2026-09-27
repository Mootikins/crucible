//! Resolved wikilink index (`note_links` v2).
//!
//! Phase 3 of the file-tree work: links used to be stored as raw text and
//! matched fuzzily at query time, which made rename/move rewrites unsound by
//! construction (see the Phase-3 plan §2 — `[[async]]` structurally matches
//! every note whose stem is `async`). This module makes resolution a
//! **deterministic function of vault state, computed once at index time and
//! persisted per occurrence**, so:
//!
//! - backlinks are an exact `SELECT ... WHERE resolved_target = ?`;
//! - a rename rewrite splices exactly the rows that resolve to the moved
//!   note, by stored byte span;
//! - moving a note into/out of folders **converges the index no matter how
//!   the file moved** (rename RPC, DnD `fs.move`, shell `mv`): every note
//!   add/remove/title-change triggers re-resolution of all rows whose
//!   `target_key` the changed note could satisfy ([`reresolve_keys`]).
//!
//! Resolution precedence (first match wins), total and deterministic:
//! 1. exact extension-less path (`[[notes/async]]` → `notes/async.md`)
//! 2. unique title match
//! 3. unique path-suffix match: the file stem (`[[async]]`) or a partial
//!    path (`[[sub/note]]` → `a/sub/note.md`)
//! 4. ambiguous suffix (≥2 notes): deterministic winner (shortest path, then
//!    lexicographic) with `is_ambiguous = 1` — backlinks stay coherent, but
//!    rewrites skip the row and surface a warning
//! 5. no match → `resolved_target IS NULL` (dangling; never rewritten)
//!
//! Matching ignores Unicode case. Among exact path matches, a path with the
//! written case wins, so `a.md` and `A.md` stay distinct.
//!
//! `note_link_keys` holds every key a note can satisfy, folded in Rust
//! (SQLite `lower()` folds only ASCII). Resolution reads candidates by key
//! through its index instead of scanning `notes`.

use crucible_core::storage::{GraphLink, InboundLink, LinkOccurrence};
use rusqlite::{params, Connection, OptionalExtension};

/// `note_links` v2 DDL. `span_start` doubles as the per-source discriminator
/// (PK), so span-less legacy rows use negative sentinels.
pub(crate) const NOTE_LINKS_V2_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS note_links (
    source_path      TEXT NOT NULL,
    resolved_target  TEXT,
    raw_target       TEXT NOT NULL,
    target_key       TEXT NOT NULL,
    span_start       INTEGER NOT NULL,
    span_end         INTEGER NOT NULL,
    kind             INTEGER NOT NULL DEFAULT 0,
    heading_ref      TEXT,
    is_ambiguous     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (source_path, span_start),
    FOREIGN KEY (source_path) REFERENCES notes(path) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS note_links_resolved_idx ON note_links(resolved_target);
CREATE INDEX IF NOT EXISTS note_links_key_idx ON note_links(target_key);
"#;

/// Folded identity keys per note (see [`note_keys`]). Derived from `notes`.
pub(crate) const NOTE_LINK_KEYS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS note_link_keys (
    key   TEXT NOT NULL,
    path  TEXT NOT NULL,
    PRIMARY KEY (key, path),
    FOREIGN KEY (path) REFERENCES notes(path) ON DELETE CASCADE
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS note_link_keys_path_idx ON note_link_keys(path);
"#;

/// Idempotent migration: detect a v1 `note_links` (no `target_key` column),
/// drop it, and create v2. Returns `true` when existing notes need a relink
/// pass (v1 rows are raw text without spans — unrecoverable in place; the
/// kiln open path re-extracts links from the files on disk).
pub(crate) fn ensure_note_links_v2(conn: &Connection) -> rusqlite::Result<bool> {
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'note_links'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let mut needs_relink = false;
    if table_exists {
        let has_v2_shape: bool = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('note_links') WHERE name = 'target_key'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !has_v2_shape {
            // Dropping is only legitimate because `note_links` is derived: the
            // kiln open path re-extracts every link from the files on disk.
            // Asserted rather than remembered, so a future migration cannot
            // quietly point this at a canonical table.
            debug_assert!(
                crate::storage::sqlite::schema::DERIVED_TABLES.contains(&"note_links"),
                "note_links must be registered as derived before it is dropped"
            );
            conn.execute("DROP TABLE note_links", [])?;
            let notes: i64 = conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))?;
            needs_relink = notes > 0;
        }
    }

    conn.execute_batch(NOTE_LINKS_V2_SCHEMA)?;
    ensure_note_link_keys(conn)?;
    let has_fragment = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('note_links') WHERE name='heading_ref'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_fragment {
        conn.execute("ALTER TABLE note_links ADD COLUMN heading_ref TEXT", [])?;
        needs_relink = true;
    }

    // A kiln indexed before the link index existed at all (no v1 table to
    // drop) has notes but an empty note_links — and change detection never
    // reprocesses unchanged files, so the index would stay empty forever.
    // Notes-without-links means "never built". Cost of the heuristic: a kiln
    // genuinely containing zero wikilinks repeats the (parse-only) relink
    // pass on each open.
    if !needs_relink {
        let notes: i64 = conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))?;
        if notes > 0 {
            let links: i64 = conn.query_row("SELECT COUNT(*) FROM note_links", [], |r| r.get(0))?;
            needs_relink = links == 0;
        }
    }
    Ok(needs_relink)
}

/// Creates `note_link_keys` and fills it from `notes` when it is new.
/// Idempotent: an existing table is kept.
fn ensure_note_link_keys(conn: &Connection) -> rusqlite::Result<()> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'note_link_keys'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    conn.execute_batch(NOTE_LINK_KEYS_SCHEMA)?;
    if !exists {
        let notes: Vec<(String, String)> = conn
            .prepare("SELECT path, title FROM notes")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (path, title) in notes {
            write_note_keys(conn, &path, &title)?;
        }
    }
    Ok(())
}

/// Replaces the stored keys of one note. Call it after the `notes` upsert.
pub(crate) fn write_note_keys(conn: &Connection, path: &str, title: &str) -> rusqlite::Result<()> {
    delete_note_keys(conn, path)?;
    let mut insert =
        conn.prepare_cached("INSERT OR IGNORE INTO note_link_keys (key, path) VALUES (?1, ?2)")?;
    for key in note_keys(path, title) {
        insert.execute(params![key, path])?;
    }
    Ok(())
}

/// Removes the stored keys of one note. The FK cascade does the same only
/// when the connection enables `foreign_keys`.
pub(crate) fn delete_note_keys(conn: &Connection, path: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM note_link_keys WHERE path = ?1", [path])?;
    Ok(())
}

/// The resolution of one raw wikilink target against current vault state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkResolution {
    pub resolved_target: Option<String>,
    pub target_key: String,
    pub is_ambiguous: bool,
}

/// Normalized identity a raw target resolves through: lowercased,
/// fragment-free, extension-less.
pub(crate) fn target_key(raw: &str) -> String {
    let base = raw.split('#').next().unwrap_or(raw).trim();
    let base = base.strip_suffix(".md").unwrap_or(base);
    base.to_lowercase()
}

/// Resolve one raw target. Pure function of the `notes` table.
pub(crate) fn resolve_raw_target(conn: &Connection, raw: &str) -> rusqlite::Result<LinkResolution> {
    let mut stmt = conn.prepare_cached(
        "SELECT n.path, n.title FROM note_link_keys k JOIN notes n ON n.path = k.path
         WHERE k.key = ?1",
    )?;
    let candidates = stmt
        .query_map([target_key(raw)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(resolve_candidates(
        raw,
        candidates.iter().map(|(p, t)| (p.as_str(), t.as_str())),
    ))
}

/// Shared identity policy for the persisted index and a fresh filesystem query.
/// Candidate acquisition may differ; precedence and ambiguous-name handling must not.
pub(crate) fn resolve_candidates<'a>(
    raw: &str,
    candidates: impl Iterator<Item = (&'a str, &'a str)>,
) -> LinkResolution {
    let key = target_key(raw);
    let written = raw.split('#').next().unwrap_or(raw).trim();
    let mut exact = vec![];
    let mut titles = vec![];
    let mut suffixes = vec![];
    if !key.is_empty() {
        let tail = format!("/{key}");
        for (path, title) in candidates {
            let normalized = path.to_lowercase();
            let extless = normalized.strip_suffix(".md").unwrap_or(&normalized);
            if extless == key || normalized == key {
                exact.push(path);
            }
            if title.to_lowercase() == key {
                titles.push(path);
            }
            if extless.ends_with(&tail) || normalized.ends_with(&tail) {
                suffixes.push(path);
            }
        }
    }
    // The written case names one file even when another differs only in case.
    let cased = exact
        .iter()
        .copied()
        .find(|p| *p == written || p.strip_suffix(".md") == Some(written));
    let mut choices = if let Some(path) = cased {
        vec![path]
    } else if !exact.is_empty() {
        exact
    } else if titles.len() == 1 {
        titles
    } else {
        suffixes
    };
    choices.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    LinkResolution {
        resolved_target: choices.first().map(|p| (*p).to_owned()),
        target_key: key,
        is_ambiguous: choices.len() > 1,
    }
}

/// Rebuild the link rows for one source note (DELETE-by-source + INSERT,
/// same shape the v1 junction used). Occurrences without span data (legacy
/// callers that only carry `links_to`) get negative sentinel spans: still
/// resolvable/backlinkable, never spliceable.
pub(crate) fn write_links(
    conn: &Connection,
    source_path: &str,
    links: &[LinkOccurrence],
    raw_fallback: &[String],
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM note_links WHERE source_path = ?1",
        [source_path],
    )?;

    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO note_links
         (source_path, resolved_target, raw_target, target_key, span_start, span_end, kind, is_ambiguous, heading_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    if links.is_empty() {
        for (i, raw) in raw_fallback.iter().enumerate() {
            let res = resolve_raw_target(conn, raw)?;
            stmt.execute(params![
                source_path,
                res.resolved_target,
                raw,
                res.target_key,
                -(i as i64) - 1,
                -1i64,
                0i64,
                res.is_ambiguous as i64,
                None::<String>,
            ])?;
        }
        return Ok(());
    }

    for occ in links {
        let res = resolve_raw_target(conn, &occ.raw_target)?;
        stmt.execute(params![
            source_path,
            res.resolved_target,
            occ.raw_target,
            res.target_key,
            occ.span_start as i64,
            occ.span_end as i64,
            occ.is_embed as i64,
            res.is_ambiguous as i64,
            occ.heading_ref,
        ])?;
    }
    Ok(())
}

/// The keys a note at `path` (with `title`) can satisfy — the rows to
/// re-resolve when that note appears, disappears, or changes title. Every
/// `/`-bounded path suffix (with and without `.md`) and the title, folded.
pub(crate) fn note_keys(path: &str, title: &str) -> Vec<String> {
    let normalized = path.to_lowercase();
    let mut keys = vec![];
    let mut push = |key: &str| {
        if !key.is_empty() && !keys.iter().any(|k| k == key) {
            keys.push(key.to_owned());
        }
    };
    let starts = std::iter::once(0).chain(normalized.match_indices('/').map(|(i, _)| i + 1));
    for start in starts {
        let suffix = &normalized[start..];
        push(suffix.strip_suffix(".md").unwrap_or(suffix));
        push(suffix);
    }
    push(&title.to_lowercase());
    keys
}

/// Re-resolve every row whose `target_key` is in `keys` — the convergence
/// step that keeps the index deterministic across ANY note add/remove/move.
/// Returns the number of rows whose resolution changed.
pub(crate) fn reresolve_keys(conn: &Connection, keys: &[String]) -> rusqlite::Result<usize> {
    if keys.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; keys.len()].join(",");
    let sql = format!(
        "SELECT source_path, span_start, raw_target, resolved_target, is_ambiguous
         FROM note_links WHERE target_key IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(String, i64, String, Option<String>, bool)> = stmt
        .query_map(rusqlite::params_from_iter(keys.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut update = conn.prepare(
        "UPDATE note_links SET resolved_target = ?1, is_ambiguous = ?2
         WHERE source_path = ?3 AND span_start = ?4",
    )?;
    let mut changed = 0;
    for (source, span_start, raw, old_target, old_ambiguous) in rows {
        let res = resolve_raw_target(conn, &raw)?;
        if res.resolved_target != old_target || res.is_ambiguous != old_ambiguous {
            update.execute(params![
                res.resolved_target,
                res.is_ambiguous as i64,
                source,
                span_start,
            ])?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Every inbound occurrence resolving to `target_path` (rewrite input).
pub(crate) fn inbound_links(
    conn: &Connection,
    target_path: &str,
) -> rusqlite::Result<Vec<InboundLink>> {
    let mut stmt = conn.prepare(
        "SELECT source_path, span_start, span_end, raw_target, is_ambiguous
         FROM note_links WHERE resolved_target = ?1
         ORDER BY source_path, span_start",
    )?;
    let rows = stmt
        .query_map([target_path], |r| {
            Ok(InboundLink {
                source_path: r.get(0)?,
                span_start: r.get(1)?,
                span_end: r.get(2)?,
                raw_target: r.get(3)?,
                is_ambiguous: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// The whole note-link graph as deduped directed edges. Resolved rows carry
/// the `resolved_target` note path (so it joins on `notes.path`); dangling
/// rows fall back to the normalized `target_key`. Self-links (a note linking
/// to itself) are excluded. `DISTINCT` collapses the multiple occurrences of
/// the same edge (e.g. `[[async]]` and `[[notes/async]]` in one note both
/// resolving to the same file, or repeated raw-fallback rows).
pub(crate) fn graph_links(conn: &Connection) -> rusqlite::Result<Vec<GraphLink>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source_path,
                COALESCE(resolved_target, target_key) AS target,
                resolved_target IS NOT NULL AS resolved
         FROM note_links
         WHERE COALESCE(resolved_target, target_key) <> source_path
         ORDER BY source_path, target",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(GraphLink {
                source: r.get(0)?,
                target: r.get(1)?,
                resolved: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Distinct source paths with at least one link resolving to `target_path`.
pub(crate) fn backlink_sources(
    conn: &Connection,
    target_path: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source_path FROM note_links
         WHERE resolved_target = ?1 ORDER BY source_path",
    )?;
    let rows = stmt
        .query_map([target_path], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE notes (
                path TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT ''
            );
            "#,
        )
        .unwrap();
        conn.execute_batch(NOTE_LINKS_V2_SCHEMA).unwrap();
        conn.execute_batch(NOTE_LINK_KEYS_SCHEMA).unwrap();
        conn
    }

    fn add_note(conn: &Connection, path: &str, title: &str) {
        conn.execute(
            "INSERT INTO notes (path, title) VALUES (?1, ?2)",
            params![path, title],
        )
        .unwrap();
        write_note_keys(conn, path, title).unwrap();
    }

    fn remove_note(conn: &Connection, path: &str) {
        conn.execute("DELETE FROM notes WHERE path = ?1", [path])
            .unwrap();
        delete_note_keys(conn, path).unwrap();
    }

    fn occ(raw: &str, start: usize) -> LinkOccurrence {
        LinkOccurrence {
            raw_target: raw.to_string(),
            span_start: start,
            span_end: start + raw.len(),
            is_embed: false,
            heading_ref: None,
        }
    }

    #[test]
    fn canonical_candidates_agree_with_sql_for_unicode_and_after_rename() {
        let conn = mem_db();
        add_note(&conn, "notes/Été.md", "Été");
        for path in ["notes/Été.md", "archive/Été.md"] {
            let old: String = conn
                .query_row("SELECT path FROM notes", [], |r| r.get(0))
                .unwrap();
            remove_note(&conn, &old);
            add_note(&conn, path, "Été");
            let scanned = resolve_candidates("été", std::iter::once((path, "Été")));
            let stored = resolve_raw_target(&conn, "été").unwrap();
            assert_eq!(scanned.resolved_target.as_deref(), Some(path));
            assert_eq!(stored.resolved_target, scanned.resolved_target);
            assert_eq!(stored.is_ambiguous, scanned.is_ambiguous);
        }
    }

    #[test]
    fn partial_paths_resolve_by_suffix_and_follow_case() {
        let conn = mem_db();
        add_note(&conn, "a/sub/note.md", "");
        add_note(&conn, "other.md", "");
        for written in ["sub/note", "Sub/Note.md", "a/sub/note", "note"] {
            let stored = resolve_raw_target(&conn, written).unwrap();
            assert_eq!(
                stored.resolved_target.as_deref(),
                Some("a/sub/note.md"),
                "[[{written}]]"
            );
            assert!(!stored.is_ambiguous);
        }
        assert_eq!(
            resolve_raw_target(&conn, "ub/note")
                .unwrap()
                .resolved_target,
            None
        );
        add_note(&conn, "b/sub/note.md", "");
        let r = resolve_raw_target(&conn, "sub/note").unwrap();
        assert!(r.is_ambiguous, "two partial-path matches");
        add_note(&conn, "A.md", "");
        add_note(&conn, "a.md", "");
        for (written, expected) in [("a", "a.md"), ("A", "A.md"), ("a.md", "a.md")] {
            let r = resolve_raw_target(&conn, written).unwrap();
            assert_eq!(r.resolved_target.as_deref(), Some(expected), "{written}");
            assert!(!r.is_ambiguous, "{written}");
        }
    }

    #[test]
    fn non_ascii_candidates_come_from_the_key_index() {
        let conn = mem_db();
        add_note(&conn, "notes/Été.md", "Résumé");
        for i in 0..50 {
            add_note(&conn, &format!("ünïcode/{i}.md"), "");
        }
        let plan: Vec<String> = conn
            .prepare(
                "EXPLAIN QUERY PLAN SELECT n.path, n.title FROM note_link_keys k
                 JOIN notes n ON n.path = k.path WHERE k.key = ?1",
            )
            .unwrap()
            .query_map(["été"], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            plan.iter().all(|step| !step.starts_with("SCAN")),
            "{plan:?}"
        );
        assert_eq!(
            resolve_raw_target(&conn, "ÉTÉ")
                .unwrap()
                .resolved_target
                .as_deref(),
            Some("notes/Été.md")
        );
        assert_eq!(
            resolve_raw_target(&conn, "résumé")
                .unwrap()
                .resolved_target
                .as_deref(),
            Some("notes/Été.md")
        );
    }

    #[test]
    fn key_table_is_backfilled_once_from_existing_notes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE notes (path TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '');
             INSERT INTO notes (path, title) VALUES ('x/Été.md', 'Summer');",
        )
        .unwrap();
        ensure_note_links_v2(&conn).unwrap();
        assert_eq!(
            resolve_raw_target(&conn, "été")
                .unwrap()
                .resolved_target
                .as_deref(),
            Some("x/Été.md")
        );
        ensure_note_links_v2(&conn).unwrap();
        let keys: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_link_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(keys as usize, note_keys("x/Été.md", "Summer").len());
    }

    #[test]
    fn embed_fragments_survive_storage_and_v2_upgrade_requests_relink() {
        let conn = mem_db();
        add_note(&conn, "host.md", "Host");
        add_note(&conn, "boards/Tasks.base", "Tasks");
        let mut link = occ("Tasks.base", 2);
        link.is_embed = true;
        link.heading_ref = Some("Board".into());
        write_links(&conn, "host.md", &[link], &[]).unwrap();
        let stored: (String, String, i64) = conn
            .query_row(
                "SELECT resolved_target, heading_ref, kind FROM note_links",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored, ("boards/Tasks.base".into(), "Board".into(), 1));
        conn.execute("ALTER TABLE note_links DROP COLUMN heading_ref", [])
            .unwrap();
        assert!(
            ensure_note_links_v2(&conn).unwrap(),
            "old v2 links need a fragment rebuild"
        );
        assert!(
            !ensure_note_links_v2(&conn).unwrap(),
            "migration is idempotent"
        );
    }

    /// Every form a wikilink can be written in resolves to the same note
    /// (ports the coverage of the deleted fuzzy candidate matcher).
    #[test]
    fn all_written_target_forms_resolve() {
        let conn = mem_db();
        add_note(&conn, "Help/Wikilinks.md", "Wikilink Syntax");
        for written in [
            "Wikilinks",
            "wikilinks",
            "Help/Wikilinks",
            "Help/Wikilinks.md",
            "Wikilink Syntax",
            "Wikilinks#Aliases",
            "Wikilinks#^block-id",
        ] {
            let r = resolve_raw_target(&conn, written).unwrap();
            assert_eq!(
                r.resolved_target.as_deref(),
                Some("Help/Wikilinks.md"),
                "[[{written}]] must resolve"
            );
        }
        assert_eq!(
            resolve_raw_target(&conn, "Tags").unwrap().resolved_target,
            None
        );
    }

    #[test]
    fn resolution_precedence_path_title_stem() {
        let conn = mem_db();
        add_note(&conn, "notes/async.md", "Async Deep Dive");
        add_note(&conn, "guide.md", "The Guide");

        // path form beats everything
        let r = resolve_raw_target(&conn, "notes/async").unwrap();
        assert_eq!(r.resolved_target.as_deref(), Some("notes/async.md"));
        assert!(!r.is_ambiguous);
        // with extension too
        let r = resolve_raw_target(&conn, "notes/async.md").unwrap();
        assert_eq!(r.resolved_target.as_deref(), Some("notes/async.md"));
        // unique title
        let r = resolve_raw_target(&conn, "The Guide").unwrap();
        assert_eq!(r.resolved_target.as_deref(), Some("guide.md"));
        // unique stem
        let r = resolve_raw_target(&conn, "async").unwrap();
        assert_eq!(r.resolved_target.as_deref(), Some("notes/async.md"));
        // case-insensitive
        let r = resolve_raw_target(&conn, "Async").unwrap();
        assert_eq!(r.resolved_target.as_deref(), Some("notes/async.md"));
        // dangling
        let r = resolve_raw_target(&conn, "ghost").unwrap();
        assert_eq!(r.resolved_target, None);
    }

    #[test]
    fn ambiguous_stem_gets_deterministic_winner_and_flag() {
        let conn = mem_db();
        add_note(&conn, "concurrency/async.md", "");
        add_note(&conn, "rust/async.md", "");

        let r = resolve_raw_target(&conn, "async").unwrap();
        assert!(r.is_ambiguous);
        // shortest path wins; equal length → lexicographic ("concurrency/..."
        // is longer, so "rust/async.md" wins on length)
        assert_eq!(r.resolved_target.as_deref(), Some("rust/async.md"));

        // path-style is NOT ambiguous — it names one file exactly
        let r = resolve_raw_target(&conn, "rust/async").unwrap();
        assert!(!r.is_ambiguous);
        assert_eq!(r.resolved_target.as_deref(), Some("rust/async.md"));
    }

    #[test]
    fn write_links_persists_resolution_and_spans() {
        let conn = mem_db();
        add_note(&conn, "a.md", "");
        add_note(&conn, "notes/b.md", "");
        write_links(&conn, "a.md", &[occ("b", 10), occ("ghost", 30)], &[]).unwrap();

        let inbound = inbound_links(&conn, "notes/b.md").unwrap();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].source_path, "a.md");
        assert_eq!(inbound[0].span_start, 10);
        assert_eq!(inbound[0].span_end, 11);

        // dangling row exists but resolves to nothing
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM note_links WHERE resolved_target IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn raw_fallback_rows_use_negative_sentinel_spans() {
        let conn = mem_db();
        add_note(&conn, "a.md", "");
        add_note(&conn, "b.md", "");
        write_links(&conn, "a.md", &[], &["b".to_string(), "b".to_string()]).unwrap();
        let inbound = inbound_links(&conn, "b.md").unwrap();
        assert_eq!(inbound.len(), 2);
        assert!(inbound.iter().all(|l| l.span_start < 0));
    }

    /// THE folder-move invariant: a note moving into/out of a folder is a
    /// delete+create as far as the index is concerned; re-resolving by the
    /// affected keys must repoint every bare-stem link with NO text edit.
    #[test]
    fn reresolve_converges_after_move_into_folder() {
        let conn = mem_db();
        add_note(&conn, "target.md", "");
        add_note(&conn, "linker.md", "");
        write_links(&conn, "linker.md", &[occ("target", 5)], &[]).unwrap();
        assert_eq!(
            inbound_links(&conn, "target.md").unwrap().len(),
            1,
            "resolves at root"
        );

        // Simulate the move: root file disappears, same stem appears deeper.
        remove_note(&conn, "target.md");
        add_note(&conn, "archive/target.md", "");
        let changed = reresolve_keys(&conn, &note_keys("archive/target.md", "")).unwrap();
        assert_eq!(changed, 1);

        assert!(inbound_links(&conn, "target.md").unwrap().is_empty());
        let inbound = inbound_links(&conn, "archive/target.md").unwrap();
        assert_eq!(inbound.len(), 1, "bare-stem link follows the move");
        assert!(!inbound[0].is_ambiguous);
    }

    /// Moving a second same-stem note INTO the vault flips existing rows to
    /// ambiguous; moving it back out flips them back. (Adds/removes both
    /// converge — the "or out" half of the invariant.)
    #[test]
    fn reresolve_tracks_ambiguity_both_directions() {
        let conn = mem_db();
        add_note(&conn, "a/note.md", "");
        add_note(&conn, "linker.md", "");
        write_links(&conn, "linker.md", &[occ("note", 5)], &[]).unwrap();
        assert!(!inbound_links(&conn, "a/note.md").unwrap()[0].is_ambiguous);

        add_note(&conn, "b/note.md", "");
        reresolve_keys(&conn, &note_keys("b/note.md", "")).unwrap();
        let inbound = inbound_links(&conn, "a/note.md").unwrap();
        assert_eq!(inbound.len(), 1, "deterministic winner keeps a/note.md");
        assert!(inbound[0].is_ambiguous, "now flagged — rewrites must skip");

        remove_note(&conn, "b/note.md");
        reresolve_keys(&conn, &note_keys("b/note.md", "")).unwrap();
        assert!(!inbound_links(&conn, "a/note.md").unwrap()[0].is_ambiguous);
    }

    #[test]
    fn graph_links_dedupes_excludes_self_and_flags_resolution() {
        let conn = mem_db();
        add_note(&conn, "a.md", "");
        add_note(&conn, "b.md", "");
        // a → b (resolved), a → missing (dangling), a → a (self, excluded),
        // and a duplicate a → b via a second raw form that resolves the same.
        write_links(
            &conn,
            "a.md",
            &[
                occ("b", 10),
                occ("missing", 30),
                occ("a", 50),
                occ("b.md", 70),
            ],
            &[],
        )
        .unwrap();

        let mut edges = graph_links(&conn).unwrap();
        edges.sort_by(|x, y| {
            (x.source.clone(), x.target.clone()).cmp(&(y.source.clone(), y.target.clone()))
        });

        assert_eq!(edges.len(), 2, "self-link excluded, duplicate a→b deduped");

        let resolved = &edges[0];
        assert_eq!(resolved.source, "a.md");
        assert_eq!(resolved.target, "b.md", "resolved edge uses note path");
        assert!(resolved.resolved);

        let dangling = &edges[1];
        assert_eq!(dangling.source, "a.md");
        assert_eq!(dangling.target, "missing", "dangling edge uses target_key");
        assert!(!dangling.resolved);
    }

    #[test]
    fn migration_drops_v1_and_requests_relink() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE notes (path TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '');
            CREATE TABLE note_links (
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                PRIMARY KEY (source_path, target_path)
            );
            INSERT INTO notes (path) VALUES ('a.md');
            INSERT INTO note_links VALUES ('a.md', 'b');
            "#,
        )
        .unwrap();

        assert!(ensure_note_links_v2(&conn).unwrap(), "v1 + notes → relink");
        // v2 shape now present and empty
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        // Still-empty index with notes keeps requesting a relink (the pass
        // hasn't happened yet); once links exist it becomes a no-op.
        assert!(ensure_note_links_v2(&conn).unwrap());
        conn.execute(
            "INSERT INTO note_links (source_path, resolved_target, raw_target, target_key,
             span_start, span_end, kind, is_ambiguous)
             VALUES ('a.md', 'b.md', 'b', 'b', 0, 1, 'wikilink', 0)",
            [],
        )
        .unwrap();
        assert!(!ensure_note_links_v2(&conn).unwrap());
    }

    /// A kiln DB indexed before note_links existed AT ALL (no v1 table to
    /// drop) must also get the relink pass — creating the v2 table empty and
    /// never flagging left such kilns with a permanently empty link index.
    #[test]
    fn preexisting_notes_without_any_links_table_request_relink() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE notes (path TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '');
            INSERT INTO notes (path) VALUES ('a.md');
            "#,
        )
        .unwrap();
        assert!(
            ensure_note_links_v2(&conn).unwrap(),
            "notes + no index → relink"
        );
    }

    /// Fresh empty DB (new kiln): nothing to relink — the pipeline will
    /// write links as it processes notes.
    #[test]
    fn fresh_db_without_notes_needs_no_relink() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE notes (path TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '');",
        )
        .unwrap();
        assert!(!ensure_note_links_v2(&conn).unwrap());
    }
}
