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

## Step 1 — the identity seam — landed

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
nine routes — not six, as this first said — and command invocation is not the
largest hole:

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

### What landed, and three corrections

`routes/plugin_caller.rs` holds the identity: `App`, `Plugin(name)`, or an
extractor rejection for absent. Six routes take it — `command` and `option`
compare against the owning plugin, the three lifecycle routes are `app`-only,
and `publications` narrows a plugin caller to its own rows. `api.ts` sends
`X-Crucible-Plugin` on every request, defaulting to `app`; `usePublication` and
`KanbanBlock` send the plugin the block draws for instead.

**The TUI was never a caller here.** `chat_runner/actions.rs` calls
`DaemonClient::connect()` and speaks JSON-RPC to the daemon directly — it does
not pass through `crucible-web` at all, so it needed no change and could not
have been broken by this step. The daemon's own RPC surface has no caller
identity of any kind; it is protected by the per-uid 0700 socket and nothing
else. Anything wanting a plugin identity *there* is a separate piece of work.

**`GET /api/plugins/events` cannot carry this header.** Browsers open it with
`EventSource`, which sets no headers — the same constraint that made the auth
cookie HttpOnly rather than a bearer header. The push stream stays ungated, so
a block learns *that* another plugin republished, though not what it published.

**`GET /api/plugins`, `/commands` and `/options` stay ungated** for now: they
are enumerations the plugins panel needs whole, and gating them buys nothing
while a block can call itself `app`.

## Step 2 — Graph, built and measured. Verdict: negative.

**Done, and the answer is that a parameterised read must not sit behind a
per-move RPC while the storage primitive underneath it is a full scan.**

Built: `runtime/plugins/graph/` exposing `graph_neighborhood` as a command, and
`GraphBlock.tsx` driving it from the focused note and a depth slider.
`GraphPanel` untouched.

### The measurement

Median of 10 calls after 2 warm-ups, debug build, over the daemon socket:

| Kiln | Notes | Edges | Whole graph, once | d1 | d2 | d3 | d4 |
|---|---|---|---|---|---|---|---|
| `docs/` | 138 | 670 | 9 ms | 4.7 ms | 9.7 ms | 14.4 ms | 20.1 ms |
| synthetic | 2 000 | 11 996 | 137 ms | 80 ms | 160 ms | 260 ms | 325 ms |

A single depth-1 move on the 2 000-note kiln costs **58% of fetching the entire
graph** — and is paid again on every move, where the whole-graph fetch is paid
once and answers every later move in the browser for free.

### Why, precisely

The cause is under the command, not in it. `scoped_neighbors` calls
`visible_paths` and `store.graph_links()` — the whole scoped note list and the
whole link table — on **every invocation**. So the reduction saves a megabyte on
the wire and saves the daemon nothing.

One refinement on how this was first reported: `neighbors` does *one* scan per
call, not one per hop; its own comment says reading the table once "is what
keeps the walk to a single `graph_links` read". The `depth` multiplier comes
from the *plugin* calling it once per hop, which it must do because the
primitive returns a flat set with no hop distance attached. The verdict does not
depend on that multiplier — at depth 1, a single scan, it is already 58%.

### What this does not overturn

The contract's "reduce where the data is" rule holds as written. This read
simply has no storage primitive that honours it. What it wants, in cost order:

1. **A neighbourhood the store can answer** — a recursive CTE over the link
   table returning `(path, hops)`. One indexed query instead of a scan, and the
   only change that makes a per-move read defensible.
2. **A bulk edge read in `cru.kiln`.** There is none, so no block can draw edges
   at all: `outlinks` answers for one note and rescans to do it, making edges
   among N notes cost N scans. The graph block ships rings without edges for
   this reason.
3. **Failing both, do not put this behind a per-move RPC.** Fetch once, walk in
   the browser — which is what `GraphPanel` already does. Keeping it was right.

**This is a storage change, not a plugin-API one.** It should be sequenced on
its own rather than folded into the plugin work.

## Step 3 — mark reads, and type the parameters — landed

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

### What landed

**The marker is `effect = "read" | "write"`, per command.** Not a capability:
step 4 rejects a read/write axis in the *capability* vocabulary, and this is
not one — a capability is a grant, and this is a property of one callable.
`CommandEffect` (`crucible-lua/src/command_effect.rs`) carries it,
`commands_json` ships it, `PluginCommand.effect` receives it.

Three decisions inside that shape, each of which could have gone the other way:

- **Absent means `write`.** Every command that predates the field lands there.
  Defaulting to `read` would advertise an undeclared write as safe to press,
  costing a file; defaulting to `write` costs a question. The type derives no
  `Default`, so the choice is made once, where the absence is seen.
