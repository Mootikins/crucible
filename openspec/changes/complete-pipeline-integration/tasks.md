## 1. Add Pipeline Trait (DIP Architecture)

- [ ] 1.1 Create `crucible-core/src/processing/pipeline.rs`
- [ ] 1.2 Define `NotePipelineOrchestrator` trait with `process()` method
- [ ] 1.3 Define `ProcessingResult` enum (Success, Skipped, NoChanges)
- [ ] 1.4 Define `PipelineMetrics` struct for performance tracking
- [ ] 1.5 Export trait from `crucible-core/src/processing/mod.rs`

## 2. Complete Phase 4 Integration in NotePipeline

- [ ] 2.1 Remove placeholder enrichment code in NotePipeline::process()
- [ ] 2.2 Wire up EnrichmentService::enrich() with parsed note and changed blocks
- [ ] 2.3 Handle enrichment errors with proper context
- [ ] 2.4 Update phase 4 metrics (timing, embeddings count)
- [ ] 2.5 Implement `NotePipelineOrchestrator` trait for `NotePipeline`

## 3. Complete Phase 5 Integration in NotePipeline

- [ ] 3.1 Add NoteIngestor as dependency (inject via constructor)
- [ ] 3.2 Remove placeholder storage code in NotePipeline::process()
- [ ] 3.3 Call NoteIngestor::ingest_enriched() with enriched note
- [ ] 3.4 Keep existing Merkle tree and file state storage
- [ ] 3.5 Update phase 5 metrics (timing, storage success)

## 4. Complete ingest_enriched TODOs

- [ ] 4.1 Store enrichment metadata as EAV properties (namespace: "computed")
- [ ] 4.2 Map metadata fields: reading_time, complexity_score, language
- [ ] 4.3 Store inferred relations (when available)
- [ ] 4.4 Add error handling for metadata/relation storage failures

## 5. Remove NoteEnricher (Eliminate Duplication)

- [ ] 5.1 Delete `crates/crucible-enrichment/src/note_enricher.rs`
- [ ] 5.2 Remove NoteEnricher exports from `crates/crucible-enrichment/src/lib.rs`
- [ ] 5.3 Update any remaining references in doc comments
- [ ] 5.4 Keep DefaultEnrichmentService (core enrichment logic)

## 6. Integration Tests

- [ ] 6.1 Create test for full pipeline flow (Phases 1-5)
- [ ] 6.2 Test enrichment with embeddings enabled
- [ ] 6.3 Test enrichment with embeddings disabled (metadata only)
- [ ] 6.4 Test error scenarios (file not found, parse errors, storage failures)
- [ ] 6.5 Validate metrics collection across all phases

## 7. Documentation and Cleanup

- [ ] 7.1 Remove all TODO comments from NotePipeline
- [ ] 7.2 Update NotePipeline doc comments with complete examples
- [ ] 7.3 Update crucible-pipeline lib.rs docs to reference trait
- [ ] 7.4 Add example showing trait-based usage for frontends
