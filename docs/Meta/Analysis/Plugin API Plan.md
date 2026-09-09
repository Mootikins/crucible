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
   publications unless it asks for another and is allowed to. **`PluginBlockPanel`
   reads every plugin's publications with no key and is the app**, so it
   declares `app` — a review found this item would otherwise break a step-0
   consumer on the day it landed.

**Widen the step beyond the two obvious routes.** `routes/plugin.rs` exposes
six plugin routes, and command invocation is not the largest hole:

| Route | Why it matters |
|---|---|
| `POST /api/plugins/{name}/option` | reads and writes *any* plugin's settings tree; `{name}` is caller-supplied |
| `POST /api/plugins` | **installs a plugin from a git URL** |
| `DELETE /api/plugins/{name}` | removes any plugin |
| `POST /api/plugins/{name}/reload` | reloads any plugin |

Installing arbitrary code from a URL is a bigger hole than calling another
plugin's command, and it sits on the same origin behind the same cookie. The
three lifecycle routes should be `app`-only outright; `option` needs the same
per-plugin comparison as `command`.

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

## Step 2 — Graph as the forcing function

**Changed from Backlinks by review, and the review is right.** Backlinks fails
on its own merits: `GET /api/backlinks` already serves it richly (`title`,
`abs_path`, `span_start`), `cru.kiln.backlinks` returns only `{ string }` from
an exact path, and the panel additionally **writes into the open editor
buffer** — `applySuggestion` calls `updateFileContent`, a channel no daemon
endpoint and no plugin command provides and no sandboxed block could reach. So
"keep the panel until the block reaches parity" concealed a reimplementation of
the link index plus an editor-write channel that does not exist. It would also
have taught the wrong lesson: the block would be slower and poorer, and the
plan's "including if it is worse" would have read as a verdict on parameterised
reads rather than on choosing a consumer the daemon already serves.

Graph is the right one:

- `GET /api/kiln/graph` returns the **whole** edge list and `GraphPanel`
  traverses it in the browser. There is no neighbourhood endpoint anywhere —
  verified.
- `cru.kiln.neighbors(path, depth)` is scope-filtered per hop and cycle-safe,
  and is reachable only from Lua. The claim the plan misapplied to backlinks is
  true here.
- The argument is one the **user moves**: the focused note, plus a depth
  control the user turns.
- It has no editor-write leg, so parity is a rendering question rather than a
  rewrite.
- Its cost is size — which is the point. A per-move read over RPC on a large
  kiln is exactly the latency answer this plan says it wants.

1. A `graph` plugin in Luau exposing a read-only command over
   `cru.kiln.neighbors`.
2. A `GraphBlock` in TS that invokes it as the focused note and depth change.
3. Keep `GraphPanel` until the block reaches parity.

**Done when:** the block renders a real neighbourhood for the focused note at a
user-chosen depth, and the latency of a per-move RPC on a large kiln is
measured and written down — including if it is unacceptable.

### Built, and the number

`runtime/plugins/graph/` exposes `graph_neighborhood` as a command (and as a
tool). `crucible-web/web/src/components/blocks/GraphBlock.tsx` invokes it for
the focused note and re-invokes it on every move of a depth slider.
`GraphPanel` is untouched.

Two kilns, measured over the daemon socket (`plugin.run_command`), median of
ten calls after two warm-ups, on a debug build:

| Kiln | Notes | Edges | Whole graph, once | depth 1 | depth 2 | depth 3 | depth 4 |
|---|---|---|---|---|---|---|---|
| `docs/` | 138 | 670 (661 resolved) | 9 ms / 80 KiB | 4.7 ms | 9.7 ms | 14.4 ms | 20.1 ms |
| synthetic | 2 000 | 11 996 | 137 ms / 1 009 KiB | 80 ms | 160 ms | 260 ms | 325 ms |

**The verdict is: for a read this shape, the command is the wrong side of the
wire, and the reason is not the wire.** On the 2 000-note kiln one depth-1
move costs 80 ms — 58% of what fetching the *entire* graph costs, and it is
paid again on the next move, while the whole-graph fetch is paid once and
answers every move afterwards in the browser for free. The 1 009 KiB the
reduction saves is real; the daemon work it saves is zero.

**Why.** `cru.kiln.neighbors` is not a neighbourhood *query*. It reads the
whole scoped note list plus the whole `graph_links` table and then walks a BFS
in Rust (`crucible-lua/src/vault/mod.rs`, `storage/scoped_links.rs`). So a
neighbourhood costs a full graph scan, and the per-hop growth in the table is
that scan repeated: hop distance is only obtainable by asking once per hop,
because the primitive attaches no distance to what it returns.

**What it wants instead**, in the order the cost argues for:

1. **A neighbourhood the store can answer** — a recursive CTE over the link
   table, returning `(path, hops)`. That collapses `depth` scans into one
   indexed query and is the only change that makes the per-move shape
   defensible. Until it exists, the reduction is a transport reduction wearing
   the name of a storage one.
2. **A bulk edge read in `cru.kiln`.** There is none, so the block draws rings
   rather than edges: `outlinks(path)` answers for one note and re-scans the
   whole graph doing it, making an edge view N full scans.
3. **Failing both, do not put this behind a per-move RPC.** Fetch once, walk in
   the browser — which is what `GraphPanel` already does, and why keeping it
   was right.

The step still earned its keep: the command path itself is fine. `POST
/api/plugins/command` → `plugin.run_command` → Luau round-trips in single-digit
milliseconds on the small kiln, so the envelope is not the cost. The cost is
the primitive underneath it, and that is a storage change, not a plugin-API
one.

## Step 3 — mark reads, and type the parameters

Both come from the same place and should land together.

