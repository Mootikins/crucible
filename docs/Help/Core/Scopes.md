---
description: The three scopes that tools and scripts operate within
status: draft
tags:
  - architecture
  - scopes
  - core
---

# Scopes

Crucible tools and scripts operate within three orthogonal scopes that define where data lives and what it means:

```
┌─────────────────────────────────────────────────────────────┐
│ Kiln - Permanent knowledge storage                          │
│ "What you know"                                             │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ Session - Current interaction                               │
│ "What you're doing right now"                               │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ Workspace - Active work output                              │
│ "Where work products go"                                    │
└─────────────────────────────────────────────────────────────┘
```

## Kiln

The **kiln** is your permanent knowledge base - notes, documents, and the knowledge graph built from them.

| Aspect | Description |
|--------|-------------|
| **Path** | The kiln directory (e.g., `~/notes/`) |
| **Lifetime** | Permanent, survives sessions |
| **Contents** | Markdown notes, wikilinks, tags, semantic index |
| **Purpose** | Long-term memory and episodic knowledge |

Tools that operate on the kiln:
- `create_note`, `read_note`, `update_note`, `delete_note`
- `semantic_search`, `text_search`, `property_search`

See [[Help/Concepts/Kilns]] for details.

## Session

A **session** is a continuous sequence of interactions - a conversation, agent execution, or workflow run.

| Aspect | Description |
|--------|-------------|
| **ID** | Unique identifier (e.g., `workspace/2024-01-15_1030`) |
| **Path** | Session folder (`<kiln>/sessions/<workspace>/<timestamp>/`) |
| **Lifetime** | Duration of interaction, then archived |
| **Contents** | Conversation log, artifacts (fetched pages, generated files) |
| **Purpose** | Audit trail, resumption, interaction continuity |

Tools that operate on sessions:
- `web_fetch` can save fetched content as session artifacts
- Workflow tools log execution progress
- Agent tools can checkpoint state

See [[Help/Core/Sessions]] for details.

### Session Artifacts

Sessions can store **artifacts** - files generated or fetched during the session:

```
<kiln>/sessions/<workspace>/<timestamp>/
├── log.md           # Conversation/execution log
└── artifacts/       # Generated/fetched content
    ├── fetched/     # Web pages, API responses
    ├── generated/   # Code, summaries, reports
    └── exports/     # User-requested exports
```

Artifacts are linked from the session log:
```markdown
Fetched API documentation and saved to [[artifacts/fetched/api-docs.md]]
```

## Workspace

The **workspace** is where active work happens - files being edited, code being written.

| Aspect | Description |
|--------|-------------|
| **Path** | Working directory OR isolated scratch space |
| **Lifetime** | Active work session |
| **Contents** | Code, documents, any files the agent creates |
| **Purpose** | Separate "doing" from "knowing" |

Workspace isolation options:

| Mode | Path | Use Case |
|------|------|----------|
| **cwd** | Current working directory | Direct file editing |
| **scratch** | `<session>/workspace/` | Isolated agent work |
| **project** | Explicit project path | Multi-project work |

> [!note] Workspace vs Kiln
> The **kiln** stores knowledge (notes, ideas, reference). The **workspace** is where you *do* things (code, documents, outputs). They serve different purposes and shouldn't be conflated.

## Scope Access

When a tool or script runs, it may have access to any combination of scopes:

```
User message
     │
     ▼
┌─────────────────────────────────────────────────────────────┐
│ Tool Execution                                              │
│                                                             │
│   kiln: Some(KilnScope)         ← always available          │
│   session: Some(SessionScope)   ← if in active session      │
│   workspace: Some(WorkspaceScope) ← if configured           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
     │
     ▼
Tool reads notes (kiln), saves artifact (session), writes file (workspace)
```

## For Tool Authors

Tools declare which scopes they need:

```rust
// Tool that only needs kiln
async fn search_notes(&self, kiln: &dyn KilnScope, query: &str) -> Result<Vec<Note>>

// Tool that needs session for artifacts
async fn web_fetch(&self, session: Option<&dyn SessionScope>, url: &str) -> Result<String>

// Tool that needs workspace for file operations
async fn write_file(&self, workspace: &dyn WorkspaceScope, path: &str, content: &str) -> Result<()>
```

If a tool needs a scope that isn't available, it should either:
- Return an error explaining what's missing
- Degrade gracefully (e.g., skip saving artifact if no session)

## For Script Authors

In Rune scripts, scopes are available via the execution environment:

```rune
// Access kiln
let notes = kiln.search("query")?;

// Access session (if available)
if let Some(session) = session {
    session.save_artifact("output.md", content)?;
}

// Access workspace
let path = workspace.path().join("output.txt");
```

## See Also

- [[Help/Concepts/Kilns]] - Kiln fundamentals
- [[Help/Core/Sessions]] - Session details
- [[Help/Extending/Custom Tools]] - Building tools
- [[Help/Rune/Crucible API]] - Scripting API
