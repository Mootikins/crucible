---
title: "Embedding & Enrichment Configuration"
description: Configure embedding providers for semantic search
tags:
  - reference
  - config
---

# Embedding & Enrichment Configuration

Semantic search, precognition, and similarity features all run through the **enrichment pipeline**. This page documents the `enrichment` table in `init.lua`.

> Previous versions used a flat top-level `embedding` section. This is no longer supported — Crucible now rejects configs containing `embedding`. Use `enrichment` with a nested `provider` table as shown below.

## Configuration Location

Add to `~/.config/crucible/init.lua`:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "fastembed",
        },
    },
})
```

The `enrichment` section has two sub-tables, both optional:

| Sub-table | Purpose |
|---|---|
| `enrichment.provider` | Which embedding backend to use + its settings |
| `enrichment.pipeline` | Pipeline tuning — one knob, `max_precognition_chars` (see below) |

Omitting the whole `enrichment` section is meaningful, though: the daemon then skips
embedding generation, and semantic search returns nothing.

## Providers

Select a provider by setting `type = "..."`. Each type has its own fields.

**Supported:** `fastembed`, `ollama`, `openai`, and `mock`. The types `cohere`,
`vertexai`, `custom` and `burn` were removed: a config that names one fails at
load with an error that lists the supported types.

### FastEmbed (default, local)

Fast local embeddings with no API key needed:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "fastembed",
            model = "bge-small-en-v1.5",  -- default
            batch_size = 32,
            -- cache_dir = "/path/to/cache"     -- optional
        },
    },
})
```

`cru models embeddings` prints the whole catalog of local models, with the
vector width, the retrieval score and what is already on disk.
`cru models embeddings use <NAME>` writes the two keys above for you. See
[[Help/CLI/models]]. Both the short name and the HuggingFace name resolve, so
`bge-small-en-v1.5` and `BAAI/bge-small-en-v1.5` name the same model.

`model`, `batch_size` and `cache_dir` are all read. The real vector dimension comes
from the model itself. The removed knobs `dimensions` and `num_threads` still load
without an error; the values are ignored.

**Advantages:** no API key, offline, free, fast for batch processing.

### Ollama

Use Ollama's embedding models locally:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "ollama",
            model = "nomic-embed-text",
            base_url = "http://localhost:11434",
            batch_size = 32,
        },
    },
})
```

**Setup:** `ollama pull nomic-embed-text`

### OpenAI

<!-- crucible:not-config — `api_key` is required, and only the reader's own environment holds it -->
```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "openai",
            api_key = os.getenv("OPENAI_API_KEY"),  -- required
            model = "text-embedding-3-small",
            -- base_url = "https://api.openai.com/v1"   -- optional
        },
    },
})
```

The model name decides the vector dimension. The removed knobs `dimensions`,
`retry_attempts` and `headers` still load without an error; the values are ignored.

### Removed types: cohere, vertexai, custom, burn

These types had a config shape and no backend. The configs are deleted. A config
that names one now fails at load; the error lists the supported types. For local
embeddings, use `fastembed`.

### Mock

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "mock",
        },
    },
})
```

Returns deterministic stub vectors. Used by tests and local dev.

## `enrichment.pipeline`

The pipeline table has one knob: `max_precognition_chars` (default 3000 — the
aggregate character budget for precognition context snippets). The removed knobs
(`worker_count`, `batch_size`, `timeout_ms`, `max_queue_size`, `retry_attempts`,
`retry_delay_ms`, `circuit_breaker_threshold`, `circuit_breaker_timeout_ms`) still
load without an error; the values are ignored.

```lua
cru.config.set({
    enrichment = {
        pipeline = {
            max_precognition_chars = 3000,
        },
    },
})
```

## Dimensions

Different models produce different vector sizes:

| Model | Dimensions |
|-------|------------|
| `BAAI/bge-small-en-v1.5` (default) | 384 |
| `nomic-embed-text-v1.5` | 768 |
| `text-embedding-3-small` | 1536 |
| `text-embedding-3-large` | 3072 |

Changing model changes the vector dimension, which makes old vectors unusable — reprocess after switching with `cru process --force`. `cru models embeddings` prints the dimension of every local model.

## Processing

Embeddings are generated during `cru process`:

```bash
cru process               # incremental
cru process --force       # regenerate all embeddings
```

## Storage

Embeddings live alongside the other daemon state in the kiln:

```
<kiln>/.crucible/crucible-sqlite.db    # notes, blocks, links, properties — and embeddings
```

Each note's embedding is stored on its row in the SQLite database; semantic search is an exact cosine scan over that column. Embeddings can be rebuilt from the markdown source with `cru process --force` — cache, not source of truth (though rebuilding re-pays the embedding provider).

## Example Configurations

### Local Development (default)

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "fastembed",
        },
    },
})
```

No setup required.

### High-Quality Local

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "ollama",
            model = "nomic-embed-text",
        },
    },
})
```

### Cloud API

<!-- crucible:not-config — `api_key` is required, and only the reader's own environment holds it -->
```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "openai",
            api_key = os.getenv("OPENAI_API_KEY"),
            model = "text-embedding-3-small",
        },
    },
})
```

`enrichment.provider` has no `batch_size` for the `openai` type, and the
`enrichment.pipeline` `batch_size` field is currently unread — there is no working
batching knob for cloud providers.

## Troubleshooting

### "Embedding service unavailable"

For Ollama, check it's running: `ollama list`.

### Slow processing

Switch to FastEmbed (local, no network). `batch_size` only affects the `ollama` provider
type — it is ignored by the others.

### Out of memory

For the `ollama` provider, decrease its `batch_size`.

### Switched models

Reprocess: `cru process --force`.
