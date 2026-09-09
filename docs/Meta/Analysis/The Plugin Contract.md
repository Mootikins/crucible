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

## What is still not built

- **No TUI surface hosts a plugin view.** Oil is now unambiguously the TUI's
  own plugin API rather than a cross-frontend one, which makes its
  terminal-specific primitives correct rather than defective. But a Lua-built
  tree still has to cross a process boundary to reach the TUI, so it needs
  `Node: Deserialize` and an action dispatcher. See [[Meta/Analysis/Oil in Documents]], section "P1".
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
