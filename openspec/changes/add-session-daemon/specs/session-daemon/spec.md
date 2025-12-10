# Session Daemon Specification

## Overview

The session daemon enables concurrent access to a kiln from multiple agent sessions. It manages the database connection, session registry, and inbox queries.

## Scope

**V1:** Single kiln concurrency. One daemon per kiln, sessions scoped to that kiln.

**Future (multi-kiln):** Sessions and DB connections are separate concepts:
- Sessions live in personal kiln (episodic memory, always yours)
- DB connections can target any configured kiln
- A session can access multiple kilns during one conversation
- Session metadata tracks `kilns_accessed: ["work", "personal"]` for traceability

## Daemon Lifecycle

### Auto-Start

When `cru chat` starts:

1. Check for existing socket at `$KILN/.crucible/daemon.sock`
2. If socket exists and responds to ping → connect as client
3. If socket missing or unresponsive → start daemon, then connect

```
cru chat
  │
  ├─ socket exists + responsive?
  │    ├─ yes → connect as client
  │    └─ no  → fork daemon, wait for ready, connect
  │
  └─ register session with daemon
```

### Auto-Stop

When a client disconnects:

1. Deregister session from registry
2. If no clients remain → start grace period (5s default)
3. If new client connects during grace → cancel shutdown
4. If grace expires → daemon exits, removes socket

### Crash Recovery

- Daemon writes PID to `$KILN/.crucible/daemon.pid`
- On startup, if socket exists but process dead → remove stale socket
- Sessions have heartbeat; daemon cleans up orphaned sessions after timeout

## Socket Protocol

JSON-RPC 2.0 over Unix socket.

### Session Management

```jsonc
// Register a new session
{
  "method": "session.register",
  "params": {
    "worktree": "/path/to/worktree",  // optional, null for main repo
    "agent_type": "acp",               // "acp" | "internal"
    "agent_name": "claude-code"        // agent identifier
  }
}
// Returns: { "session_id": "01HQ..." }

// List active sessions
{
  "method": "session.list"
}
// Returns: { "sessions": [...] }

// Heartbeat (keep session alive)
{
  "method": "session.heartbeat",
  "params": { "session_id": "01HQ..." }
}

// Deregister session
{
  "method": "session.deregister",
  "params": { "session_id": "01HQ..." }
}
```

### Inbox Operations

```jsonc
// Send message to inbox (from agent session)
{
  "method": "inbox.send",
  "params": {
    "session_id": "01HQ...",
    "msg_type": "decision_needed",  // see Message Types
    "title": "Which auth strategy?",
    "body": "Options:\n1. JWT\n2. Session\n3. OAuth",
    "options": ["JWT", "Session", "OAuth"]  // optional, for decisions
  }
}

// Query inbox
{
  "method": "inbox.list",
  "params": {
    "unread_only": true,  // optional, default false
    "limit": 20           // optional, default 50
  }
}
// Returns: { "messages": [...] }

// Mark messages as read
{
  "method": "inbox.mark_read",
  "params": { "ids": ["01HQ...", "01HQ..."] }
}
```

### Database Proxy

```jsonc
// Execute SurrealQL query
{
  "method": "db.query",
  "params": {
    "sql": "SELECT * FROM notes WHERE ...",
    "bindings": { "tag": "rust" }
  }
}
// Returns: { "result": [...] }
```

## Session Registry

### Schema

```surql
DEFINE TABLE sessions SCHEMAFULL;

DEFINE FIELD id ON sessions TYPE string;
DEFINE FIELD worktree ON sessions TYPE option<string>;
DEFINE FIELD agent_type ON sessions TYPE string;  -- "acp" | "internal"
DEFINE FIELD agent_name ON sessions TYPE string;
DEFINE FIELD status ON sessions TYPE string;      -- "active" | "idle"
DEFINE FIELD started_at ON sessions TYPE datetime;
DEFINE FIELD last_heartbeat ON sessions TYPE datetime;
DEFINE FIELD display_name ON sessions TYPE string;  -- computed: "worktree @ agent"

DEFINE INDEX idx_sessions_status ON sessions FIELDS status;
```

### Display Name

Format: `{worktree_name} @ {agent_name}`

Examples:
- `main @ claude-code`
- `feat/auth @ ollama`
- `test @ gemini-cli`

## Inbox System

### Philosophy

Inbox is a **view over session logs**, not separate storage:

- Messages written to session log files (markdown)
- Inbox queries aggregate unread/actionable items
- Full history preserved in logs, purgeable per-session

### Message Types

| Type | Purpose | Blocks Agent? |
|------|---------|---------------|
| `decision_needed` | Agent needs human choice | Yes |
| `approval_required` | Agent wants to do something risky | Yes |
| `task_complete` | Finished assigned work | No |
| `error` | Something went wrong | Maybe |
| `info` | FYI, no action needed | No |

