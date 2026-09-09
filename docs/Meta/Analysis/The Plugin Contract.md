---
title: The Plugin Contract
description: A plugin owns data; each frontend draws it natively. The successor to the Oil-view spike, with the seam moved from a view tree to published data.
type: analysis
status: implemented
updated: 2026-09-09
tags:
  - meta
  - plugins
  - architecture
  - web
  - oil
---

# The Plugin Contract

**A plugin owns data. Each frontend draws it natively.**

That sentence is the whole design, and it replaces the one the earlier spike
tried: *a plugin declares a view, and both frontends render it*. See
[[Meta/Analysis/Oil in Documents]] for how that failed and what it taught.

## The seam was in the wrong place

The spike made **Oil the cross-frontend contract**. That put a terminal's
vocabulary between a plugin and a browser, so the browser inherited cells
instead of pixels, sixteen terminal colours instead of theme tokens, no way to
say "stack when narrow", and no gesture of any kind.

The correcting analogy is the one that motivated the spike in the first place.
A Neovim plugin adds a file tree by manipulating buffers, windows and extmarks
— editor-specific APIs, with no portability claim anywhere. nvim-tree does not
run in a browser. **The concept ports; the code never does.**

So Oil is not the plugin's interface to the frontends. Oil is one renderer's
way of drawing a plugin's data.

## Two sides, and only two

**The daemon holds executable primitives. The frontends display data and
collect input.** Everything below is a consequence.

A frontend never performs a plugin's work. It renders what the plugin knows,
takes what the user typed, and invokes a primitive. That is true of the web and
of the TUI equally, which is what stops the two drifting into different
capabilities.

### Where computation goes

Four placements, and the criteria are not a matter of taste:

| Placement | Choose it when |
|---|---|
| **Lua, published** | it needs a secret or privileged access, or it reduces a lot of data to a little |
| **Lua, invoked** | it performs a write the plugin owns |
| **TS over daemon endpoints** | the daemon already exposes what is needed |
| **TS, computed in the browser** | it is presentational, interactive, or must work offline |

The reduction case is the one most easily missed. A graph plugin wanting a
three-hop neighbourhood must not ship ten thousand edges to the browser to
compute it — `kiln.graph` hands back a flat edge list and the web traverses it
client-side today, which is the wrong shape the moment the graph is large.
`cru.kiln.neighbors(path, depth)` already exists in Lua, scope-filtered per hop
and cycle-safe, and is reachable from nowhere else. It is waiting for exactly
this.

A file tree is the opposite case: `/api/fs/list` already returns what a tree
needs, so a file-tree plugin needs **no Lua at all**.

Auth proxying is not a preference but a boundary. A credential that reaches the
browser has left the daemon, and nothing puts it back.

### What a publication is, and is not

A publication is ephemeral daemon state. It dies with the daemon and nothing
durable depends on it, so it may hold anything useful — a derived index, a hot
reduction, a proxied result. It is not a claim about where truth lives, and
publishing computed data is not a compromise of the markdown contract. SQLite
is already the same kind of thing.

The invariant that does matter sits elsewhere: **the kiln stays rebuildable
from its files.** The surface that could break that is `cru.storage`, which is
durable and per-plugin, not the publication channel.

## The contract

| Layer | Owns | Runs |
|---|---|---|
| Lua plugin | the data, and every write to it | on the daemon |
| Publication | the wire shape, opaque JSON | daemon → clients |
| TS component | how it looks in a browser | in the browser |
| Oil view | how it looks in a terminal | not yet built — see below |

**Read path.** `cru.plugin.publish("<key>", value)` stores a value the daemon
never inspects, and now fires a `publication_changed` event on the system
channel. `GET /api/plugins/publications` reads it; `GET /api/plugins/events`
pushes the invalidation. A block re-reads when told, rather than polling.

**Write path.** A frontend never edits the plugin's data. It invokes a plugin
**command** (`POST /api/plugins/command`), the plugin performs the write and
republishes, and every client redraws from the push. One description of the
state, and the plugin owns it.

**Fallback.** A plugin that publishes and ships no component still renders, as
a plain table of what it published (`GenericBlock.tsx`). This is load-bearing:
without it the ecosystem splits by surface, and a plugin that shipped no TS is
silently invisible rather than merely plain.

## The reference implementation

`runtime/plugins/kanban/` is ~230 lines of Luau that reads a folder of markdown
tickets, publishes `{columns, tickets}`, and accepts a `kanban_move` command.
It describes no layout, no colour and no widget.

`crucible-web/web/src/components/blocks/KanbanBlock.tsx` draws it with real
drag-and-drop, wrapping columns and theme tokens — three things an Oil tree
cannot express, and the reason the seam moved.

A note embeds it with a fence that names the block and carries no layout:

```
kanban/board
{ "kiln": "oil-board", "folder": "tickets" }
```

