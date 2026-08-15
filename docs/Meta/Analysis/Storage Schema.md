---
title: Storage Schema
description: The kiln database's one migration ladder, which of its tables are rebuildable, and the procedure for adding a column
status: current
tags:
  - architecture
  - storage
  - migrations
aliases:
  - Migration Ladder
  - Adding a Column
---

# Storage Schema

The kiln database is `<kiln>/.crucible/crucible-sqlite.db`. Everything that
creates, alters or drops anything in it lives in one module —
`crates/crucible-daemon/src/storage/sqlite/schema/` — and is recorded in a
`schema_migrations` row. This note exists so that adding a column does not
require rediscovering why.

It did not always work that way. Until 2026-08-11 there were **four** DDL
owners running at three different points of kiln open, and only one of them
recorded anything. The consequence was not untidiness: `apply_migrations` ran
*before* the `notes` table existed, so a migration that needed to
`ALTER TABLE notes` had nowhere to live at all. The plan that fixed it —
`docs/Meta/Plans/2026-08-11-dead-code-and-schema-migrations.md` — carries the full
pre-change inventory and the evidence behind each decision.

## The ladder

`schema::apply_migrations(&Connection) -> StorageResult<MigrationOutcome>` runs
once per database open, from `SqlitePool::new`. Steps are numbered, each is
recorded, and `SCHEMA_VERSION` is the count.

| Step | What it does | DDL constant lives in |
|---|---|---|
| — | Creates `schema_migrations` itself, then repairs it with `ALTER TABLE schema_migrations ADD COLUMN binary_version` | `schema/mod.rs` |
| v1 | `entities` and `properties`, with their indexes | `SCHEMA_V1`, `schema/mod.rs` |
| v2 | Normalises and deduplicates `notes` paths | `schema/mod.rs` |
| v3 | Rebuilds `properties` without its foreign key into `entities` | `schema/mod.rs` |
| v4 | `notes` + 2 indexes, then adds `embedding_model` / `embedding_dimensions` to an older table | `NOTES_SCHEMA`, `note_store.rs` |
| v5 | `note_links` v2 + 2 indexes; drops a v1 raw-text table if it finds one | `NOTE_LINKS_V2_SCHEMA`, `link_index.rs` |
| v6 | `notes_fts` (FTS5 virtual table) | `NOTES_FTS_SCHEMA`, `fts.rs` |

Three properties are worth stating rather than inferring:

1. **The DDL constants stay next to the logic that owns each table's shape.**
   Only their *execution* moved into `schema/`. `link_index.rs` owns what a
   resolved link looks like; it should keep owning the columns.
2. **The `binary_version` `ALTER` is the one permitted unrecorded change.** It
   repairs the ledger, so it cannot be recorded in the ledger.
3. **v5 runs on every open, not only below its version.** Its DDL is
   idempotent, but its *return value* answers a per-open question — "does this
   kiln owe a relink pass?" — and a version gate would answer that once. A kiln
   whose relink was interrupted has notes and an empty link index, and change
   detection never reprocesses unchanged files, so a one-shot check would leave
   it empty forever. The cost is two `COUNT(*)`s.

`MigrationOutcome` is how a migration tells its caller something it could not
finish itself. It has exactly one field, `needs_link_reindex`, and it should
grow only when a migration genuinely cannot do its own job. `SqlitePool` holds
the outcome by value and `SqliteNoteStore::new` seeds its
`needs_link_reindex` flag from it; the kiln open path consumes the flag and
runs the relink.

## Derived, derived-but-costly, canonical

The classification everything else turns on.

- **Derived** — reconstructible from files on disk, with no external service.
- **Derived (costly)** — reconstructible only by paying an embedding provider.
- **Canonical** — the database is the only copy.

| Object | Class | Rebuilt by |
|---|---|---|
| `notes` (path, content_hash, title, tags, links_to, properties, updated_at) | derived | reparsing markdown; `kiln.open --process --force` re-walks every file |
| `notes_fts` | derived | re-reading note bodies; the kiln open path already backfills it |
| `note_links` | derived | re-extracting wikilinks — v1 rows have no spans and cannot be upgraded in place, which is why v5's drop-and-recreate is legitimate |
| `notes.embedding`, `.embedding_model`, `.embedding_dimensions` | **derived (costly)** | re-embedding every note. This column is the *only* vector store: every semantic entry point scores it with an exact cosine scan. (The former LanceDB mirror at `<kiln>/.crucible/crucible-vectors.lance` was deleted after benchmarking; an orphaned directory of that name in old kilns is dead weight and safe to remove.) |
| `properties` | **CANONICAL** | nothing. `cru.storage.set` is its only writer, carrying plugin-authored values with no on-disk source |
| `entities` | vestigial, empty | n/a — kept one release as a downgrade's foreign-key target |
| `schema_migrations` | canonical-ish | reconstructible only by guessing. v1–v3 are idempotent, so losing it is survivable but not by design |

