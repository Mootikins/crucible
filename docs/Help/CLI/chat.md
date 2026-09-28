---
title: "cru chat"
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

#### `-a, --acp <PROFILE>`

ACP profile to use — an external agent subprocess. Skips the splash screen and
connects directly. `--agent` is the older spelling of this flag and still works.

```bash
cru chat --acp claude
cru chat --acp gemini
cru chat --acp codex
```

Available profiles: `claude`, `gemini`, `codex`, `cursor`, `opencode`, `hermes`, `antigravity`, or any custom profile defined in `init.lua`. The agent must be installed and available in your PATH; `cru agents` reports which are.

#### `--card <NAME>`

Start interactive or one-shot chat on an [[Help/Extending/Agent Cards|agent card]]:

```bash
cru chat --card researcher
cru chat --card researcher "Review this design"
```

The daemon resolves the card in the selected workspace and kiln scope; the
CLI does not discover or compose it. An unknown card creates no session.
`--card` cannot combine with `--acp`, `--resume` or `--replay`. `--agent`
remains an alias for `--acp` here; `cru session create --agent <card>` keeps
its existing card meaning. Use `cru agents` to see available names.

#### `--provider <PROVIDER>`

LLM provider from your `llm.providers` config section.

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

### Mode & Configuration

#### `--plan`

Start in plan mode (read-only) instead of normal mode. The agent can search and read notes but can't execute write operations. Toggle during a session with `/plan` and `/default` commands.

```bash
cru chat --plan
```

#### `--set <KEY[=VALUE]>`

Session configuration overrides using the same syntax as the TUI `:set` command. Can be repeated.

```bash
cru chat --set model=llama3 --set contextstrategy=truncate
cru chat --set perm.autoconfirm_session
```

#### `-e, --env <KEY=VALUE>`

Environment variables to pass to the ACP agent. Can be repeated.

```bash
cru chat --acp claude --env ANTHROPIC_BASE_URL=http://localhost:4000
```

### Runtime

#### `--standalone`

Run with an in-process daemon instead of connecting to the background server. Useful for single-session use, restricted environments, or testing. Data persists to the kiln's `.crucible/` directory.

```bash
cru chat --standalone
```

### Display

The chat draws full screen, on the alternate screen. The TUI scrolls, selects and copies. On exit, the TUI prints the transcript to the main screen, so the session stays in the terminal scrollback.

| Key | Action |
|-----|--------|
| `PageUp` / `PageDown`, mouse wheel | Scroll the transcript. A scroll to the bottom follows new text again. |
| Drag, double click, triple click | Select text, a word or a line. The button release copies the selection. |
| `Esc` | Clear the selection. |
| `F2` | Turn mouse capture off and on. With capture off, the terminal selects text. |
| `F3` | Print the finished transcript into the terminal scrollback. |

The copy goes through OSC 52 first. Outside SSH, it also goes to the native clipboard. Inside tmux, it also goes to the tmux buffer.

The full-screen mode has no search yet. To search, press `F3` and use the search of the terminal, or use `--inline`. With a long transcript, a change of the terminal width can take a short time to draw.

#### `--inline`

Draw the chat on the main screen for this run. The chat then prints into the terminal, and the terminal owns the scroll, the selection and the scrollback. The TUI does not capture the mouse.

```bash
cru chat --inline
```

To make the inline mode the default, set `cli.screen` in the config (see [[Help/Configuration]]):

```lua
cru.config.set({ cli = { screen = "inline" } })
```

The chat also uses the inline mode when stdout is not a terminal. The setup prompts (the first-run wizard, the kiln prompt and `cru init`) always print on the main screen, before the chat starts.

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
| `/mode` | Cycle through chat modes |
| `/default` | Switch to normal (ask-for-writes) mode |
| `/plan` | Switch to plan (read-only) mode |
| `/auto` | Switch to auto (full access) mode |
| `/undo [N]` | Undo the last N exchanges (default 1) |
| `/help [topic]` | Show help (same as `:help`) |
| `/model [name]` | Switch the model, or list the models with no name |
| `/resume [id]` | Resume an earlier session: a picker, or the session with that id |
| `/export [path]` | Export the session to markdown |
| `/search <query>` | Search sessions |

The daemon builds one command catalog per session: the built-in commands
above, each declared mode, each plugin command, each discovered skill, and
each command an ACP agent advertises. A Lua-declared `review` mode is
reachable as `/review`. When two sources name the same command, the earlier
source in that order keeps it. Anything typed with a leading `/` that names
no command in the catalog is **not** an error: it is forwarded to the agent
as ordinary chat text.

### REPL Commands

| Command | Description |
|---------|-------------|
| `:model` | Open model picker popup |
| `:model <name>` | Switch to specific model |
| `:set option=value` | Set runtime config option |
| `:quit` / `:q` | Exit chat |

See [[Help/TUI/Commands]] for complete REPL command reference.

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+C` | Cancel / Exit |
| `Ctrl+T` | Toggle thinking display |
| `Shift+Tab` | Cycle mode (Normal, Plan, Auto) |

## Agent Access

In chat mode, the agent has access to these tools:

**Read operations:**
- `semantic_search` - Find conceptually related notes
- `grep_notes` - Find exact text matches
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
cru chat --acp claude "Summarize my notes on API design"
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
cru chat --provider ollama --set model=llama3.2 --set contextstrategy=truncate
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

For models that reason (Claude with extended thinking, DeepSeek-R1, etc.),
Crucible sets no cap: the model reasons as much as it decides to. The `:set`
keys below control only whether the TUI shows the reasoning:

```
:set thinking               # Show thinking in UI
:set nothinking             # Hide thinking display
```

Toggle thinking display with `Ctrl+T`.

## Session Resume

Sessions auto-save and can be resumed:

```bash
cru session list                          # See available sessions
cru chat --resume chat-20250102-1430-a1b2 # Resume specific session
cru session open chat-20250102-1430-a1b2  # Same as chat --resume
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

## See Also

- [[Help/TUI/Commands]] - REPL command reference
- [[Help/TUI/Keybindings]] - Keyboard shortcuts
- [[Help/Core/Sessions]] - Session management
- [[Help/Config/llm]] - LLM configuration
- [[Help/Config/agents]] - Agent configuration
- [[Help/Concepts/Agent Client Protocol]] - ACP specification
