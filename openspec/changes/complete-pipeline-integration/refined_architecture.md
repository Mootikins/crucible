# Simplified Architecture for Pipeline Integration

## Why

After critical review, we identified that:
1. The proposed ContentEnrich/MetadataEnrich split is unnecessary - `DefaultEnrichmentService` already has good internal separation
2. There's architectural duplication between `NotePipeline` (new) and `NoteEnricher` (old)
3. Following DIP, we need a pipeline trait abstraction, not just a concrete type
4. Circuit breaker and complex configuration are premature for MVP

This simplified approach completes the pipeline while maintaining clean architecture principles.

## What Changes

### Architecture Refinements
- **Add Pipeline Trait**: `NotePipelineOrchestrator` trait in crucible-core (DIP pattern)
- **Remove Duplication**: Delete `NoteEnricher` entirely - `NotePipeline` is the single orchestrator
- **Keep Service Simple**: `DefaultEnrichmentService` stays as-is (already well-structured)
- **Trait-Based DI**: Frontends depend on trait, not concrete implementation

### Implementation Changes
- Add `NotePipelineOrchestrator` trait to `crucible-core/src/processing/`
- Complete Phase 4 in NotePipeline (wire up existing `EnrichmentService`)
- Complete Phase 5 in NotePipeline (call existing `ingest_enriched()`)
- Finish TODOs in `ingest_enriched()` (metadata + relations storage)
- Remove `note_enricher.rs` from crucible-enrichment crate
- Add integration tests for end-to-end pipeline

### What We're NOT Doing (Unnecessary Complexity)
- ❌ ContentEnrich/MetadataEnrich split - service already handles this well internally
- ❌ Circuit breaker pattern - defer to post-MVP
- ❌ Complex configuration centralization - keep it simple with defaults

## Impact

- Affected specs: pipeline (new capability)
- Affected code:
  - `crates/crucible-core/src/processing/pipeline.rs` (new trait)
  - `crates/crucible-pipeline/src/note_pipeline.rs` (complete + implement trait)
  - `crates/crucible-enrichment/src/` (remove note_enricher.rs)
  - `crates/crucible-surrealdb/src/eav_graph/ingest.rs` (complete TODOs)
- Frontends: CLI, Desktop, MCP, Obsidian plugin (all benefit from complete, unified pipeline)

## Simplified Pipeline Architecture

### Data Flow (5 Phases)
```
File → [Filter] → [Parse] → [Merkle Diff] → [Enrich] → [Store]
  Phase 1    Phase 2       Phase 3          Phase 4    Phase 5
```

### Trait-Based Architecture
```rust
// crucible-core defines abstraction
pub trait NotePipelineOrchestrator {
    async fn process(&self, path: &Path) -> Result<ProcessingResult>;
}

// crucible-pipeline provides implementation
impl NotePipelineOrchestrator for NotePipeline { ... }

// Frontends depend on trait
fn process_note(pipeline: Arc<dyn NotePipelineOrchestrator>) { ... }
```

### Component Responsibilities
- **NotePipeline**: Orchestrates all 5 phases (coordinates "what and when")
- **EnrichmentService**: Embeddings + metadata + relations (handles "how")
- **NoteIngestor**: Storage operations (persists results)

This approach follows SOLID principles while avoiding unnecessary complexity.