---
title: Plugin API Plan
description: The sequenced work to make a third-party plugin possible, with the identity seam first and the packaging decision last
type: plan
status: proposed
updated: 2026-09-09
tags:
  - meta
  - plugins
  - web
  - plan
---

# Plugin API Plan

Implements [[Meta/Analysis/The Plugin Contract]]. Tracked here rather than in
`docs/Meta/Plans/`, which is gitignored — this needs review.

**The ordering principle: build the identity seam before anything that depends
on identity, and decide the delivery mechanism before packaging a surface for
it.** Two things in the analysis argue for that order and against the obvious
one. A client package buys taxonomy rather than safety while every block is
in-tree; and if delivery turns out to be `postMessage`, the right surface is an
RPC envelope, not a function library, so packaging early is rework.

## Step 0 — done

- `GET /api/plugins/commands` — the enumeration a button needs (`e7d123246`).
- `?key=` narrowing on publications (`ddcc00030`).
- `PluginBlockPanel` — a block can be a panel, not only a fence in a note
  (`683dc215a`).

## Step 1 — the identity seam

**The problem.** `POST /api/plugins/command` reads `name` and `args` and
dispatches. No caller identity, no scoping. Today the only caller is the app,
behind auth. The moment third-party code runs on this origin it inherits the
app's session and can invoke *any* plugin's commands.

`GET /api/plugins/publications` has the same shape: it answers for every
plugin. The `?key=` narrowing is a courtesy to the caller, not a boundary.

**What to build.**

1. A per-mount identity. When `PluginBlock` mounts, it knows which plugin it is
   drawing for; that identity must reach the daemon on every call. A header is
   the cheapest carrier. It is *not* a secret yet and must not be described as
   one — see the note on theft below.
2. An axum middleware that resolves the header to a plugin name and attaches it
   to the request.
3. `plugin_run_command` refuses a command whose owning plugin is not the
   calling plugin. `commands_json` already records `plugin` per command, so the
   check is a comparison, not a new registry.
4. The same for publications: a caller identifying as plugin X reads X's
   publications unless it asks for another and is allowed to.

**The flaw a review found, and the fix.** The first draft of this step would
have built a gate whose default is open, and whose test could not see it.

Two live callers have no plugin identity and must keep working: the app itself
(`api.ts` `getProviderTargets` and `resolveWorkspace`, which invoke `oci` and
`worktree` commands from `CenterComposer`), and the TUI
(`chat_runner/actions.rs`). So "no identity" would have had to mean "allowed" —
and a block would then bypass the check by **omitting** the header rather than
forging one. The proposed test (a block asks for another plugin's command and
is refused) passes while that bypass works, and red-proofing by deleting the
check still passes it.

So the identity is three-valued, not two: `app`, a named plugin, or absent —
and **absent is refused**. The app and the TUI declare themselves as `app`.
That is what makes the header a seam rather than a decoration.

**A second identity problem, unsolved.** `BlockProps.plugin` comes from the
fence's first line, so a *note author* picks the string a block mounts under.
The mount identity is caller-supplied before any script forges anything. Until
blocks are isolated this cannot be closed; record it rather than imply the
header fixes it.

**What this deliberately does not claim.** Until blocks are isolated, a header
is forgeable by any script on the origin, so this is not a security boundary —
it is the *seam* one would attach to, plus an honest error for the accidental
case. Say that in the code, or someone will later believe it is a gate.

**Done when:** a request carrying no identity is refused; the app and the TUI
still work because they declare `app`; and a block asking for another plugin's
command is refused. Red-proof each by deletion — the omission case is the one
the first draft could not observe.

## Step 2 — Backlinks as the forcing function

Chosen over Skills deliberately. Skills would prove publish and push, which
kanban already proves. Backlinks is the first consumer whose read depends on an
argument **the user moves** — the focused note — so it forces the parameterised
read end to end.

1. A `backlinks` plugin in Luau exposing a read-only command over
   `cru.kiln.backlinks`, which exists and is reachable from nowhere else.
2. A `BacklinksBlock` in TS that invokes it as the focused note changes.
3. Keep the existing `BacklinksPanel` until the block reaches parity. Do not
   delete a working panel to prove a point.

**What it will teach, and is meant to:** whether an argument-keyed read wants a
different shape from a command; whether the focused-note argument belongs in
the call or in a viewer-scoped publication; and what a per-keystroke-ish read
costs over RPC.

**Done when:** the block renders real backlinks for the focused note, and the
comparison against the existing panel is written down — including if it is
worse.

## Step 3 — mark reads, and type the parameters

Both come from the same place and should land together.

1. A read/write marker on a command. `kanban_board` is a read; `kanban_move` is
   a write. Nothing distinguishes them, so a permission layer cannot treat them
   differently and a UI cannot know which is safe to call speculatively.
2. `parameters` crosses as opaque JSON. `signature.rs` already renders a
   declaration as JSON Schema for tools; a command's parameters come from the
   same `ToolDefinition`, so this is pointing existing machinery at an existing
   field.

**Done when:** a dialog is *generated* from a command's declared parameters
rather than hand-written, for a command the dialog code has never seen.

## Step 4 — the capability question

**Do not start this until steps 1–3 are done, and treat its premise as
unproven.** `Capability` has ten variants and is enforced in two places, both
`InterceptTools`. `filesystem`, `kiln`, `config` and five others gate nothing.
So this is not extending a working model; it is the first real use of one.

The specific gap: `Capability::Kiln` cannot say "read the kiln, write nothing".
A block that legitimately needs `getNote` gets `saveNote` with it — and could
then write kanban's ticket files directly, bypassing `kanban_move`. Same bytes,
wrong author.

Open, and to be answered with evidence rather than taste:

- Is a read/write split worth the granularity, or does it collapse the way
  Obsidian's ecosystem suggests? (Under research.)
- Does path scoping belong here, or in `cru.fs`, which today has no read and no
  write at all?
- Does a capability gate mean anything before block isolation exists?

## Step 5 — decide delivery, then package

**Not before.** The choice is a sandboxed opaque origin with a `postMessage`
bridge, versus something else. It determines whether the plugin-facing surface
is a function library or an RPC envelope, and packaging the wrong one is the
expensive mistake available here.

Once decided: extract the safe subset, make `KanbanBlock` consume it instead of
`@/lib/api`, and let that prove sufficiency before anything third-party exists.

## Not in this plan

- **Offline queueing.** Refusing offline is cheap and belongs with the first
  button; an outbox, replay and conflict resolution is separate product work
  whose failure mode is silent data loss.
- **Generated types across the boundary.** No codegen exists; types are
  hand-written on both sides and cast. Worth doing, orthogonal to this
  sequence, and cheaper before the surface is public than after.
- **Rebuilding Files, Graph, Changes.** Files needs no Lua at all. Graph
  follows Backlinks. Changes needs session-scoped publications, which is step 2
  of the union, not of this plan.

## Links

- [[Meta/Analysis/The Plugin Contract]] — the design this sequences
- [[Meta/Analysis/Oil in Documents]] — the spike that produced it
