---
title: "Search Tools"
description: Search operations for finding notes in your kiln
tags:
  - reference
  - cli
  - search
  - mcp
---

# Search Tools

Crucible searches from two places: the `cru search` command, for you, and MCP tools, for agents and external programs.

## The `cru search` command

```bash
cru search "wikilinks"                  # full-text + semantic (default)
cru search "wikilink" --type text       # full-text only
cru search "how do links work" --type semantic
cru search "architecture" --limit 5 -f json
cru search "wikilinks" --preview            # add a content preview per hit (-c)
```

Results are title and path by default. `-c/--preview` adds a snippet of each note's
content — the first two lines, capped so a long line cannot flood the terminal.

Full-text search runs over an FTS5 index of every note's **title and body**, ranked by BM25, so a word that appears only inside a note is found. Semantic search embeds the query and searches the vector index; it needs an embedding provider configured (see `:h config.embedding`) and notes that have been processed.

Notes are indexed as they are processed — by `cru process`, and automatically by the file watcher while the daemon is running. A kiln first opened by a build that predates the text index is backfilled once, on open.

Your query is treated as literal words, not FTS5 query syntax: punctuation and operators like `AND` search for themselves.

## MCP search tools

The same knowledge base is searchable programmatically by agents, through three MCP tools.

### Available Search Tools

1. **semantic_search** - Find notes by meaning using vector embeddings
2. **text_search** - Fast full-text search with regex support
3. **property_search** - Query notes by frontmatter properties and tags

## Semantic Search

Search notes using semantic similarity based on vector embeddings.

### Tool Name
`semantic_search`

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | Yes | - | Natural language search query |
| `limit` | number | No | 10 | Maximum results to return |

### Example

```json
{
  "query": "machine learning algorithms",
  "limit": 5
}
```

### Use Cases

- Finding conceptually related notes
- Discovering connections between ideas
- Locating notes when you don't remember exact wording
- Building context for AI agents

## Text Search

Fast full-text search across markdown files.

### Tool Name
`text_search`

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | Yes | - | Text to search for |
| `folder` | string | No | null | Subfolder to search within |
| `case_insensitive` | boolean | No | true | Case-insensitive search |
| `limit` | number | No | 10 | Maximum matches to return |

### Examples

**Basic search:**
```json
{
  "query": "TODO",
  "limit": 10
}
```

**Search in specific folder:**
```json
{
  "query": "FIXME",
  "folder": "Projects/Active",
  "case_insensitive": false
}
```

### Use Cases

- Finding exact text matches
- Locating TODOs, FIXMEs, or other markers
- Searching within specific project folders

## Property Search

Search notes by frontmatter properties, including tags.

### Tool Name
`property_search`

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `properties` | object | Yes | - | Key-value pairs to match |
| `limit` | number | No | 10 | Maximum results to return |

### Examples

**Single property:**
```json
{
  "properties": { "status": "draft" },
  "limit": 10
}
```

**Tag search (OR logic):**
```json
{
  "properties": { "tags": ["urgent", "important"] },
  "limit": 20
}
```

### Matching Logic

- **Multiple properties**: ALL must match (AND logic)
- **Array values**: Matches if ANY value matches (OR logic)

## Access Methods

### Via MCP Server

```bash
cru mcp --stdio
```

### Via Chat Mode

```bash
cru chat "Find all notes about machine learning"
```

## Search Strategy Guide

**When to use semantic_search:**
- Finding related concepts
- Exploring topic connections
- When you know the idea but not exact words

**When to use text_search:**
- Finding exact phrases or terms
- Locating action items (TODO, FIXME)
- Quick literal lookups

**When to use property_search:**
- Filtering by metadata
- Finding notes by status/type
- Tag-based queries

## See Also

- `:h mcp` - MCP server documentation
- `:h config.embedding` - Embedding configuration
- `:h frontmatter` - YAML frontmatter format
- `:h tags` - Tag system documentation
