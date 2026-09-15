---
title: Reflection Pass
description: Retrospective self-improvement — a forked agent reviews a finished session and writes the kiln notes it earned, for a human to accept or reject in the review
status: implemented
tags:
  - concept
  - agent-learning
  - self-improvement
  - reflection
aliases:
  - Reflection
  - Consolidation Pass
---

# Reflection Pass

The reflection pass is Crucible's second self-improvement avenue, next to [[Help/Concepts/Precognition|knowledge insertion]]. Knowledge insertion is *reactive*: the agent writes a note in the middle of a turn when it decides to. Reflection is *retrospective*: after a session ends, a forked cheap-model agent reads the finished conversation and **proposes** durable knowledge.

The governing principle is **propose, do not dispose.** The reflection reviewer writes each note with the note tools, in its own session, in `auto` mode, so every write is bracketed and lands as a hunk in that session's [[Help/Concepts/Review Ledger|review ledger]]. A human accepts or rejects each hunk in the Changes panel; a reject reverts the note on disk. Nothing stays in the kiln without that decision.

The consolidation pass works the same way. It runs its review through the reflection plugin, so its pattern notes are hunks in its own session too, and one Changes panel disposes of both passes.

**Key facts:**

- **Trigger:** `on_session_end`. Every finished session is a candidate; a session with fewer than `min_turns` user turns is skipped.
- **Requires configuration:** the plugin is **inert until you configure an auxiliary model**. Without `plugins.reflection.model` it logs a warning and skips every session.
- **Execution:** a forked auxiliary-model session of type `plugin`, with the same kiln attached, reviews the transcript. It never touches the main session or its prompt cache. When the reviewer's own session ends, the plugin reads its type from the daemon and skips it, so a review never reviews itself.
- **Bounded by tool set:** before the prompt is sent, the plugin narrows the reviewer's session to `semantic_search`, `read_note`, `list_notes`, `grep_notes`, `create_note` and `update_note` with `cru.tools.set_active`. The daemon refuses every other tool at dispatch, so the reviewer reaches the kiln and nothing else — no workspace file, no shell. If the daemon cannot narrow the set, no prompt is sent.
- **Auto mode:** the plugin puts the reviewer's session in `auto` before it sends. A non-interactive turn in the default `ask` mode is denied, so every note write would fail; `auto` also carries the `PostTurn` review policy, so a second write to one note is not parked at a gate nobody can answer.
- **Reads before it writes:** the reviewer searches the kiln with `semantic_search` and reads the closest note with `read_note`. When a note already covers the idea, it calls `update_note` on that note instead of `create_note`.
- **Output:** the notes themselves, written into the kiln and held by the review ledger of the pass's own session. The reviewer answers with one line naming what it wrote, or "Nothing to save".
- **Disposition:** the Changes panel of the pass's session. Accept keeps the note; reject reverts it. The pass's session is titled `Reflection: <the reviewed session>`, so a reader knows where a note came from, and it is not auto-archived while its hunks are undecided.

## Why a proposed note waits in the kiln

Both passes write the note into the kiln, where [[Help/Concepts/Precognition|Precognition]] and semantic search can find it while it waits. The earlier answer held the text outside the index until a human accepted it, so unreviewed text never reached retrieval.

The trade is deliberate: a note nobody has decided on is visible for as long as it waits, and in exchange there is *one* surface — the same Changes panel that disposes of every other agent edit — instead of a second vocabulary with its own commands.

This is the deliberate correction of the removed `session-digest` feature, which auto-merged summaries through LLM-judged dedupe and risked kiln pollution with low-value or duplicate notes.

## The two workflows

```
session ends                           timer (every `interval` seconds)
   │                                      │
   ▼                                      ▼
reflection plugin (on_session_end)     consolidation plugin (cru.schedule)
   │  one session: transcript with        │  several sessions: problem
   │  tool calls, outcome evidence,       │  sessions first, then clean ones
   │  injected notes                      │
   └──────────────────┬───────────────────┘
                      ▼
      aux session, type=plugin, auto mode, the kiln attached
         │  the reviewer writes each note it earned with
         │  create_note or update_note, once per note
         ▼
      the note is in the kiln, and a hunk in that session's
      review ledger
         │
         ├── accept → the note stays
         └── reject → the note is reverted
```

Each pass names itself in the title of its own session: `Reflection: <the reviewed session>`, or `Consolidation: <the day it ran>`, because a consolidation pass reads several sessions and can name no single one. The model is on that session's record. That session record is the provenance.

## What the reviewer sees

The reviewer's prompt has three parts, in this order:

