## User

What is a kiln in Crucible? Use semantic search to find relevant docs.

> Context injected: 0 note(s) for "What is a kiln in Crucible? Use semantic search to find relevant docs."

### Tool: ToolSearch

### Tool: Semantic Search

### Tool: Get Kiln Info

### Tool: Grep

### Tool: Grep

### Tool: Read File

### Tool: Find

### Tool: Terminal

### Tool: Find

### Tool: Grep

### Tool: Terminal

### Tool: Terminal

### Tool: Grep

### Tool: Terminal

### Tool: Terminal

### Tool: Terminal

### Tool: Terminal

### Tool: Terminal

### Tool: ToolSearch

### Tool: mcp__crucible__semantic_search

### Tool: mcp__crucible__get_kiln_info

### Tool: grep -l | head -20 --include="*.md" "kiln" /home/user/.crucible

### Tool: grep -l | head -20 --include="*.{toml,yaml,yml,json,rs,py,ts}" "kiln" /home/user/.crucible

### Tool: Read projects.json

### Tool: Find `/home/user/.crucible` `*.md`

### Tool: ls /home/user/.crucible/.crucible/

### Tool: Find `/home/user/.crucible/.crucible` `*.md`

### Tool: grep -C 3 | head -30 "kiln" /home/user/.crucible/sessions/chat-2026-04-17T1736-1mwul4/session.md

### Tool: ls /home/user/.crucible/

### Tool: ls /home/user/.crucible/runes/ 2>/dev/null; echo "---"; ls /home/user/.crucible/tools/ 2>/dev/null

### Tool: grep -i -C 5 | head -40 "kiln" /home/user/.crucible/sessions/chat-2026-04-17T1811-56p6o7/session.md

### Tool: which crucible 2>/dev/null; crucible --help 2>/dev/null | head -50

### Tool: find /home/user -maxdepth 3 -name "crucible" -type f 2>/dev/null; find /usr/local/bin /usr/bin -name "crucible*" 2>/dev/null; which crucible-server 2>/dev/null; ps aux | grep -i crucible 2>/dev/null | head -5

### Tool: type cru 2>/dev/null; cru --help 2>/dev/null | head -60

### Tool: cru init --help 2>/dev/null

### Tool: cru stats 2>/dev/null; echo "==="; cru status 2>/dev/null

## Assistant

Semantic search returned no results, and the kiln info shows basic stats. Let me dig deeper into the codebase.Let me check the Crucible source or binary for more context on what a kiln is.Now I have a clear picture. Here's what a **kiln** is in Crucible:

A **kiln** is Crucible's term for a **knowledge store** — a directory that has been initialized to hold and manage notes, documents, and their semantic embeddings. It's the core data unit in Crucible.

Key details:

- **Created with `cru init`** — initializes a directory as a kiln (or project). This creates a `.crucible/` subdirectory inside it containing a SQLite database (`crucible-sqlite.db`) for storing notes and embeddings.
- **Contains notes** — markdown files that are parsed, enriched, and stored with semantic embeddings for search.
- **Has its own database** — each kiln gets its own `.crucible/` directory with a SQLite DB, enabling local semantic search, text search, and property search.
- **Tracked in `projects.json`** — Crucible's global registry maps projects to their kilns. A project can have zero or more kilns (see the `"kilns": [...]` array in `projects.json`).
- **Used as workspace context** — chat sessions are scoped to a kiln (`kiln: /home/user/.crucible` in session metadata), so the AI agent knows which knowledge base to search.

Your current active kiln points to `/home/user/crucible/docs` with 351 notes and 173 markdown files. The name follows the pottery/forge metaphor of "Crucible" — a kiln is where raw materials (documents) are fired (processed/enriched) into something useful (a searchable knowledge base).
