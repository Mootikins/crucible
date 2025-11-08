# Crucible Refactor Status – 2025-02-14

## Current State
- Surreal hash storage now reads/writes exclusively through the EPR `entities` table. All lookups, batch queries, content-hash searches, timestamp scans, and CRUD helpers normalize vault paths into `entities:note:*` IDs and operate on entity metadata instead of the legacy `notes` rows.
- `KilnProcessor::needs_processing` no longer queries `notes`. It reuses the EPR-backed hash lookup path, ensuring change detection compares against the same metadata the parser/ingestor writes.
- Hash lookup unit tests now run on the EPR schema (in-memory Surreal + `initialize_kiln_schema`), exercising the new normalization helpers and entity payload expectations.
- Targeted `cargo test -p crucible-surrealdb hash_lookup` passes with warnings limited to unrelated legacy modules.

## Next Work
1. Wire the hybrid Merkle tree (`crucible-core::merkle::hybrid`) into the parser/storage bridge so section hashes persist alongside the new block rows.
2. Replace the remaining legacy helpers (`normalize_document_id` call sites, archived fixtures, Surreal client examples) that still hard-code `notes:` IDs; either delete or migrate them to the EPR dialect.
3. Introduce a trait-focused Surreal abstraction so embedding/kiln modules depend on capabilities (entity read/write, relation CRUD) instead of the concrete client. This unblocks alternative backends and makes ACP/CLI work testable.
4. Expand integration coverage to include chunk hash persistence + deletion via `get_document_chunk_hashes`/`delete_document_chunks`, and add reranking tests that start from `entities:` IDs.
