---
title: Trust and Classification
description: Controlling which providers can see which data
status: implemented
tags:
  - trust
  - security
  - configuration
  - privacy
---

# Trust and Classification

Crucible's trust system controls which LLM providers can access which data. The core idea: your private journal shouldn't get sent to a cloud API, but your public project notes are fine to share.

This is privacy by design. You label how sensitive your data is, you label how much you trust each provider, and Crucible enforces the boundary.

## Two Sides of the Coin

The system has two parts that work together:

**Trust levels** describe how much you trust a provider. Set these on your LLM providers.

**Data classifications** describe how sensitive your notes are. Set these on your kilns.

When you start a session, Crucible checks that the provider's trust level is high enough for the kiln's data classification. If it isn't, the session won't start.

## Trust Levels

Every LLM provider has a trust level. From most trusted to least:

| Level | Meaning | Typical Use |
|-------|---------|-------------|
| `local` | Runs entirely on your machine | Ollama on localhost, local embedding models |
| `cloud` | Runs on a remote server you have an account with | OpenAI, Anthropic, hosted Ollama |
| `untrusted` | Minimal trust, possibly third-party | Experimental or unknown providers |

Crucible assigns sensible defaults based on the provider type. Cloud APIs default to `cloud`. Local embedding backends default to `local`. You can override these in your config.

## Data Classifications

Every kiln has a data classification. From least sensitive to most:

| Classification | Meaning | Minimum Trust Required |
|----------------|---------|----------------------|
| `public` | Safe to share with anyone | Any provider (even `untrusted`) |
| `internal` | Not secret, but not for strangers | `cloud` or `local` |
| `confidential` | Private, sensitive content | `local` only |

If you don't set a classification, kilns default to `public`.

## How They Fit Together

The rule is simple: a provider's trust level must meet or exceed the kiln's required trust level.

```
confidential kiln  →  requires local trust    →  only local providers
internal kiln      →  requires cloud trust    →  cloud or local providers
public kiln        →  requires untrusted      →  any provider
```

A `local` provider can access everything. A `cloud` provider can access `public` and `internal` data but not `confidential`. An `untrusted` provider can only touch `public` data.

## Configuration

### Setting Trust on Providers

Add `trust_level` to any provider in your `crucible.toml`:

```toml
[llm.providers.local-llama]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"
trust_level = "local"

[llm.providers.openai]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"
# trust_level defaults to "cloud" for OpenAI
```

If you omit `trust_level`, Crucible uses the provider type's default. Most cloud APIs default to `cloud`. Local-only backends (like local embedding models) default to `local`.

### Classifying Your Kilns

Add `data_classification` to a kiln attachment in your workspace config:

```toml
[[kilns]]
path = "~/notes/public-wiki"
# data_classification defaults to "public"

[[kilns]]
path = "~/notes/work-docs"
data_classification = "internal"

[[kilns]]
path = "~/notes/personal-journal"
data_classification = "confidential"
```

## Practical Example

Say you have three kilns and two providers:

```toml
# Providers
[llm.providers.ollama]
type = "ollama"
trust_level = "local"

[llm.providers.openai]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"
# defaults to cloud trust

# Kilns
[[kilns]]
path = "~/notes/recipes"
# defaults to public

[[kilns]]
path = "~/notes/work"
data_classification = "internal"

[[kilns]]
path = "~/notes/journal"
data_classification = "confidential"
```

With this setup:

- **Ollama** (local trust) can access all three kilns
- **OpenAI** (cloud trust) can access recipes and work docs, but not your journal
- If you tried to start a session with OpenAI on your journal kiln, Crucible would refuse

Your private thoughts stay on your machine. Your work notes can go to cloud providers. Your recipes go anywhere.

## Delegation and Trust

When an agent [[Delegation|delegates]] a task to another agent, trust checks apply to the child session too. The delegated agent must have sufficient trust for the kiln's classification.

If a parent agent with `local` trust delegates to a child agent backed by a `cloud` provider, and the kiln is classified as `confidential`, the delegation fails. The child's trust level doesn't meet the kiln's requirements.

This prevents accidental data leaks through delegation chains. Even if you trust the parent agent completely, the child agent still has to earn its own access.

## Defaults and Overrides

A few things to keep in mind:

- **Kilns default to `public`.** If you don't classify a kiln, any provider can access it.
- **Providers default based on type.** Cloud APIs get `cloud` trust. Local backends get `local` trust.
- **You can override defaults.** Running Ollama on a remote server? Set its trust to `cloud`. Self-hosting an OpenAI-compatible API locally? Set it to `local`.
- **Trust is checked at session creation.** Crucible validates the match before any data flows.

## See Also

- [[Delegation]] for how trust propagates through agent chains
- [[Kilns]] for kiln basics and setup
- [[Agent Client Protocol]] for the protocol agents use to communicate
