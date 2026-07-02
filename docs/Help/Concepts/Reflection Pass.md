---
title: Reflection Pass
description: Retrospective self-improvement — a forked agent reviews a finished session and proposes kiln notes for human review
status: implemented
tags:
  - concept
  - agent-learning
  - self-improvement
  - reflection
aliases:
  - Reflection
  - Proposals
---

# Reflection Pass

The reflection pass is Crucible's second self-improvement avenue, alongside [[Help/Concepts/Precognition|knowledge insertion]]. Where knowledge insertion is *reactive* — the agent writes a note mid-turn when it thinks to — reflection is *retrospective*: after a session ends, a forked cheap-model agent reviews the finished conversation and **proposes** durable notes worth keeping.

The governing principle is **propose, don't dispose.** Proposals are staged outside the live knowledge graph; a human accepts or rejects each one. Nothing is ever auto-merged.

**Key facts:**

- **Trigger:** `on_session_end`. Every finished session is eligible; trivial ones are skipped below a `min_turns` threshold.
- **Execution:** a forked auxiliary-model session (configurable, kept cheap) reviews the transcript. It never burdens the main session or its prompt cache.
- **Output:** proposed notes staged in `KILN/.crucible/proposals/`, *outside* the indexed kiln.
- **Disposition:** `cru proposals {list,show,accept,reject}` — a human decides.

## Why staging lives outside the index

Proposals are written to `KILN/.crucible/proposals/`. The `.crucible/` directory is excluded from indexing and file-watching, so a staged proposal never reaches [[Help/Concepts/Precognition|Precognition]] or semantic search until a human accepts it. Writing `status: proposed` *into* the indexed kiln would surface unreviewed notes in retrieval — staging outside the index sidesteps that entirely.

This is the deliberate correction of the removed `session-digest` feature, which auto-merged summaries via LLM-judged dedupe and risked polluting the kiln with low-value or duplicate notes.

## The proposals workflow

```
session ends
   │
   ▼
reflection plugin (on_session_end)
   │  fork cheap-model session, review transcript
   ▼
KILN/.crucible/proposals/<id>.md   ← staged, unindexed, provenance frontmatter
   │
   ▼
cru proposals list / show <id>     ← human review
   │
   ├── accept <id> → note moves into the kiln (provenance stripped) → indexed
   └── reject <id> → file deleted
```

Each staged proposal carries provenance frontmatter:

```yaml
---
source: reflection
status: proposed
session: "[[chat-20260702-1430-a1b2]]"
created: "2026-07-02T14:32:10Z"
title: "How the daemon resolves the socket path"
tags:
  - learned
---
```

`cru proposals accept` strips the `source`, `status`, `session`, and `created` keys (keeping `title`, `tags`, and any user fields), writes the note into the kiln — at the proposal's optional `target` path or the kiln root — and removes it from staging. The daemon's file watcher indexes it on its next scan.

## Guardrails against kiln pollution

The reviewer prompt ports Nous Research's Hermes Agent "DO NOT capture" list, which is the anti-pollution core. The reflection pass deliberately does **not** capture:

- **Environment-dependent failures** — missing binaries, unconfigured credentials, wrong working directory, machine-specific paths.
- **Negative claims about tools** — "X is broken", "the API doesn't work" — usually transient or environment-specific, not durable knowledge.
- **Transient errors** that resolved themselves on retry.
- **One-off task narratives** — "I did X then Y then Z for this request."
- **Secrets** — tokens, credentials, personal data.

The framing is conservative and propose-only: emitting nothing ("nothing to save") is a valid and common outcome.

## Configuration

Reflection ships as the default `reflection` runtime plugin. Configure it in `init.lua`:

```lua
require("reflection").setup({
  model = "claude-haiku-4-5-20251001",  -- cheap auxiliary model
  enabled = true,
  min_turns = 3,      -- skip trivial sessions
  max_proposals = 5,  -- cap staged notes per session
  timeout = 120,
})
```

Or via TOML:

```toml
[plugins.reflection]
model = "claude-haiku-4-5-20251001"
enabled = true
```

Because policy lives in Lua, the plugin is fully shadowable — the reviewer prompt, capture criteria, and trigger are all user-editable. The Rust runtime provides only the missing primitive (a turn-capped blocking subagent).

## Related

- [[Help/Concepts/Precognition]] — the retrieval side of the knowledge loop
- [[Help/Concepts/Agent Skills]] — reflection can draft skill notes as proposals
- [[Meta/Product#Self-Improvement Avenues]] — where this fits in the product