- **A misspelt effect refuses the load**, as a misspelt type does. A fallback
  would answer `write` to an author who wrote `raed` meaning `read`.
- **`read` means "changes nothing a user could lose"**, not "touches nothing".
  `kanban`'s republish re-derives the board and publishes it: a read. That line
  is the one worth writing down, because the other reading makes every command
  that emits an event a write.

**A command's declared types were never validated.** `validate_declared_types`
ran on tools only, so a command's `type = "array<"` became `any` in the schema
in silence. Both are checked now.

**The dialog is generated, and `web/src/lib/command-form.ts` is the whole
generator**: JSON Schema in, one of five controls per parameter out, coerced
values on the way back. `PluginCommandDialog` draws it and knows no command
name. `PluginBlockPanel` is the placement — the panel host step 0 built,
rather than a new surface. The test that proves generation invents a command
(`spectrometer_calibrate`) that exists nowhere else in the tree.

**Web only, deliberately.** The TUI reaches commands through slash
autocomplete, which carries `(name, description)` and not even `hint` today;
widening that tuple and its stories is a separate change, and a typed dialog
has no home on a command line a person is already typing arguments into.

### The verdict the dialog produced: the type vocabulary has no enum

Asked to draw `kanban_move`, the generator gives `to` a free-text box. `to` is
a column name, and its whole domain is the four columns that board has. **The
Luau type grammar has no string-literal type**, so `"todo"|"doing"` cannot be
declared, `to_json_schema` never emits `enum`, and no work in the dialog can
produce a dropdown. The same gap costs a note-path picker (`path` is `string`),
a default value, and a range.

This matters more than the dialog does. A generated form is only as good as
what the declaration can say, and the first parameter anyone would want a
control for is the first one the vocabulary cannot describe. Widening
`LuaType` — a literal type, and `enum` in the emitted schema — is the change
that makes generated forms worth having, and it is small: one primary in the
grammar, one arm in `to_json_schema`, one control here.

**A permission prompt is still absent**, as budgeted. `PluginCommandDialog`
names the place: a `write` invoked from a button is the contract's item 7, the
gate belongs on the daemon side of `POST /api/plugins/command`, and the badge
this ships is a *declaration* — shown with a title saying nothing verifies it,
because step 4's warning about labels without gates applies to this label too.

## Step 4 — path scoping, not a read/write split — landed

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

**Does Obsidian's reasoning transfer to us? The stated reason does not, and
the argument this paragraph once made for it was wrong.** It read `cru.shell`
as the only door to a process. `PluginShellPolicy::default()` blocks four
command names with an empty allow-list and never reads the arguments, so that
door was open all along; the host now ships `io.popen` and `os.execute` and
says so. What does hold is import authority: `modules.rs` owns `require`, and
`loadlib` is absent, so no plugin reaches native code.

**A second reason does transfer, and it is the operative one.** Obsidian starts
disclosures opt-in because thousands of plugins must migrate. Enforcement cost
is not the gate — it is every plugin, doc and test that must name the new grant.
Our enum enforces one variant of ten. A *finer* vocabulary widens that gap
before it closes it.

### What to build instead

1. ~~**Enforce the ten variants that already exist.**~~ **Ruled out by the
   owner** — see *What landed*. Restricting the Lua API is not in scope. The
   ten variants stay declarative, which is where Obsidian also landed.
2. **Path scoping.** Scope answers "which files", which is the question that
   actually binds. `cru.fs` has no read and no write at all, so plugins use raw
   `io.open` unscoped — kanban included. A scoped read and write is the real
   work, and it is a `cru.fs` change, not a capability change.
3. **A mode axis only where read alone is the whole attack.** We have no
   clipboard case today. If one appears, split that one and nothing else.

### The web half: do not pretend

A block is same-origin script that can call any endpoint. **Do not print a
capability label on a web block.** A label without a gate teaches a user to
trust a promise nothing keeps, which is precisely the criticism Obsidian is now
taking — and with the Lua half declarative too, there is no gate anywhere for
such a label to stand on.

### The residual, unchanged

`Capability::Kiln` still cannot say "read the kiln, write nothing", so a block
that needs `getNote` gets `saveNote` with it. Path scoping narrows the blast
radius; it does not close this. The mitigation is the contract's invariant — a
plugin's published state stays derivable from its files — rather than a gate,
because a user editing their own note by hand is a legal move that a plugin must
survive anyway.

### What landed

**The owner ruled Lua-API restriction out of scope.** In their words: *like
Neovim, restricting the Lua API is NOT in scope; only direct agent/model output
should ever have to go through validation steps.* A plugin is trusted code the
operator installed. It gets the API, the way an nvim plugin gets the editor. So
item 1 above — capability-gating the `cru.*` namespaces — was built, reviewed
and then **removed**. `CruNamespace::required_capability`, the `GRANTS_NOTHING`
list and the `Ns::func` wrap are gone.

