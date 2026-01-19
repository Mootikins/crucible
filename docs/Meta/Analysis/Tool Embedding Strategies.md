---
title: Tool Embedding Strategies
type: analysis
status: draft
created: 2025-01-17
tags:
  - embeddings
  - tool-discovery
  - mcp
---

# Tool Embedding Strategies for Intent Detection

Analysis of embedding strategies for matching user/LLM intent to MCP tools.

## Context

Goal: Detect when streaming LLM output or user input indicates tool use, enabling proactive tool injection without false positives on conversational content.

## Test Setup

- **Model**: `nomic-embed-text-v1.5-q8_0` (local, ~10ms/query)
- **True positives**: 12 queries that should trigger tools
- **False positives**: 18 queries that should NOT trigger (greetings, fillers, questions)

## Building Blocks

| Modifier | Description | Example |
|----------|-------------|---------|
| `base` | Natural language use-cases | "write a new document" |
| `+prefix` | Tool name prefix | "create_note: write a new document" |
| `+mcp` | MCP framing | "MCP tool create_note: write a new document" |
| `+llm` | LLM reasoning patterns | + "I should call create_note" |
| `+action` | Action verb variations | + "persist", "record", "store" |

## Results Matrix

Sorted by separation gap (TP min - FP max). Positive gap = clean separation.

| Combination | Gap | Accuracy | Threshold | TP% | FP% |
|-------------|-----|----------|-----------|-----|-----|
| **+prefix +mcp** | +0.008 | **83%** | 0.55 | 100% | **0%** |
| +prefix | +0.011 | 75% | 0.65 | 83% | 0% |
| +mcp +llm | +0.002 | 75% | 0.60 | 92% | 0% |
| +prefix +llm | +0.007 | 75% | 0.65 | 83% | 0% |
| base only | -0.055 | 75% | 0.65 | 92% | 6% |
| base +action | -0.081 | 83% | 0.70 | 50% | 0% |

## Pareto Optimal Strategies

Best tradeoffs (not dominated on all metrics):

1. **`+prefix +mcp`** - Best overall (83% acc, 0% FP, threshold 0.55)
2. **`+prefix`** - Simplest with good separation (+0.011 gap)
3. **`+mcp +llm`** - Best for LLM output detection (92% TP)

## Recommendations

### For Production: `+prefix +mcp`

```
Embedding format:
  "MCP tool create_note: write a new document"
  "MCP tool create_note: save this information"
  "MCP tool create_note: create a file"
  ...

Threshold: 0.55
Expected: 100% TP capture, 0% FP
```

### For Streaming Interruption: `+mcp +llm`

Better at catching LLM reasoning patterns:
- "I should call semantic_search"
- "Let me use the read_note tool"
- "I need to invoke create_note"

### Threshold Strategy

| Confidence | Threshold | Action |
|------------|-----------|--------|
| High | ≥0.70 | Auto-inject tool info |
| Medium | 0.55-0.70 | Queue for confirmation |
| Low | <0.55 | Ignore |

## False Positive Analysis

Worst offenders at low thresholds:

| Query | Score | Matched | Why |
|-------|-------|---------|-----|
| "I understand the user wants help" | 0.64 | discover_tools | "help" in use-cases |
| "let me see" | 0.63 | read_note | "see" ≈ "show" |
| "thanks" | 0.62 | discover_tools | semantic similarity |

**Mitigation**: MCP prefix creates namespace separation, pushing these below threshold.

## Storage Format

```json
{
  "model": "nomic-embed-text-v1.5-q8_0",
  "strategy": "+prefix +mcp",
  "threshold": 0.55,
  "tools": {
    "create_note": {
      "cases": [
        "MCP tool create_note: write a new document",
        "MCP tool create_note: save this information",
        ...
      ],
      "vector": [0.123, -0.456, ...]
    }
  }
}
```

## Technical Content Problem

Pure embedding approaches fail on technical content that mentions tool-related words as **objects** not **actions**:

