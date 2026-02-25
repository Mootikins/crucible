---
title: Data Flow Analysis
description: Data Flow Analysis
tags:
  - help
---

# Data Flow Analysis

> Generated analysis of message dispatch, RPC boundaries, and trait implementations.
> Goal: identify redundant/unnecessary paths for simplification.

---

## 1. TUI Message & Action Flow

### Core Types

```
Action<ChatAppMsg>
├── Continue          — no-op
├── Quit              — exit app
├── Send(ChatAppMsg)  — dispatch message to runner + app
└── Batch(Vec<Action>) — execute multiple actions
```

### Dispatch Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  Two parallel dispatch paths exist:                                │
│                                                                    │
│  PATH A (async): Key events & UI actions                           │
│  ┌──────────┐    Action::Send(msg)    ┌──────────────────┐         │
│  │ app      │ ──────────────────────► │ runner           │         │
│  │ .update()│                         │ .process_action()│         │
│  └──────────┘                         └────────┬─────────┘         │
│       ▲                                        │                   │
│       │          app.on_message(msg)           │                   │
│       └────────────────────────────────────────┘                   │
│                  returns Action → recurse                          │
│                                                                    │
│  PATH B (sync): Channel messages (stream chunks, background)       │
│  ┌──────────┐    msg_rx.try_recv()    ┌──────────────────┐         │
│  │ channel  │ ──────────────────────► │ runner           │         │
│  │ (msg_tx) │                         │ .process_message()│        │
│  └──────────┘                         └────────┬─────────┘         │
│       ▲                                        │                   │
│       │          app.on_message(msg)           │                   │
│       └────────────────────────────────────────┘                   │
│                  returns Action → recurse (sync only)              │
└────────────────────────────────────────────────────────────────────┘
```

### Which messages go through which path?

| ChatAppMsg | Path A (process_action) | Path B (process_message) | Overlap? |
|---|---|---|---|
| `UserMessage` | ✅ agent.send_message_stream() | ✅ agent.send_message_stream() | **YES — duplicated** |
| `ClearHistory` | ✅ agent.cancel() + agent.clear_history() | ✅ drops stream only (no await) | **YES — degraded copy** |
| `StreamCancelled` | ✅ agent.cancel() | ✅ drops stream only | **YES — degraded copy** |
| `SwitchModel` | ✅ agent.switch_model() | ❌ | |
| `FetchModels` | ✅ agent.fetch_available_models() | ❌ | |
| `SetThinkingBudget` | ✅ agent.set_thinking_budget() | ❌ | |
| `SetTemperature` | ✅ agent.set_temperature() | ❌ | |
| `SetMaxTokens` | ✅ agent.set_max_tokens() | ❌ | |
| `CloseInteraction` | ✅ agent.interaction_respond() | ❌ | |
| `ModeChanged` | ✅ agent.set_mode_str() | ❌ | |
| `ExecuteSlashCommand` | ✅ agent.send_message_stream() | ❌ | |
| All others | ❌ (only app.on_message) | ❌ (only app.on_message) | |

**3 messages have duplicated handling** in both paths, with `process_message` being a degraded sync copy that can't call async agent methods.

### Submit Flow (Enter → LLM Stream)

```
Enter key
  → input.handle(Submit) → Some(content)
  → handle_submit(content)
     ├── starts with '/' → handle_slash_command()
     ├── starts with ':' → handle_repl_command()
     ├── starts with '!' → handle_shell_command()
     └── else:
          1. submit_user_message(content)     ← UI update (spinner, bubble)
          2. return Action::Send(UserMessage)  ← triggers stream in runner

runner.process_action(Send(UserMessage)):
  1. bridge.ring.push(SessionEvent)
  2. agent.send_message_stream(content) → active_stream
  3. app.on_message(UserMessage) → guarded by is_streaming() to prevent double-add
```

### Deferred Queue Flow

```
During streaming, Enter pressed:
  → Action::Send(QueueMessage(content))
  → app.on_message(QueueMessage)
  → is_streaming() → push to deferred_messages

