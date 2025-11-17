# Implementation Tasks

## 0. Architecture Cleanup (Pre-Implementation)
- [ ] 0.1 Re-enable `semantic.rs.disabled` as `semantic.rs` (831 lines of production-ready code)
- [ ] 0.2 Delete `kiln_scanner.rs` (1,429 lines - old polling architecture, marked with TODO)
- [ ] 0.3 Delete `kiln_pipeline_connector.rs` (757 lines - replaced by NotePipeline)
- [ ] 0.4 Refactor `kiln_integration.rs` - extract semantic search, delete old processing (~1,700 lines to remove)
- [ ] 0.5 Delete 11 `.disabled` files from old architecture (CLI common/, commands/)
- [ ] 0.6 Update `lib.rs` exports after module deletions
- [ ] 0.7 Fix compilation errors from deleted modules
- [ ] 0.8 Run full test suite and fix broken tests: `cargo test --workspace`
- [ ] 0.9 Commit cleanup: "refactor: remove old polling architecture (~6,000 lines)"

**Rationale**: The codebase has ~6,000 lines of redundant code from the old embedding_pool polling architecture. The new `NotePipeline` orchestrator supersedes these modules. Cleaning up first provides a clear foundation for the CLI rework.

**Key Finding**: `semantic.rs.disabled` (831 lines) contains a fully-implemented semantic search command with progress bars, reranking, and JSON output. It just needs to be re-enabled and integrated with the new facade pattern.

## 1. Foundation & Setup
- [ ] 1.1 Add dependencies to Cargo.toml (agent-client-protocol, rustyline, indicatif, walkdir)
- [ ] 1.2 Create new module structure (acp/, pipeline/, core_facade.rs)
- [ ] 1.3 Update clap CLI structure with new commands
- [ ] 1.4 Create `CrucibleCore` facade with trait-based interfaces

## 2. Core Facade Layer
- [ ] 2.1 Implement `CrucibleCore::from_config()` initialization
- [ ] 2.2 Add `process_file()` method using `NotePipeline`
- [ ] 2.3 Add `process_kiln()` method for batch processing
- [ ] 2.4 Add `search()` method for semantic search
- [ ] 2.5 Add `get_stats()` method for kiln statistics
- [ ] 2.6 Write unit tests for facade methods

## 3. ACP Client Implementation
- [ ] 3.1 Create `acp/client.rs` with `CrucibleAcpClient` struct
- [ ] 3.2 Implement `Client` trait methods (fs_read_text_file, fs_write_text_file)
- [ ] 3.3 Implement `session_update()` for streaming responses
- [ ] 3.4 Implement `request_permission()` with auto-approve for MVP
- [ ] 3.5 Create `AcpConnection` wrapper for connection management
- [ ] 3.6 Add agent spawning logic (claude-code, gemini, codex)
- [ ] 3.7 Write unit tests with mock agents

## 4. Context Enrichment
- [ ] 4.1 Create `acp/context.rs` for context assembly
- [ ] 4.2 Integrate existing `semantic_search_with_reranking()` from kiln_integration (already implemented!)
- [ ] 4.3 Add context formatting for agent prompts (markdown format from design.md)
- [ ] 4.4 Wire configurable `agent.context_size` from config to search queries
- [ ] 4.5 Write integration tests for context enrichment in chat mode

**Note**: Semantic search functionality already exists in `semantic.rs` (831 lines). The `semantic_search_with_reranking()` function in `kiln_integration.rs` provides full vector search + reranking. We're integrating existing code, not building from scratch.

## 5. Chat Command
- [ ] 5.1 Create `commands/chat.rs` module
- [ ] 5.2 Implement interactive mode with rustyline
- [ ] 5.3 Add one-shot query mode (--query flag)
- [ ] 5.4 Add agent selection (--agent flag)
- [ ] 5.5 Implement streaming response display
- [ ] 5.6 Add error handling and recovery
- [ ] 5.7 Write integration tests for chat command

