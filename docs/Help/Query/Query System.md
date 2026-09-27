---
title: Query System
description: Obsidian Bases and search replace the retired Crucible query DSL
status: active
tags:
  - query
  - search
---

# Query System

[[Help/Query/Bases]] provides saved Obsidian `.base` queries and embedded views.
There is no separate Crucible query DSL or query cache. Earlier descriptions of
`tag:#meeting AND created:2024-01`, recency/diversity ranking and `--tag`/`--since`
search flags were never implemented. [[Help/Query/Index]] retains that history.

## What actually exists

| Want | Use |
|---|---|
| Saved filters, formulas and structured views | [[Help/Query/Bases]] and `cru base query` |
| Full-text search over note bodies | `cru search "query" --type text`, backed by the `notes_fts` FTS5 index |
| Semantic / similarity search | `cru search "query" --type semantic`, or the `semantic_search` agent tool |
| Both, merged | `cru search "query"` (the default) |
| Automatic context injection during chat | [[Help/Concepts/Precognition]] |

## Related

- [[Help/Query/Index]] — the retired query DSL
- [[Help/Concepts/Semantic Search]] — how meaning-based search works
- [[Help/CLI/search]] — search command reference
- [[Search & Discovery]] — all search methods
