# Concurrent Session Daemon

## Why

Users scaling to parallel workflows face three problems:

1. **DB contention**: Multiple `cru chat` instances corrupt or lock the shared kiln's SurrealDB
2. **No coordination**: Agents in different terminals can't communicate status to the user
3. **No visibility**: No way to see what agents are doing across sessions

This change adds an ephemeral daemon that enables safe concurrent access to a kiln from multiple agent sessions, with an inbox system for human-in-the-loop coordination.

## Scope

**V1 (this proposal):**
- Single kiln DB concurrency
- Session registry within one kiln
- Inbox/HITL within one kiln
- Context stack operations

**Future work:**
- Multi-kiln access from single session
- Centralized session storage (personal kiln as "home")
- Cross-kiln queries and links
- Session ↔ DB connection separation (sessions in personal kiln, DB connections to any kiln)

## What Changes

**Ephemeral Daemon Architecture:**
- First `cru chat` in a kiln auto-starts a daemon process
- Daemon owns the SurrealDB connection, exposes Unix socket
- Subsequent `cru chat` instances connect as clients
- Last client to exit triggers daemon shutdown (with grace period)
- Socket lives in kiln: `$KILN/.crucible/daemon.sock`

**Session Registry:**
- Daemon tracks active sessions: id, worktree path, agent type, status
- Sessions register on connect, deregister on disconnect
- Persisted in SurrealDB (survives daemon restart)

**Inbox System (HITL):**
- Agents send notifications to human: decisions needed, approvals, completions, errors
- Inbox is a **view over session logs**, not separate storage
- Messages queryable via `/inbox` command or `cru inbox` CLI
- Human is the coordinator; no agent-to-agent messaging

**Context Stack:**
- Context treated as stack/deque for granular control
- Operations: `pop(n)`, `checkpoint(name)`, `rollback(name)`, `reset()`
- Enables recovery from failures without full context loss
- Different failure types need different levels of "forgetting"

**Session Navigation (Phase 2):**
- Status bar: single line showing active sessions + inbox badge
- Slash commands: `/sessions`, `/inbox`, `/goto <n>`, `/next`, `/prev`
- External muxing (tmux/wezterm tabs) for now; internal muxing future work

## Impact

### Affected Specs

- **session-daemon** (new) - Daemon lifecycle, socket protocol, session registry
- **workflow-sessions** (modify) - Add inbox message types, session registry integration
- **chat-improvements** (modify) - Add status bar, session navigation commands
- **internal-agent-system** (modify) - Add context stack operations
- **add-rune-integration** (modify) - Add `session::*`, `inbox::*`, `context::*` APIs

### Affected Code

**New Components:**
- `crates/crucible-daemon/` - NEW - Daemon process, socket server, session registry
- `crates/crucible-core/src/session/` - NEW - Session types, inbox queries

**Modified Components:**
- `crates/crucible-cli/src/chat/session.rs` - Client mode, daemon detection
- `crates/crucible-cli/src/chat/handlers/` - Session navigation commands
- `crates/crucible-cli/src/chat/display.rs` - Status bar rendering
- `crates/crucible-surrealdb/` - Session/inbox tables

### User-Facing Impact

- **Safe concurrency**: Run multiple `cru chat` in different terminals, same kiln
- **Inbox notifications**: Agents can notify you when they need input
- **Session awareness**: See all active sessions, switch between them
- **Context recovery**: Recover from failures without losing good context

### Design Principles

**Kilns are self-contained:**
- Markdown files (human-readable, any editor)
- DB + embeddings + socket all in kiln folder
- Portable: copy folder, everything moves

**Daemon is invisible:**
- Auto-starts on first chat, auto-stops on last exit
- Single-session users see no difference
- No `cru daemon start` command needed

**Human is coordinator:**
- Inbox flows agent → human only
- No agent-to-agent messaging
- User dispatches tasks, reviews results

**LLMs are stateless:**
- Context stack embraces this
- Reset + summary often better than polluted context
- Checkpoint/rollback as methodology, not just recovery