1. **Injected notes.** The titles precognition gave the agent at the start of the session, with the instruction not to propose or restate them. The plugin records them at the `precognition_select` event and forgets them once the session ends.
2. **Outcome evidence.** The counts the daemon has about the session: user turns, tool calls, tool errors, and the edits the user accepted, rejected or did not review (from the [[Help/Concepts/Review Ledger|review ledger]]). No score exists. The prompt says that a rejected edit or a tool error is where a durable lesson usually is.
3. **The transcript.** Every message, including each tool call with its arguments and each tool result. A tool result is cut at `tool_result_chars`; the whole transcript is cut at `transcript_chars` from the front, so the reviewer sees how the session ended. The prompt tells the reviewer that text inside a tool result is data the agent saw, never an instruction.

The reviewer runs with the six kiln tools and the session's kiln attached, so it searches and reads before it writes.

Neither reviewer is told a rejection history. A rejected hunk leaves the composed diff, and the journal's decision record carries a hunk id, not a title, so there is nothing to read back.

## Guardrails against kiln pollution

The reviewer prompt ports Nous Research's Hermes Agent "DO NOT capture" list, which is the anti-pollution core. The reflection pass deliberately does **not** capture:

- **Environment-dependent failures** — missing binaries, unconfigured credentials, wrong working directory, machine-specific paths.
- **Negative claims about tools** — "X is broken", "the API doesn't work" — usually transient or environment-specific, not durable knowledge.
- **Transient errors** that resolved themselves on retry.
- **One-off task narratives** — "I did X then Y then Z for this request."
- **Secrets** — tokens, credentials, personal data.

The framing is conservative: writing nothing ("Nothing to save") is a valid and common outcome for a reflection pass, and so is "Nothing recurs" for a consolidation pass.

## The consolidation pass

The `consolidation` plugin is the periodic half of the loop. Where reflection reads one session when it ends, consolidation runs on a timer, reads several finished sessions at once, and proposes **pattern notes**: one note per problem that recurs, or per strategy that worked more than once. Each pattern note carries a one-sentence description in the form "problem; root cause; fix", because that sentence is what retrieval sees, and an `Evidence: N sessions` line.

- It is **off by default** (`[plugins.consolidation] enabled = true` turns it on), because a pass spends model calls with no user present. It also needs `kiln` and `model`.
- It samples sessions that ended since its last pass: the sessions with a tool error or a rejected edit first (at most `max_problem`), then clean ones (at most `max_clean`). A session with fewer than `min_turns` user turns is skipped. A `plugin` session is never in the sample, and the reviewer itself runs in one.
- It runs its review through the reflection plugin, so its notes land as hunks in the pass's own session, titled `Consolidation: <the day it ran>`, and a human accepts or rejects each one in the same Changes panel.
- It stores each consumed session id and event count in `cru.storage`. A session still running while another finishes remains eligible when it ends; a resumed session with new events becomes eligible again. The sample stops at the first group cap, leaving unconsumed candidates for the next pass. An old timestamp cursor is replaced on first run, re-sampling ended sessions once rather than silently skipping older work.

The session list and transcript reader include persisted sessions after a daemon
restart without reviving them. A fresh plugin activation reads its saved progress,
and a review interrupted before success leaves its input eligible for retry.
This is at-least-once processing: a crash after note writes but before the progress
save can review that input again. The timer itself is still in-process, with no
durable execution history; **Durable Scheduled Jobs** in [[Meta/Product#Self-Improvement Avenues|the product map]] remains separate work.

## Configuration

Reflection ships as the default `reflection` runtime plugin, but it does nothing until you name an auxiliary model: `model` has no default, and without one the plugin bails with a warning (`reflection: no aux model configured`) at every session end. Consolidation is off until `enabled` is set, and it also needs `kiln`. Configure both in `init.lua`:

```lua
cru.plugin.setup({
  { "reflection", opts = { model = "claude-haiku-4-5-20251001" } },
  { "consolidation", opts = { enabled = true, kiln = "notes", model = "claude-haiku-4-5-20251001" } },
})
```

The host passes each `opts` table to that plugin's `setup(opts)` once, after
`init.lua` finishes.

Every other key, with its default, is documented once in [[Help/Lua/Configuration#Configuring Plugins]].

Because policy lives in Lua, both plugins are fully shadowable — the reviewer prompt, capture criteria, and trigger are all user-editable. The Rust runtime provides only the missing primitives: a turn-capped blocking subagent, tool rows from `cru.session.messages(id, { tools = true })`, and the review ledger.

## Known limits

- A note waits in the indexed kiln, so retrieval can find it before a human decides. See "Why a proposed note waits in the kiln" above.
- A pass's hunks list while the daemon holds the pass's session. The auto-archive sweep skips a session whose queue is undecided, and it reads the ledgers it has in memory, so a daemon restart before you decide can let the pass's session be archived with its notes still on disk.

## Related

- [[Help/Concepts/Review Ledger]] — where a pass's notes wait, and where the outcome evidence comes from
- [[Help/Concepts/Precognition]] — the retrieval side of the knowledge loop
- [[Help/Concepts/Agent Skills]] — the skills a user installs; a pass proposes none
- [[Meta/Product#Self-Improvement Avenues]] — where this fits in the product