### Message Schema

```surql
DEFINE TABLE inbox_messages SCHEMAFULL;

DEFINE FIELD id ON inbox_messages TYPE string;
DEFINE FIELD session_id ON inbox_messages TYPE string;
DEFINE FIELD session_name ON inbox_messages TYPE string;
DEFINE FIELD timestamp ON inbox_messages TYPE datetime;
DEFINE FIELD msg_type ON inbox_messages TYPE string;
DEFINE FIELD title ON inbox_messages TYPE string;
DEFINE FIELD body ON inbox_messages TYPE option<string>;
DEFINE FIELD options ON inbox_messages TYPE option<array<string>>;
DEFINE FIELD read ON inbox_messages TYPE bool DEFAULT false;
DEFINE FIELD responded ON inbox_messages TYPE bool DEFAULT false;
DEFINE FIELD response ON inbox_messages TYPE option<string>;

DEFINE INDEX idx_inbox_unread ON inbox_messages FIELDS read, timestamp;
DEFINE INDEX idx_inbox_session ON inbox_messages FIELDS session_id;
```

### Agent API

From within a chat session:

```rust
// Notify human (non-blocking)
ctx.notify("Finished implementing auth flow")?;

// Request decision (blocking until human responds)
let choice = ctx.request_decision(
    "Which auth strategy?",
    &["JWT", "Session", "OAuth"]
)?;

// Request approval (blocking)
let approved = ctx.request_approval("About to delete 47 files")?;
```

## Context Stack

### Model

Context is a stack where each entry is a message (human, agent, tool call, tool result):

```
┌─────────────────────────────────┐
│ Tool result: error              │ ← top (newest)
├─────────────────────────────────┤
│ Tool call: edit file X          │
├─────────────────────────────────┤
│ Agent: "I'll edit file X..."    │
├─────────────────────────────────┤
│ Human: "Fix the auth bug"       │
├─────────────────────────────────┤
│ Agent: "I found the issue..."   │
├─────────────────────────────────┤
│ System prompt                   │ ← bottom (never popped)
└─────────────────────────────────┘
```

### Operations

| Operation | Description |
|-----------|-------------|
| `pop(n)` | Remove top N entries |
| `checkpoint(name)` | Mark current position |
| `rollback(name)` | Pop until named checkpoint |
| `replace_top(summary)` | Pop top, push summary |
| `reset()` | Pop all except system prompt |
| `summarize()` | LLM-generate summary of current context |

### Failure Patterns

| Failure Type | Action | Rationale |
|--------------|--------|-----------|
| Tool error | `pop(1)` + inject error msg | Bad execution, not bad thinking |
| Wrong approach | `rollback(checkpoint)` | Keep problem understanding, discard bad path |
| Confusion spiral | `reset()` + summary | Polluted context, fresh start |
| Fundamental misunderstanding | `reset()` + human clarification | Need new information |

### Slash Commands

```
/context                    Show context stack summary
/context pop [n]            Remove last N entries (default 1)
/context checkpoint <name>  Create named checkpoint
/context rollback <name>    Rollback to checkpoint
/context reset              Clear all except system prompt
/context summarize          Replace context with LLM summary
```

## CLI Integration

### Status Bar

Single line, always visible at bottom of chat:

```
[1] main/claude  [2] feat/auth/ollama  [3] test/gemini  |  2  /sessions
```

Format: `[n] worktree/agent ... | {unread_count} | /sessions`

### Slash Commands

| Command | Description |
|---------|-------------|
| `/sessions` | Show session list with inbox summary |
| `/inbox` | Show full inbox |
| `/goto <n>` | Switch to session by number |
| `/next` or `/n` | Next session |
| `/prev` or `/p` | Previous session |
| `/new [--worktree <path>] [--agent <type>]` | Create new session |

### `cru inbox` CLI

Works outside chat sessions:

```bash
cru inbox              # List unread messages
cru inbox --all        # List all messages
cru inbox show <id>    # Show message details
cru inbox clear        # Clear read messages
```

## Configuration

In kiln's `.crucible.toml`:

```toml
[daemon]
grace_period_secs = 5       # Time before shutdown after last client
heartbeat_interval_secs = 30
session_timeout_secs = 300  # Cleanup orphaned sessions after this

[inbox]
default_limit = 50
```

## Implementation Notes

### Crate Structure

```
crates/
  crucible-daemon/
    src/
      lib.rs
      server.rs      # Socket server, request handling
      registry.rs    # Session registry
      inbox.rs       # Inbox queries
      lifecycle.rs   # Start/stop, crash recovery
```

### Dependencies

- `tokio` - Async runtime
- `tokio-util` - Unix socket codec
- `serde_json` - JSON-RPC serialization
- Uses existing `crucible-surrealdb` for DB access
