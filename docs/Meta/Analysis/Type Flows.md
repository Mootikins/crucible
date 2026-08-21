---
title: Type Flows
description: The types every feature flow crosses, read from handler signatures, and the names that have more than one declaration.
tags:
  - meta
  - architecture
  - types
---

# Type Flows

This document answers one question: **for a given feature, which types does the
request cross, and which file owns each one?**

Read it with [[Systems]], which gives the boundaries, and `AGENTS.md`, which
gives the rule this measures: *never duplicate types between crates — one
canonical location, then re-export.*

## How this was measured, and what it cannot show

The source is **handler signatures**, not the call graph. Every RPC method
reaches a `handle_*` function; 116 of them parse. A signature is where a type
crosses a boundary, so it answers the ownership question directly.

The call graph does not work for this. graphify records **one** `calls` edge for
`handle_session_send_message`, which calls many things. Rust call resolution
across modules is outside what the AST extractor does. Do not build a call
sequence from it. Module-level dependency edges are reliable; call edges are not.

So this document lists **types**, not calls. It does not show order within a
handler, and it does not show what a handler reaches through a trait object.

## The spine

Ten types carry almost every flow. A change to one of these reaches most of the
daemon.

| Type | Owner | Reaches |
|---|---|---|
| `Request` / `Response` | `crates/crucible-core/src/protocol/rpc/mod.rs` | all 116 handlers |
| `AgentManager` | `crates/crucible-daemon/src/agent_manager/mod.rs` | 35 handlers |
| `SessionManager` | **3 declarations — see below** | 23 handlers |
| `KilnManager` | `crates/crucible-daemon/src/kiln_manager.rs` | 23 handlers |
| `SessionEventMessage` | `crates/crucible-core/src/protocol/rpc/mod.rs` | 19 handlers |
| `ProjectManager` | `crates/crucible-daemon/src/project_manager.rs` | 11 handlers |
| `DaemonPluginLoader` | `crates/crucible-daemon/src/daemon_plugins/` | 11 handlers |
| `McpServerManager` | `crates/crucible-daemon/src/mcp_server.rs` | 3 handlers |
| `RpcContext` | `crates/crucible-daemon/src/rpc/context.rs` | 1 handler, holds the rest |

`SessionEventMessage` is the one type all four wire bindings share. The topology
measurement agrees: it is the most connected type in the graph, at degree 235.

## Per-flow type listing

```
=== agent  (2 handlers) ===
     2x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     2x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
     2x Response                     crates/crucible-core/src/protocol/rpc/mod.rs

=== embed  (1 handlers) ===
     1x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     1x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     1x Response                     crates/crucible-core/src/protocol/rpc/mod.rs

=== kiln  (5 handlers) ===
     5x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     5x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     5x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
     1x KilnRegistry                 crates/crucible-daemon/src/kiln_registry.rs
     1x DaemonPluginLoader           (external)
     1x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs

=== note  (5 handlers) ===
     5x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     5x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     5x Response                     crates/crucible-core/src/protocol/rpc/mod.rs

=== other  (38 handlers) ===
    36x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
    36x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
    10x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     9x ProjectManager               crates/crucible-daemon/src/project_manager.rs
     7x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
     5x SessionManager               crates/crucible-daemon/src/session_manager.rs
     4x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs
     3x DashMap                      (external)
     3x LuaSessionState              (external)
     3x McpServerManager             crates/crucible-daemon/src/mcp_server.rs
     1x DaemonPluginLoader           (external)
     1x PluginRegistry               crates/crucible-daemon/src/plugin_tools.rs

=== plugin  (9 handlers) ===
     9x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     9x DaemonPluginLoader           (external)
     9x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
     1x OptionAction                 crates/crucible-daemon/src/server/plugins.rs

=== search  (3 handlers) ===
     3x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     3x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     3x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
     1x ProjectManager               crates/crucible-daemon/src/project_manager.rs

=== session  (43 handlers) ===
    43x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
    43x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
    26x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
    18x SessionManager               crates/crucible-daemon/src/session_manager.rs
    15x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs
     2x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     2x LlmConfig                    crates/crucible-core/src/config/components/llm.rs
     1x RpcContext                   crates/crucible-daemon/src/rpc/context.rs
     1x ProjectManager               crates/crucible-daemon/src/project_manager.rs
     1x DaemonPluginLoader           (external)

=== skill  (3 handlers) ===
     3x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     3x Response                     crates/crucible-core/src/protocol/rpc/mod.rs

=== turn  (1 handlers) ===
     1x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     1x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
     1x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs
     1x Response                     crates/crucible-core/src/protocol/rpc/mod.rs

=== ui  (2 handlers) ===
     2x RpcContext                   crates/crucible-daemon/src/rpc/context.rs
     2x Request                      crates/crucible-core/src/protocol/rpc/mod.rs

=== workflow  (4 handlers) ===
     4x RpcContext                   crates/crucible-daemon/src/rpc/context.rs
     4x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     4x RpcResult                    crates/crucible-daemon/src/rpc/dispatch.rs
```

