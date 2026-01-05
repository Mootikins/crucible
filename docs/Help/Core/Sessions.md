---
description: Track conversation and execution history
status: planned
tags:
  - sessions
  - logging
---

# Sessions

> [!warning] Sessions are not yet implemented.

A session is a continuous sequence of events — a conversation, an agent's internal monologue, or a workflow execution. Sessions provide audit trails and enable resumption.

## Architecture

Sessions follow Crucible's "plaintext first" philosophy:

- **Markdown is truth** — Each session is a folder with markdown logs
- **DB is index** — SurrealDB indexes sessions for fast queries
- **Dual write** — Events append to both simultaneously
- **Rebuild on demand** — If DB corrupts, reindex from markdown

This means you can always `grep` your session logs, and DB corruption doesn't lose history.

## Session Types

Sessions capture different kinds of activity:

| Type | What it logs |
|------|--------------|
| **Chat** | User/assistant conversation with tool calls |
| **Agent** | Internal agent reasoning and actions |
| **Workflow** | Workflow execution steps and results |
| **Plugin** | Plugin-initiated activity |

## Session Storage

Each session is a folder containing markdown logs:

```
.crucible/sessions/
├── chat-abc123/
│   ├── 2024-01-15-1030.md     # Session log (conversation-style)
│   ├── 2024-01-15-1045.md     # Continuation after compaction
│   └── artifacts/              # Attachments (fetched pages, exports)
│       ├── research-page.md
│       └── generated-summary.md
├── agent-def456/
│   └── ...
```

### Session Log Format

Session logs read like a conversation or narrative, not structured event data:

```markdown
---
session_id: chat-abc123
type: chat
started: 2024-01-15T10:30:00Z
continued_from: null
---

# Chat Session

## 2024-01-15T10:30:00Z

**User:** Find all notes tagged #project and summarize them

**Assistant:** I'll search for notes with the project tag.

[Tool: search_by_tags tags=["project"]]
Found 12 notes matching #project.

Let me read through these and create a summary...

## 2024-01-15T10:31:15Z

**Assistant:** Here's a summary of your project notes:

- **Project Alpha**: 5 notes, last updated yesterday
- **Project Beta**: 4 notes, blocked on review
- **Project Gamma**: 3 notes, completed

Created summary at [[artifacts/project-summary.md]]
```

### Linking Sessions

When a session is compacted or branches:

```markdown
---
session_id: chat-abc123-2
continued_from: [[2024-01-15-1030.md]]
---

# Chat Session (continued)

*Compacted from previous session. Context: discussing project notes.*

## 2024-01-15T10:45:00Z

**User:** Tell me more about Project Beta
```

### Artifacts

Attachments (fetched pages, generated files, exports) live in `artifacts/` and are linked from the session log:

```markdown
Fetched the documentation page and saved to [[artifacts/api-docs.md]]
```

## Searching Sessions

Because sessions are indexed in the DB, you can query across all history:

```bash
# Find sessions mentioning a term
cru session search "Project Alpha"

# Find sessions by type
cru session search --type agent

# Find sessions from a date range
cru session search --since 2024-01-01 --until 2024-01-15
```

## Reindexing

If the database needs rebuilding:

```bash
# Reindex all sessions from markdown
cru session reindex
```

## Cleanup

Remove old sessions:

```bash
# Remove sessions older than 30 days
cru session cleanup --older-than 30d

# Archive instead of deleting
cru session cleanup --older-than 90d --archive ~/session-archive
```

## See Also

- [[Help/Workflows/Index]] — Workflow definitions (programmed automation, orthogonal to sessions)
- [[Help/CLI/chat]] — Interactive chat sessions
- [[Help/Skills/Index]] — Skills (prompted automation, also orthogonal to sessions)
