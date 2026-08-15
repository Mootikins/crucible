---
title: Query System
description: There is no query system — see the Query index for the record, and cru search for what exists
status: rejected
tags:
  - query
  - search
---

# Query System

> **⚠️ Not a feature.** Crucible has no query system, no query language, and no
> query cache. An earlier version of this page described a metadata query
> syntax (`tag:#meeting AND created:2024-01`), multi-factor result ranking
> (recency, diversity, link density), `--tag`/`--since` CLI flags, and
> automatic query caching — none of which was ever implemented.

[[Help/Query/Index]] is the authoritative record: a query DSL was built,
explored, and removed, and that note carries the evidence and the verdict to
start from if the idea is ever revisited.

## What actually exists

| Want | Use |
|---|---|
| Full-text search over note bodies | `cru search "query" --type text`, backed by the `notes_fts` FTS5 index |
| Semantic / similarity search | `cru search "query" --type semantic`, or the `semantic_search` agent tool |
| Both, merged | `cru search "query"` (the default) |
| Automatic context injection during chat | [[Help/Concepts/Precognition]] |

## Related

- [[Help/Query/Index]] — why there is no query language
- [[Help/Concepts/Semantic Search]] — how meaning-based search works
- [[Help/CLI/search]] — search command reference
- [[Search & Discovery]] — all search methods
