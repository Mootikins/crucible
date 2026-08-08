---
title: Tool Capabilities
description: Shared capability matrix for built-in agent roles.
tags: [agents, tools]
---

# Tool Capabilities

The tool names an agent card may list in its `tools:` block. Names not in this table are
still valid — plugins, upstream MCP servers, and `just` recipes all contribute tools — but
these are the ones Crucible itself always provides.

`cru tools list` is the authoritative, live answer for a given kiln and workspace; this page
is the stable subset.

## Kiln tools

Reads and writes against the knowledge graph. These operate on notes, not arbitrary files,
and are scoped to the session's kiln.

| Tool | Does |
|------|------|
| `create_note` | Create a new note in the kiln |
| `read_note` | Read note content, optionally a line range |
| `read_metadata` | Read a note's frontmatter and properties without loading the body |
| `update_note` | Update an existing note |
| `delete_note` | Delete a note |
| `list_notes` | List notes in a directory |
| `semantic_search` | Embedding-based similarity search |
| `text_search` | Full-text search |
| `property_search` | Search by frontmatter property, including tags |
| `get_kiln_info` | Kiln root path and statistics |

## Workspace tools

Filesystem and shell access, contained to the session's workspace directory.

| Tool | Does |
|------|------|
| `read_file` | Read a file, returned with line numbers |
| `write_file` | Write a file, creating parent directories |
| `edit_file` | Replace an exact string in a file |
| `glob` | Match paths by glob pattern |
| `grep` | Search file contents |
| `bash` | Run a shell command; `background=true` for long-running ones |

## Session and orchestration tools

| Tool | Does |
|------|------|
| `skill_view` | Load a discovered skill's instructions into context |
| `delegate_session` | Hand a task to another agent card or ACP profile |
| `list_jobs` | List background jobs |
| `get_job_result` | Fetch a finished background job's result |
| `cancel_job` | Cancel a running background job |
| `discover_tools` | Search the live tool list by name or description |
| `get_tool_schema` | Fetch one tool's full JSON schema |

`delegate_session` is only advertised when delegation is enabled for the session; its
description is rewritten at list time to name the targets actually available.

## Naming conventions for non-built-in tools

Tool names carry their origin as a prefix, which is how the `discover_tools` tool
classifies and filters them:

| Prefix | Source |
|--------|--------|
| *(none)* | `builtin` — the tables above |
| `lua_` | A tool registered by a Lua or Fennel plugin |
| `just_` | A `justfile` recipe exposed as a tool |
| `gh_`, `mcp_`, or a name containing `::` | An upstream MCP server reached through the gateway |

## Declaring capabilities on a card

An agent card's `tools:` block sets *permissions*, not membership — the values are
`true`/`allow`, `ask`, or `false`/`deny`:

```yaml
tools:
  semantic_search: true
  read_note: true
  write_file: ask
  bash: deny
```

Tools you don't list keep their default behaviour: read-only tools run freely, mutating
tools go through the permission gate. See [[Help/Extending/Agent Cards]] for the full
frontmatter schema and the trust implications of `allow`.

## MCP servers

A card's `mcps:` list names upstream MCP servers the agent may use; the gateway aggregates
their tools under the server's configured prefix. Any server capable of note search,
frontmatter access, directory listing, and note creation covers what the gallery agents in
this directory expect. See [[Help/Extending/MCP Gateway]] for configuring upstreams.