**The Rust side is already wired.** `extract_params_from_table` reads a
command's `params`, `register_plugin` schemas them through
`discovered_params_to_json_schema` → `to_input_schema`, `commands_json` ships
them, and `PluginCommand.parameters` receives them. The work is not Rust.

1. **Luau declarations.** No shipped plugin declares `params` on a command —
   every one carries a free-text `hint`. Declare them on at least one, and
   document the field.
2. **A TS consumer.** `getPluginCommands` has zero callers today.
3. **A read/write marker.** Nothing distinguishes a read from a write, so the
   permission layer cannot treat them differently and a UI cannot know which is
   safe to call speculatively.

**Budget it honestly.** The dialog is about a day. The *surface* that offers a
command as a button is not — it needs a placement and a permission answer for a
person-invoked write, which is the contract's item 7 and is unsequenced. Budget
them separately or this step's done-when slips.

**Done when:** a dialog is *generated* from a command's declared parameters for
a command the dialog code has never seen, **and** a read is distinguishable
from a write without reading the plugin's source.

## Step 4 — path scoping, not a read/write split

**Research answered this, and the answer is: do not build the mode axis.** It
was the open question; it is now closed enough to act on.

**Obsidian makes no read/write distinction and holds no permission object at
all.** A loaded plugin gets the whole `App` — vault reads, writes, the raw
filesystem adapter, and `child_process`. Their help page gives the reason
plainly: *"Obsidian cannot reliably restrict plugins to specific permissions or
access levels"*, so *"plugins inherit Obsidian's access levels."* Restricted
Mode is one binary switch, not a per-plugin scope. `app.vault.adapter` is
discouraged for **portability**, never safety, and nothing detects or blocks
it. In May 2026 they announced capability *disclosures* — network, filesystem,
clipboard — which are **declarative, not enforced, and opt-in**.

**WebExtensions is the one large enforced ecosystem, and it expresses the split
exactly once.** `clipboardRead` and `clipboardWrite` are separate strings.
`bookmarks`, `history`, `storage`, `cookies`, `topSites` and `sessions` are each
**one grant covering both read and modify**. `downloads` / `downloads.open`
splits by sub-API, not by mode. So the ecosystem that *can* enforce declined the
mode axis everywhere except the single resource where read alone is the whole
attack.

**Does Obsidian's reasoning transfer to us? The stated reason does not.** Luau
ships no `io.popen` and no `os.execute`, `cru.shell` is the only gated door, and
`modules.rs` owns `require` because lookup is import authority. There is no
`child_process` and no raw adapter: the daemon genuinely can hold the line.

**A second reason does transfer, and it is the operative one.** Obsidian starts
disclosures opt-in because thousands of plugins must migrate. Enforcement cost
is not the gate — it is every plugin, doc and test that must name the new grant.
Our enum enforces one variant of ten. A *finer* vocabulary widens that gap
before it closes it.

### What to build instead

1. **Enforce the ten variants that already exist.** `filesystem`, `kiln`,
   `config` and five others gate nothing. Declared-and-ungated is the state
   Obsidian is shipping toward and being criticised for; we are already there
   by accident.
2. **Path scoping.** Scope answers "which files", which is the question that
   actually binds. `cru.fs` has no read and no write at all, so plugins use raw
   `io.open` unscoped — kanban included. A scoped read and write is the real
   work, and it is a `cru.fs` change, not a capability change.
3. **A mode axis only where read alone is the whole attack.** We have no
   clipboard case today. If one appears, split that one and nothing else.

### The web half: do not pretend

A block is same-origin script that can call any endpoint. That is Obsidian's
position, not ours — our Lua half can be enforced and our web half cannot, yet.
So enforce on the Lua side where the daemon owns the door, and **do not print a
capability label on a web block until real isolation exists**. A label without a
gate teaches a user to trust a promise nothing keeps, which is precisely the
criticism Obsidian is now taking.

### The residual, unchanged

`Capability::Kiln` still cannot say "read the kiln, write nothing", so a block
that needs `getNote` gets `saveNote` with it. Path scoping narrows the blast
radius; it does not close this. The mitigation is the contract's invariant — a
plugin's published state stays derivable from its files — rather than a gate,
because a user editing their own note by hand is a legal move that a plugin must
survive anyway.

## Step 5 — decide delivery, then package

**Not before.** The choice is a sandboxed opaque origin with a `postMessage`
bridge, versus something else. It determines whether the plugin-facing surface
is a function library or an RPC envelope, and packaging the wrong one is the
expensive mistake available here.

Once decided: extract the safe subset, make `KanbanBlock` consume it instead of
`@/lib/api`, and let that prove sufficiency before anything third-party exists.

## On the write path, and a correction

Kanban writes by rewriting the whole file (`io.open(path, "w")`), so its
conflict story is last-write-wins with no `If-Match`. A neighbouring design for
anchored `{expect, replace}` batches is under discussion for the mobile shell,
and this plan should sit on it rather than grow a second write primitive.

**One correction to how that was first described here and in correspondence.**
The anchored batch is better than kanban's write in its *conflict unit* — an
edit that fails loudly when its anchor changed, rather than clobbering. It is
**not** better in *anchor precision*. Kanban anchors on
`"(\nstatus:%s*)[%w_-]+"` limited to the first occurrence: a leading newline, a
key, and a character class terminating the value. A bare-substring `expect`
matches inside a longer word, inside a fenced code block, and in prose — and
*under*-matches when a document holds two identical lines, which refuses the
edit rather than performing it.

So the two halves come from different places: precision from a line-anchored
pattern, detection from the anchor being re-validated at apply time. A write
primitive worth adopting needs both, and taking the batch as specified today
would trade a precision problem for a detection fix.

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
