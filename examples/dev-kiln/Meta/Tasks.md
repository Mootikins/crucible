---
type: tasks
status: active
updated: 2025-12-13
---

# Project Tasks

## Active

### Near Term
- [ ] **Config naming cleanup** - Rename CliConfig types to avoid confusion
- [ ] **Fix watch mode tests** - Ignored tests need to be fixed and triggered processing verified
- [ ] **Provider refactor** - Unify embedding/LLM providers with capability-based design
  - Models have specific capabilities (embeddings, chat, vision, etc.)
  - Provider trait hierarchy instead of separate embedding vs LLM providers

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
| 2025-12-13 | Keep Anthropic provider, refactor to capability-based | Will need LLM providers too, not just embeddings |
| 2025-12-13 | Security policies deferred to post-MVP | Single-user local operation for MVP |