## Names with more than one declaration

`AGENTS.md` says never to duplicate a type between crates. **35 names
are declared in more than one crate.** They are not all the same problem.

Rows leave this table as the duplication goes. `EventFilter` left on
2026-08-21: the `crucible-core` half lived in an `events::subscriber` module
that no caller named, so the daemon's is now the only one.

### Same concept, two definitions — fix these

These are the real violations. Each is one idea with two owners, so a reader
cannot tell which is canonical and a change has to be made twice.

| Name | Declared in |
|---|---|
| `ModelsResponse` | `crates/crucible-daemon/src/agent_manager/context_length.rs` <br> `crates/crucible-daemon/src/provider/copilot.rs` <br> `crates/crucible-web/src/routes/helpers.rs` |
| `SessionManager` | `crates/crucible-core/src/traits/acp.rs` <br> `crates/crucible-daemon/src/session_manager.rs` <br> `crates/crucible-lua/src/session_api.rs` |
| `AgentError` | `crates/crucible-core/src/turn/mod.rs` <br> `crates/crucible-daemon/src/agent_manager/mod.rs` |
| `DiscoveryConfig` | `crates/crucible-core/src/discovery.rs` <br> `crates/crucible-daemon/src/llm/model_discovery.rs` |
| `EmbeddingResponse` | `crates/crucible-core/src/traits/provider.rs` <br> `crates/crucible-daemon/src/llm/embeddings/provider.rs` |
| `FastEmbedConfig` | `crates/crucible-core/src/config/enrichment.rs` <br> `crates/crucible-daemon/src/llm/embeddings/fastembed.rs` |
| `FileState` | `crates/crucible-core/src/processing/change_detection.rs` <br> `crates/crucible-daemon/src/watch/backends/polling_backend.rs` |
| `GrepSearchRequest` | `crates/crucible-daemon/src/rpc_client/client/storage_requests.rs` <br> `crates/crucible-web/src/routes/search.rs` |
| `LlmConfig` | `crates/crucible-cli/src/config.rs` <br> `crates/crucible-core/src/config/components/llm.rs` |
| `OptionAction` | `crates/crucible-daemon/src/server/plugins.rs` <br> `crates/crucible-web/src/routes/plugin.rs` |
| `PermissionDecision` | `crates/crucible-core/src/config/components/permissions/types.rs` <br> `crates/crucible-daemon/src/observe/events.rs` |
| `SecretsFile` | `crates/crucible-core/src/config/credentials.rs` <br> `crates/crucible-daemon/src/webhook/mod.rs` |
| `SessionKilnRequest` | `crates/crucible-daemon/src/rpc_client/client/agent.rs` <br> `crates/crucible-web/src/routes/session/mod.rs` |
| `ShellPolicy` | `crates/crucible-core/src/config/security.rs` <br> `crates/crucible-lua/src/shell.rs` |
| `ShowRequest` | `crates/crucible-core/src/interaction/edit.rs` <br> `crates/crucible-daemon/src/agent_manager/context_length.rs` |
| `StorageHandle` | `crates/crucible-cli/src/factories/storage.rs` <br> `crates/crucible-daemon/src/kiln_manager.rs` |
| `ToolExecutor` | `crates/crucible-core/src/traits/tools.rs` <br> `crates/crucible-daemon/src/acp/tools.rs` |
| `ToolResult` | `crates/crucible-core/src/traits/tools.rs` <br> `crates/crucible-lua/src/types.rs` |

