---
title: Query
description: Why Crucible has no query language, and what to use instead
status: rejected
tags:
  - query
  - search
---

> **⚠️ Not a feature, and not planned.** Crucible has no query language and no
> work in progress toward one. This note exists to record that a query DSL was
> built, explored and removed, so the next person to reach for one starts from
> the evidence rather than from scratch.

# Query System

This note used to promise a syntax — `notes where tags contains "project"` and
about eighty lines of examples in the same shape — and describe it as "the
intended design". No code ever implemented that syntax, in any form.

## What actually existed, and why it went

`crates/crucible-daemon/src/storage/sqlite/` carried an unwired query pipeline:
a graph IR, four front-end parsers (Cypher, SQL:2023 PGQ `MATCH`, a jaq-style
syntax, and SQL sugar), a SQLite renderer, and seventeen golden-SQL snapshots.
It was ~5,700 lines and no caller ever reached it — there was no `kiln.query`
RPC method, no CLI surface and no Lua binding.

It was deleted once before, in July 2026, on the grounds of "no callers", and
restored a day later because that argument was correct but not sufficient: an
unwired subsystem can still be deliberate infrastructure. The reason it is gone
now is different and stronger:

- **The renderer targeted a schema Crucible has never had.** Its default preset
  joined an `edges` table, which does not exist in any migration. Its
  Crucible-specific preset joined `entities.path`, and `entities` has no `path`
  column.
- **So both committed golden SQL strings fail to prepare** against a real kiln
  database. The part of the subsystem that "worked" was the part tested against
  golden files of SQL that cannot execute.

Wiring it up was therefore not a wiring task: it started by discarding the
renderer and all its tests — the only finished half — and re-deriving them
against `notes`/`note_links`.

## The verdict to start from, if this is revisited

Carried forward verbatim from the design note that justified the 2026-07 restore
(`docs/Meta/Ideas/Storage Agnostic Query Language.md`, itself deleted four days
after that restore and recoverable only from git history at `16504bb64^`):

> **Primary**: SQL virtual tables or CSS selectors
>
> **Verdict**: Skip DSL entirely

Four hand-written parser front-ends was the option that document argued
*against*, and it is the option that got built. A third attempt should begin
there.

## What to use instead

Everything the examples above described is already served, without a DSL:

| Want | Use |
|---|---|
| Full-text search over note bodies | `cru search`, backed by the `notes_fts` FTS5 index |
| Semantic / similarity search | the `semantic_search` agent tool and the `search_vectors` RPC |
| Backlinks and outlinks | `cru.kiln.backlinks()` / `outlinks()` / `neighbors()` in Lua, exact over the resolved-link index |
| The whole link graph | the `kiln.graph` RPC, over `NoteStore::graph_links` |
| Tag and path filtering | [[Help/Tags]] and the `Filter`/`Op` predicates the note store already takes |

## See Also

- [[Search & Discovery]] - All search methods
- [[Help/Tags]] - Tag syntax
- `docs/Meta/Plans/2026-08-11-dead-code-and-schema-migrations.md` - the deletion,
  its evidence, and the commit to revert if this is ever wanted back. A path
  rather than a wikilink: `docs/Meta/Plans/` is gitignored, and the docs-kiln
  gate resolves links only against files a commit would contain
