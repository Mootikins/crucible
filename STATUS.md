# Crucible Refactor Status – 2025-11-08

This file captures the in-flight restart of Crucible so hand-offs are painless. It supplements the OpenSpec docs with the concrete plan we aligned on in this branch.

---

## Guiding Principles

- **Start-from-scratch mindset**: Use the new EPR schema everywhere, defer CRDT and plugin/Rune concerns until ACP + chat MVP is solid.
- **SOLID + DI-first**: Prefer traits over concrete Surreal calls so alternative backends/mocks can slide in. Keep modules small with explicit responsibilities.
- **Concise MVP**: Build only the surfaces needed for ACP + chat CLI; legacy helpers that add scope creep get deleted, not ported.
- **TDD Pragmatism**: Add unit/integration coverage around each new subsystem (EPR ingest, hybrid Merkle, chunk hashing). Focus on correctness, not exhaustive stress tests.
- **Surreal-first ACP**: ACP precedes the CLI chat shell. CLI should be a thin chat interface that hits ACP APIs instead of a QL REPL.

---

## Original Plan (Phases)

1. **Phase 0 – Prep**  
   - Audit SurrealDB crates for `notes:` usage, document target files, ensure test targets run with `CCACHE_DISABLE=1`.

2. **Phase 1 – SurrealDB EPR schema/types** ✅  
   - Land `schema_epr.surql`, typed façade (`epr::{types, store, ingest}`), unit tests.  
   - Route ingestion + document storage through `DocumentIngestor`.

3. **Phase 2 – Core ingestion, Merkle, watch pipeline** (in progress)  
   - Wire the watcher/parser/storage bridge into the new EPR entities and hybrid Merkle tree.  
   - Remove legacy kiln helpers (`notes` tables, placeholder wiki/tag code).  
   - Make change detection rely on the hybrid tree + chunk hashes.

4. **Phase 3 – ACP + chat CLI** (pending)  
   - Embed Zed’s ACP implementation.  
   - Replace CLI REPL with chat-oriented UX that brokers ACP requests via DI’d traits instead of Surreal-specific code.  
   - Leave plugin/Rune work until after ACP is stable.

---

## Current State (2025-11-08)

- **EPR hash lookup**: All hash queries and writes now hit `entities` rows (`entities:note:*` IDs). Lookups (single, batch, by-content, since-timestamp) normalize paths, fetch entity payload fields (relative_path, file_size, source_modified_at), and convert to `crucible_core::StoredHash`. (`crates/crucible-surrealdb/src/hash_lookup.rs`)
- **Write path parity**: `store_hashes`, `remove_hashes`, `get_all_hashes`, `clear_all_hashes` mutate the entity metadata so dedupe, change detection, and chunk hashing all share the same data origin.
- **Kiln processor**: `needs_processing` now calls the EPR-backed hash lookup and compares stored vs current hashes/timestamps, eliminating bespoke `notes` SQL. (`crates/crucible-surrealdb/src/kiln_processor.rs`)
- **Tests**: `cargo test -p crucible-surrealdb hash_lookup` passes (with legacy warnings). The test harness spins up in-memory Surreal, applies the EPR schema, and exercises normalization helpers + conversions.
- **Documentation**: This status page records scope, principles, and outstanding work so the branch has a living changelog.

---

## Next Work (Ordered)

1. **Hybrid Merkle Integration**
   - Persist section/block hashes emitted by `parser::storage_bridge` into `blocks` plus the new hybrid tree structure.
   - Update change detection consumers to request section-level diffs instead of binary trees.

2. **Legacy Cleanup**
   - Remove or migrate lingering `notes:` helpers, archived fixtures, Surreal client demos, and CLI docs that still assume the old schema.
   - Normalize all document ID handling through `normalize_document_id` or equivalent helpers until the codebase is EPR-native.

3. **Database Capability Traits**
   - Introduce thin trait layers (e.g., `EntityStore`, `RelationStore`) so ACP/chat services depend on behavior rather than `SurrealClient`.
   - Provide mock implementations for tests and pave the way for alternate DB backends.

4. **Chunk Hash + Embedding Coverage**
   - Add integration tests for `get_document_chunk_hashes` / `delete_document_chunks` and ensure incremental embedding updates clean up relations/tags.
   - Expand semantic search/reranking tests that start from `entities:` IDs and verify `record_ref_to_string` normalization.

5. **ACP + Chat CLI Foundations**
   - Port Zed’s ACP core, expose it through DI-friendly traits, and replace the CLI REPL with a chat interface that proxies ACP requests.
   - Keep the CLI minimal: chat UX + basic status/diff commands that lean on ACP/stateful services rather than direct DB access.

---

## Reminders

- Always run tests with `CCACHE_DISABLE=1`.
- Ignore CRDT work until ACP + chat milestone is complete.
- Prefer deleting stale modules over scaffolding compatibility layers; the goal is a smaller, healthier codebase ready for the ACP-powered future.
