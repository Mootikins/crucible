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

The source is **handler signatures**, not the call graph. The `rpc_methods!`
table in `crates/crucible-daemon/src/rpc/dispatch.rs:83` lists 156 methods.
The `crucible-daemon/src/server` and `src/rpc` modules declare 128 `handle_*`
functions. A signature is where a type crosses a boundary, so it answers the
ownership question directly. Counts below come from the code at commit
7053bcfe7 (2026-08-22). Earlier versions of this document counted 116 methods.

The call graph does not work for this. graphify records **one** `calls` edge for
`handle_session_send_message`, which calls many things. Rust call resolution
across modules is outside what the AST extractor does. Do not build a call
sequence from it. Module-level dependency edges are reliable. Call edges are not.

So this document lists **types**, not calls. It does not show the order inside a
handler. It does not show what a handler reaches through a trait object.

## The spine

Nine types carry almost every flow. A change to one of these reaches most of the
daemon.

| Type | Owner | Reaches |
|---|---|---|
| `Request` / `Response` | `crates/crucible-core/src/protocol/rpc/mod.rs:17` | 124 / 109 handlers |
| `AgentManager` | `crates/crucible-daemon/src/agent_manager/mod.rs:363` | 36 handlers |
| `KilnManager` | `crates/crucible-daemon/src/kiln_manager.rs:383` | 26 handlers |
| `SessionManager` | `crates/crucible-daemon/src/session_manager.rs:147` | 23 handlers |
| `SessionEventMessage` | `crates/crucible-core/src/protocol/rpc/mod.rs:85` | 22 handlers |
| `DaemonPluginLoader` | `crates/crucible-daemon/src/daemon_plugins/mod.rs:113` | 12 handlers |
| `ProjectManager` | `crates/crucible-daemon/src/project_manager.rs:77` | 11 handlers |
| `RpcContext` | `crates/crucible-daemon/src/rpc/context.rs:61` | 7 handlers (ui, workflow, one session), holds the rest |
| `McpServerManager` | `crates/crucible-daemon/src/mcp_server.rs:40` | 3 handlers |

`SessionEventMessage` is the one type all four wire bindings share. An earlier
graphify topology measurement agreed: it was the most connected type in the graph.

## Per-flow type listing

Flows group handlers by the first word after `handle_`. The count in each
heading is the number of `handle_*` functions, not the number of RPC methods.
The `session.*` group has 75 methods but 44 handlers, because several methods
share a handler.

```
=== session  (44 handlers) ===
    44x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
    44x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
    27x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
    18x SessionManager               crates/crucible-daemon/src/session_manager.rs
    16x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs

=== plugin  (9 handlers) ===
     9x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     9x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
     9x DaemonPluginLoader           crates/crucible-daemon/src/daemon_plugins/mod.rs
     1x OptionAction                 crates/crucible-daemon/src/server/plugins.rs

=== lua  (8 handlers) ===
     8x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     7x Response                     crates/crucible-core/src/protocol/rpc/mod.rs
     3x LuaSessionState              (external)
     3x DashMap                      (external)

=== note  (5 handlers) ===
     5x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     5x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs

=== kiln  (5 handlers) ===
     5x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     5x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     1x DaemonPluginLoader           crates/crucible-daemon/src/daemon_plugins/mod.rs

=== review  (5 handlers) ===
     5x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     5x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs
     5x SessionManager               crates/crucible-daemon/src/session_manager.rs
     4x SessionEventMessage          crates/crucible-core/src/protocol/rpc/mod.rs

=== workflow  (4 handlers) ===
     4x RpcContext                   crates/crucible-daemon/src/rpc/context.rs
     4x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     4x RpcResult                    crates/crucible-daemon/src/rpc/dispatch.rs

=== project  (4 handlers) ===
     4x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     4x ProjectManager               crates/crucible-daemon/src/project_manager.rs

=== fs  (4 handlers) ===
     4x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     4x ProjectManager               crates/crucible-daemon/src/project_manager.rs
     3x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs

=== storage  (4 handlers) ===
     4x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs

=== search  (3 handlers) ===
     3x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     3x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs
     1x ProjectManager               crates/crucible-daemon/src/project_manager.rs

=== mcp  (3 handlers) ===
     3x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     3x McpServerManager             crates/crucible-daemon/src/mcp_server.rs
     1x PluginRegistry               crates/crucible-daemon/src/plugin_tools.rs
     1x KilnManager                  crates/crucible-daemon/src/kiln_manager.rs

=== skills  (3 handlers) ===
     3x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs

=== ui  (2 handlers) ===
     2x RpcContext                   crates/crucible-daemon/src/rpc/context.rs
     2x Request                      crates/crucible-core/src/protocol/rpc/mod.rs

=== config  (2 handlers) ===
     2x Request                      crates/crucible-core/src/protocol/rpc/mod.rs
     2x RpcResult                    crates/crucible-daemon/src/rpc/dispatch.rs

=== agents  (2 handlers) ===
     2x Request, Response            crates/crucible-core/src/protocol/rpc/mod.rs
     2x AgentManager                 crates/crucible-daemon/src/agent_manager/mod.rs

=== one handler each ===
  embed, list_notes, get_note_by_name, get_backlinks, suggest, process (2)
                                      KilnManager
  models, providers                   AgentManager
  scm                                 ProjectManager
  ping, shutdown, capabilities, subscribe, unsubscribe, subagent, webhook
                                      RpcResult + serde_json::Value only
```

