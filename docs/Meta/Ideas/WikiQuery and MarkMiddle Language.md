---
description: Ideas for wq (wikiquery) - jq-like tool for markdown with wikilinks
status: idea
tags:
  - query-language
  - tooling
  - toon
  - wikilinks
related:
  - "[[Help/TOON Format]]"
  - "[[Meta/Ideas/Executable Markdown and Hooks]]"
---

# WikiQuery and MarkMiddle Language

Ideas for `wq` - a query/manipulation tool for wikilink-rich markdown.

## The Tool Landscape

| Tool | Domain | What it does |
|------|--------|--------------|
| `jq` | JSON | Query, transform, filter |
| `yq` | YAML | jq for YAML |
| `tq` | TOON | jq for markdown+frontmatter |
| `wq` | Wikitext | Query across structure + prose |

## Why "wq" (WikiQuery)?

Wikilinks are the key semantic element:
- **MediaWiki** originated `[[Link]]` syntax
- **Obsidian** made it mainstream for PKM
- The linking IS the structure

Not "mq" (markdown query) because plain markdown is just text. Wikilinks add the graph semantics worth querying.

## The MarkMiddle Problem

Markdown sits between structured and unstructured:

```
┌─────────────────────────────────────────┐
│  STRUCTURED      │  MIDDLE    │  LOOSE  │
│  (JSON/YAML)     │  (TOON)    │ (prose) │
├─────────────────────────────────────────┤
│  frontmatter     │  blocks    │  text   │
│  typed fields    │  checkboxes│  prose  │
│  arrays          │  wikilinks │  flow   │
│  objects         │  code      │         │
└─────────────────────────────────────────┘
```

Queries need to cross boundaries: "find notes where `type: skill` AND body has `[!]` checkbox AND links to `[[Auth]]`"

## Query Semantics

```bash
# Frontmatter query (structured)
wq '.type == "skill"' notes/*.md

# Body query (parsed blocks)
wq '.blocks[] | select(.checkbox == "!")' notes/*.md

# Link query (graph)
wq '.links[] | select(.target == "Auth")' notes/*.md

# Combined (the MarkMiddle query)
wq 'select(.type == "skill" and any(.blocks[]; .checkbox == "!"))' notes/*.md
```

## Relation to Storage Query

| Layer | Tool | Purpose |
|-------|------|---------|
| **File manipulation** | `wq` | Transform, filter, pipe markdown files |
| **Database query** | SurrealQL | Query indexed/stored notes |
| **Agent interface** | SurrealQL | LLMs generate SQL-like queries |

wq is for shell pipelines and scripting. SurrealQL is for the indexed corpus.

They're complementary:
- `wq` works on files (like `jq` on JSON files)
- SurrealQL works on the database (like SQL on a DB)

## AST Requirements

For wq to work, need unified AST that treats:
- Frontmatter as typed fields
- Blocks as queryable nodes (checkbox, codeblock, heading, list)
- Wikilinks as first-class edges
- Inline metadata (`[key:: value]`) as block-level fields

This is what TOON parsing already provides via `crucible-parser`.

## Open Questions

1. Should wq output TOON, JSON, or both? (jq outputs JSON)
2. How to handle multi-file graph queries? (`wq` per-file vs corpus?)
3. Streaming for large vaults?

## See Also

- [[Help/TOON Format]] - The data format
- [[Meta/Ideas/Executable Markdown and Hooks]] - Tasks as queryable structure