Stream completes:
  → app.on_message(StreamComplete)
  → finalize_streaming()
  → process_deferred_queue()
  → pops next → submit_user_message() + Action::Send(UserMessage)
  → process_message handles the action recursively
```

### :command Dispatch

```
":clear" → handle_repl_command()
  → reset_session() + Action::Send(ClearHistory)

":set model gpt-4" → handle_set_command()
  → Action::Send(SwitchModel("gpt-4"))

":set thinkingbudget 1024" → handle_set_command()
  → Action::Send(SetThinkingBudget(1024))

":model" (no arg) → open model picker popup
":model gpt-4" → Action::Send(SwitchModel("gpt-4"))
":q" / ":quit" → Action::Quit
```

---

## 2. CLI ↔ Daemon RPC Boundary

### All 41 RPC Methods — Usage Status

| # | RPC Method | DaemonClient method | Called from CLI? | Called internally? | Status |
|---|---|---|---|---|---|
| 1 | `ping` | `ping()` | ✅ daemon.rs | | LIVE |
| 2 | `shutdown` | `shutdown()` | ✅ daemon.rs | | LIVE |
| 3 | `daemon.capabilities` | `capabilities()` | | ✅ check_version() | LIVE (indirect) |
| 4 | `kiln.open` | `kiln_open()` | ✅ storage.rs | | LIVE |
| 5 | `kiln.close` | `kiln_close()` | | | **DEAD** |
| 6 | `kiln.list` | `kiln_list()` | ✅ daemon.rs | | LIVE |
| 7 | `search_vectors` | `search_vectors()` | | ✅ storage.rs | LIVE |
| 8 | `list_notes` | `list_notes()` | | ✅ storage.rs | LIVE |
| 9 | `get_note_by_name` | `get_note_by_name()` | | ✅ storage.rs | LIVE |
| 10 | `note.upsert` | `note_upsert()` | | ✅ storage.rs | LIVE |
| 11 | `note.get` | `note_get()` | | ✅ storage.rs | LIVE |
| 12 | `note.delete` | `note_delete()` | | ✅ storage.rs | LIVE |
| 13 | `note.list` | `note_list()` | | ✅ storage.rs | LIVE |
| 14 | `process_file` | `process_file()` | | | **DEAD** |
| 15 | `process_batch` | `process_batch()` | ✅ process.rs | | LIVE |
| 16 | `session.create` | `session_create()` | ✅ agent.rs, session.rs | ✅ agent.rs clear_history | LIVE |
| 17 | `session.list` | `session_list()` | ✅ agent.rs, session.rs | | LIVE |
| 18 | `session.get` | `session_get()` | ✅ session.rs | | LIVE |
| 19 | `session.pause` | `session_pause()` | ✅ session.rs | | LIVE |
| 20 | `session.resume` | `session_resume()` | ✅ agent.rs, session.rs | | LIVE |
| 21 | `session.end` | `session_end()` | ✅ session.rs | ✅ agent.rs clear_history | LIVE |
| 22 | `session.resume_from_storage` | `session_resume_from_storage()` | ✅ chat.rs, session.rs | | LIVE |
| 23 | `session.compact` | `session_compact()` | | | **DEAD** |
| 24 | `session.subscribe` | `session_subscribe()` | ✅ session.rs | ✅ agent.rs | LIVE |
| 25 | `session.unsubscribe` | `session_unsubscribe()` | | ✅ agent.rs clear_history | LIVE |
| 26 | `session.configure_agent` | `session_configure_agent()` | ✅ agent.rs, session.rs | ✅ agent.rs clear_history | LIVE |
| 27 | `session.send_message` | `session_send_message()` | ✅ session.rs | ✅ agent.rs | LIVE |
| 28 | `session.interaction_respond` | `session_interaction_respond()` | | ✅ agent.rs | LIVE |
| 29 | `session.test_interaction` | `session_test_interaction()` | | | **DEAD** |
| 30 | `session.cancel` | `session_cancel()` | | ✅ agent.rs | LIVE |
| 31 | `session.switch_model` | `session_switch_model()` | | ✅ agent.rs | LIVE |
| 32 | `session.list_models` | `session_list_models()` | | ✅ agent.rs | LIVE |
| 33 | `session.set_thinking_budget` | `session_set_thinking_budget()` | | ✅ agent.rs | LIVE |
| 34 | `session.get_thinking_budget` | `session_get_thinking_budget()` | | ✅ agent.rs | LIVE |
| 35 | `session.set_temperature` | `session_set_temperature()` | | ✅ agent.rs | LIVE |
| 36 | `session.get_temperature` | `session_get_temperature()` | | ✅ agent.rs | LIVE |
| 37 | `session.set_max_tokens` | `session_set_max_tokens()` | | ✅ agent.rs | LIVE |
| 38 | `session.get_max_tokens` | `session_get_max_tokens()` | | ✅ agent.rs | LIVE |
| 39 | `session.add_notification` | `session_add_notification()` | | | **DEAD** |
| 40 | `session.list_notifications` | `session_list_notifications()` | | | **DEAD** |
| 41 | `session.dismiss_notification` | `session_dismiss_notification()` | | | **DEAD** |

### Dead RPC Methods (7 total)

| Method | Notes |
|---|---|
| `kiln.close` | Opened kilns are never explicitly closed |
| `process_file` | Only `process_batch` is used |
| `session.compact` | Feature not wired to UI yet |
| `session.test_interaction` | Debug/test method, never called |
| `session.add_notification` | Notification system not wired to daemon |
| `session.list_notifications` | Notification system not wired to daemon |
| `session.dismiss_notification` | Notification system not wired to daemon |

### Connection Topology

```
TUI Session (typical):
  ┌─────────────────────────────────────────────┐
  │ Unix Socket Connection 1 (event mode)        │
  │  → DaemonAgentHandle (agent RPC + events)    │
  ├─────────────────────────────────────────────┤
  │ Unix Socket Connection 2 (simple mode)       │
  │  → DaemonStorageClient (note/search RPC)     │
  └─────────────────────────────────────────────┘

