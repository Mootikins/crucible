---
description: Architecture analysis of crucible-core traits and type organization
type: analysis
system: core
status: review
updated: 2024-12-13
tags:
  - meta
  - analysis
  - architecture
---

# Core Types & Traits Analysis

## Executive Summary

The crucible-core crate follows **Dependency Inversion** architecture where traits define abstractions and implementations depend on core, not vice versa.

**Key Findings:**
- Strong adherence to SOLID principles with trait-based abstractions
- Well-organized type ownership (parser types in parser::types, hash types split appropriately)
- Some potentially unused traits (RelationalDB, GraphDB, DocumentDB)
- Hash types split between parser::types::BlockHash and types::hashing for circular dependency avoidance

---

## Critical Issues

- [ ] **Large Unused Traits: RelationalDB, GraphDB, DocumentDB**
  - Location: `/home/moot/crucible/crates/crucible-core/src/database.rs`
  - Issue: Comprehensive trait definitions (100+ lines each), but Storage trait is what's actually used
  - Recommendation: Verify usage. If unused, consider moving to experimental/examples or removing

## High Priority Issues

- [ ] **Type Duplication: Record/RecordId/QueryResult**
  - Location: `database.rs` vs `traits/storage.rs`
  - Issue: Database.rs defines these types, Storage trait re-exports them. Potential confusion.
  - Recommendation: Document that StorageError = DbError alias

- [ ] **Name Conflict: Note vs NoteNode**
  - Location: `database::Note` vs `note::NoteNode`
  - Issue: Two different "Note" concepts
  - Recommendation: Rename database::Note to Document or DatabaseNote

## Resolved Issues

- [x] **Type Name Collisions Clarified**
  - Location: Multiple crates
  - Resolution: Added clarifying documentation to semantic collisions:
    - `ToolProvider` (session events) vs `ToolSource` (tool indexing)
    - `ModelCapability` (provider-level vs feature-level)
    - `MessageRole` (canonical LLM API vs domain-specific)
    - `SessionConfig` (application vs transport vs session layer)
  - See PR #37

## Medium Priority Issues

- [ ] **Hash Type Split: BlockHash Location**
  - Location: BlockHash defined in parser::types, re-exported in types::hashing
  - Issue: Could be confusing - hash type split between modules
  - Recommendation: Add prominent documentation explaining circular dependency avoidance

- [ ] **Documentation Coverage: Database Complex Types**
  - Location: database.rs structs (TableSchema, SelectQuery, JoinQuery, etc.)
  - Issue: Complex types with minimal documentation
  - Recommendation: Add doc comments with examples

## Type Ownership Summary

**Clear Ownership:**
- Parser types: `parser::types/` (canonical)
- Hash types: `types::hashing` (FileHash), `parser::types` (BlockHash)
- ACP types: `types::acp` (canonical)
- Database types: `database.rs` (canonical)
- Processing types: `processing/mod.rs` (canonical)
- Agent types: `agent::types` (canonical)
- Enrichment types: `enrichment::types` (canonical)

## Trait Implementation Matrix

| Trait | Core Defines | Implemented In | Status |
|-------|-------------|----------------|--------|
| Storage | ✅ | crucible-surrealdb | ✅ Active |
| MarkdownParser | ✅ | crucible-parser | ✅ Active |
| ContentHasher | ✅ | core (Blake3/SHA256) | ✅ Active |
| ToolExecutor | ✅ | crucible-lua, crucible-just | ✅ Active |
| EmbeddingProvider | ✅ | crucible-enrichment | ✅ Active |
| RelationalDB | ✅ | None visible | ❓ Unused? |
| GraphDB | ✅ | None visible | ❓ Unused? |
| DocumentDB | ✅ | None visible | ❓ Unused? |

**Overall Assessment**: High quality architecture with minor cleanup opportunities.
