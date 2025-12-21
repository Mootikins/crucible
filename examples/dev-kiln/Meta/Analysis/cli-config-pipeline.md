---
title: CLI, Config & Pipeline Analysis
description: Architecture analysis of CLI commands, configuration, and processing pipeline
type: analysis
system: cli
status: review
updated: 2025-12-13
tags:
  - meta
  - analysis
  - cli
---

# CLI, Config & Pipeline Analysis

## Executive Summary

The CLI provides 9 main commands with a 5-phase processing pipeline. Configuration is comprehensive with 10 embedding providers supported.

**Issues Found (2 of 3 FIXED):**
- ~~File processing is DISABLED (empty stub)~~ ✅ Fixed
- ~~Deprecated CLI flags still present~~ ✅ Fixed
- Config naming confusion (CliConfig dual meaning) - remaining

---

## Critical Issues

- [x] **File processing DISABLED** (HIGH) ✅ FIXED
  - Location: `main.rs:19-22`
  - Issue: `process_files_with_change_detection()` is empty stub
  - Impact: CLI commands operate on stale data
  - **Status**: Fixed 2025-12-13 - Re-enabled using sync.rs + factories pattern

- [x] **Deprecated CLI flags still present** (HIGH) ✅ FIXED
  - Location: `cli.rs:67-72`
  - Flags: `--db-path`, `--tool-dir`
  - Issue: Non-functional but still in CLI args
  - **Status**: Fixed 2025-12-13 - Removed flags, tests, and docs

- [ ] **Config naming confusion** (MEDIUM)
  - Issue: "CliConfig" used for two different types
  - **FIX**: Rename CliAppConfig → AppConfig, small CliConfig → CliSettings

---

## CLI Commands

| Command | Purpose | Entry Point |
|---------|---------|-------------|
| chat | ACP agent interaction | commands/chat.rs |
| mcp | MCP server (stdio/SSE) | commands/mcp.rs |
| process | 5-phase pipeline | commands/process.rs |
| stats | Kiln statistics | commands/stats.rs |
| config | Config management | commands/config.rs |
| status | Storage status | commands/status.rs |
| storage | DB maintenance | commands/storage.rs |
| agents | Agent cards | commands/agents.rs |
| cluster | Knowledge clustering | commands/cluster.rs |

---

## 5-Phase Pipeline

```
File Path
  ↓ Phase 1: Quick Filter
FileState check (hash + mtime) → Skip if unchanged
  ↓ Phase 2: Parse
Markdown → ParsedNote (AST)
  ↓ Phase 3: Merkle Diff
HybridMerkleTree → changed_block_ids
  ↓ Phase 4: Enrich
Embeddings + metadata → EnrichedNote
  ↓ Phase 5: Store
Database persistence → ProcessingResult
```

**Issues:**
- [ ] Section-level vs block-level ID mismatch (FIXED in enrichment - embeds all blocks)
- [ ] Bug #3: Phase 2/4/5 errors lack file path context

---

## Configuration

### Config Loading Priority
1. CLI flags (--embedding-url, --embedding-model)
2. Environment variables (CRUCIBLE_*)
3. Config file (~/.config/crucible/config.toml)
4. Default values

### Embedding Providers (10)

| Provider | Default Model | Max Concurrent |
|----------|--------------|----------------|
| FastEmbed | bge-small-en-v1.5 | num_cpus/2 |
| Ollama | nomic-embed-text | 1 |
| OpenAI | text-embedding-3-small | 8 |
| Cohere | embed-english-v3.0 | 8 |
| VertexAI | textembedding-gecko@003 | 8 |
| Burn | nomic-embed-text | 1 |
| LlamaCpp | nomic-embed-text-v1.5.Q8_0 | 1 |
| Custom | - | 4 |
| Mock | - | 16 |
| ⚠️ Anthropic | claude-3-haiku | 8 |

**Issue**: Anthropic doesn't have embedding models - using LLM is incorrect

---

## Enrichment Service

**Operations:**
- `generate_embeddings()` - Batch embedding generation
- `extract_metadata()` - Reading time, complexity score
- `build_breadcrumbs()` - Heading hierarchy for context

**Constants:**
- Min words for embedding: 5
- Max batch size: 10

**Incomplete:**
- [ ] `infer_relations()` - Placeholder, returns empty Vec
- [ ] Language detection - Defaults to "en"
- [ ] Pipeline metrics - Not collected

---

## Recommendations

### Immediate
1. ~~Re-enable file processing or remove startup logic~~ ✅ Done
2. ~~Remove deprecated `--db-path` and `--tool-dir` flags~~ ✅ Done
3. Improve error context for pipeline phases

### Medium Term
1. Rename config types to avoid confusion
2. Remove or fix Anthropic embedding provider
3. Implement block-level granularity in pipeline

### Low Priority
1. Add pipeline metrics collection
2. Implement relation inference
3. Add actual language detection
