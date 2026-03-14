---
title: Sessions
description: Track conversation and execution history
status: implemented
tags:
  - sessions
  - logging
  - daemon
---

# Sessions

A session is a continuous sequence of events: a conversation with an AI agent, including tool calls, thinking, and responses. Sessions provide audit trails, enable resumption, and persist as markdown files.

## Architecture

Sessions follow Crucible's "plaintext first" philosophy:

- **Markdown is truth** — Each session saves as a markdown file
- **Daemon manages state** — the daemon tracks active sessions via RPC
- **Resume anytime** — Pick up previous sessions with `cru session resume`

### Daemon Integration

Sessions are managed by the daemon (`cru daemon serve`):

```
┌─────────────────┐         ┌─────────────────────┐
│ cru chat        │◄───────►│ cru daemon serve    │
│                 │ JSON-RPC│                     │
│ TUI/CLI client  │         │ SessionManager      │
│                 │         │ AgentManager        │
└─────────────────┘         └─────────────────────┘
```

**RPC Methods:**
- `session.create` — Start new session
- `session.list` — List all sessions
- `session.get` — Get session details
- `session.load` — Load a persisted session from storage into daemon memory
- `session.pause` — Pause a running session
- `session.unpause` — Unpause a paused session (programmatic, no TUI)
- `session.resume` — Resume a session in the TUI (interactive)
- `session.end` — End session
- `session.send_message` — Send a message and stream the response
- `session.configure_agent` — Configure the agent for a session
- `session.subscribe` / `session.unsubscribe` — Event streaming
- `session.archive` — Archive a session (remove from active list)
- `session.unarchive` — Restore an archived session to active
- `session.delete` — Permanently delete a session

> **`resume` vs `unpause`**: These are different operations. `session.resume` opens the session in the interactive TUI for human use. `session.unpause` reactivates a paused daemon session programmatically, without opening a TUI. Scripts and automation tools should use `unpause`; humans picking up a conversation should use `resume`.

## Session Storage

Sessions are saved to your kiln's sessions directory. Session IDs follow the format `chat-YYYYMMDD-HHMM-xxxx` (e.g., `chat-20250102-1430-a1b2`).

```
~/your-workspace/sessions/
├── chat-20250102-1430-a1b2.jsonl
├── chat-20250121-0900-c3d4.jsonl
└── ...
```

### Session Log Format

Sessions are readable markdown when exported:

```markdown
---
session_id: chat-20250102-1430-a1b2
workspace: /home/user/project
model: claude-3-5-sonnet
started: 2025-01-02T14:30:00Z
---

# Chat Session

## User
Find all notes tagged #project and summarize them

## Assistant
I'll search for notes with the project tag.

### Tool Call: search_by_tags
```json
{"tags": ["project"]}
```

### Tool Result
Found 12 notes matching #project.

Let me read through these and create a summary...

## User
Tell me more about Project Beta
```

## CLI Commands

Crucible provides a full set of session subcommands. They split into two groups: **user-facing** commands for everyday use, and **daemon** commands for programmatic session control.

### User-Facing Commands

#### List Sessions

```bash
cru session list
```

Shows recent sessions with ID, workspace, and timestamp.

#### Search Sessions

```bash
cru session search "rust"
```

Search sessions by title or content.

#### Show Session Details

```bash
cru session show chat-20250102-1430-a1b2
```

Display details for a specific session, including model, message count, and timestamps.

#### Resume a Session (Interactive TUI)

```bash
cru session resume chat-20250102-1430-a1b2
```

Opens the session in the interactive TUI. The conversation history loads and you can continue chatting. This is the command humans use to pick up where they left off.

#### Export Session

```bash
cru session export chat-20250102-1430-a1b2 -o session.md
```

Export a session to a standalone markdown file.

#### Reindex Sessions

```bash
cru session reindex
```

Rebuild the session index from JSONL files on disk. Useful after manual edits or recovery.

#### Cleanup Old Sessions

```bash
cru session cleanup
```

Remove old or orphaned sessions.

### Daemon Commands

These commands control sessions at the daemon level. They're designed for scripts, automation, and multi-client workflows rather than interactive use.

#### Create a Session

```bash
cru session create
cru session create --agent claude
```

Create a new daemon session. Optionally specify an agent profile.

#### Pause a Session

```bash
cru session pause chat-20250102-1430-a1b2
```

Pause a running daemon session. The session stays in memory but stops processing.

#### Unpause a Session

```bash
cru session unpause chat-20250102-1430-a1b2
```

Unpause a paused daemon session. This reactivates the session programmatically without opening a TUI. Use this in scripts and automation workflows. For interactive use, prefer `cru session resume` instead.

#### End a Session

```bash
cru session end chat-20250102-1430-a1b2
```

