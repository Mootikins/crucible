---
description: Crucible's built-in agent system with session logging and memory
status: draft
tags:
  - extending
  - agents
  - sessions
  - memory
  - storage
aliases:
  - Internal Agent
  - Session Logging
  - Agent Memory
---

# Internal Agent

Crucible's internal agent is a built-in AI assistant that runs locally, logs sessions as markdown, and uses your kiln as its memory. Unlike external agents via [[Help/Concepts/Agents & Protocols|ACP]], the internal agent has direct access to your files and can persist state across sessions.

## Overview

The internal agent provides:
- **Session logging** - Conversations saved as markdown files
- **Task tracking** - ACP-style task lists as working memory
- **Explicit search** - Use `/search` to inject context when needed

Future: Precognition (auto-RAG), Lua hooks, session compaction.

## Memory Architecture

The agent operates with two tiers of memory, all stored as plaintext:

```
┌─────────────────────────────────────────────────┐
│              Memory Tiers                       │
├─────────────────────────────────────────────────┤
│                                                 │
│  Session Memory    Current conversation + tasks │
│                    Logged to markdown file      │
│                                                 │
│  Kiln (via /search)  Your notes + embeddings   │
│                      Explicit search injection  │
│                                                 │
└─────────────────────────────────────────────────┘
```

### Session Memory

The current conversation, task list, and tool calls. Logged to a markdown file in your personal kiln's session folder. Session files have `index: false` frontmatter so they don't bloat your embedding database.

### Kiln Search

Use `/search query` to find relevant notes and inject them into the conversation. This explicit approach keeps you in control of what context the agent sees.

## Session Files

Sessions are markdown notes with special frontmatter. They're stored in your personal kiln and can be searched, linked, and analyzed like any other note.

### Location

Sessions are stored by workspace (directory/repo name):

```
~/Documents/your-kiln/
└── sessions/
    └── <WORKSPACE_DIR>/
        ├── 2024-12-24_1930.md     # Session log
        ├── 2024-12-25_0900.md     # Another session
        └── ...
```

Where `WORKSPACE_DIR` is the name of the git repo or working directory you started the chat from.

### Session Format

```markdown
---
type: session
workspace: crucible
started: 2024-12-24T19:30:00Z
ended: 2024-12-24T21:00:00Z
---

# Session

## Tasks

- [x] Read existing agent crates
- [~] Design memory architecture
- [ ] Implement session logging

## Log

### User 19:30
Research internal agent abstractions...

### Agent 19:30:15
I'll start by exploring the existing code...

### Tool: semantic_search 19:30:20
```json
{"query": "agent handle trait", "limit": 5}
```

**Result:** Found 5 relevant notes...

---

*Session ended*
```

### Frontmatter Fields

| Field | Description |
|-------|-------------|
| `type: session` | Marks this as a session log |
| `workspace` | Workspace directory name |
| `started` | Session start timestamp |
| `ended` | Session end timestamp (added on close) |

### Embedding Exclusion

The `sessions` folder is configured as an embedding exclusion directory. This means:
- **Metadata is indexed** - searchable by properties like `type:session`
- **No embeddings generated** - saves vector DB space

```bash
# Search sessions by property
cru search --properties "type:session workspace:crucible"

# Text search within sessions folder
cru search "error handling" --folder sessions/crucible
```

## Compaction

When sessions get long, use `/compact` to summarize and continue in a new file:

1. Agent generates numbered summary of key points
2. Summary appended to current file with link to continuation
3. New file created with summary as context
4. Logging continues in new file

### File Structure

Sessions are **folders** so agents can write files to session namespace:

```
~/Documents/your-kiln/sessions/crucible/
├── 2024-12-24_1930/                    # Session folder
│   ├── log.md                          # Conversation log
│   └── ...                             # Any files agent writes
└── 2024-12-24_1930_01/                 # After compaction
    └── log.md

~/.crucible/sessions/                   # Hidden state (machine-readable)
├── index.json                          # Session discovery
└── state/crucible/
    ├── 2024-12-24_1930.json            # Full conversation state
    └── 2024-12-24_1930_01.json
```

