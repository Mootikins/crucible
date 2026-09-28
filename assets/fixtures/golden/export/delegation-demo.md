## User

What is a kiln in Crucible? Use semantic search to find relevant docs.

> [!system]- System Prompt
> Context injected: 0 note(s) for "What is a kiln in Crucible? Use semantic search to find relevant docs."

### Tool: `ToolSearch` (id: toolu_01QVCEYKhaD48SCtS7Z8XTMS)

```json
{
  "query": "select:mcp__crucible__semantic_search,mcp__crucible__get_kiln_info",
  "max_results": 2
}
```

### Tool: `mcp__crucible__semantic_search` (id: toolu_01DvBwfxfLUoeZY14e8EFKLu)

```json
{
  "query": "what is a kiln in Crucible",
  "limit": 10
}
```

### Tool: `mcp__crucible__get_kiln_info` (id: toolu_01PMAioVH4jxsPuJfGkpazvm)

```json
{}
```

### Tool: `grep -l | head -20 --include="*.md" "kiln" /home/user/.crucible` (id: toolu_01V3hE7VKWyF67tQF6Wz4vTT)

```json
{
  "pattern": "kiln",
  "path": "/home/user/.crucible",
  "glob": "*.md",
  "output_mode": "files_with_matches",
  "head_limit": 20
}
```

### Tool: `grep -l | head -20 --include="*.{toml,yaml,yml,json,rs,py,ts}" "kiln" /home/user/.crucible` (id: toolu_01XGvCfzhPqHc7Lx2qxm91Wb)

```json
{
  "pattern": "kiln",
  "path": "/home/user/.crucible",
  "glob": "*.{toml,yaml,yml,json,rs,py,ts}",
  "output_mode": "files_with_matches",
  "head_limit": 20
}
```

### Tool: `Read projects.json` (id: toolu_01FjzVjMW8SYXjwp4iXNCLdd)

```json
{
  "file_path": "/home/user/.crucible/projects.json"
}
```

### Tool: `Find `/home/user/.crucible` `*.md`` (id: toolu_01N1HMnQfUSNYudC9Vf94Zbc)

```json
{
  "pattern": "*.md",
  "path": "/home/user/.crucible"
}
```

### Tool: `ls /home/user/.crucible/.crucible/` (id: toolu_01GxdMMio33bxEmEnVrq2LK9)

```json
{
  "command": "ls /home/user/.crucible/.crucible/",
  "description": "List the kiln directory contents"
}
```

### Tool: `Find `/home/user/.crucible/.crucible` `*.md`` (id: toolu_0129FLGarSbQ9PzvLz4zL2sj)

```json
{
  "pattern": "*.md",
  "path": "/home/user/.crucible/.crucible"
}
```

### Tool: `grep -C 3 | head -30 "kiln" /home/user/.crucible/sessions/chat-2026-04-17T1736-1mwul4/session.md` (id: toolu_016XF4ermrfcZkXbWJ5BV6tg)

```json
{
  "pattern": "kiln",
  "path": "/home/user/.crucible/sessions/chat-2026-04-17T1736-1mwul4/session.md",
  "output_mode": "content",
  "-C": 3,
  "head_limit": 30
}
```

### Tool: `ls /home/user/.crucible/` (id: toolu_01VKoCrNpqo2eL4RwF3DpZnJ)

```json
{
  "command": "ls /home/user/.crucible/",
  "description": "List top-level crucible directory"
}
```

### Tool: `ls /home/user/.crucible/runes/ 2>/dev/null; echo "---"; ls /home/user/.crucible/tools/ 2>/dev/null` (id: toolu_01MEZBU2TW5kW8qQorwEpNvc)

```json
{
  "command": "ls /home/user/.crucible/runes/ 2>/dev/null; echo \"---\"; ls /home/user/.crucible/tools/ 2>/dev/null",
  "description": "List runes and tools directories"
}
```

### Tool: `grep -i -C 5 | head -40 "kiln" /home/user/.crucible/sessions/chat-2026-04-17T1811-56p6o7/session.md` (id: toolu_01Xy2CfUpV6SrxZTgyzapNpY)

```json
{
  "pattern": "kiln",
  "path": "/home/user/.crucible/sessions/chat-2026-04-17T1811-56p6o7/session.md",
  "output_mode": "content",
  "-C": 5,
  "head_limit": 40,
  "-i": true
}
```

### Tool: `which crucible 2>/dev/null; crucible --help 2>/dev/null | head -50` (id: toolu_01YQNcFfRS3KWSw69AvpZ394)

```json
{
  "command": "which crucible 2>/dev/null; crucible --help 2>/dev/null | head -50",
  "description": "Check crucible CLI help for kiln info"
}
```

### Tool: `find /home/user -maxdepth 3 -name "crucible" -type f 2>/dev/null; find /usr/local/bin /usr/bin -name "crucible*" 2>/dev/null; which crucible-server 2>/dev/null; ps aux | grep -i crucible 2>/dev/null | head -5` (id: toolu_01Efh5JZBMMmPeM6JaVhENPe)

```json
{
  "command": "find /home/user -maxdepth 3 -name \"crucible\" -type f 2>/dev/null; find /usr/local/bin /usr/bin -name \"crucible*\" 2>/dev/null; which crucible-server 2>/dev/null; ps aux | grep -i crucible 2>/dev/null | head -5",
  "description": "Find crucible binary and running processes"
}
```

### Tool: `type cru 2>/dev/null; cru --help 2>/dev/null | head -60` (id: toolu_01MPSL979UDmgZRVDWciR8R3)

```json
{
  "command": "type cru 2>/dev/null; cru --help 2>/dev/null | head -60",
  "description": "Check cru CLI help"
}
```

### Tool: `cru init --help 2>/dev/null` (id: toolu_01CHrELwcdVvqPsqZzHsygz5)

```json
{
  "command": "cru init --help 2>/dev/null",
  "description": "Check init subcommand help for kiln details"
}
```

### Tool: `cru stats 2>/dev/null; echo "==="; cru status 2>/dev/null` (id: toolu_01K2z8gMRHRiWYMqms2JRq6b)

```json
{
  "command": "cru stats 2>/dev/null; echo \"===\"; cru status 2>/dev/null",
  "description": "Get kiln stats and status"
}
```

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
