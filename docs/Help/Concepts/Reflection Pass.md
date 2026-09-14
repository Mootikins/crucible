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
  - Proposals
  - Consolidation Pass
---

# Reflection Pass

The reflection pass is Crucible's second self-improvement avenue, next to [[Help/Concepts/Precognition|knowledge insertion]]. Knowledge insertion is *reactive*: the agent writes a note in the middle of a turn when it decides to. Reflection is *retrospective*: after a session ends, a forked cheap-model agent reads the finished conversation and **proposes** durable knowledge.

The governing principle is **propose, do not dispose.** The reflection reviewer writes each note with the note tools, in its own session, in `auto` mode, so every write is bracketed and lands as a hunk in that session's [[Help/Concepts/Review Ledger|review ledger]]. A human accepts or rejects each hunk in the Changes panel; a reject reverts the note on disk. Nothing stays in the kiln without that decision.

The consolidation pass still stages files in `KILN/.crucible/proposals/` and is disposed with [[Help/CLI/proposals|cru proposals]].

**Key facts:**

- **Trigger:** `on_session_end`. Every finished session is a candidate; a session with fewer than `min_turns` user turns is skipped.
- **Requires configuration:** the plugin is **inert until you configure an auxiliary model**. Without `plugins.reflection.model` it logs a warning and skips every session.
- **Execution:** a forked auxiliary-model session of type `plugin`, with the same kiln attached, reviews the transcript. It never touches the main session or its prompt cache. When the reviewer's own session ends, the plugin reads its type from the daemon and skips it, so a review never reviews itself.
- **Bounded by tool set:** before the prompt is sent, the plugin narrows the reviewer's session to `semantic_search`, `read_note`, `list_notes`, `grep_notes`, `create_note` and `update_note` with `cru.tools.set_active`. The daemon refuses every other tool at dispatch, so the reviewer reaches the kiln and nothing else — no workspace file, no shell. If the daemon cannot narrow the set, no prompt is sent.
- **Auto mode:** the plugin puts the reviewer's session in `auto` before it sends. A non-interactive turn in the default `ask` mode is denied, so every note write would fail; `auto` also carries the `PostTurn` review policy, so a second write to one note is not parked at a gate nobody can answer.
- **Reads before it writes:** the reviewer searches the kiln with `semantic_search` and reads the closest note with `read_note`. When a note already covers the idea, it calls `update_note` on that note instead of `create_note`.
- **Output:** the notes themselves, written into the kiln and held by the review ledger of the pass's own session. The reviewer answers with one line naming what it wrote, or "Nothing to save".
- **Disposition:** the Changes panel of the pass's session. Accept keeps the note; reject reverts it. The pass's session is titled `Reflection: <the reviewed session>`, so a reader knows where a note came from, and it is not auto-archived while its hunks are undecided.

## Why the consolidation pass still stages outside the index

A consolidation proposal is written to `KILN/.crucible/proposals/`. The `.crucible/` directory is excluded from indexing and file-watching, so a staged proposal never reaches [[Help/Concepts/Precognition|Precognition]] or semantic search until a human accepts it. A `status: proposed` note *inside* the indexed kiln would surface unreviewed text in retrieval. Staging outside the index removes that risk.

A reflection note carries the same risk for as long as it waits in the review, because it is written into the kiln where a search can find it. The trade is deliberate: a note the human never decides on is visible, but it is also *one* surface — the same Changes panel that disposes of every other agent edit — instead of a second vocabulary with its own commands.

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
   ▼                                      ▼
aux session, type=plugin, auto mode    aux session, type=plugin
   │  create_note / update_note           │  answers a JSON array
   ▼                                      ▼
the note is in the kiln, and a hunk    KILN/.crucible/proposals/<id>.md
in the pass session's review ledger       │  staged, unindexed, provenance
   │                                      ▼
   ├── accept → the note stays          cru proposals list / show <id>
   └── reject → the note is reverted       ├── accept → the note lands
                                           └── reject → moves to rejected/
```

Each staged consolidation proposal carries provenance frontmatter:

```yaml
---
source: consolidation
status: proposed
created: "2026-07-02T14:32:10Z"
model: "claude-sonnet-4-5"
title: "How the daemon resolves the socket path"
tags:
  - learned