**Exactly one live table in the kiln database holds unrecoverable data:
`properties`.** Every other live table is a cache. That is the fact the design
exploits, and the reason the code carries a registry rather than a comment:

```rust
// storage/sqlite/schema/mod.rs
pub(crate) const DERIVED_TABLES: &[&str] = &["notes", "notes_fts", "note_links"];
```

A test asserts each name is created by exactly one DDL constant, and that
`properties` is created by none of them. `link_index.rs`'s `DROP TABLE
note_links` carries a `debug_assert!` against the same list, so the property is
enforced where it is relied on rather than remembered.

Session data is **not** covered by any of this. `session.jsonl`, `meta.json`
and `review.jsonl` are canonical files on disk, outside SQLite, and a kiln-DB
migration ladder does nothing for them.

## Procedure: adding a column to a kiln table

1. **Classify the table.** Is it in `DERIVED_TABLES`?
   - **Derived** → you may `DROP` and recreate it, then set the matching
     rebuild flag on `MigrationOutcome`. Cheapest correct option; this is what
     `note_links` does. **Exception:** `notes` carries the costly embedding
     columns, so a `notes` rebuild must either preserve them via
     `ALTER`/`INSERT … SELECT`, or be a re-index that re-embeds — never a bare
     `DROP`.
   - **Canonical** (`properties`) → `ALTER TABLE … ADD COLUMN` only. Never
     `DROP`, never `DELETE`, never a rebuild. Additive-only, nullable or with a
     default, so a downgraded binary still reads the table.
2. **Add the DDL to its owning constant** — `NOTES_SCHEMA`,
   `NOTE_LINKS_V2_SCHEMA`, `NOTES_FTS_SCHEMA`, or `SCHEMA_V1` — so a fresh
   database gets the final shape in one statement.
3. **Add a migration step** `apply_migration_vN` in `schema/`, and add its
   `if current_version < N` arm. Existing databases reach the same shape by a
   different route; both routes must converge.
4. **Bump `SCHEMA_VERSION` to N.**
5. **Write the tests before the migration**, per `AGENTS.md`:
   - fresh DB → `apply_migrations` twice → version is N, no error
     (idempotence);
   - a DB seeded at version N−1 with representative rows → migrate → the rows
     are still there and the new column exists;
   - for a canonical table, a row-count assertion across the migration.
6. **State the rollback** in the commit message: what an older binary sees
   after this migration runs. Additive columns are invisible to it; a rebuilt
   derived table is repopulated on next open; a dropped canonical column is not
   recoverable and is therefore not allowed.

## Migrations that move canonical rows

Only one has ever done so — v3, which rebuilt `properties` to drop a foreign
key that made every plugin write fail. The pattern it establishes, for the next
one:

- The copy is an `INSERT … SELECT` inside a transaction, and **nothing is
  dropped until the copy has been verified**: the row count is asserted equal
  first, and a mismatch returns an error rather than proceeding.
- Use `Connection::unchecked_transaction`, not `SqlitePool::with_transaction`.
  The pool's mutex is already held by the caller and is not reentrant, and
  `apply_migrations` receives only a `&Connection`. `unchecked_transaction`
  rolls back on drop, which covers an early return and a panic alike.
- Prefer a **shape-based** idempotence guard over a version-based one where you
  can — v3 probes `pragma_foreign_key_list` — so a fresh database created by
  the new binary and a migrated old one converge on the same check.

## Related

- [[Systems]] — the `storage` system this sits inside
- [[Help/Concepts/Kilns]] — what a kiln is
- `docs/Meta/Plans/2026-08-11-dead-code-and-schema-migrations.md` — the plan
  that unified the four DDL owners. Not a wikilink: `docs/Meta/Plans/` is
  gitignored, so the docs-kiln gate would read the link as dangling