CLI Commands (ephemeral):
  ┌─────────────────────────────────────────────┐
  │ Unix Socket Connection (simple mode)         │
  │  → One-shot RPC, then disconnect             │
  └─────────────────────────────────────────────┘
```

No global singleton — each subsystem opens its own connection.

---

## 3. AgentHandle Implementations

### Trait Method × Implementation Matrix

| Method | Required? | DaemonAgent | RigAgent | AcpClient | DynamicAgent |
|---|---|---|---|---|---|
| `send_message_stream` | **YES** | ✅ RPC stream | ✅ Rig streaming | ✅ ACP subprocess | ✅ delegates |
| `is_connected` | **YES** | ✅ field | ✅ always true | ✅ session check | ✅ delegates |
| `set_mode_str` | **YES** | ✅ local store | ✅ validates + rebuild | ✅ delegates | ✅ delegates |
| `send_message` | default | default | default | default | default |
| `supports_streaming` | default | ✅ true | ✅ true | default true | ✅ delegates |
| `on_commands_update` | default | ✅ no-op | default | default | ✅ delegates |
| `get_modes` | default | ✅ None | ✅ Some(modes) | default None | ✅ delegates |
| `get_mode_id` | default | ✅ field | ✅ field | ✅ field | ✅ delegates |
| `get_commands` | default | ✅ empty | default | ✅ from session | ✅ delegates |
| `clear_history` | default | ✅ full reset | ✅ clears vec | ✅ delegates | ✅ delegates |
| `switch_model` | default | ✅ RPC | ✅ rebuild flag | default ❌ | ✅ delegates |
| `current_model` | default | ✅ cached | ✅ field | default None | ✅ delegates |
| `available_models` | default | default | default | default | default |
| `fetch_available_models` | default | ✅ RPC | ✅ HTTP /api/tags | default | ✅ delegates |
| `cancel` | default | ✅ RPC | default no-op | default | **❌ NOT DELEGATED** |
| `set_thinking_budget` | default | ✅ RPC | ✅ rebuild | default ❌ | **❌ NOT DELEGATED** |
| `get_thinking_budget` | default | ✅ cached | ✅ cached | default None | **❌ NOT DELEGATED** |
| `set_temperature` | default | ✅ RPC | ✅ rebuild | default ❌ | **❌ NOT DELEGATED** |
| `get_temperature` | default | ✅ cached | ✅ cached | default None | **❌ NOT DELEGATED** |
| `set_max_tokens` | default | ✅ RPC | ✅ rebuild | default ❌ | **❌ NOT DELEGATED** |
| `get_max_tokens` | default | ✅ cached | ✅ cached | default None | **❌ NOT DELEGATED** |
| `interaction_respond` | default | ✅ RPC | default ❌ | default ❌ | ✅ delegates |
| `take_interaction_receiver` | default | ✅ returns rx | default None | default None | ✅ delegates |

### DynamicAgent Delegation Gap (RESOLVED)

`DynamicAgent` was eliminated entirely in `bccd68a6`. The TUI now holds `Box<dyn AgentHandle>` directly, removing the delegation layer and its 7 missing method delegations.
The error is logged but the setting silently fails to reach the daemon.

### Creation Flow

```
create_agent(config, params)
  ├── try create_daemon_agent() → Box<DaemonAgentHandle>
  │     └── on failure: fall through ↓
  ├── AgentType::Internal → create_internal_agent() → Box<RigAgentHandle<M>>
  └── AgentType::Acp → discover + spawn → CrucibleAcpClient

