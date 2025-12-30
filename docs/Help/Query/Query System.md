---
title: Query System
description: How agents discover and retrieve knowledge from your kiln
tags:
  - query
  - search
  - context
  - agents
---

# Query System

The query system provides standardized interfaces for agents to discover, retrieve, and rank knowledge from your kiln. It powers context enrichment — automatically providing relevant notes to agents during conversations.

## Query Types

### Semantic Query

Natural language queries that find conceptually related content:

```
"How does authentication work in this project?"
"Notes about deployment strategies"
```

Results ranked by semantic similarity using embeddings.

### Metadata Query

Precise filtering by tags, dates, or properties:

```
tag:#meeting AND created:2024-01
type:book AND rating:>4
```

Returns exact matches based on note metadata.

### Hybrid Query

Combines semantic relevance with metadata constraints:

```
"authentication patterns" AND tag:#security
```

Prioritizes results satisfying both criteria.

## Query Patterns

### Exploratory

Broad searches when investigating a topic:
- Returns diverse results across related concepts
- Includes unexpected but potentially relevant connections
- Suggests follow-up queries and related topics

### Targeted

Specific information retrieval:
- Prioritizes precision over recall
- Supports exact matching
- Includes confidence indicators

### Temporal

Time-based queries:
- Filter by date ranges
- Track concept evolution over time
- Surface recent vs historical context

## Result Ranking

Results are ranked by multiple factors:

| Factor | Description |
|--------|-------------|
| **Relevance** | Semantic similarity to query |
| **Recency** | When the note was last modified |
| **Diversity** | Avoiding topic concentration |
| **Connections** | Link density in knowledge graph |

## Context Enrichment

When you chat with an agent, the query system automatically:

1. Analyzes conversation context
2. Identifies relevant knowledge needs
3. Retrieves and ranks matching notes
4. Injects context into the agent's prompt

This happens transparently — agents receive relevant knowledge without explicit queries.

### Context Window Optimization

When results exceed available context:
- Most relevant results prioritized
- Diversity maintained to cover different aspects
- Summaries used for large documents

## Using Queries

### Via Chat

Agents automatically query your kiln during conversation. You can also explicitly request searches:

```
"Search my notes for React patterns"
"Find notes tagged #meeting from last week"
```

### Via Tools

The `semantic_search` tool is available to agents:

```json
{
  "query": "authentication implementation",
  "limit": 10,
  "include_content": true
}
```

### Via CLI

```bash
cru search "your query here"
cru search --tag meeting --since 2024-01-01
```

## Performance

| Operation | Target |
|-----------|--------|
| Cached queries | <100ms |
| Uncached queries | <500ms |
| Large result sets | Streamed progressively |

Frequent query patterns are cached automatically.

## Related

- [[Semantic Search]] — Underlying search implementation
- [[Tags]] — Metadata for filtering
- [[Wikilinks]] — Knowledge graph connections
