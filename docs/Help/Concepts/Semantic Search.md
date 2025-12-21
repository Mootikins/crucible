---
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

This happens at the **block level** - each paragraph, heading, or list is indexed separately. This means searches return specific sections, not just whole files.

## Using Semantic Search

```bash
# Find content similar to your query
cru semantic "how do I stay focused while working?"

# Limit results
cru semantic "project planning" --limit 5
```

## When to Use It

**Semantic search** works best for:
- Exploratory queries ("notes about creativity")
- Finding connections you forgot existed
- Questions in natural language

**Text search** (`cru search`) works best for:
- Exact phrases ("meeting notes 2024")
- Known keywords ("TODO", "FIXME")
- Specific names or terms

## Block-Level Precision

Because Crucible indexes at block level, searches return the **specific paragraph** that matches, not just the file. This is especially useful in long notes.

## Configuration

Semantic search requires an embedding provider. See [[Help/Config/embedding]] for setup options including:
- Local (Ollama, FastEmbed)
- Cloud (OpenAI)

## See Also

- [[Help/CLI/search]] - Search command reference
- [[Search & Discovery]] - All search methods
- [[Help/Concepts/The Knowledge Graph]] - How links complement search
