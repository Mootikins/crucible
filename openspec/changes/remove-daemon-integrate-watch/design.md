# Design Document: Remove Daemon Architecture and Integrate Watch

## Context

The Crucible system has evolved from a multi-process architecture to a single-binary design, but remnants of the old daemon-based architecture remain. The `crucible-daemon` crate has been removed from the workspace, yet code still exists that attempts to spawn external processes. Additionally, while the system has sophisticated file watching capabilities, they aren't fully integrated into the main CLI workflow to ensure data freshness before operations.

Key constraints:
- Must maintain existing CLI command behavior and interfaces
- Cannot break existing file watching real-time capabilities
- Must ensure data is processed before CLI operations
- Should eliminate process spawning overhead and complexity
- Must preserve concurrent database operation performance

## Goals / Non-Goals

**Goals:**
- Eliminate all daemon and process spawning code from the codebase
- Integrate file processing into CLI startup for data freshness
- Maintain single-binary architecture with no external dependencies
- Preserve existing file watching and real-time update capabilities
- Ensure backward compatibility for CLI command interfaces
- Provide clear feedback during file processing startup

**Non-Goals:**
- Change the fundamental architecture of CLI commands
- Modify the existing file watching real-time behavior
- Alter database schemas or storage formats
- Change the CLI command interface or arguments
- Implement new file processing algorithms

## Decisions

### Decision 1: Blocking File Processing on Startup
**What**: Process all pending file changes before executing CLI commands
**Why**:
- Ensures commands operate on up-to-date data
- Eliminates race conditions between file changes and command execution
- Leverages existing sophisticated `EventDrivenEmbeddingProcessor`
- Provides predictable behavior for users

**Alternatives considered:**
- Background processing only (risk of stale data)
- Lazy loading per command (inconsistent state)
- Separate processing step (breaks seamless UX)

### Decision 2: Complete Removal of Process Spawning Code
**What**: Remove all `spawn_kiln_processor()` and related process management code
**Why**:
- The "kiln" binary doesn't exist and attempts to spawn it fail
- Process spawning adds unnecessary complexity and overhead
- Single-binary architecture is simpler and more reliable
- Eliminates process coordination and timing issues

**Alternatives considered:**
- Keep the code for future use (unnecessary complexity)
- Replace with different process (doesn't solve core issue)
- Fix the spawning to work correctly (still multi-process complexity)

### Decision 3: Integration with Existing Watch Infrastructure
**What**: Use existing `EventDrivenEmbeddingProcessor` and related systems
**Why**:
- Leverages sophisticated, tested file processing pipeline
- Maintains existing real-time watching capabilities
- Preserves investment in watch infrastructure
- Minimal code changes required

**Alternatives considered:**
- Build new file processing system (redundant effort)
- Simplified file scanning (loses existing capabilities)
- External file processing tools (adds dependencies)

### Decision 4: Graceful Startup Integration
**What**: Add file processing as a startup step with progress feedback
**Why**:
- Provides clear user feedback during processing
- Allows for error handling and recovery
- Maintains responsive CLI experience
- Enables configuration options for processing behavior

**Alternatives considered:**
- Silent processing (poor UX for large file sets)
- Force users to run separate commands (breaks workflow)
- Background processing with polling (complex coordination)

## Risks / Trade-offs

**Risk**: Increased startup time for CLI commands
**Mitigation**:
- Processing is incremental and cached
- Progress indicators provide user feedback
- Configuration options for processing behavior
- Performance monitoring and optimization

**Risk**: Memory usage during file processing
**Mitigation**:
- Streaming processing of large files
- Proper cleanup after processing completion
- Memory monitoring and limits
- Existing efficient processing pipeline

**Trade-off**: Simplicity vs. Processing Flexibility
**Decision**: Prioritize simplicity and reliability for common use cases
**Impact**: Users with custom processing needs may need additional configuration

**Trade-off**: Startup Time vs. Data Freshness
**Decision**: Prioritize data freshness for better user experience
**Impact**: Slightly slower startup, but more reliable and predictable results

## Migration Plan

### Phase 1: Code Cleanup (Low Risk)
1. Remove `spawn_kiln_processor()` function
2. Clean up related imports and dependencies
3. Update any callers to use in-process alternatives
4. Remove daemon-related test cases

### Phase 2: File Processing Integration (Medium Risk)
1. Add file processing startup to main CLI
2. Integrate existing `EventDrivenEmbeddingProcessor`
3. Add error handling and progress feedback
4. Test with various file set sizes

### Phase 3: User Experience Polish (Low Risk)
1. Add configuration options for processing behavior
2. Improve progress indicators and feedback
3. Update documentation and help text
4. Performance optimization based on usage data

### Phase 4: Validation (Low Risk)
1. Comprehensive testing of all CLI commands
2. Performance benchmarking and optimization
3. User acceptance testing and feedback
4. Documentation and training updates

## Open Questions

- Should we add a `--no-process` flag for users who want faster startup? (Likely yes)
- What's the optimal timeout for file processing before erroring? (Needs testing)
- Should we provide estimated processing time for large file sets? (Nice to have)
- Can we cache processing results between CLI invocations for better performance? (Future enhancement)