commands/chat.rs wraps result:
  InitializedAgent::Internal(handle) → DynamicAgent::Local(handle)
  InitializedAgent::Acp(client)      → DynamicAgent::Acp(Box::new(client))

chat_runner receives: Box<DynamicAgent> as Box<dyn AgentHandle>
```

The Rig path is **reachable** as fallback (daemon down + auto-start fails, or `--local` flag).
It is **not dead code**, but in normal operation the daemon path wins.

---

## 4. Identified Redundancies & Issues

### HIGH — Should Fix

| # | Issue | Location | Status |
|---|---|---|---|
| **H1** | DynamicAgent missing 7 method delegations | `dynamic_agent.rs` | ✅ FIXED — DynamicAgent eliminated, 289 lines removed (`bccd68a6`) |
| **H2** | process_message duplicates 3 handlers from process_action | `chat_runner.rs` | ✅ FIXED — Dead arms removed (`db1d941b`) |

### MEDIUM — Should Clean Up

| # | Issue | Location | Status |
|---|---|---|---|
| **M1** | 7 dead RPC methods | `client.rs` | ✅ FIXED — 121 lines removed (`a1270751`) |
| **M2** | DaemonAgentHandle overrides 4 methods identically to defaults | `agent.rs` | ✅ FIXED — Redundant overrides removed (`612bdbba`) |
| **M3** | Dual socket connections per TUI session | `factories/` | DEBUNKED — Interactive TUI already uses one connection |
| **M4** | `:model name` and `:set model name` are duplicate paths to SwitchModel | `chat_app.rs` | ✅ FIXED — `:model <name>` delegates to `:set model` (`1c78bb67`) |

### LOW — Nice to Have

| # | Issue | Location | Status |
|---|---|---|---|
| **L1** | `send_message` default impl (collect stream) is never called | All impls use `send_message_stream` | DEFERRED — Has one production caller, leave as-is |
| **L2** | `ListModels` session command calls sync `available_models()` (always empty) | `chat_runner.rs` | ✅ FIXED — Now calls async `fetch_available_models()` (`d14fde23`) |
| **L3** | RigAgentHandle doesn't implement thinking_budget despite having the field | `handle.rs` | ✅ FIXED — `set_thinking_budget`/`get_thinking_budget` wired (`00136861`) |

---

## 5. Simplification Status

All actionable items from the analysis have been resolved:

| Phase | Items | Status |
|---|---|---|
| **Phase 1 — Fix bugs** | H1 (DynamicAgent), L2 (ListModels), L3 (thinking_budget) | ✅ Complete |
| **Phase 2 — Reduce duplication** | H2 (sync dispatch), M2 (trait overrides), M4 (:model path) | ✅ Complete |
| **Phase 3 — Remove dead code** | M1 (dead RPC methods) | ✅ Complete |
| **Deferred** | M3 (debunked), L1 (has caller) | N/A |
