## Why

The pipeline crate has been implemented with a clean five-phase architecture, but Phase 4 (Enrichment) and Phase 5 (Storage) integration have placeholder implementations with TODO comments. We need to complete the integration to make the pipeline fully functional for all frontends.

## What Changes

- Complete Phase 4 enrichment integration in NotePipeline
- Complete Phase 5 storage integration with NoteIngestor
- Wire up real enrichment service calls instead of placeholder code
- Enable end-to-end pipeline processing
- Add comprehensive integration tests

## Impact

- Affected specs: pipeline (new capability)
- Affected code: `crates/crucible-pipeline/src/note_pipeline.rs`, `crates/crucible-enrichment/`, `crates/crucible-surrealdb/`
- Frontends: CLI, Desktop, MCP, Obsidian plugin (all benefit from complete pipeline)