## Names with more than one declaration

`AGENTS.md` says never to duplicate a type between crates. **Six names are still
declared in more than one crate with the same meaning.** Eleven more share a
name but not a meaning.

Rows leave this table as the duplication goes. `EventFilter` left on
2026-08-21: the `crucible-core` half lived in an `events::subscriber` module
that no caller named, so the daemon's is now the only one. On 2026-08-22 a check
against the code removed eleven more rows. Renames had already resolved them:
`DiscoveryConfig` became `ModelDiscoveryConfig` in the daemon, `SecretsFile`
became `WebhookSecretsFile`, `ShellPolicy` became `PluginShellPolicy` in
`crucible-lua`, `StorageHandle` became `CliStorageHandle`, `ToolExecutor` became
`AcpToolExecutor`. `FastEmbedConfig`, `LlmConfig` and `PermissionDecision` now
have one declaration in `crucible-core`. `SessionManager`, `SessionKilnRequest`,
`GrepSearchRequest` and `OptionAction` have one owner each.

### Same concept, two definitions — fix these

These are the real violations. Each is one idea with two owners, so a reader
cannot tell which is canonical. A change has to be made twice.

| Name | Declared in |
|---|---|
| `ModelsResponse` | `crates/crucible-daemon/src/agent_manager/context_length.rs:37` <br> `crates/crucible-daemon/src/provider/copilot.rs:194` <br> `crates/crucible-web/src/routes/helpers.rs:13` |
| `AgentError` | `crates/crucible-core/src/turn/mod.rs:220` <br> `crates/crucible-daemon/src/agent_manager/mod.rs:90` |
| `EmbeddingResponse` | `crates/crucible-core/src/traits/provider.rs:16` <br> `crates/crucible-daemon/src/llm/embeddings/provider.rs:400` |
| `FileState` | `crates/crucible-core/src/processing/change_detection.rs:72` <br> `crates/crucible-daemon/src/watch/backends/polling_backend.rs:31` |
| `ShowRequest` | `crates/crucible-core/src/interaction/edit.rs:84` <br> `crates/crucible-daemon/src/agent_manager/context_length.rs:92` |
| `ToolResult` | `crates/crucible-core/src/traits/tools.rs:11` (a `Result` alias) <br> `crates/crucible-lua/src/types.rs:71` (a struct) |

`ModelsResponse` is the largest of these. It has three declarations in three
places.

### One name, two concepts — rename, do not merge

These are separate ideas that share a word. To merge them would be
wrong. The hazard is that `use ...::Event` reads as unambiguous and is not.

