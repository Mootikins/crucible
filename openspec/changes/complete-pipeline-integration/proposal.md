## Why

The pipeline crate has been implemented with a clean five-phase architecture, but Phase 4 (Enrichment) and Phase 5 (Storage) integration have placeholder implementations with TODO comments. Additionally, there is architectural duplication between `NotePipeline` (new, 5-phase orchestrator) and `NoteEnricher` (old, 4-phase orchestrator). We need to complete NotePipeline and remove the duplication to provide a single, clean pipeline interface for all frontends.

## What Changes

**Architecture:**
- Add `NotePipelineOrchestrator` trait to crucible-core (Dependency Inversion Principle)
- Remove `NoteEnricher` from crucible-enrichment (eliminate duplication)
- Keep `DefaultEnrichmentService` as-is (already well-structured)

**Implementation:**
- Complete Phase 4 enrichment integration in NotePipeline (wire up EnrichmentService)
- Complete Phase 5 storage integration with NoteIngestor (already has `ingest_enriched()`)
- Add metadata and relations storage to `ingest_enriched()` (complete TODOs)
- Add comprehensive integration tests

**Simplifications (based on critical review):**
- No ContentEnrich/MetadataEnrich split (unnecessary - service already well-separated internally)
- No circuit breaker pattern (defer to post-MVP - use simple retry logic)
- Configuration stays in NotePipeline (minimal, with defaults from enrichment service)

## Impact

- Affected specs: pipeline (new capability)
- Affected code:
  - `crates/crucible-core/src/processing/` (new trait)
  - `crates/crucible-pipeline/src/note_pipeline.rs` (complete phases 4-5)
  - `crates/crucible-enrichment/src/` (remove note_enricher.rs)
  - `crates/crucible-surrealdb/src/eav_graph/ingest.rs` (complete TODOs)
- Frontends: CLI, Desktop, MCP, Obsidian plugin (all benefit from complete, unified pipeline)