The fence, the sanitizer allowlist and the island mounting survived the rewrite
unchanged in shape (`markdown.ts`, `blocks/mount.ts`). Only what gets mounted
changed — which was the one part of the spike worth keeping.

## Two bugs this surfaced

Both were pre-existing, and both were found by using the seam rather than by
reading it.

**1. A late publish was attributed to the wrong plugin.** The kanban board
published itself as `web-search`, then as `reflection`, depending on plugin
load order. `cru.plugin.publish` captured the plugin name in its closure at
bind time — but every plugin shares one `cru` table, so the last plugin to bind
won for every caller that published *after* load, which is every command, hook
and timer.

`plugin_context.rs` had already written this failure down for `cru.storage`:

> A per-plugin rebind of the shared `cru.storage` table cannot do this work:
> all plugins share one `cru` table, so the last rebind would win for every
> late caller.

`publish` now reads the running plugin from that same context at call time.
Regression test: `a_late_publish_is_attributed_to_the_running_plugin`, which
reproduces `web-search` exactly when the fix is reverted.

**2. Command dispatch never entered the plugin's context.** The loader brackets
a plugin's body and the handler dispatcher brackets a handler, but
`run_command` called straight through. So anything reading identity at call
time — `cru.storage`'s namespace, and now publish's attribution — was filed
under whatever context was left behind. Fixed in `plugin_tools.rs`, restoring
on both paths.

## Executable primitives are one thing wearing four names

Tools, commands, slash commands, skills and workflows are the same shape: a
name, a description, typed parameters, and something that runs. They are
registered separately, enumerated separately, and invoked through separate
paths, for no reason anyone wrote down.

**Half the unification already exists and nobody noticed.** A plugin command
and a plugin tool share one `ToolDefinition`: `commands_json`
(`plugin_tools.rs`) emits `name`, `description`, `hint` and `parameters` from
the same struct the tool registry uses. The difference between them today is
which map they land in and which RPC reaches them — not what they are.

### Why this matters for the web

If executable primitives are one enumerable set with declared parameter types,
then a frontend can offer **any** of them without knowing what it is: a button,
and — when the primitive takes arguments — a generated dialog. A user pins one
to a panel; the panel needs no code per primitive.

That is not a new mechanism. `signature.rs` already renders one declaration
three ways — `to_luau`, `to_json_schema`, `to_input_schema` — and
`PluginSettings.tsx` already switches on a declared node type to build a form
that a plugin shipped after it was written gets for free. An argument dialog is
the fourth projection of the same declaration, built the same way.

### What is missing

- One registry, or at least one enumeration, spanning the four kinds.
- Skills and workflows carry no `parameters` today, so they cannot yet be
  offered with a dialog.
- A permission answer: a primitive invoked from a button is invoked by a
  *person*, not an agent, and the `ask` disposition currently has nobody to
  prompt from a Lua call (see the 2026-02-03 `cru.tools.call` row in
  [[Meta/Product Decision Log]]). A button is precisely the surface that
  could make `ask` mean what it says.

## Offline: which half is minor

Knowing you are offline is minor — a failed fetch and `navigator.onLine`
settle it, and a primitive that cannot run should grey out and say why.

Queueing one is not minor, and the distinction is worth keeping sharp. An
executable primitive runs on the daemon by definition, so offline it does not
run at all. Deferring it means an outbox, replay and a conflict answer against
a daemon that owns writes — which `docs/Meta/Architecture/Mobile Shell.md`
sequences last precisely because the failure mode is silent data loss.

So: refusing offline is cheap and should ship with the first button. Queueing
offline is a separate piece of product work and should not be smuggled in
beside it.

## What the web API must have

Derived by asking which existing panels would exercise the most surface if
rebuilt as plugins, rather than by design from first principles.

1. **Enumerate, type and mark the executable primitives.** The mechanism for a
   parameterised read already exists — `POST /api/plugins/command` returns the
   plugin's value verbatim, so a read-only command *is* an argument-keyed read,
   and `kanban_board` is one. Missing: the HTTP enumeration (now built, `GET
   /api/plugins/commands`), `parameters` crossing as a declared type rather
   than opaque JSON, and a read/write marker the permission layer can use.
2. **Scoped publications.** One shape exists — global, per plugin. A review
   index wants a key scoped to a *session*; tree expansion and graph forces
   want a key scoped to a *viewer*.
3. **A refusal a UI can act on.** `FsMoveOutcome` is the shape to copy; a
   command returns opaque JSON.
4. **Declared file access.** Kanban reads and writes with `io.open`, unscoped —
   `cru.fs` has no read and no write at all. A scoped alternative assumes
   capability enforcement that **does not exist**: `Capability` has ten
   variants and is checked in exactly two places, both `InterceptTools`.
   `filesystem`, `kiln` and `config` gate nothing today.
