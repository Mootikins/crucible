---
title: "Embedding & Enrichment Configuration"
description: Configure embedding providers for semantic search
tags:
  - reference
  - config
---

# Embedding & Enrichment Configuration

Semantic search, precognition, and similarity features all run through the **enrichment pipeline**. This page documents the `[enrichment]` section in `config.toml`.

> Previous versions used a flat top-level `[embedding]` section. This is no longer supported — Crucible now rejects configs containing `[embedding]`. Use `[enrichment]` with a nested `provider` table as shown below.

## Configuration Location

Add to `~/.config/crucible/config.toml`:

```toml
[enrichment.provider]
type = "fastembed"
```

The `[enrichment]` section has two sub-tables, both optional:

| Sub-table | Purpose |
|---|---|
| `[enrichment.provider]` | Which embedding backend to use + its settings |
| `[enrichment.pipeline]` | Pipeline tuning — parsed, but currently only `max_precognition_chars` is read (see below) |

Omitting the whole `[enrichment]` section is meaningful, though: the daemon then skips
embedding generation, and semantic search returns nothing.

## Providers

Select a provider by setting `type = "..."`. Each type has its own fields.

**Supported at runtime:** `fastembed`, `ollama`, `openai`, and `mock`. The other types
below (`cohere`, `vertexai`, `custom`, `burn`) still parse, but the daemon refuses to
create a provider for them — see each entry.

### FastEmbed (default, local)

Fast local embeddings with no API key needed:

```toml
[enrichment.provider]
type = "fastembed"
model = "BAAI/bge-small-en-v1.5"   # default
batch_size = 32
dimensions = 384
# cache_dir = "/path/to/cache"     # optional
# num_threads = 4                  # optional (auto-detected)
```

`model` is the only field the FastEmbed backend actually reads. `batch_size`,
`cache_dir`, and `num_threads` parse but are currently ignored — the daemon hardcodes a
batch size of 32 and FastEmbed's default cache directory, and the vector dimension comes
from the model itself.

**Advantages:** no API key, offline, free, fast for batch processing.

### Ollama

Use Ollama's embedding models locally:

```toml
[enrichment.provider]
type = "ollama"
model = "nomic-embed-text"
base_url = "http://localhost:11434"
batch_size = 32
```

**Setup:** `ollama pull nomic-embed-text`

### OpenAI

```toml
[enrichment.provider]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"           # required
model = "text-embedding-3-small"
# base_url = "https://api.openai.com/v1"   # optional
# dimensions = 1536                        # optional
```

### Cohere — parses, not supported at runtime

```toml
[enrichment.provider]
type = "cohere"
api_key = "{env:COHERE_API_KEY}"           # required
model = "embed-english-v3.0"
```

The config shape parses, but the daemon has no Cohere embedding backend — creating the
provider fails with "Unsupported provider type".

### Vertex AI — parses, not supported at runtime

```toml
[enrichment.provider]
type = "vertexai"
project_id = "my-gcp-project"              # required
model = "text-embedding-004"
```

Same status as Cohere: parses, but the daemon cannot create a Vertex AI embedding
provider.

### Burn — removed

```toml
[enrichment.provider]
type = "burn"
```

The Burn backend has been removed from the daemon. The config still parses, but creating
the provider hard-errors with "Burn provider is no longer included in
crucible-daemon::llm". Use `fastembed` for local embeddings.

### Custom — parses, not supported at runtime

```toml
[enrichment.provider]
type = "custom"
base_url = "http://your-service/embed"     # required
model = "my-embedding-model"               # required
dimensions = 768                           # required
```

Intended for HTTP-based providers that aren't first-class, but no runtime backend exists
yet — creating the provider fails with "Unsupported provider type".

### Mock

```toml
[enrichment.provider]
type = "mock"
```

Returns deterministic stub vectors. Used by tests and local dev.

## `[enrichment.pipeline]`

The pipeline table parses these fields: `worker_count`, `batch_size`, `max_queue_size`,
`timeout_ms`, `retry_attempts`, `retry_delay_ms`, `circuit_breaker_threshold`,
`circuit_breaker_timeout_ms`, and `max_precognition_chars`.

**Only `max_precognition_chars` is currently read** (default 3000 — the aggregate
character budget for precognition context snippets). The other eight fields are accepted
but have no effect on the enrichment pipeline today; setting them changes nothing.

```toml
[enrichment.pipeline]
max_precognition_chars = 3000
```

## Dimensions

Different models produce different vector sizes:

| Model | Dimensions |
|-------|------------|
| `BAAI/bge-small-en-v1.5` (default) | 384 |
| `nomic-embed-text-v1.5` | 768 |
| `text-embedding-3-small` | 1536 |
| `text-embedding-3-large` | 3072 |

Changing model changes the vector dimension, which makes old vectors unusable — reprocess after switching with `cru process --force`.

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

```toml
[enrichment.provider]
type = "fastembed"
```

No setup required.

### High-Quality Local

```toml
[enrichment.provider]
type = "ollama"
model = "nomic-embed-text"
```

### Cloud API

```toml
[enrichment.provider]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"
model = "text-embedding-3-small"
```

`[enrichment.provider]` has no `batch_size` for the `openai` type, and the
`[enrichment.pipeline]` `batch_size` field is currently unread — there is no working
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