**Why this structure:**
- Session folders let agents write scratch files
- Kiln stays clean markdown
- JSON state hidden, enables full resume
- Index enables fast session discovery

### Example

End of `2024-12-24_1930/log.md`:
```markdown
---

**Continued in:** [[2024-12-24_1930_01/log]]
```

New `2024-12-24_1930_01/log.md`:
```markdown
---
type: session
workspace: crucible
started: 2024-12-24T19:30:00Z
continued_from: 2024-12-24_1930
---

# Session (continued)

## Summary

1. Researched internal agent patterns
2. Designed session logging with markdown files
3. Decided on embedding exclusion approach

## Log

### User 20:15
...
```

## Task List

The agent uses an ACP-style task list as working memory. Tasks track progress within a session:

```markdown
## Tasks

- [x] Completed task
- [~] In progress task
- [ ] Pending task
```

### Task States

| Marker | Status | Description |
|--------|--------|-------------|
| `[ ]` | pending | Not started |
| `[~]` | in_progress | Currently working on |
| `[x]` | completed | Finished |

### Future: Lua Task Hooks

> Task validation via Lua hooks is planned for v2.

## Future: Precognition (Auto Context)

> This feature is planned for v2.

Precognition will automatically search your kiln before each LLM call and inject relevant context. For now, use `/search` explicitly.

## Future: Lua Integration

> Lua hooks and storage namespace are planned for v2.

### Storage Namespace

Plugins can use namespaced storage within the kiln:

```lua
local storage = require("cru.storage")  -- or require("crucible.storage")

-- Get a namespace for your plugin
local store = storage.namespace("my-plugin")

-- Append to a log
store:append_log("events", entry)

-- Read/write state
local state = store:get_state("config")
store:set_state("config", new_state)
```

### Hook Points

| Event | Description |
|-------|-------------|
| `agent:session_start` | Session beginning |
| `agent:session_end` | Session closing |
| `agent:before_llm` | Before LLM call (modify context) |
| `agent:after_llm` | After LLM response |
| `agent:task_update` | Task list changed |
| `agent:tool_call` | Tool about to execute |

### Example: Custom Context Injection

```lua
--- Inject recently modified notes into context
-- @handler event="agent:before_llm" pattern="*" priority=100
function inject_recent(ctx, state)
    -- Add recently modified notes to context
    local recent = cru.kiln.search({
        modified_after = os.time() - 86400  -- 24 hours
    })

    if #recent > 0 then
        state:inject_context("## Recent Activity", recent)
    end

    return state
end
```

## Using the Internal Agent

### From CLI

```bash
# Start a new session (logs to sessions/<workspace>/<timestamp>.md)
cru chat

# Resume most recent open session for this workspace
cru chat --resume
```

### Session Commands

During a session:

| Command | Action |
|---------|--------|
| `/tasks` | Show current task list |
| `/search` | Search kiln and inject context |
| `/compact` | Summarize and continue in new file |

## Configuration

In global config (`~/.config/crucible/config.toml`):

```toml
# Personal kiln path
personal_kiln = "~/Documents/crucible-testing"

# Directories excluded from embedding (metadata still indexed)
embedding_exclusions = ["sessions"]
```

## Best Practices

### Workspace Organization

Sessions are automatically organized by the workspace directory you run `cru chat` from:

```
sessions/
├── crucible/           # Work in ~/code/crucible
├── my-app/             # Work in ~/code/my-app
└── dotfiles/           # Work in ~/dotfiles
```

Start chat from the relevant project directory to keep sessions organized.

### When to Use Internal vs External Agents

| Use Internal Agent | Use External Agent (ACP) |
|-------------------|-------------------------|
| Local-first operation | Cloud AI services |
| Session persistence | Stateless queries |
| Kiln integration | External tool access |

## See Also

- [[Help/Extending/Agent Cards]] - Define agent personas
- [[Help/Extending/Event Hooks]] - React to agent events
- [[Help/Extending/Custom Tools]] - Add tools for agents
- [[Help/Concepts/Agents & Protocols]] - MCP vs ACP
- [[Help/Lua/Language Basics]] - Lua scripting
- [[AI Features]] - All AI capabilities
