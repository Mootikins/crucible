# Add Batch-Aware Database Consistency and File Watching

## Why

The Crucible knowledge management system supports concurrent operations but lacks consistency guarantees when batch processing is enabled. When file system changes are being processed in batches, metadata reads may return stale data, leading to race conditions between database state and in-flight batch operations. Additionally, the file watching capabilities (crucible-watch) were previously removed from the workspace, breaking the integration between file system events and batch processing.

Core problems this addresses:
- **Metadata Inconsistency**: Database reads don't account for pending batch operations, creating stale reads
- **Race Conditions**: CLI commands can read metadata while files are being processed in batches
- **Missing Integration**: File watching capabilities were removed, breaking event-driven processing
- **No Queue Awareness**: Applications cannot choose appropriate consistency levels for their use case

## What Changes

- **Restore crucible-watch**: Re-enable file watching capabilities with enhanced batch operation tracking
- **Implement BatchAwareSurrealClient**: Create wrapper around SurrealClient with queue-aware consistency levels
- **Add Consistency Framework**: Define Eventual, ReadAfterWrite, and Strong consistency levels
- **Event Processor Integration**: Connect batch-aware client with EventDrivenEmbeddingProcessor
- **Extension Traits**: Provide easy conversion from existing SurrealClient instances

## Impact

- **Affected crates**:
  - `crates/crucible-watch/` - restored to workspace with pending operation tracking
  - `crates/crucible-surrealdb/` - added batch_aware_client and consistency modules
  - `crates/crucible-cli/` - can now use batch-aware clients for metadata consistency
- **New capabilities**:
  - Queue-aware database reads with three consistency levels
  - File watching with batch operation status tracking
  - Event processor integration for pending operation monitoring
  - Consistent metadata operations across concurrent CLI usage
- **Performance impact**:
  - No impact on existing concurrent read operations (RwLock-based)
  - Optional consistency checking with configurable timeouts
  - Efficient pending operation indexing by file path

## Timeline

This change has been fully implemented and tested:
- ✅ BatchAwareSurrealClient with 3 consistency levels
- ✅ Event processor integration with EventDrivenEmbeddingProcessor
- ✅ Extension traits for easy client conversion
- ✅ Comprehensive test coverage (3 tests passing)
- ✅ All code committed and ready for review