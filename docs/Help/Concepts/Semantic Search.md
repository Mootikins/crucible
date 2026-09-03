---
title: Semantic Search
description: How Crucible finds content based on meaning, not just keywords
status: implemented
tags:
  - concept
  - search
  - ai
---

# Semantic Search

Semantic search finds content based on **meaning**, not just matching words. When you search for "productivity techniques", it also finds notes about "getting things done", "focus methods", and "time management" - even if they don't contain your exact words.

## How It Works

1. **Indexing**: When you run `cru process`, Crucible reads each note and creates an "embedding" - a numerical representation of its meaning
2. **Searching**: When you search, your query is also converted to an embedding
3. **Matching**: Crucible finds notes whose embeddings are closest to your query's embedding

Crucible embeds each block of a note and stores it in `note_blocks`, one row per block with its byte span. `cru search`, the `search_vectors` RPC, the search tool and **precognition** all retrieve through that table, so a hit names the passage that matched rather than the file it sat in. A `search_vectors` row carries `block` (`span_start`, `span_end`, `kind`) and `snippet` (the block's text). A kiln indexed before the table existed has no rows yet. It falls back to whole notes, embedded once each, title first, until its next index pass; those hits carry no `block`.

Under the hood, matching is an **exact cosine-similarity scan** over the kiln's SQLite database. The scan reads `note_blocks` first, so every embedded block is scored against the query. Only a kiln with no block rows falls back to the `embedding` column of `notes`, one vector per note. There is no approximate (ANN) index and no separate vector store; at kiln scale the exact scan is fast, and exact means recall is always 100%.

## Using Semantic Search

```bash
# Find content similar to your query
cru search "how do I stay focused while working?" --type semantic

# Limit results
cru search "project planning" --type semantic --limit 5
```

Without `--type`, `cru search` combines semantic and text results.

## When to Use It

**Semantic search** works best for:
- Exploratory queries ("notes about creativity")
- Finding connections you forgot existed
- Questions in natural language

**Text search** (`cru search --type text`) works best for:
- Exact phrases ("meeting notes 2024")
- Known keywords ("TODO", "FIXME")
- Specific names or terms

## Note-Level Granularity

Results point at whole notes. Because every note carries one averaged document vector, a long note that covers many topics dilutes its own signal - focused, single-topic notes rank better. This is one more reason to keep notes atomic.

## Configuration

Semantic search requires an embedding provider. See [[Help/Config/embedding]] for setup options including:
- Local (Ollama, FastEmbed)
- Cloud (OpenAI)

## See Also

- [[Help/CLI/search]] - Search command reference
- [[Search & Discovery]] - All search methods
- [[Help/Concepts/The Knowledge Graph]] - How links complement search
