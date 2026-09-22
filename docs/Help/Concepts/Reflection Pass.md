---
title: Reflection Pass
description: Retrospective self-improvement — an auxiliary agent reviews a finished session and proposes the kiln notes it earned, for a human to accept or reject in the Inbox
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

The reflection pass is Crucible's second self-improvement avenue, next to [[Help/Concepts/Precognition|knowledge insertion]]. Knowledge insertion is *reactive*: the agent writes a note in the middle of a turn when it decides to. Reflection is *retrospective*: after a session ends, a separate auxiliary-model agent reads the finished conversation and **proposes** durable knowledge.

The governing principle is **propose, do not dispose.** The reflection reviewer writes each note with the note tools, in its own session, in `propose` mode. In that mode a note write goes into a **proposal**, and the note on disk does not change. A human accepts or rejects the proposal in the Inbox, in the diff pane or with `cru proposal`. Until that decision, the kiln does not hold the text, so retrieval cannot find it.

The consolidation pass works the same way. It runs its review through the reflection plugin, so its pattern notes are proposals too, and the one Inbox holds the proposals of both passes.

**Key facts:**

- **Trigger:** `on_session_end`. Every finished session is a candidate; a session with fewer than `min_turns` user turns is skipped.
- **Requires configuration:** the plugin is **inert until you configure an auxiliary model**. Without `plugins.reflection.model` it logs a warning and skips every session.
- **Execution:** a separate auxiliary-model session of type `plugin`, with the same kiln attached, reviews the transcript. It never touches the main session or its prompt cache. Plugin sessions are excluded from reflection, so a reviewer is never input to another reflection pass.
- **Bounded by tool set:** before the prompt is sent, the plugin narrows the reviewer's session to `semantic_search`, `read_note`, `list_notes`, `grep_notes`, `create_note` and `update_note` with `cru.tools.set_active`. The daemon refuses every other tool at dispatch, so the reviewer reaches the kiln and nothing else — no workspace file, no shell. If the daemon cannot narrow the set, no prompt is sent.
- **Propose mode:** the plugin puts the reviewer's session in `propose` before it sends. A non-interactive turn in the default `ask` mode is denied, so every note write would fail. `propose` allows every tool, and its note writes go into a proposal, so no review gate holds a write that nobody can answer. The `mode` key selects a different mode.
- **Agent configuration:** the daemon resolves the auxiliary model and provider when it creates the pass, with the reviewer's system prompt and an explicitly empty MCP server list. No second configuration call can silently leave the pass using another model or restore external tools. The pass has no project workspace.
- **Reads before it writes:** the reviewer searches the kiln with `semantic_search` and reads the closest note with `read_note`. When a note already covers the idea, it calls `update_note` on that note instead of `create_note`.
- **Output:** one proposal for the turn of the pass. It holds each note write, with the text the file had when the reviewer read it. The reviewer answers with one line naming what it proposed, or "Nothing to save".
- **Disposition:** the proposal. Accept writes every note of the proposal through the daemon's checked write. If the note changed on disk after the pass read it, the daemon merges the two changes. A merge conflict writes no file, and the proposal holds the conflict until you resolve it. Reject keeps the proposal and its reason, and changes no file. Dismiss removes a proposal from the Inbox with no decision. The pass's session is titled `Reflection: <the reviewed session>`, and the proposal names the plugin that ran the pass as its author.

## Why a proposed note waits outside the kiln

A plugin pass and a user session attach the same kiln. If the pass wrote the note to disk, a user session could build on the text, and a later reject would revert text that the user session wrote on top of. The review ledger of one session cannot dispose of a note that other writers share. So the pass proposes, and the note on disk does not change until a human accepts.

The note therefore stays out of [[Help/Concepts/Precognition|Precognition]] and semantic search while it waits. A proposal never expires. It stays in the Inbox until you accept, reject or dismiss it, also when it is stale, conflicted or superseded.

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
      aux session, type=plugin, propose mode, the kiln attached
         │  the reviewer writes each note it earned with
         │  create_note or update_note, once per note
         ▼
      one proposal in the Inbox; the note on disk does not change
         │
         ├── accept  → the daemon writes the notes (with a merge)
         ├── reject  → no file changes; the reason is kept
         └── dismiss → no file changes; the proposal leaves the Inbox
```

Each pass names itself in the title of its own session: `Reflection: <the reviewed session>`, or `Consolidation: <the day it ran>`, because a consolidation pass reads several sessions and can name no single one. The model is on that session's record. The proposal names the session and the plugin, so that session record is the provenance.

A new proposal from the same plugin for the same note makes the older open proposal **superseded**. The older proposal stays in the Inbox, with a link to the newer one, until you dismiss it.

## What the reviewer sees

The reviewer's prompt has four parts, in this order:

1. **Rejected proposals.** The titles and reasons of the most recent rejected proposals (at most 20), newest first, from `cru.proposals.rejected`, with the instruction not to propose them again. The part is absent when no proposal was rejected.
2. **Injected notes.** The titles precognition gave the agent at the start of the session, with the instruction not to propose or restate them. The plugin records them at the `precognition_select` event and forgets them once the session ends.
3. **Outcome evidence.** The counts the daemon has about the session: user turns, tool calls, tool errors, and the edits the user accepted, rejected or did not review (from the [[Help/Concepts/Review Ledger|review ledger]]). No score exists. The prompt says that a rejected edit or a tool error is where a durable lesson usually is.
4. **The transcript.** Every message, including each tool call with its arguments and each tool result. A tool result is cut at `tool_result_chars`; the whole transcript is cut at `transcript_chars` from the front, so the reviewer sees how the session ended. The prompt tells the reviewer that text inside a tool result is data the agent saw, never an instruction.

The reviewer runs with the six kiln tools and the session's kiln attached, so it searches and reads before it writes.

Both reviewers read the rejection history, because the daemon keeps every rejected proposal with its title and its reason.

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
- It runs its review through the reflection plugin, so its notes go into a proposal from the pass's own session, titled `Consolidation: <the day it ran>`, and a human accepts or rejects it in the same Inbox.
- It stores each consumed session id and event count in `cru.storage`. A session still running while another finishes remains eligible when it ends; a resumed session with new events becomes eligible again. The sample stops at the first group cap, leaving unconsumed candidates for the next pass. An old timestamp cursor is replaced on first run, re-sampling ended sessions once rather than silently skipping older work.

The session list and transcript reader include persisted sessions after a daemon
restart without reviving them. A fresh plugin activation reads its saved progress,
and a review interrupted before success leaves its input eligible for retry.
This is at-least-once processing: a crash after the proposal but before the progress
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

Because policy lives in Lua, both plugins are fully shadowable — the reviewer prompt, capture criteria, and trigger are all user-editable. The Rust runtime provides only the missing primitives: a turn-capped blocking subagent, tool rows from `cru.session.messages(id, { tools = true })`, the `propose` write mode, `cru.proposals.rejected` and the review ledger that gives the outcome evidence.

## Known limits

- A proposed note is not in the kiln, so retrieval cannot find it before a human accepts it. See "Why a proposed note waits outside the kiln" above.
- A proposal only creates or replaces notes. A pass cannot delete or rename a note.
- A proposal never expires. Proposals that nobody decides stay in the Inbox until you dismiss them.

## Related

- [[Help/Concepts/Review Ledger]] — where the outcome evidence comes from
- [[Help/Concepts/Precognition]] — the retrieval side of the knowledge loop
- [[Help/Concepts/Agent Skills]] — the skills a user installs; a pass proposes none
- [[Meta/Product#Self-Improvement Avenues]] — where this fits in the product