| Query | Expected | Problem |
|-------|----------|---------|
| "the search algorithm" | NO MATCH | "search" as noun |
| "delete operations are slow" | NO MATCH | "delete" as noun |
| "To create a note, you would use the API" | NO MATCH | explanation |
| "my notes on kubernetes" | NO MATCH | "notes" as subject |

**Finding**: At any threshold that captures >80% TPs, technical FP rate is 60%+.

## Solution: Tiered Approach

```
1. NEGATIVE patterns → BLOCK (never trigger)
2. HIGH confidence patterns → semantic match (any threshold)
3. Fallback → semantic match with HIGH threshold (≥0.68)
```

### Pattern Categories

**HIGH confidence** (always trigger semantic match):
```regex
\b(call|invoke|use)\s+(the\s+)?(tool|MCP)
\bI\s+(should|need\s+to|will|'ll)\s+(create|read|search|...)
\b(let\s+me|going\s+to)\s+(create|read|search|...)
\b(please|can\s+you)\s+(create|read|search|...)
```

**NEGATIVE** (always block):
```regex
\bthe\s+(create|read|...)\s+(endpoint|operation|query|API|latency)
\b(create|read|...)\s+(operations?|complexity|algorithm|results?)
\b(soft|hard)\s+delete
\bCRUD\b
\bcreate\s+table
\bread\s+replica
\blist\s+comprehension
\bsearch\s+(algorithm|engine|index|feature)
\bnotes?\s+on\s+\w
```

### Results

| Metric | Embedding Only | Tiered Approach |
|--------|----------------|-----------------|
| TP Accuracy | 83% | **81%** |
| Technical FP | 64% | **6%** |
| Conversational FP | 20% | **0%** |
| **Total FP** | 50%+ | **4%** |

## Strategy Matrix Summary

| Strategy | Gap | Accuracy | FP@threshold | Best For |
|----------|-----|----------|--------------|----------|
| base only | -0.055 | 75% | 6% | Baseline |
| +prefix | +0.011 | 75% | 0% | Simple, good separation |
| +prefix +mcp | +0.008 | 83% | 0% | Best pure embedding |
| +mcp +llm | +0.002 | 75% | 0% | LLM output detection |
| **Tiered** | N/A | **81%** | **4%** | **Production (technical content)** |

## Recommended Implementation

```python
def should_match_tool(query: str, tool_vecs: dict, threshold=0.68) -> tuple[str, float] | None:
    # 1. Check negatives first
    if matches_negative_pattern(query):
        return None
    
    # 2. Check high-confidence patterns
    if matches_high_pattern(query):
        return semantic_match(query, tool_vecs, threshold=0.50)  # Lower threshold OK
    
    # 3. Fallback to high-threshold embedding
    result = semantic_match(query, tool_vecs, threshold=threshold)
    return result if result and result[1] >= threshold else None
```

## Future Work

- [ ] Add negative pattern for explanatory language ("you would use", "works by")
- [ ] Test with more MCP servers (different tool vocabularies)
- [ ] Evaluate clustering for tool families
- [ ] Benchmark on real streaming transcripts
- [ ] Test incremental/streaming detection (partial phrases)

## Quick Reference: Building Blocks

| Modifier | Effect | Example |
|----------|--------|---------|
| `base` | Natural language | "write a new document" |
| `+prefix` | Tool name prefix | "create_note: write a new document" |
| `+mcp` | MCP namespace | "MCP tool create_note: ..." |
| `+llm` | LLM reasoning phrases | + "I should call create_note" |
| `+action` | Action verb variants | + "persist", "record" |
| `+negative` | Exclusion patterns | Block "the search algorithm" |
| `+tiered` | Pattern gate + fallback | HIGH→NEGATIVE→embedding@0.68 |

## Decision Tree

```
Is this for production with technical content?
├─ YES → Use TIERED approach (+negative patterns)
│        TP: 81%, FP: 4%
│
└─ NO → Is this for LLM output detection?
        ├─ YES → Use +mcp +llm
        │        Best for streaming interruption
        │
        └─ NO → Use +prefix +mcp
                 Simplest with 0% FP
```
