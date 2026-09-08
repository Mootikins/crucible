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
which one this daemon uses, which ones are on disk, and how to change the
choice. The choice is the daemon's, not a kiln's: one enrichment provider
serves every kiln the daemon holds.

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

The default model is `bge-small-en-v1.5`. It is the smallest model Crucible
recommends. A larger model retrieves better and costs more CPU time for each
note.

## `cru models`

Lists the chat models. The command keeps its old behaviour and its old flags.

```bash
cru models
cru models -f json
```

## `cru models embeddings`

Prints one row for each model Crucible curates, plus the configured model when
it sits outside that set.

```bash
cru models embeddings
cru models embeddings -f json
```

| Column | Meaning |
|---|---|
| marks | `*` configured · `+` curated, so `download` can fetch it · `v` in the cache |
| Model | The name to write in the config file |
| Dims | The width of the vector |
| Params | The parameter count, in millions |
| Context | The longest input the model accepts, in tokens |
| MTEB | MTEB v1 English retrieval, nDCG@10, as the model authors publish it |
| Note | Why to pick this model, or why not |

The MTEB, Params and Context columns are empty for a model Crucible does not
curate. Crucible never prints a guess there.

The context column is a property of the model. fastembed truncates the input at
512 tokens today, so a model that accepts 8192 tokens still sees 512.

### The curated models

These four are the models Crucible recommends and can download.

| Model | Dims | MTEB | Why |
|---|---|---|---|
| `bge-small-en-v1.5` | 384 | 51.68 | The default. The smallest of the four. |
| `bge-base-en-v1.5` | 768 | 53.25 | It retrieves better, and it costs more CPU time. |
| `arctic-embed-m` | 768 | 54.90 | The best published score of the four. |
| `gte-base-en-v1.5` | 768 | 54.09 | A strong score, and the model accepts 8192 tokens. |

### Any other model

The set is curated, not closed. Name any model the backend supports in the
config file and it runs:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "fastembed",
            model = "BAAI/bge-large-en-v1.5",
        },
    },
})
```

Crucible resolves the name against the backend's own registry, so the name you
know works even when the registry hosts the model under a mirror. It reports
the width, and nothing else: no score, no size, no note, because Crucible has
not measured that model. `download` does not offer it, so the daemon fetches it
on first use instead.

A name that addresses two models is refused, and the refusal names both. Write
the full repository name to choose between them.

### How a stored vector records its model

A vector records `<backend>/<model>`, such as `fastembed/bge-small-en-v1.5` or
`ollama/nomic-embed-text`. A model name alone is ambiguous across backends, and
that pair is the key the block store reuses a vector by, so a vector is never
reused for a backend that did not produce it. A kiln indexed before this rule
is renamed once, on the next daemon start.

## `cru models embeddings download <NAME>`

Fetches the model into the daemon's model cache, then prints the directory and
the size. The daemon does the download, because the CLI links no ONNX runtime.
The command refuses a name the backend does not know, and names the curated
models.

```bash
cru models embeddings download arctic-embed-m
cru models embeddings -f json download arctic-embed-m
```

The download reports no progress. The daemon fetches the files, so the progress
bar goes to the daemon's log and not to your terminal. A large model takes
minutes on a slow link, and the command prints nothing until it finishes.

A download is not necessary before `use`: the daemon fetches a missing model
the first time it embeds. Download it first when you want the wait to happen
now rather than during `cru process`.

## `cru models embeddings use <NAME>`

Saves the model as a durable preference. The command resolves the name against
the daemon's catalog first, so a typo never lands in the store and an alias
lands as the canonical name; `-f json` reports the result to a script.

The write is a `config.save`: the daemon merges `enrichment.provider` into the
running store and persists it in `settings.json`, beside your `init.lua`.

**A key your `init.lua` sets is refused, by name.** Your own line outranks a
saved preference and re-applies at the next boot, so the command names the file
and the line rather than saving a value that would vanish. Edit it there:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "fastembed",
            model = "arctic-embed-m",
        },
    },
})
```

**A change of model needs a daemon restart, then a reprocess.** The running
daemon holds the config it started with and reads no file again, so a reprocess
before the restart re-embeds every note with the *old* model and reports
success. Each stored vector comes from the model that made it, so semantic
search stays wrong until you rebuild them:

```bash
cru daemon restart
cru process --force
```

A different model also gives a different vector width, which is why the old
vectors cannot be reused.

## A remote embedding service

The model stays configurable. `cru models embeddings` covers the local backend
only, because that is the backend whose files Crucible fetches. To use a remote
service, write `enrichment.provider` yourself.

Ollama, on this machine or another one:

```lua
cru.config.set({
    enrichment = {
        provider = {
            type = "ollama",
            model = "nomic-embed-text",
            base_url = "http://localhost:11434",
        },
    },
})
```

OpenAI:

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

The same reprocess rule applies: run `cru process --force` after the change.
See [[Help/Config/embedding]] for every field of each provider type.

## Troubleshooting

`cru doctor` reports the embedding backend. When it finds none, it points at
`cru models embeddings`.

A daemon that was built without the `fastembed` feature runs no local model.
`cru models embeddings` then says so, and a remote provider is the way forward.

## See Also

- [[Help/Config/embedding]] — every `enrichment` field
- [[Help/CLI/process]] — `cru process --force`
- [[Help/CLI/search]] — the semantic search these vectors serve
- [[Help/CLI/doctor]] — the backend check