| Name | Declared in |
|---|---|
| `Event` | `crates/crucible-cli/src/tui/oil/event.rs:4` <br> `crates/crucible-core/src/events/emitter.rs:298` (an associated type) <br> `crates/crucible-daemon/src/file_watch_bridge.rs:40` (an associated type) |
| `Direction` | `crates/crucible-daemon/src/acp/client/recording.rs:28` <br> `crates/crucible-oil/src/node.rs:130` |
| `Drawer` | `crates/crucible-cli/src/tui/oil/components/drawer.rs:7` (an alias of `OilDrawer`) <br> `crates/crucible-oil/src/components/drawer.rs:39` |
| `MapSerializer` | `crates/crucible-core/src/serde_md/serializer.rs:277` <br> `crates/crucible-daemon/src/observe/serde_md.rs:283` |
| `Op` | `crates/crucible-core/src/storage/note_store.rs:318` <br> `crates/crucible-oil/src/proptest_strategies.rs:224` |
| `ParseError` | `crates/crucible-cli/src/tui/oil/commands/set.rs:6` <br> `crates/crucible-core/src/parser/error.rs:55` |
| `Record` | `crates/crucible-core/src/types/database.rs:56` <br> `crates/crucible-daemon/src/review/journal.rs:52` |
| `SeqSerializer` | `crates/crucible-core/src/serde_md/serializer.rs:210` <br> `crates/crucible-daemon/src/observe/serde_md.rs:216` |
| `StructSerializer` | `crates/crucible-core/src/serde_md/serializer.rs:307` <br> `crates/crucible-daemon/src/observe/serde_md.rs:313` |
| `ToolOutput` | `crates/crucible-cli/src/commands/tools.rs:9` <br> `crates/crucible-core/src/types/acp.rs:375` |
| `Verdict` | `crates/crucible-core/src/session/types/review.rs:601` <br> `crates/crucible-daemon/src/tools/fs_scope.rs:501` |

`Session` in `crucible-web/src/middleware/auth/session.rs:33` is a private HTTP
auth token holder. It takes the name of the most central domain type here.

`ChatEvent` used to be on this list, and it turned out not to be a naming problem.
A deleted module in `crucible-core` (`traits::input`) declared `ChatEvent`,
`InputMode`, `KeyCode`, `KeyPattern`, `Modifiers`, `KeyAction` and
`SessionAction`. **Nothing outside that module used any of them.** The TUI takes `KeyCode` from
`crossterm` and `InputMode` from its own components. The module was deleted,
not renamed. `ChatEvent` now has one declaration, in `crucible-web/src/events.rs:7`.
Check for a consumer before you rename: a collision with dead code is a deletion.

### Not a duplicate

`Session` and `Config` used to be **associated types** on an ACP trait in
`crucible-core`, with one impl in the daemon. Both are gone. The trait had three
methods that only set a field, and every caller was a test. An ACP session is an
ordinary `Session` held by the daemon's `SessionManager` struct, so there was
never a second kind of session for a trait to abstract over.
`crates/crucible-core/src/traits/acp.rs` is now a comment-only tombstone.
Per-crate `Result` and `Error` aliases follow the documented `<Domain>Result<T>`
convention and are also correct.

## What to do

1. **Collapse `ModelsResponse`.** Three declarations remain, and it is the
   largest open item. Decide whether the Copilot wire shape and the web response
   shape are the same type. If they are, one owner. If not, two names.
2. **Rename the colliding names**, and start with the web auth `Session`. A
   rename is cheap. A reader who trusts the wrong one is not. Check for a
   consumer first: `ChatEvent` looked like a rename and was a deletion.
3. **Done.** `SessionManager` has one owner (`session_manager.rs:147`).
   `GrepSearchRequest`, `OptionAction` and `SessionKilnRequest` each have one
   declaration; `crucible-web` uses the daemon's. The web copy of
   `GrepSearchRequest` hardcoded its default limit at 100 while the daemon's
   reads `GREP_DEFAULT_LIMIT` (`server/grep.rs:24`, also 100). The two agreed by
   coincidence, and one edit to that constant would have separated them.
