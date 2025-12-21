---
description: Active development tasks and backlog for Crucible
type: tasks
status: active
updated: 2025-12-14
tags:
  - meta
  - tasks
  - tracking
---

# Project Tasks

## Active

### Near Term
- [ ] **Config naming cleanup** - Rename CliConfig types to avoid confusion
- [x] **Fix watch mode tests** - Implemented timeout patterns for watch tests ✅
- [x] **Provider refactor** - Unify embedding/LLM providers with capability-based design ✅
  - [x] Phase 0: Library evaluation (genai, kalosm) - Decided: build our own
  - [x] Track A: Unified config types (BackendType, ProviderConfig, ProvidersConfig)
  - [x] Track B: Extension traits (Provider, CanEmbed, CanChat)
  - [x] Track C: Unified factory and adapters
  - [x] Track D: CLI integration with migration support

### Medium Term
- [ ] Add pipeline error context (Phase 2/4/5 errors lack file path)
- [ ] Implement `infer_relations()` placeholder
- [ ] Add pipeline metrics collection
- [ ] Unify ChatToolCall vs ToolCall types
- [ ] Refactor InternalAgentHandle to use AgentRuntime

### Low Priority
- [ ] Rune compilation caching
- [ ] Just parameter type inference
- [ ] Rune arity limit workaround (>5 params)
- [ ] Language detection (currently defaults to "en")
- [ ] TokenBudget integration with ContextManager
- [ ] LayeredPromptBuilder - use or remove

---

## Completed (2025-12-14)

- [x] **Provider refactor complete** - Unified embedding/LLM providers
  - Flat `[providers.name]` config format
  - Unified model discovery with caching
  - Automatic migration from legacy `[embedding]` section
  - CLI integration with backward compatibility

## Completed (2025-12-13)

- [x] Path traversal security fix (NoteTools)
- [x] Tag query optimization (indexes + batch operations)
- [x] Re-enable startup file processing
- [x] Remove deprecated CLI flags (--db-path, --tool-dir)
- [x] Archive superseded openspec proposals
- [x] Codebase analysis documentation

---

## Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-14 | Build own unified provider layer vs genai/kalosm | genai is alpha (0.5.0-alpha.5), kalosm overlaps fastembed, both add 300+ deps |
| 2025-12-13 | Keep Anthropic provider, refactor to capability-based | Will need LLM providers too, not just embeddings |
| 2025-12-13 | Security policies deferred to post-MVP | Single-user local operation for MVP |
