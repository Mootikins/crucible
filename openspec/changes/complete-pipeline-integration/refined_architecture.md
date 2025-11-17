# Refined Architecture for Pipeline Integration

## Why

The pipeline crate has been implemented with a clean five-phase architecture, but Phase 4 (Enrichment) and Phase 5 (Storage) integration have placeholder implementations with TODO comments. We need to complete the integration using a refined architecture that gives crucible-pipeline proper control over data flow and resource management while breaking apart the monolithic NoteEnricher approach.

## What Changes

### Architecture Refinements
- **Break apart Phase 4**: Split monolithic NoteEnricher into balanced 2-step approach (ContentEnrich + MetadataEnrich)
- **Centralize configuration**: Move ALL configuration from enrichment crate to config crate
- **Pipeline resource control**: Pipeline controls "diameter of the pipe" (batch sizes, parallelism, memory limits)
- **Clear separation**: Pipeline coordinates "what and when", services handle "how"

### Implementation Changes
- Implement ContentEnrichStep (block selection + embedding generation)
- Implement MetadataEnrichStep (metadata extraction + relation inference)
- Complete Phase 5 storage integration with NoteIngestor
- Centralize all configuration in crucible-config
- Add pipeline-level resource management and strategy selection
- Enable end-to-end pipeline processing with proper error bubbling
- Add comprehensive integration tests

## Impact

- Affected specs: pipeline (new capability)
- Affected code: `crates/crucible-pipeline/src/note_pipeline.rs`, `crates/crucible-enrichment/`, `crates/crucible-surrealdb/`, `crates/crucible-config/`
- Frontends: CLI, Desktop, MCP, Obsidian plugin (all benefit from complete pipeline with resource control)

## Refined Pipeline Architecture

### Data Flow
```
File → [Filter] → [Parse] → [Merkle Diff] → [Content Enrich] → [Metadata Enrich] → [Store]
  Phase 1    Phase 2       Phase 3          Phase 4a            Phase 4b           Phase 5
```

### Pipeline Control Points
- **Resource Management**: Batch sizes, parallelism, memory limits
- **Strategy Selection**: Incremental vs full enrichment approaches
- **Configuration**: All settings centralized in config crate
- **Error Handling**: Simple error bubbling with clear boundaries

### Service Responsibilities
- **ContentEnrichService**: Block selection and embedding generation
- **MetadataEnrichService**: Metadata extraction and relation inference
- **NoteIngestor**: Storage and persistence operations

This approach provides pipeline control over the "diameter of the pipe" while keeping service complexity manageable and maintaining clear separation of concerns.