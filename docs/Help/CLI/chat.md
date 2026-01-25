---
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
cru [chat] [MESSAGE] [OPTIONS]
```

Running `cru` with no arguments starts chat mode.

## Description

The chat command connects an AI agent to your knowledge base. The agent can search, read, and explore your notes. In act mode, it can also create and modify notes.

## Options

### `--internal`

Use Crucible's built-in agent instead of external ACP agents.

```bash
cru chat --internal "What notes do I have about Rust?"
```

### `--provider <PROVIDER>`

LLM provider for internal agent: `ollama`, `openai`, `anthropic`

```bash
cru chat --internal --provider openai "Summarize my notes"
```

### `--model <MODEL>`

Specific model to use.

```bash
cru chat --internal --provider ollama --model llama3.2
```

### `--agent <AGENT>` (ACP Mode)

Specify which ACP agent to use. Skips the splash screen and connects directly.

```bash
cru chat --agent opencode
cru chat --agent claude
```

Available agents: `opencode`, `claude`, `gemini`, `codex`, `cursor` (requires agent to be installed)

When `--agent` is specified, Crucible bypasses the splash screen and connects directly to the specified agent.

## Chat Modes

### Plan Mode (Default)

The agent can search and read, but cannot modify notes.

```
/plan
```

Safe for exploration. The agent provides information and suggestions without changing anything.

### Act Mode

The agent can create, modify, and delete notes.

```
/act
```

Enable for workflows that require changes. The agent will confirm before destructive operations.

## In-Chat Commands

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/plan` | Switch to read-only mode |
| `/act` | Enable write mode |
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
| `Shift+Tab` | Cycle mode |

## Statusline Notifications

The statusline displays notifications when files change in your kiln:

- **File changes** appear dimmed on the right side (e.g., "notes.md modified")
- **Multiple changes** batch together (e.g., "3 files modified")
- **Errors** appear in red and stay visible longer

Notification timing:
- Info notifications: 2 seconds
- Error notifications: 5 seconds

This provides real-time feedback when other tools or editors modify your notes while you're chatting.

## Agent Access

In chat mode, the agent has access to these tools:

**Read operations:**
- `semantic_search` - Find conceptually related notes
- `text_search` - Find exact text matches
- `property_search` - Filter by metadata
- `read_note` - Read note contents

**Write operations (act mode only):**
- `create_note` - Create new notes
- `update_note` - Modify existing notes
- `delete_note` - Remove notes (with confirmation)

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

### Create Note in Act Mode

```bash
cru
```

```
/act
Please create a note summarizing our discussion about API design
```

### Use Specific Agent

```bash
cru
```

```
/agent researcher
Deep dive into my notes on machine learning
```

## Provider Configuration

### Ollama (Local)

```bash
cru chat --internal --provider ollama
```

Requires Ollama running locally with a model installed.

### OpenAI

```bash
export OPENAI_API_KEY=your-key
cru chat --internal --provider openai --model gpt-4o
```

### Anthropic

```bash
export ANTHROPIC_API_KEY=your-key
cru chat --internal --provider anthropic --model claude-3-5-sonnet
```

## External Agents

By default, Crucible looks for ACP-compatible agents. To use Claude Code:

```bash
cru chat
```

This connects to `claude-code` if available in your PATH.

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

**Related modules:**
- `crates/crucible-agents/` - Agent system
- `crates/crucible-llm/` - LLM provider integration

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
cru session list            # See available sessions
cru session load ses_abc123 # Resume specific session
```

Or from within chat:
```
:session list
:session load ses_abc123
```

## See Also

- [[Help/TUI/Commands]] - REPL command reference
- [[Help/TUI/Keybindings]] - Keyboard shortcuts
- [[Help/Core/Sessions]] - Session management
- [[Help/Config/llm]] - LLM configuration
- [[Help/Config/agents]] - Agent configuration
