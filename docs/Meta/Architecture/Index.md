---
title: Architecture
description: Current architecture entry points, focused designs, and clearly dated audit history
tags: [meta, architecture]
---

# Architecture

Start with [[Meta/Product]] for goals, behavior and current proof status.
[[Meta/CONTEXT]] defines the vocabulary. The repository agent guide records
the implementation boundaries and required workflow.

## As-built subsystem pages

Each page below cites paths and symbols in the current tree, not line
numbers. Read a page before you change its subsystem.

| Page | What it covers |
| --- | --- |
| [[Crate Map]] | The six-crate Rust workspace, dependency order, each crate's root module, and the crate boundary AGENTS.md sets |
| [[Data Flows]] | The main end-to-end flows across crates, from daemon boot through a user turn, note write, plugin activation, kiln indexing, web and ACP/MCP entry, a Bases query and write, diffset review and proposal decision, the web event stream, and fork/undo/delegation, each as a numbered sequence and a diagram |
| [[Core Domain Types]] | The crucible-core session, review, turn, event, interaction, wire-protocol, runtime-path, trait and workflow types, and their shared test-support infrastructure |
| [[Parser]] | The markdown, task-file and workflow parser in crucible-core, and the note-edit, note-merge and note-frontmatter write primitives it feeds |
| [[Core Config]] | The crucible-core config domain: schema, permission engine, credentials, provenance and the flat ConfigStore, and its seams into the daemon, CLI, Lua host and web |
| [[Knowledge Storage and Retrieval]] | Kiln and project registration, the note/block/property storage traits and their SQLite backend, the note-processing pipeline, multi-kiln search, and the file-watch pipeline that keeps the index and the review ledger current |
| [[Bases]] | The Obsidian Bases document, expression and evaluation engine: a canonical AST in crucible-core, and the daemon's query, write, policy and plugin surface over it |
| [[Daemon Server]] | The daemon's Unix-socket JSON-RPC server: connection lifecycle, session RPC handlers, event fan-out, plugin/session wiring, and the process boot/shutdown sequence |
| [[Session Services]] | The daemon's session-support layer: session CRUD, persistence, migration, the plugin-facing session bridge, delegation, agent construction, recording/replay, background bash jobs, workflow step handlers, and Agent Skills discovery |
| [[Agent Manager]] | The daemon's per-session agent-lifecycle hub: AgentManager, SessionSlot, the turn pipeline, precognition, and the permission/review/isolation gates a tool call passes through |
| [[Tools and Admission]] | The daemon's tool-execution and filesystem-admission layer: dispatch, containment, MCP surfaces, and the trust classification that gates a sandboxed session |
| [[Review]] | The review ledger's per-tool-call attribution, the diffset and comment model that carries its read surface, and the propose-write disposition for a write that needs a decision |
| [[Providers and LLM]] | The daemon's chat-provider and embedding-provider seam: genai adapter mapping, the tool-loop agent handle, Copilot OAuth, model listing, and the embeddings factory |
| [[ACP and MCP]] | The Agent Client Protocol client and the daemon's MCP surfaces: the wire-level bridge to external agents and tool clients |
| [[Luau Host]] | The plugin VM, the discovery/activation lifecycle, the handler/hook registry, and the config/options/prelude infrastructure that runs plugin and user Lua for the daemon |
| [[Luau APIs]] | The cru.* Lua namespace bindings for session, tools, UI, storage, statusline, surfaces, vault and theming, and the daemon traits they marshal through |
| [[RPC Client]] | The daemon-side client library that CLI, TUI, ACP and web callers use to reach the daemon's own JSON-RPC surface |
| [[CLI Commands]] | The cru binary: the clap argument surface, per-command handlers, and the CLI-side helpers that turn a flag into a daemon RPC |
| [[TUI Components]] | The Oil-based TUI's leaf components, event loop, config overlay, markdown renderer, theme stores and full-screen chat view in crucible-cli |
| [[TUI Chat App]] | OilChatApp: the reducer, message vocabulary and transcript model behind the TUI chat screen |
| [[Oil Renderer]] | crucible-oil, the terminal-rendering primitives crate: the Node tree, Taffy-backed layout, ANSI/CellGrid painting, and the diffing terminal driver the CLI's TUI builds on |
| [[Web Server]] | The crucible-web Axum backend: router assembly, auth and host defense, daemon RPC forwarding, and SSE projection for the SolidJS frontend |
| [[Test Architecture]] | The Rust test suite: harnesses, fixtures, mock agents, property tests and source-scan gates across every crate |
| [[Vendored Markdown-it]] | The vendored markdown-it CommonMark parser and its Crucible patches, at vendor/markdown-it |

## Current entry points

| Question | Read |
| --- | --- |
| Which subsystem owns the change? | The ownership table in the repository agent guide; then verify the current types and callers |
| How do configuration and plugins start? | [[Config Boot]], [[State Stores]], [[Meta/Plugin Conventions]] |
| What is the plugin data/render contract? | [[Meta/Plugin Conventions]], [[Meta/Plugin User Stories]] |
| Where does a new tool, provider, client or RPC land? | [[Consolidation Plan#Extension seams]] |
| How does the web window manager work, and where does a layout feature go? | [[Web Windowing]] |
| What does a user do? | The relevant note under `docs/Help/`, rather than an implementation report |
| Which crate owns a type, module or symbol? | [[Crate Map]] |
| How does a user turn flow end to end? | [[Data Flows]] |
| What does a tool call pass through before it runs? | [[Agent Manager]], [[Tools and Admission]] |
| How does the daemon expose its RPC surface? | [[Daemon Server]], [[RPC Client]] |
| How does a plugin get discovered, activated and reloaded? | [[Luau Host]] |
| What cru.* Lua APIs can a plugin call? | [[Luau APIs]] |
| How does note review capture and gate a diff? | [[Review]] |
| How does a diffset or a proposal reach a decision? | [[Review]] |
| How does the web server route and forward daemon RPC? | [[Web Server]] |
| How does a kiln get indexed and searched? | [[Knowledge Storage and Retrieval]] |
| How does a Bases query or write run, and who enforces its policy? | [[Bases]] |
| How does the test suite structure its harnesses and fixtures? | [[Test Architecture]] |

The as-built subsystem pages above live in this kiln. They cite paths and
symbols, not line numbers. A path or a symbol name changes less often than a
line number.

Some working notes still live outside this kiln, under
`docs/Meta/Analysis/`. They cover the systems inventory, type-flow reports,
the storage schema, the filesystem containment layers, the bash permission
layers, and workspace and runtime targets not yet promoted to an as-built
page. Those notes cite line numbers that move. They stay untracked and
absent from a clone.

Before you act on a claim from any note, reproduce it against the current
code.

## Designs, not implementation promises

[[Mobile Shell]] is a design draft. It includes implemented pieces and proposed
work; check [[Meta/Product]] and the relevant Help note before treating an
individual section as shipped. The chosen third-party web isolation design and
the canvas and Oil-in-documents rendering designs are working notes in the same
untracked tree.

[[Simplification Plan]] is a proposal. It orders the steps that delete
duplicate layers and copies, so that each concept has one obvious owner.

## Historical audits

[[Expected]], [[Actual]] and [[Gaps]] began as the **2026-08-22** comparison at
`7053bcfe7`, with later dated amendments. They are evidence of that review,
not current normative architecture or an active defect queue. Their source
paths and line numbers belong to those revisions.

[[Consolidation Plan]] retains the resulting decisions and extension seams;
completed per-symbol inventories live in git history. Later reduction reviews
are working notes too: use their lessons, but reproduce an old finding before
promoting it to current work.