End a daemon session. The session is finalized and persisted.

#### Send a Message

```bash
cru session send chat-20250102-1430-a1b2 "Analyze the auth module"
```

Send a message to a session and stream the response. Useful for non-interactive, scripted interactions with an agent.

#### Configure Agent

```bash
cru session configure chat-20250102-1430-a1b2 --model gpt-4o
```

Configure the agent for a session (model, temperature, tools, etc.).

#### Load a Session

```bash
cru session load chat-20250102-1430-a1b2
```

Load a persisted session from storage into daemon memory. This doesn't open a TUI; it just makes the session available for daemon operations like `send`, `pause`, or `configure`.

### Start a New Chat

```bash
cru chat                     # Auto-creates a new session
```

Running `cru chat` starts a fresh session. If there's a recent session for the current workspace, it may auto-resume.

## In-TUI Session Management

### Resume on Send

When you start `cru chat`, if there's a recent session for the current workspace, it auto-resumes. Your first message continues the previous conversation.

### Switch Sessions

Use the `:session` command:

```
:session list              # Show available sessions
:session new               # Start fresh session
```

## Session Archiving

Sessions have two states: **active** and **archived**. Active sessions appear in `session.list` by default. Archived sessions are hidden unless you explicitly ask for them.

### Active vs Archived

| | Active | Archived |
|---|---|---|
| Visible in `session.list` | Yes | Only with `include_archived: true` |
| Can receive messages | Yes | No (must unarchive first) |
| Counts toward "recent sessions" | Yes | No |
| Data preserved | Yes | Yes |
| Can be resumed | Yes | Yes (after unarchive) |

Archiving doesn't delete anything. It moves the session out of the active view so your session list stays manageable. Unarchive brings it back.

### Manual Archive and Unarchive

Use the `session.archive` and `session.unarchive` RPC methods. Both require `session_id` and `kiln` parameters:

```bash
# Archive a session
cru session archive chat-20250102-1430-a1b2

# Unarchive it later
cru session unarchive chat-20250102-1430-a1b2
```

### Deleting Sessions

To permanently remove a session, use `session.delete`. This cleans up both the daemon state and the agent resources. Requires `session_id` and `kiln` parameters:

```bash
cru session delete chat-20250102-1430-a1b2
```

Deletion is irreversible. The session's JSONL file and any associated daemon state are removed.

### Listing Archived Sessions

By default, `session.list` only returns active sessions. Pass `include_archived: true` to see everything:

```bash
cru session list --all    # includes archived sessions
```

### Auto-Archive

The daemon automatically archives stale sessions. A background sweep runs every 30 minutes, checking for sessions that have been inactive (no messages, no subscribers) beyond a configurable threshold.

**Default threshold:** 72 hours of inactivity.

Configure it in `~/.config/crucible/config.toml`:

```toml
[server]
auto_archive_hours = 72    # default; set to 0 to disable
```

The sweep skips sessions that have active subscribers (connected clients). It also re-checks activity timestamps before archiving to avoid race conditions where a session receives new activity between the staleness check and the archive operation.

Auto-archived sessions can be unarchived at any time. No data is lost.

## Session Configuration

Session behavior is configured through the `[chat]` section in `~/.config/crucible/config.toml`. The daemon manages session persistence automatically; sessions always save to your kiln's `sessions/` directory.

```toml
[chat]
# Default chat model (can be overridden per session)
# model = "llama3.2"

# Enable markdown rendering in terminal output
enable_markdown = true

# Show thinking/reasoning tokens from models that support it
# show_thinking = false
```

## Agent Configuration per Session

Each session tracks agent configuration:

- **Model** — LLM model (e.g., `claude-3-5-sonnet`, `gpt-4o`)
- **Thinking Budget** — Token budget for extended thinking
- **Temperature** — Response randomness
- **Tools** — Available MCP tools

Change mid-session via `:set` or `:model`:

```
:set model claude-3-5-sonnet
:set thinkingbudget 8000
:model gpt-4o                   # Opens model picker
```

Changes persist for the session and sync across connected clients.

## Events

Sessions emit events that can be subscribed to via RPC:

| Event | Description |
|-------|-------------|
| `stream_start` | Response streaming begins |
| `stream_chunk` | Text chunk received |
| `stream_end` | Response complete |
| `tool_call` | Tool invocation |
| `tool_result` | Tool completed |
| `thinking` | Thinking/reasoning tokens |
| `error` | Error occurred |

Subscribe from external tools:

```bash
cru session subscribe <session-id>
```

## See Also

- [[Help/CLI/chat]] — Interactive chat command
- [[Help/TUI/Commands]] — TUI REPL commands
- [[Help/Config/agents]] — Agent configuration