## 6. Process Command
- [ ] 6.1 Create `commands/process.rs` module
- [ ] 6.2 Implement single file processing
- [ ] 6.3 Implement full kiln scanning with progress bar
- [ ] 6.4 Add force reprocess flag (--force)
- [ ] 6.5 Add watch mode (--watch) with file watcher
- [ ] 6.6 Add metrics summary output
- [ ] 6.7 Write integration tests for process command

## 7. Status Command
- [ ] 7.1 Refactor `commands/status.rs` to use facade
- [ ] 7.2 Display file/note/block statistics
- [ ] 7.3 Show embedding status
- [ ] 7.4 Display recent activity
- [ ] 7.5 Add detailed mode (--detailed flag)
- [ ] 7.6 Write tests for status display

## 8. Search Command (Already Exists!)
- [ ] 8.1 Verify re-enabled `semantic.rs` works with current codebase
- [ ] 8.2 Integrate with `CrucibleCore` facade pattern
- [ ] 8.3 Update CLI module imports to use re-enabled semantic command
- [ ] 8.4 Test `cru search` command end-to-end
- [ ] 8.5 Verify JSON output format and progress bars work correctly

**Note**: `semantic.rs.disabled` (831 lines) is a complete, production-ready implementation with:
- Full `SemanticSearchService` trait
- Vector search + optional reranking
- Progress bars with `indicatif`
- JSON output support
- Snippet extraction
- Error handling

It was disabled during a previous refactor but is ready to re-enable. No new code needed, just integration with the new facade.

## 9. Config Command (Keep Existing)
- [ ] 9.1 Review existing config command - minimal changes needed
- [ ] 9.2 Ensure compatibility with new facade pattern
- [ ] 9.3 Add any new config options (agent.default, etc.)

## 10. Background Processing
- [ ] 10.1 Create `pipeline/processor.rs` for background tasks
- [ ] 10.2 Implement startup processing (unless --no-process)
- [ ] 10.3 Add timeout handling for long-running processes
- [ ] 10.4 Implement graceful degradation on processing errors
- [ ] 10.5 Write tests for background processing logic

## 11. Final Code Cleanup (Post-Implementation)
- [ ] 11.1 Remove `commands/repl/` directory (after chat/act commands proven working)
- [ ] 11.2 Remove `commands/fuzzy.rs` (replaced by semantic search)
- [ ] 11.3 Remove `commands/diff.rs` (not needed for MVP)
- [ ] 11.4 Verify all `.disabled` files removed in Phase 0
- [ ] 11.5 Update imports throughout codebase
- [ ] 11.6 Remove unused dependencies from Cargo.toml

**Note**: Major cleanup (~6,000 lines of old polling architecture) happens in Phase 0. This section is for final cleanup after the new CLI is proven working.

## 12. Testing
- [ ] 12.1 Create integration test suite (`tests/cli_integration.rs`)
- [ ] 12.2 Add tests for each command (chat, process, status, search, config)
- [ ] 12.3 Add error handling tests (invalid config, missing kiln, etc.)
- [ ] 12.4 Add ACP client tests with mock agents
- [ ] 12.5 Add context enrichment tests
- [ ] 12.6 Ensure all tests pass with >80% coverage

## 13. Documentation
- [ ] 13.1 Update CLI README with new commands
- [ ] 13.2 Add ACP setup instructions
- [ ] 13.3 Add migration guide for users (REPL → chat)
- [ ] 13.4 Document agent installation requirements
- [ ] 13.5 Add troubleshooting section

## 14. Final Integration
- [ ] 14.1 Test with real kiln (validate end-to-end flow)
- [ ] 14.2 Test with claude-code agent
- [ ] 14.3 Test background processing with large kilns
- [ ] 14.4 Performance testing (startup time, search speed)
- [ ] 14.5 Fix any bugs discovered during integration testing
- [ ] 14.6 Final code review and cleanup
