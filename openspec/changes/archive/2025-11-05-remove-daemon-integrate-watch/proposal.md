# Remove Daemon Architecture and Integrate Watch into CLI

## Why

The Crucible system currently has legacy daemon-related code and process spawning logic that is inconsistent with the actual single-binary architecture. While the system already operates without external daemons, there are remnants of daemon functionality and attempts to spawn non-existent processes. Additionally, the sophisticated file watching capabilities exist but are not fully integrated into the main CLI workflow to ensure files are processed before operations.

Core problems this addresses:
- **Legacy Process Spawning**: Code in `kiln_processor.rs` attempts to spawn a non-existent "kiln" binary
- **Inconsistent Architecture**: Documentation and some tests reference daemon concepts that no longer exist
- **Stale Data Risk**: Files may not be processed before CLI operations, potentially returning outdated results
- **Integration Gaps**: Powerful watch capabilities exist but aren't fully integrated into command execution flow

## What Changes

- **Remove Legacy Daemon Code**: Eliminate `spawn_kiln_processor()` function and any process spawning attempts
- **Integrate Blocking File Processing**: Add file processing to CLI startup using existing `EventDrivenEmbeddingProcessor`
- **Update Architecture Documentation**: Remove daemon references and document single-binary design
- **Clean Up Tests**: Remove or update tests that expect daemon behavior
- **Ensure Up-to-date Data**: Files are processed before every CLI command execution

## Impact

- **Affected crates**:
  - `crates/crucible-surrealdb/` - remove process spawning code from `kiln_processor.rs`
  - `crates/crucible-cli/` - integrate blocking file processing into main binary
  - `crates/crucible-watch/` - better integration with main CLI workflow
  - `tests/` - remove daemon-related tests
- **Architecture Changes**:
  - Single binary operation with no external process dependencies
  - Files processed before every command execution for data freshness
  - Simplified startup process without process coordination
  - Consistent behavior across all CLI commands
- **Performance Impact**:
  - Eliminate process spawning overhead
  - Additional startup time for file processing (offset by data freshness benefits)
  - Better resource utilization with in-process processing

## Timeline

- **Phase 1** (1-2 hours): Remove legacy process spawning code
- **Phase 2** (2-3 hours): Integrate blocking file processing into CLI startup
- **Phase 3** (1 hour): Clean up daemon-related tests and documentation
- **Phase 4** (1-2 hours): Testing and validation
- **Total**: 5-8 hours of development work

## Benefits

- **Simplified Architecture**: True single-binary operation without external dependencies
- **Data Freshness**: Files always processed before CLI operations
- **Better Reliability**: No process coordination or timing issues
- **Improved Performance**: Eliminate process spawning overhead
- **Consistent Behavior**: All commands operate on up-to-date data