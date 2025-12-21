---
title: Tools, MCP & Rune Analysis
description: Architecture analysis of MCP tools, Rune scripting, and extensibility layer
type: analysis
system: tools
status: review
updated: 2025-12-13
tags:
  - meta
  - analysis
  - tools
---

# Tools, MCP & Rune Analysis

## Executive Summary

The extensibility layer consists of three interconnected systems:
1. **crucible-tools**: MCP server with 12 core tools + dynamic Just/Rune tools
2. **crucible-rune**: Rune scripting with event hooks and MCP gateway
3. **crucible-just**: Just recipe parser and MCP tool generation

All integrate through a **unified EventBus** for tool discovery, execution interception, and result transformation.

---

## Critical Issues

- [x] **Path traversal security concern** (HIGH) ✅ FIXED
  - Location: `crucible-tools/src/notes.rs` - NoteTools
  - Issue: No explicit path validation - could write `../../../etc/passwd.md`
  - **FIX**: Added `validate_path_within_kiln()` in `utils.rs` with 3-layer defense
  - **Status**: Fixed 2025-12-13 - 19 security tests added, all 7 vulnerable functions secured

---

## High Priority Issues

- [ ] **Rune arity limit** (MEDIUM)
  - Location: `crucible-rune/src/mcp_module.rs`
  - Issue: Rune 0.14 supports max 5 params per MCP tool
  - Impact: Tools with >5 params are skipped (logged warning)
  - Recommendation: Consider JSON object wrapper for complex tools

- [ ] **Parser reprocessing TODOs** (MEDIUM)
  - Location: `notes.rs` lines 117, 238, 339
  - Issue: TODO comments "Notify parser for reprocessing"
  - Recommendation: Implement parser notification or document as future work

---

## MCP Server Architecture

### Tool Categories

**Kiln Tools (12 core)**:
- NoteTools (6): create_note, read_note, read_metadata, update_note, delete_note, list_notes
- SearchTools (3): semantic_search, text_search, property_search
- KilnTools (3): get_kiln_info, get_kiln_roots, get_kiln_stats

**Dynamic Tools**:
- Just recipes → `just_<recipe>` tools
- Rune scripts → `rune_<tool>` tools
- Upstream MCP → `<prefix>_<tool>` tools

### Event System

**EventBus** (`event_bus.rs`) - Central dispatcher:
- Event types: ToolBefore, ToolAfter, ToolError, ToolDiscovered, NoteParsed, NoteCreated, NoteModified
- **Fail-open** semantics - handler errors don't stop pipeline
- Priority ordering, glob pattern matching

**Built-in Hooks**:
- `test_filter` - Filter verbose test output (priority 10)
- `recipe_enrichment` - Categorize Just recipes (priority 5)
- `tool_selector` - Filter/prefix tool discovery (priority 5, disabled)
- `event_emit` - Publish events externally (priority 200)

---

## Tool Execution Flow

```
MCP call_tool("just_test", args)
  ↓
EventBus.emit(tool:before) - can modify args or cancel
  ↓
Check event.is_cancelled() → return error if true
  ↓
Execute tool (JustTools.execute / RuneExecutor / NoteTools)
  ↓
Built-in filter (if test output detected)
  ↓
EventPipeline (Rune plugins)
  ↓
EventBus.emit(tool:after) - can transform result
  ↓
Return CallToolResult
```

---

## Just Integration

**JustTools** (`crucible-just/src/tools.rs`):
- Lazy-loading justfile parser
- Recipe → McpTool with JSON schema
- Name mangling: `just_<recipe>` with hyphens → underscores
- ⚠️ All params typed as string (no type inference)

---

## Rune Integration

**RuneToolRegistry** (`registry.rs`):
- Scans `~/.crucible/runes/` and `{kiln}/runes/`
- Metadata macros: `#[tool(...)]`, `#[param(...)]`, `#[hook(...)]`

**MCP Gateway** (`mcp_gateway.rs`):
- Connect to external MCP servers (stdio/SSE)
- Tool prefixing, allowed/blocked tools filtering

---

## Security Assessment

| Concern | Risk | Mitigation | Status |
|---------|------|------------|--------|
| Path traversal in NoteTools | HIGH | `validate_path_within_kiln()` | ✅ Fixed |
| Command injection in Just | LOW | Recipe validated against justfile | ✅ Safe |
| Rune sandboxing | LOW | Rune VM has no unsafe/FFI | ✅ Safe |
| Upstream MCP trust | MEDIUM | User controls config | ✅ Acceptable |

### Security Roadmap

**MVP Scope:**
- [x] Path traversal validation in NoteTools ✅ Complete

**Post-MVP (Planned):**
- [ ] Kiln-level security policies (access control, allowed paths)
- [ ] Agent security policies (tool restrictions, execution sandboxing)
- [ ] LLM provider policies (rate limits, content filtering, cost caps)

These policies are intentionally deferred - MVP focuses on single-user local operation where the user trusts their own agents and LLM providers. Multi-user and hosted deployments will need these controls.

---

## Performance

- **Just**: O(1) lazy load + cache
- **Rune**: O(n) file scan on startup
- **Tool execution**: 10-50ms subprocess spawn (Just), 1-20ms (Rune)
- **Event processing**: ~1-5ms total for 3-5 handlers

---

## Recommendations

### Immediate
1. ~~Add path validation in NoteTools (security)~~ ✅ Done
2. Implement parser notification TODOs

### Medium Term
1. Consider Rune compilation caching
2. Add Just parameter type inference
3. Expose Rune compilation errors in results

### Low Priority
1. EventBus metrics (handler timing)
2. Tool documentation generation from schemas