5. **One push channel, or a stated reason for two.** `/api/plugins/events`
   carries `publication_changed`; `/api/fs/events` is separate.
6. **A panel host** — now built. Blocks previously mounted only through the
   ```plugin fence in `MarkdownPreview`, so a plugin could contribute content
   to a document and could not contribute a panel. That blocked every
   candidate.
7. **A prompter for a person-invoked primitive**, so `ask` can mean ask.

### The first one to rebuild

**Backlinks**, not Skills. Skills would prove publish and push, which kanban
already proves, so it forces nothing new. Backlinks is the first consumer whose
read depends on an argument *the user moves* — the focused note — so it forces
item 1 end to end: enumeration, the type crossing, and the read marker.

Graph is the same shape at ten times the size and should follow it. Files is
worth rebuilding as an **API consumer** and never as a Lua plugin: `/api/fs/list`
already answers it, so it needs no Lua at all.

Never: chat (the turn loop is the product), the terminal (a duplex byte stream,
where the publication channel carries JSON snapshots), settings and plugins
(`PluginSettings.tsx` already *is* the declaration-driven surface — rebuilding
the host inside itself is circular), and canvas (every placement is "TS in the
browser", so it teaches nothing about the daemon seam).

### Types are hand-written on both sides

No ts-rs, typeshare, utoipa or openapi anywhere in the tree. Web request structs
are local derives; `api.ts` types are hand-written and cast (`as T`). The route
contract tests pin shape by assertion against a mock daemon, which a generated
client cannot consume. The cost of leaving it: a daemon field rename yields
`undefined` at a mount point no test covers.

### The strongest argument against a client package

Until the sandboxed opaque origin exists, every block is in-tree and can import
the app's own API module whatever a package exports — so extracting
`@crucible/block-api` buys taxonomy, not safety. And if the sandbox makes
delivery `postMessage`, the right surface is an **RPC envelope, not a function
library**, which makes the extraction rework. Enumeration and caller-plugin
scoping are worth doing either way, because they add the identity seam any gate
must attach to. The package should wait until delivery is decided.

## What is still not built

- **No TUI surface hosts a plugin view.** Oil is now unambiguously the TUI's
  own plugin API rather than a cross-frontend one, which makes its
  terminal-specific primitives correct rather than defective. But a Lua-built
  tree still has to cross a process boundary to reach the TUI, so it needs
  `Node: Deserialize` and an action dispatcher. See [[Meta/Analysis/Oil in Documents]], section "P1".
- **The TS side of the contract has no surface.** Three of the four placements
  above are TypeScript, and none has a client library. `KanbanBlock.tsx` can
  `import from '@/lib/api'` only because it ships in-tree; a third-party block
  has no typed endpoint wrappers and no statement of what it may call. The
  publish path works end to end and the other three do not, so the contract as
  committed is one-legged.
- **Third-party TS cannot be loaded.** A plugin bundle the daemon serves is
  `script-src 'self'`, so the CSP would admit it and protect nothing, and the
  service worker's root scope leans on that same bound. Block components ship
  in-tree until a sandboxed opaque origin and a `postMessage` bridge exist.
  That is the largest single piece of work this design implies.
- **The editor's live preview** still shows the fence as source; only the
  reading view mounts blocks.
- **Latency is unresolved for the TUI.** A file tree tolerates an RPC per
  update. A completion source or virtual text may not. The statusline already
  dodged this by keeping evaluation TUI-side with a closed vocabulary and
  "zero RPC". Whether plugin view code runs on the daemon or in a TUI-hosted
  VM is the open architectural question, and it is upstream of Oil-versus-TS.

## On a TUI-hosted Lua VM

Raised as the natural fix for latency. One rule makes it safe, and it is the
rule `CLAUDE.md` already states for TUI-local state:

**A TUI-hosted VM may render, and may not own.** It reads published data and
draws; it holds no authoritative plugin state and performs no writes. Writes go
to the daemon as commands, exactly as the web's do.

With that rule, multiple TUI clients are not a problem — each has a renderer,
none has an opinion. Without it, N clients means N writers to state the daemon
is supposed to own, and two terminals showing the same kiln will disagree.

The cost is a second VM per client, which is small in absolute terms. The cost
that is *not* small is that a VM is a second import authority — the exact thing
`crucible-lua/src/modules.rs` exists to prevent — so a rendering VM would need
its module resolver locked to a view-only surface with no `cru.shell`, no
`cru.fs` writes and no storage namespace.

## Links

- [[Meta/Analysis/Oil in Documents]] — the spike this supersedes, and the
  evidence for why
- `runtime/plugins/kanban/` — the reference plugin
- `crates/crucible-web/web/src/components/blocks/` — the web half
- `docs/Meta/Architecture/Mobile Shell.md` (draft, `worktree-mobile-ui`) — the
  offline case that made the execution-locus split the right one
