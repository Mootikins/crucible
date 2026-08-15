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

Retrieval is **note-level**: during indexing each block of a note is embedded, and those block embeddings are averaged into a single document vector per note. Searches return matching notes, not individual paragraphs.

Under the hood, matching is an **exact cosine-similarity scan** over the `embedding` column of the kiln's SQLite database — every embedded note is scored against the query and the top results are returned. There is no approximate (ANN) index and no separate vector store; at kiln scale the exact scan is fast, and exact means recall is always 100%.

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
