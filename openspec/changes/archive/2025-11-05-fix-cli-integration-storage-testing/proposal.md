# Fix CLI Integration and Storage Testing

## Why

The Crucible CLI has strong foundational architecture with excellent file processing and database capabilities, but critical bugs prevent reliable operation for real-world usage. The search system has known issues with emoji and Unicode character handling, binary file detection is incomplete, and storage integration testing reveals gaps between CLI commands and the storage layer.

Current issues that affect user experience:
- **Search crashes with Unicode**: Emoji searches (😀🔍🎯) and accented characters cause failures
- **Binary file crashes**: Binary files processed as text can cause memory issues and crashes
- **Storage inconsistency**: CLI commands work individually but have data consistency problems
- **Missing robustness**: No comprehensive testing of integration points or edge cases

## What Changes

- **Fix Unicode Search**: Implement proper UTF-8 validation and Unicode normalization in search functionality
- **Enhance Binary Detection**: Improve binary file detection to prevent processing non-text files as text
- **Storage Integration Tests**: Add comprehensive tests verifying CLI-storage integration consistency
- **End-to-End Workflow Tests**: Create tests for complete user workflows from document creation to search
- **Performance and Concurrency Tests**: Add load testing and race condition detection

## Impact

- **Affected crates**:
  - `crates/crucible-parser/` - enhance search with Unicode support
  - `crates/crucible-cli/src/commands/` - add integration tests for CLI commands
  - `crates/crucible-core/src/storage/` - improve storage consistency validation
  - `tests/` - add comprehensive CLI integration and end-to-end tests
- **User experience impact**:
  - Search works reliably with emojis and international characters
  - CLI handles diverse content types safely without crashes
  - Storage operations maintain data integrity across CLI workflows
  - Performance under load is predictable and reliable
  - Comprehensive testing prevents regressions in critical functionality

## Timeline

- **Week 1**: Fix critical Unicode and binary file handling bugs
- **Week 2**: Implement storage integration testing framework
- **Week 3**: Add end-to-end workflow tests and performance testing
- **Week 4**: Complete concurrency testing and error recovery validation