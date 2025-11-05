# Implementation Tasks

## 1. Remove Legacy Daemon Code
- [ ] 1.1 Remove `spawn_kiln_processor()` function from `kiln_processor.rs`
- [ ] 1.2 Remove any imports or dependencies related to process spawning
- [ ] 1.3 Remove calls to process spawning functions in CLI code
- [ ] 1.4 Clean up any daemon-related configuration or constants

## 2. Integrate Blocking File Processing
- [ ] 2.1 Add file processing startup sequence to `main.rs`
- [ ] 2.2 Integrate `EventDrivenEmbeddingProcessor` for blocking processing
- [ ] 2.3 Implement error handling for file processing failures
- [ ] 2.4 Add progress indicators for file processing startup
- [ ] 2.5 Ensure database consistency using `BatchAwareSurrealClient`

## 3. CLI Architecture Updates
- [ ] 3.1 Update main CLI startup flow to include file processing
- [ ] 3.2 Modify command execution to wait for file processing completion
- [ ] 3.3 Add configuration options for file processing behavior
- [ ] 3.4 Implement graceful shutdown for file processing

## 4. Test Updates
- [ ] 4.1 Identify tests that reference daemon functionality
- [ ] 4.2 Remove or update daemon-related test cases
- [ ] 4.3 Create tests for integrated file processing workflow
- [ ] 4.4 Add performance tests for startup time impact
- [ ] 4.5 Test error handling and recovery scenarios

## 5. Documentation Updates
- [ ] 5.1 Update README.md to remove daemon references
- [ ] 5.2 Update docs/ARCHITECTURE.md for single-binary design
- [ ] 5.3 Update CLAUDE.md agent instructions
- [ ] 5.4 Add documentation for new file processing workflow
- [ ] 5.5 Create migration guide for any affected APIs

## 6. Integration Testing
- [ ] 6.1 Test all CLI commands with integrated file processing
- [ ] 6.2 Verify performance impact is acceptable
- [ ] 6.3 Test file change detection and processing
- [ ] 6.4 Validate database consistency after file processing
- [ ] 6.5 Test error handling and recovery scenarios