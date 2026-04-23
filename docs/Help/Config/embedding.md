---
title: embedding
description: Configure embedding providers for semantic search
tags:
  - reference
  - config
---

# Embedding & Enrichment Configuration

Semantic search, precognition, and similarity features all run through the **enrichment pipeline**. This page documents the `[enrichment]` section in `crucible.toml`.

> Previous versions used a flat top-level `[embedding]` section. This is no longer supported — Crucible now rejects configs containing `[embedding]`. Use `[enrichment]` with a nested `provider` table as shown below.

## Configuration Location

Add to `~/.config/crucible/config.toml`:

```toml
[enrichment.provider]
type = "fastembed"
```

The `[enrichment]` section has two sub-tables:

| Sub-table | Purpose |
|---|---|
| `[enrichment.provider]` | Which embedding backend to use + its settings |
| `[enrichment.pipeline]` | Pipeline tuning (batch processing, chunking) |

## Providers

Select a provider by setting `type = "..."`. Each type has its own fields.

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
model = "text-embedding-3-small"
# api_key read from OPENAI_API_KEY by default
# base_url = "https://api.openai.com/v1"   # optional
# dimensions = 1536                        # optional
```

### Cohere

```toml
[enrichment.provider]
type = "cohere"
model = "embed-english-v3.0"
# api_key read from COHERE_API_KEY
```

### Vertex AI

```toml
[enrichment.provider]
type = "vertexai"
model = "text-embedding-004"
```

Requires Google Cloud credentials configured in your environment.

### Burn (GPU-accelerated local)

```toml
[enrichment.provider]
type = "burn"
```

Experimental local provider backed by the Burn ML framework.

### Custom

```toml
[enrichment.provider]
type = "custom"
endpoint = "http://your-service/embed"
```

For HTTP-based providers that aren't first-class.

### Mock

```toml
[enrichment.provider]
type = "mock"
```

Returns deterministic stub vectors. Used by tests and local dev.

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
<kiln>/.crucible/crucible-sqlite.db
```

The vector index can be rebuilt from the markdown source with `cru process --force` — it's cache, not source of truth.

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
model = "text-embedding-3-small"
batch_size = 100
```

### Memory-Constrained

```toml
[enrichment.provider]
type = "fastembed"
batch_size = 8
```

## Troubleshooting

### "Embedding service unavailable"

For Ollama, check it's running: `ollama list`.

### Slow processing

Increase `batch_size` or switch to FastEmbed (local, no network).

### Out of memory

Decrease `batch_size`.

### Switched models

Reprocess: `cru process --force`.

## Implementation

**Source code:** `crates/crucible-daemon/src/llm/embeddings/`  
**Config struct:** `crates/crucible-core/src/config/enrichment.rs`
