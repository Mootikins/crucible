---
title: Search Tools
description: Search operations for finding notes in your kiln
tags:
  - reference
  - cli
  - search
  - mcp
---

# Search Tools

Crucible provides three complementary search methods through MCP (Model Context Protocol) tools.

## Overview

Search functionality is provided through MCP tools rather than a dedicated CLI command. This allows agents and external tools to search your knowledge base programmatically.

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

## Implementation

**Code reference:** `crates/crucible-tools/src/search.rs`

## See Also

- `:h mcp` - MCP server documentation
- `:h config.embedding` - Embedding configuration
- `:h frontmatter` - YAML frontmatter format
- `:h tags` - Tag system documentation