**Capabilities stay declarative.** A manifest's `capabilities` list states what
the plugin touches. It is not a sandbox and it restricts nothing, which is where
Obsidian's May 2026 disclosures also landed. The one exception predates this
work and stays: `intercept_tools`, enforced at the tool-call seam, because
`handled` fabricates a result the model reads as the tool's own and returns
before the permission gate.

**Validation belongs on direct agent and model output**, not on plugin code.
That surface is untrusted in a way a plugin is not, and it is a separate
assessment — nothing in this plan covers it yet.

#### What was kept, and why

The same work fixed three seams that ran a plugin's code with **no plugin
context at all**. Those are ATTRIBUTION fixes, not restriction, and they survive
the removal:

- **`on_session_start` hooks.** The owner table had been written since hooks
  were owner-tagged; only the end path read it.
- **Plugin tool execution.** A tool ran under whatever context was left behind,
  so `PluginToolExecutor::execute_tool` filed one plugin's storage under
  another plugin's name.
- **Deferred callbacks** — `cru.schedule` and `cru.timer.spawn`. A detached task
  carries no context of its own.

An absent context is how the host spells the OPERATOR's authority, and it is
also what `cru.storage` keys its namespace on and what `cru.plugin.publish`
attributes by. So each was a live bug independent of capabilities:
consolidation's cursor writes from its scheduled callback have been failing
silently under `pcall` since they were written.

**`cru.fs.read` and `cru.fs.write`** stay — item 2, the real work. Plugins wrote
with raw `io.open`; these are the missing primitives, and they are confined to
the roots the host binds per plugin: every registered kiln, the plugin's own
state directory, the working directory. `mkdir`, `list`, `copy` and `remove_all`
are NOT confined — `worktree` checks a destination outside all three — so
narrowing those is a separate decision with a migration behind it. Kanban still
writes with `io.open`; the primitive is here, the migration is not.

**`system` maps to no `cru.*` namespace.** Nothing in `cru.*` answers to the
manifest's "access system information". It is a dead variant, worth deleting
rather than keeping as a name a plugin can declare and a reader can misread.

#### The measured cost of the road not taken

Enforcement was implemented far enough to price it, and the price is worth
recording: **three manifest lines across thirteen shipped plugins.** One was a
real omission — `oci` calls `cru.session.get` from `cleanup_orphans` and never
declared `agent`; the other two were test doubles that claimed isolation without
declaring `intercept_tools`. The "enforcement cost is every plugin, doc and
test" argument, at our scale, was worth three lines. That is evidence about a
road not taken, not an argument to take it: the ruling above is about scope, not
about cost.

## Step 5 — delivery decided; the build waits on a trigger

**Decided: a sandboxed iframe on an opaque origin with a `MessageChannel`
bridge. The plugin-facing surface is an RPC envelope, not a function library.**
Evidence, alternatives, the envelope, the costs and the trigger are in
[[Meta/Analysis/Plugin Web Delivery]]; the measurement is reproducible from
`scripts/spikes/plugin-bridge/`.

The latency objection is dead: a bridged call costs **under 0.1 ms** more than
a direct one, and a block costs about 9 ms to mount. A Web Worker cannot be
given an opaque origin at all, so it isolates the DOM and leaves the API open —
the wrong half.

**What follows immediately, at no cost.**

- Do not extract `@crucible/block-api` as a function library. It is a
  `call(method, params)` client over the envelope.
- Do not print a capability label on a web block. The bridge is what would make
  one true, and it is not built.
- The bridge is what keeps `web/src/pwa-options.ts`'s residual honest: plugin
  JS never runs on the app origin, so `script-src 'self'` keeps meaning what
  that file says it means.

**The build waits, and the trigger is named.** Nothing third-party exists, so
the ten days would buy a proved identity in a system with no third-party
callers. The trigger is a plugin installed through `POST /api/plugins` that
ships web assets — and the loader should *refuse* to serve them until the
bridge exists, so the trigger fires as a refusal someone reads rather than as a
memory someone has.

**Enforce before you isolate.** Step 4's item 1 — enforcing the ten capability
variants that already gate nothing — is Lua-side work the daemon can win today,
and it is worth more per day than this. Isolation without enforcement is a
truthful name for an ungated door.

Once built: extract the envelope client, make `KanbanBlock` consume it instead
of `@/lib/api`, and let that prove sufficiency. `GraphBlock` is the harder
proof — it imports the app's editor context, so it needs host methods that are
not plugin data at all.

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
