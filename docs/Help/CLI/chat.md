---
title: chat
description: Interactive AI chat with your knowledge base
tags:
  - reference
  - cli
  - chat
---

# cru chat

Start an interactive AI chat session with access to your kiln.

## Synopsis

```
cru chat [OPTIONS] [QUERY]
```

Running `cru` with no arguments starts chat mode.

## Arguments

| Argument | Description |
|----------|-------------|
| `[QUERY]` | Optional one-shot query. If omitted, starts interactive mode. |

## Description

The chat command connects an AI agent to your knowledge base. The agent can search, read, and explore your notes. In normal mode it has full tool access. Switch to plan mode for read-only exploration, or auto mode to skip tool confirmation prompts.

## Options

### Agent Selection

#### `-a, --agent <AGENT>`

Preferred ACP agent to use. Skips the splash screen and connects directly.

```bash
cru chat --agent claude-code
cru chat --agent gemini-cli
cru chat --agent codex
```

Available agents: `claude-code`, `gemini-cli`, `codex`, `cursor`, or any custom profile defined in `crucible.toml`. The agent must be installed and available in your PATH.

#### `--provider <PROVIDER>`

LLM provider from your `[llm.providers]` config section.

```bash
cru chat --provider openai
cru chat --provider ollama
```

### Session Management

#### `-r, --resume <SESSION_ID>`

Resume a previous session by ID. Session IDs follow the format `chat-YYYYMMDD-HHMM-xxxx`.

```bash
cru chat --resume chat-20250102-1430-a1b2
```

#### `--record <FILE>`

Record the TUI session to a JSONL file for later replay.

```bash
cru chat --record session-recording.jsonl
```

#### `--replay <FILE>`

Replay a previously recorded JSONL session.

```bash
cru chat --replay session-recording.jsonl
```

#### `--replay-speed <N>`

Playback speed multiplier for replay (default: 1.0).

#### `--replay-auto-exit [<DELAY_MS>]`

Auto-exit after replay completes. Optional delay in milliseconds (default: 2000).

### Context & Knowledge Base

#### `--no-context`

Skip context enrichment. Faster startup, but the agent won't have knowledge base access.

```bash
cru chat --no-context "What's 2+2?"
```

#### `--max-context <TOKENS>`

Maximum context window tokens (default: 16384).

#### `--context-size <N>`

Number of context results to include (default: 5).

### Mode & Configuration

#### `--plan`

Start in plan mode (read-only) instead of normal mode. The agent can search and read notes but can't execute write operations. Toggle during a session with `/plan` and `/normal` commands.

```bash
cru chat --plan
```

#### `--set <KEY[=VALUE]>`

Session configuration overrides using the same syntax as the TUI `:set` command. Can be repeated.

```bash
cru chat --set model=llama3 --set temperature=0.5
cru chat --set perm.autoconfirm_session
```

#### `-e, --env <KEY=VALUE>`

Environment variables to pass to the ACP agent. Can be repeated.

```bash
cru chat --agent claude-code --env ANTHROPIC_BASE_URL=http://localhost:4000
```

### Runtime

#### `--standalone`

Run with an in-process daemon instead of connecting to the background server. Useful for single-session use, restricted environments, or testing. Data persists to the kiln's `.crucible/` directory.

```bash
cru chat --standalone
```

## Chat Modes

Crucible has three chat modes. Cycle between them with `Shift+Tab` during a session.

### Normal Mode (Default)

Full tool access. The agent can search, read, create, modify, and delete notes. Tool calls prompt for confirmation before executing.

### Plan Mode

Read-only. The agent can search and read your notes, but write operations are blocked. Good for exploration and brainstorming without risk of changes.

Toggle with `/plan` or start directly:

```bash
cru chat --plan
```

### Auto Mode

Full tool access with automatic approval. Tool calls execute without confirmation prompts. Useful for trusted workflows where you don't want to approve every action.

