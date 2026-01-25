---
description: Track conversation and execution history
status: implemented
tags:
  - sessions
  - logging
  - daemon
---

# Sessions

A session is a continuous sequence of events — a conversation with an AI agent, including tool calls, thinking, and responses. Sessions provide audit trails, enable resumption, and persist as markdown files.

## Architecture

Sessions follow Crucible's "plaintext first" philosophy:

- **Markdown is truth** — Each session saves as a markdown file
- **Daemon manages state** — `cru-server` tracks active sessions via RPC
- **Resume on restart** — Load previous sessions with `cru session load`

### Daemon Integration

Sessions are managed by the daemon (`cru-server`):

```
┌─────────────────┐         ┌─────────────────────┐
│ cru chat        │◄───────►│ cru-server          │
│                 │ JSON-RPC│                     │
│ TUI/CLI client  │         │ SessionManager      │
│                 │         │ AgentManager        │
└─────────────────┘         └─────────────────────┘
```

**RPC Methods:**
- `session.create` — Start new session
- `session.list` — List all sessions
- `session.get` — Get session details
- `session.load` — Load/resume existing session
- `session.pause` / `session.resume` — Pause/resume session
- `session.end` — End session
- `session.subscribe` / `session.unsubscribe` — Event streaming

## Session Storage

Sessions are saved to your workspace sessions directory:

```
~/your-workspace/sessions/
├── project-name/
│   ├── 2025-01-20_1430.md    # Session log
│   ├── 2025-01-21_0900.md    # Another session
│   └── ...
```

### Session Log Format

Sessions are readable markdown:

```markdown
---
session_id: ses_abc123
workspace: /home/user/project
model: claude-3-5-sonnet
started: 2025-01-20T14:30:00Z
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

### List Sessions

```bash
cru session list
```

Shows all sessions with ID, workspace, and timestamp.

### Load/Resume Session

```bash
cru session load <session-id>
```

Resumes an existing session. The conversation history is loaded and you can continue chatting.

### Start New Session

```bash
cru chat                     # Auto-creates session
cru chat --session new       # Explicit new session
```

## In-TUI Session Management

### Resume on Send

When you start `cru chat`, if there's a recent session for the current workspace, it auto-resumes. Your first message continues the previous conversation.

### Switch Sessions

Use the `:session` command:

```
:session list              # Show available sessions
:session load ses_abc123   # Switch to session
:session new               # Start fresh session
```

## Session Configuration

Configure session behavior in `~/.config/crucible/config.toml`:

```toml
[chat]
# Auto-save sessions (default: true)
auto_save = true

# Session save directory (default: workspace/sessions/)
session_dir = "sessions"

# Auto-resume recent sessions (default: true)
auto_resume = true
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