`SessionManager` is the worst of these because 23 handlers take one. Two of its
three declarations are the domain manager and a Lua-facing handle of the same
name.

### One name, two concepts — rename, do not merge

These are separate ideas that happen to share a word. Merging them would be
wrong. The hazard is that `use ...::Event` reads as unambiguous and is not.

| Name | Declared in |
|---|---|
| `Event` | `crates/crucible-cli/src/tui/oil/event.rs` <br> `crates/crucible-core/src/events/emitter.rs` <br> `crates/crucible-daemon/src/file_watch_bridge.rs` |
| `Direction` | `crates/crucible-daemon/src/acp/client/recording.rs` <br> `crates/crucible-oil/src/node.rs` |
| `Drawer` | `crates/crucible-cli/src/tui/oil/components/drawer.rs` <br> `crates/crucible-oil/src/components/drawer.rs` |
| `MapSerializer` | `crates/crucible-core/src/serde_md/serializer.rs` <br> `crates/crucible-daemon/src/observe/serde_md.rs` |
| `Op` | `crates/crucible-core/src/storage/note_store.rs` <br> `crates/crucible-oil/src/proptest_strategies.rs` |
| `ParseError` | `crates/crucible-cli/src/tui/oil/commands/set.rs` <br> `crates/crucible-core/src/parser/error.rs` |
| `Record` | `crates/crucible-core/src/types/database.rs` <br> `crates/crucible-daemon/src/review/journal.rs` |
| `SeqSerializer` | `crates/crucible-core/src/serde_md/serializer.rs` <br> `crates/crucible-daemon/src/observe/serde_md.rs` |
| `StructSerializer` | `crates/crucible-core/src/serde_md/serializer.rs` <br> `crates/crucible-daemon/src/observe/serde_md.rs` |
| `ToolOutput` | `crates/crucible-cli/src/commands/tools.rs` <br> `crates/crucible-core/src/types/acp.rs` |
| `Verdict` | `crates/crucible-core/src/session/types/review.rs` <br> `crates/crucible-daemon/src/tools/fs_scope.rs` |

`Session` in `crucible-web/src/middleware/auth/session.rs` is an HTTP auth
token holder, and it takes the name of the most central domain type here.

`ChatEvent` used to be on this list and turned out not to be a naming problem.
A deleted module in `crucible-core` (`traits::input`) declared `ChatEvent`,
`InputMode`, `KeyCode`, `KeyPattern`, `Modifiers`, `KeyAction` and
`SessionAction`, and **nothing outside that module used any of them** — the TUI takes `KeyCode` from
`crossterm` and `InputMode` from its own components. The module was deleted
rather than renamed. Check for a consumer before you rename: a collision with
dead code is a deletion.

### Not a duplicate

`Session` and `Config` used to be **associated types** on an ACP trait in
`crucible-core`, with one impl in the daemon. Both are gone: the trait had three
methods that only set a field, and every caller was a test. An ACP session is an
ordinary `Session` held by the daemon's `SessionManager` struct, so there was
never a second kind of session for a trait to abstract over.
Per-crate `Result` and `Error` aliases follow the documented `<Domain>Result<T>`
convention and are also correct.

## What to do

1. **Give `SessionManager` one owner.** 23 handlers take one, and there are
   three declarations.
2. **Rename the colliding names**, starting with the web auth `Session`. A
   rename is cheap; a reader who trusts the wrong one is not. Check for a
   consumer first — `ChatEvent` looked like a rename and was a deletion.
3. **Collapse the wire request pairs.** `GrepSearchRequest` and `OptionAction`
   are done: `crucible-web` now uses the daemon's declaration of each. The web
   copy of `GrepSearchRequest` hardcoded its default limit at 100 and the
   daemon's reads `GREP_DEFAULT_LIMIT`, which is also 100 — the two agreed by
   coincidence, and one edit to that constant would have separated them. `SessionKilnRequest` and `ModelsResponse` remain.