## In-Chat Commands

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/plan` | Switch to plan (read-only) mode |
| `/normal` | Switch to normal (full access) mode |
| `/clear` | Clear conversation history |
| `/agent <name>` | Switch to a different agent |

### REPL Commands

| Command | Description |
|---------|-------------|
| `:model` | Open model picker popup |
| `:model <name>` | Switch to specific model |
| `:set option=value` | Set runtime config option |
| `:set thinkingbudget=high` | Enable extended thinking |
| `:session list` | List available sessions |
| `:session load <id>` | Resume existing session |
| `:quit` / `:q` | Exit chat |

See [[Help/TUI/Commands]] for complete REPL command reference.

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+C` | Cancel / Exit |
| `Alt+T` | Toggle thinking display |
| `Shift+Tab` | Cycle mode (Normal, Plan, Auto) |

## Agent Access

In chat mode, the agent has access to these tools:

**Read operations:**
- `semantic_search` - Find conceptually related notes
- `text_search` - Find exact text matches
- `property_search` - Filter by metadata
- `read_note` - Read note contents

**Write operations (normal and auto modes):**
- `create_note` - Create new notes
- `update_note` - Modify existing notes
- `delete_note` - Remove notes (with confirmation in normal mode)

## Examples

### Quick Question

```bash
cru chat "What do I know about project management?"
```

### Interactive Session

```bash
cru
```

Then ask questions:
```
You: What are my notes about productivity?

Agent: I found several notes related to productivity...

You: Can you summarize the key techniques?

Agent: Based on your notes, the main techniques are...
```

### Use a Specific ACP Agent

```bash
cru chat --agent claude-code "Summarize my notes on API design"
```

### Resume a Previous Session

```bash
cru chat --resume chat-20250102-1430-a1b2
```

### Plan Mode Exploration

```bash
cru chat --plan "What patterns do my testing notes share?"
```

### Custom Provider with Overrides

```bash
cru chat --provider ollama --set model=llama3.2 --set temperature=0.7
```

### Record and Replay

```bash
# Record a session
cru chat --record demo.jsonl

# Replay it later
cru chat --replay demo.jsonl --replay-speed 2.0
```

## Model Switching

Change models at runtime without restarting:

```
:model                      # Opens model picker
:model claude-3-5-sonnet    # Switch directly
:model gpt-4o
```

Model changes persist for the session and sync to the daemon.

## Extended Thinking

For models that support reasoning tokens (Claude with thinking budget, DeepSeek-R1, etc.):

```
:set thinkingbudget=high    # Enable extended thinking (8192 tokens)
:set thinkingbudget=off     # Disable thinking
:set thinking               # Show thinking in UI
:set nothinking             # Hide thinking display
```

Toggle thinking display with `Alt+T`.

**Presets:** `off`, `minimal` (512), `low` (1024), `medium` (4096), `high` (8192), `max` (unlimited)

## Session Resume

Sessions auto-save and can be resumed:

```bash
cru session list                          # See available sessions
cru chat --resume chat-20250102-1430-a1b2 # Resume specific session
```

Or from within chat:
```
:session list
:session load chat-20250102-1430-a1b2
```

## Statusline Notifications

The statusline displays notifications when files change in your kiln:

- **File changes** appear dimmed on the right side (e.g., "notes.md modified")
- **Multiple changes** batch together (e.g., "3 files modified")
- **Errors** appear in red and stay visible longer

Notification timing:
- Info notifications: 2 seconds
- Error notifications: 5 seconds

This provides real-time feedback when other tools or editors modify your notes while you're chatting.

## Tips

### Effective Prompts

Be specific about what you want:
```
"Find notes about React hooks and summarize the patterns I use"
```

vs

```
"What do I have about React?"
```

### Building Context

The agent remembers conversation history. Build on previous answers:
```
You: What notes do I have about testing?
Agent: [Lists notes]
You: Focus on the integration testing ones
Agent: [Narrows down]
You: What patterns do they share?
```

### Verification

Ask the agent to cite sources:
```
"What's my approach to error handling? Cite the specific notes."
```

## Implementation

**Source code:** `crates/crucible-cli/src/commands/chat.rs`

## See Also

- [[Help/TUI/Commands]] - REPL command reference
- [[Help/TUI/Keybindings]] - Keyboard shortcuts
- [[Help/Core/Sessions]] - Session management
- [[Help/Config/llm]] - LLM configuration
- [[Help/Config/agents]] - Agent configuration
- [[Help/Concepts/Agent Client Protocol]] - ACP specification
