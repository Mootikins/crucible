---
title: "cru models"
description: List chat models, and manage the local embedding models
tags:
  - reference
  - cli
---

# cru models

`cru models` lists the chat models the configured LLM provider offers.
`cru models embeddings` covers the local embedding models: which ones exist,
which one this kiln uses, which ones are on disk, and how to change the choice.

## Synopsis

```
cru models [-f <format>]

cru models embeddings [-f <format>]
cru models embeddings download <NAME>
cru models embeddings use <NAME>
```

## Description

The embedding model turns a note into a vector. Semantic search compares those
vectors, so the model decides how good the search is. Crucible ships a catalog
of the models the local `fastembed` backend can run. The daemon owns the
catalog and the model cache; the CLI reads both over one RPC method,
`embeddings.models`.

The default model is `bge-small-en-v1.5`. It is the smallest model with a
published retrieval score. A larger model retrieves better and costs more CPU
time for each note.

## `cru models`

Lists the chat models. The command keeps its old behaviour and its old flags.

```bash
cru models
cru models -f json
```

## `cru models embeddings`

Prints one row for each model in the catalog.

```bash
cru models embeddings
cru models embeddings -f json
```

| Column | Meaning |
|---|---|
| marks | `*` configured · `+` recommended · `v` in the cache |
| Model | The name to write in the config file |
| Dims | The width of the vector |
| Params | The parameter count, in millions |
| Context | The longest input the model accepts, in tokens |
| MTEB | MTEB v1 English retrieval, nDCG@10, as the model authors publish it |
| Note | Why to pick this model, or why not |

The MTEB column is empty for a model whose authors publish no score. Crucible
never prints a guess there.

The context column is a property of the model. fastembed truncates the input at
512 tokens today, so a model that accepts 8192 tokens still sees 512.

### The recommended models

| Model | Dims | MTEB | Why |
|---|---|---|---|
| `bge-small-en-v1.5` | 384 | 51.68 | The default. The smallest of the four. |
| `bge-base-en-v1.5` | 768 | 53.25 | It retrieves better, and it costs more CPU time. |
| `arctic-embed-m` | 768 | 54.90 | The best published score in the catalog. |
| `gte-base-en-v1.5` | 768 | 54.09 | A strong score, and the model accepts 8192 tokens. |

The catalog also holds quantised builds, whose names end in `-q`. A quantised
build makes the same vector width from a smaller file.

## `cru models embeddings download <NAME>`

Fetches the model into the daemon's model cache, then prints the directory and
the size. The daemon does the download, because the CLI links no ONNX runtime.
The command refuses a name the catalog does not hold, and names the closest
entries.

```bash
cru models embeddings download arctic-embed-m
```

A download is not necessary before `use`: the daemon fetches a missing model
the first time it embeds. Download it first when you want the wait to happen
now rather than during `cru process`.

## `cru models embeddings use <NAME>`

Writes two keys into your config file:

```toml
[enrichment.provider]
type = "fastembed"
model = "arctic-embed-m"
```

The command prints the config file path, the old model and the new one. It
writes nothing else, and it keeps the rest of the file, comments included. Use
`--config <PATH>` to write to a config file other than the default.

**A change of model needs a reprocess.** Each stored vector comes from the
model that made it. The old vectors are not the new model's vectors, so
semantic search stays wrong until you rebuild them:

```bash
cru process --force
```

A different model also gives a different vector width, which is why the old
vectors cannot be reused.

> `init.luau` out-ranks `config.toml`. If your Lua config sets
> `enrichment.provider`, that value wins over the one this command writes.
> Move the setting into the Lua config, or remove it from there.

## A remote embedding service

The model stays configurable. `cru models embeddings` covers the local backend
only, because that is the backend whose files Crucible fetches. To use a remote
service, write `[enrichment.provider]` yourself.

Ollama, on this machine or another one:

```toml
[enrichment.provider]
type = "ollama"
model = "nomic-embed-text"
base_url = "http://localhost:11434"
```

OpenAI:

```toml
[enrichment.provider]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"
model = "text-embedding-3-small"
```

The same reprocess rule applies: run `cru process --force` after the change.
See [[Help/Config/embedding]] for every field of each provider type.

## Troubleshooting

`cru doctor` reports the embedding backend. When it finds none, it points at
`cru models embeddings`.

A daemon that was built without the `fastembed` feature runs no local model.
`cru models embeddings` then says so, and a remote provider is the way forward.

## See Also

- [[Help/Config/embedding]] — every `[enrichment]` field
- [[Help/CLI/process]] — `cru process --force`
- [[Help/CLI/search]] — the semantic search these vectors serve
- [[Help/CLI/doctor]] — the backend check