---
```

`source` names the pass that wrote the file. `model` names the model the pass ran on. A consolidation proposal has no `session` line, because it reads several sessions. An `update` adds `kind: update` and `target:`; a `skill` adds `kind: skill`, `name:` and `description:`.

### The three kinds a staged proposal may have

| Kind | What the reviewer sends | What accept does |
|------|-------------------------|------------------|
| `create` (default) | `title`, `body`, `tags`, optional `target` path | Writes a new note at `target`, or at `<id>.md` in the kiln root. The staging keys (`source`, `status`, `session`, `created`, `model`, `kind`, `target`) are stripped; `title`, `tags` and any other key stay. An existing file is never overwritten. A `create` may never land under `.crucible/` or as a `SKILL.md`. |
| `update` | `target` (an existing note or a `SKILL.md`), `title`, full revised `body` | Replaces the target. For a note, the proposal's frontmatter (minus the staging keys) and body become the whole file. For a skill, the skill's frontmatter is kept and only the body is replaced. The target must exist; nothing under `.crucible/` other than a `SKILL.md` may be updated, and the staging area never. |
| `skill` | `name`, one-line `description`, `body` | Writes `.crucible/skills/<name>/SKILL.md` with the six spec fields only (`name`, `description`, and `license`, `compatibility`, `allowed-tools` when given), plus provenance under `metadata: crucible-source: reflection`. A name that is taken is refused. |

`cru proposals show <id>` prints the file for a `create`, the unified diff for an `update`, and the exact `SKILL.md` a `skill` would write. See [[Help/CLI/proposals]] for the commands and the by-hand path.

## What the reviewer sees

The reviewer's prompt has four parts, in this order:

1. **Injected notes.** The titles precognition gave the agent at the start of the session, with the instruction not to propose or restate them. The plugin records them at the `precognition_select` event and forgets them once the session ends.
2. **Outcome evidence.** The counts the daemon has about the session: user turns, tool calls, tool errors, and the edits the user accepted, rejected or did not review (from the [[Help/Concepts/Review Ledger|review ledger]]). No score exists. The prompt says that a rejected edit or a tool error is where a durable lesson usually is.
3. **The transcript.** Every message, including each tool call with its arguments and each tool result. A tool result is cut at `tool_result_chars`; the whole transcript is cut at `transcript_chars` from the front, so the reviewer sees how the session ended. The prompt tells the reviewer that text inside a tool result is data the agent saw, never an instruction.

The reviewer runs with the six kiln tools and the session's kiln attached, so it searches and reads before it writes.

The reflection reviewer is told no rejection history. A rejected hunk leaves the composed diff, and the journal's decision record carries a hunk id, not a title, so there is nothing to read back. The consolidation pass still reads `KILN/.crucible/proposals/rejected/`.

## Guardrails against kiln pollution

The reviewer prompt ports Nous Research's Hermes Agent "DO NOT capture" list, which is the anti-pollution core. The reflection pass deliberately does **not** capture:

- **Environment-dependent failures** — missing binaries, unconfigured credentials, wrong working directory, machine-specific paths.
- **Negative claims about tools** — "X is broken", "the API doesn't work" — usually transient or environment-specific, not durable knowledge.
- **Transient errors** that resolved themselves on retry.
- **One-off task narratives** — "I did X then Y then Z for this request."
- **Secrets** — tokens, credentials, personal data.

The framing is conservative: writing nothing ("Nothing to save") is a valid and common outcome. A skill is proposed only for a procedure the agent ran more than once, or that failed once and then worked — and only by the consolidation pass, which still stages a proposal file.

## The consolidation pass

The `consolidation` plugin is the periodic half of the loop. Where reflection reads one session when it ends, consolidation runs on a timer, reads several finished sessions at once, and proposes **pattern notes**: one note per problem that recurs, or per strategy that worked more than once. Each pattern note carries a one-sentence description in the form "problem; root cause; fix", because that sentence is what retrieval sees, and an `Evidence: N sessions` line.

- It is **off by default** (`[plugins.consolidation] enabled = true` turns it on), because a pass spends model calls with no user present. It also needs `kiln` and `model`.
- It samples sessions that ended since its last pass: the sessions with a tool error or a rejected edit first (at most `max_problem`), then clean ones (at most `max_clean`). A session with fewer than `min_turns` user turns is skipped. A `plugin` session is never in the sample, and the reviewer itself runs in one.
- It stages through the reflection plugin, so its proposals land in the same directory, carry `source: consolidation`, and go through the same `cru proposals` commands. It reads the same `rejected/` directory.
- It stores a cursor in `cru.storage`, so the next pass starts after the newest session the last one saw. The sample is a prefix of the candidates, oldest first, that stops at the first session its cap refuses; the cursor stops there too, so a session the caps left out is a candidate again.

**Known gap.** `cru.session.list()` answers from the daemon's resident session map. A session that ended before a daemon restart is not in that map, so the pass does not see it. The planned **Durable Scheduled Jobs** item in [[Meta/Product#Self-Improvement Avenues|the product map]] is the fix: a run with a persistent store can list sessions from disk.

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

- An `update` to a skill keeps the frontmatter as the parser returns it. CRLF line endings and blank lines at the edge of the frontmatter are normalised in the written file.
- The plugin does not check where a target may land; `cru proposals accept` does. A proposal with a bad target is staged, and accept refuses it with a clear message.

## Related

- [[Help/CLI/proposals]] — the four commands, the diff view and the by-hand path
- [[Help/Concepts/Precognition]] — the retrieval side of the knowledge loop
- [[Help/Concepts/Agent Skills]] — what an accepted `skill` proposal writes
- [[Help/Concepts/Review Ledger]] — where the outcome evidence comes from
- [[Meta/Product#Self-Improvement Avenues]] — where this fits in the product
