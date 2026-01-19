---
title: Embedding-Based Intent Detection
type: analysis
status: parked
created: 2025-01-17
tags:
  - embeddings
  - scripting
  - lua
  - plugins
---

# Embedding-Based Intent Detection

Research into using embeddings for detecting tool/action intent from text streams.

## Core Idea

Expose embedding primitives to scripting (Lua/Rune) so plugin authors can build custom intent detection for their use cases.

## Proposed Scripting API

```lua
-- Generate embedding for text
local vec = cru.embed("search for notes about auth")

-- Compare similarity
local score = cru.cosine_similarity(vec, tool_vectors.semantic_search)

-- Batch embed
local vecs = cru.embed_batch({"query 1", "query 2", "query 3"})

-- Pre-built tool matching (uses internal strategy)
local match, confidence = cru.match_tool_intent(text)
if confidence > 0.7 then
  cru.inject_tool_context(match)
end
```

## Use Cases for Plugin Authors

1. **Custom tool suggestion** - Match user input to plugin-defined tools
2. **Smart autocomplete** - Suggest completions based on semantic similarity
3. **Content routing** - Route queries to appropriate handlers
4. **Anomaly detection** - Flag text that doesn't match expected patterns

## Message Type Considerations

Different detection strategies suit different contexts:

| Context | FP Tolerance | Strategy |
|---------|--------------|----------|
| User query | Low | Conservative, high threshold |
| LLM output | Medium | Pattern + embedding hybrid |
| Tool results | High | State machine / transitions |

## Research Findings

See [[tool-embedding-strategies]] for detailed benchmarks.

**TL;DR**: Pure embeddings struggle with technical content ("the search algorithm" vs "search for X"). Hybrid approach (patterns + embeddings) achieves 81% accuracy with 4% FP rate.

## Status

Parked. Core embedding infrastructure exists. Scripting API exposure is future work when plugin ecosystem needs it.

## Related

- [[tool-embedding-strategies]] - Detailed benchmark results
- [[Help/Extending/Scripting Languages]] - Plugin development guide
