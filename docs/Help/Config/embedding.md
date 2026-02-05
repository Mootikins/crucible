---
description: Configure embedding providers for semantic search
tags:
  - reference
  - config
---

# Embedding Configuration

Configure embedding providers for semantic search and similarity features.

## Configuration File

Add to `~/.config/crucible/config.toml`:

```toml
[embedding]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
batch_size = 32
```

## Providers

### FastEmbed (Default, Local)

Fast local embeddings with no API needed:

```toml
[embedding]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
```

**Available models:**
- `all-MiniLM-L6-v2` - Fast, good quality (default)
- `nomic-embed-text-v1.5` - Higher quality, slower

**Advantages:**
- No API key needed
- Works offline
- Fast for batch processing
- Free

### Ollama

Use Ollama's embedding models:

```toml
[embedding]
provider = "ollama"
model = "nomic-embed-text"
endpoint = "http://localhost:11434"
```

**Available models:**
- `nomic-embed-text` - Good general purpose
- `mxbai-embed-large` - Higher quality

**Setup:**
```bash
ollama pull nomic-embed-text
```

### OpenAI

Use OpenAI's embedding API:

```toml
[embedding]
provider = "openai"
model = "text-embedding-3-small"
```

**Environment variable:**
```bash
export OPENAI_API_KEY=your-api-key
```

**Available models:**
- `text-embedding-3-small` - Fast, cost-effective
- `text-embedding-3-large` - Highest quality

## Parameters

### batch_size

Number of texts to embed at once:

```toml
[embedding]
batch_size = 32
```

Larger batches are faster but use more memory.

### endpoint

Custom API endpoint:

```toml
[embedding]
endpoint = "http://localhost:11434"  # Ollama
```

## Embedding Dimensions

Different models produce different vector dimensions:

| Model | Dimensions |
|-------|------------|
| `all-MiniLM-L6-v2` | 384 |
| `nomic-embed-text-v1.5` | 768 |
| `text-embedding-3-small` | 1536 |
| `text-embedding-3-large` | 3072 |

Higher dimensions may provide better quality but use more storage.

## Processing

Embeddings are generated during `cru process`:

```bash
# Process with current config
cru process

# Force regenerate all embeddings
cru process --force
```

## Storage

Embeddings are stored in the local database (SQLite by default) at:
```
<kiln_path>/.crucible/crucible-sqlite.db
```

Changing embedding provider requires reprocessing with `--force`.

## Example Configurations

### Local Development

```toml
[embedding]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
batch_size = 32
```

No setup required.

### High Quality Local

```toml
[embedding]
provider = "ollama"
model = "nomic-embed-text"
endpoint = "http://localhost:11434"
```

Requires Ollama with model installed.

### Cloud API

```toml
[embedding]
provider = "openai"
model = "text-embedding-3-small"
batch_size = 100
```

Faster with larger batches, but costs per API call.

### Memory Constrained

```toml
[embedding]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
batch_size = 8  # Lower memory usage
```

## Troubleshooting

### "Embedding service unavailable"

For Ollama, check it's running:
```bash
ollama list
```

### Slow processing

Increase batch size:
```toml
[embedding]
batch_size = 64
```

Or use a faster model:
```toml
[embedding]
provider = "fastembed"
model = "all-MiniLM-L6-v2"
```

### Out of memory

Decrease batch size:
```toml
[embedding]
batch_size = 8
```

### Changed models

Reprocess to regenerate embeddings:
```bash
cru process --force
```

## Implementation

**Source code:** `crates/crucible-llm/src/embeddings/`

**Providers:**
- `crates/crucible-llm/src/embeddings/fastembed.rs`
- `crates/crucible-llm/src/embeddings/ollama.rs`
- `crates/crucible-llm/src/embeddings/openai.rs`

## See Also

- `:h config.llm` - LLM configuration
- `:h search` - Using semantic search
- [[Help/CLI/process]] - Processing reference
