---
title: Plugin API Decisions
description: Settled plugin API decisions, remaining delivery constraints, and evidence from the September 2026 implementation
status: implemented
updated: 2026-09-15
tags: [meta, plugins, architecture]
---

# Plugin API Decisions

The implementation plan and merge checklist are complete. This note retains
their decisions, not their obsolete branch instructions or source-line inventory.
The governing model is [[Meta/Analysis/The Plugin Contract]]; author-facing
contracts live in [[Help/Plugins/Lua Runtime API]] and [[Meta/Plugin Conventions]].

## Identity is not isolation

Web plugin calls carry `X-Crucible-Plugin`: `app` or a plugin name.
Missing identity is refused where the route requires it. Command and option
calls check ownership; lifecycle operations are app-only; publication reads
are narrowed for plugin callers. The app explicitly identifies itself.

This is an attribution seam, not protection from same-origin script: script
can forge the header, and a note's fence can supply a block's plugin name.
Enumerations and the EventSource notification stream are not a confidential
per-plugin channel. TUI calls use daemon JSON-RPC, not these HTTP routes;
the daemon socket's local-user access boundary is a separate contract.

Third-party web assets must not be served on the app origin. The selected
delivery design is an opaque-origin sandboxed iframe with a MessageChannel
RPC bridge. Build it when an installed plugin needs web assets; do not invent
a capability badge or extract a same-origin function library first.
[[Meta/Analysis/Plugin Web Delivery]] retains the alternatives, measurements,
envelope and implementation trigger.

## Commands describe effects; declarations are checked

`effect = "read" | "write"` describes one command, not a plugin grant.
Absent means `write`; an invalid spelling refuses activation. A read changes
nothing the user could lose: recomputing a publication may still be a read.

Command and tool parameter types must parse. The web command dialog renders
declared JSON Schema without knowing command names. Its effect badge reports
a declaration, not verified safety or permission enforcement. A person-invoked
write still needs a separate permission decision at its execution boundary.

The type vocabulary and generated controls must evolve together. Do not
promise a dropdown, path picker, range or default merely because the UI could
draw one; the declaration and schema must represent it first.

## Plugins are operator-installed code

Restricting the Lua API is out of scope. Plugins can use unscoped `io.open`,
`io.popen` and `os.execute`; `cru.shell` is not the only process door.
Host-owned module lookup remains import authority. Scoped `cru.fs.read` and
`cru.fs.write` are useful defaults, not containment of arbitrary plugin code.

The old ten-name capability enum and plugin manifest files were deleted.
Tool interception remains the capability-grade exception: a plugin needs its
fragment's `intercepts_tools` declaration before replacing or transforming
a tool call. Keep the declaration check and permission-gate ordering.
The operator's own configuration and builtins are distinct from socket eval;
see the trust rules in the repository agent guide.

Attribution is independent of permission. Start/end hooks, plugin tool calls,
scheduled work and timers must restore their registration owner's context.
Storage namespaces and publications depend on that owner. Tests should give
the plugin and its tool different names, and cross the running VM boundary.
Runtime registrations and checker-visible declarations must expose the same
functions; a function present in only one still ships broken.

## Graph evidence: smaller replies do not imply cheaper reads

The September 2026 spike measured a 2,000-note, 11,996-edge graph in a debug
build: one full graph fetch took 137 ms; a depth-one neighborhood call took
80 ms. These are historical measurements, not current performance promises.

The neighborhood read still scans visible notes and links. Returning hop
distances through `neighbors_with_hops` removed repeated per-hop calls, but
does not make the underlying query indexed. Keep the existing whole-graph
client path until an indexed neighborhood query justifies per-move RPC.
The graph block and its tests retain the measured alternative.

## Writes and remaining decisions

Kanban must rewrite only its frontmatter block, including a status key in
the first position; matching a status line in prose or a code fence is wrong.
Precise anchoring and concurrency detection solve different problems. A
validated substring alone is not a syntax-aware edit.

Published state must remain derivable from files, including edits made with
the plugin stopped. Daemon/browser participating writes use the shared write
path described in [[Help/Concepts/Note Sync]]; raw plugin I/O and external
editors do not acquire that lock.

Keep these follow-ups separate from the completed plan:

- Indexed graph neighborhood retrieval.
- Opaque-origin delivery before third-party web bundles.
- Richer parameter declarations and person-invoked command permission UX.
- Session/viewer-scoped publications, coordinated with
  [[Meta/Architecture/Mobile Shell]] rather than a second scope vocabulary.
