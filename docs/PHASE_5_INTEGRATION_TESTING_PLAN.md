# Phase 5 Integration Testing Plan

## Overview
This document outlines the comprehensive integration testing plan for Phase 5 of the Crucible OpenSpec change "2025-11-08-enhance-markdown-parser-eav-mapping". Phase 5 focuses on validating the enhanced EAV+Graph functionality and Document→Note renaming in end-to-end scenarios.

## Current Status Summary
- ✅ **Fixed failing test**: `test_filter_items_fuzzy_match` - corrected query from 'mydoc' to 'mynote'
- ✅ **All tests passing**: 741 unit tests + 9 interactive tests + 8 integration tests
- ✅ **CLI streamlined**: Removed heavy dependencies (crucible-llm, crucible-tools, crucible-watch)
- ✅ **Core CLI commands**: search, fuzzy, stats, config, diff, status, storage, parse working
- ✅ **Document→Note migration**: Completed with 3 conventional commits
- ✅ **Working tree clean**: Ready for integration testing

## Testing Infrastructure

### Available Test Data
- **Test-kiln**: 12 comprehensive markdown files in `/home/moot/crucible/examples/test-kiln/`
- **150+ test scenarios**: Various search queries and edge cases
- **45 unique property types**: Rich frontmatter with nested objects, arrays, various data types
- **Complex content**: LaTeX, Obsidian callouts, tables, code blocks, wikilinks, embedded content
- **25,000+ words**: Comprehensive feature test file (372 lines)

### Existing Test Patterns
```rust
// Standard database setup pattern
let client = SurrealClient::new_memory().await.unwrap();
apply_eav_graph_schema(&client).await.unwrap();
let store = EAVGraphStore::new(client.clone());

// Test data creation pattern
let mut doc = ParsedNote::default();
doc.path = PathBuf::from("test.md");
doc.content_hash = "test123".into();
```

## Integration Test Suite Structure

### 1. EAV+Graph Functionality Tests

#### 1.1 ASTBlockType Content Validation (21 test cases)
**File**: `tests/integration/eav_graph_content_tests.rs`

**Test Coverage**:
- All 21 ASTBlockType variants: Heading, Paragraph, CodeBlock, ListBlock, ListItem, Callout, LatexExpression, Table, Blockquote, HorizontalRule
- Metadata preservation for each block type
- Hash computation and verification
- Parent-child relationship maintenance

**Test Data**: Use comprehensive feature test from test-kiln

```rust
#[tokio::test]
async fn test_all_ast_block_types_parsing_and_storage() {
    // Test each ASTBlockType with representative content
    // Verify EAV mapping preserves all metadata
    // Validate storage and retrieval integrity
}
```

#### 1.2 Embedded Content Processing Tests
**File**: `tests/integration/embedded_content_tests.rs`

**Test Coverage**:
- Obsidian wikilinks: `[[Note]]`, `[[Note|Alias]]`, `[[path/to/note]]`
- Embedded content: `![[note]]`, `![[image.png]]`, `![[PDF]]`
- Block references: `^block-id`, `[[note#^block-id]]`
- Heading links: `[[note#heading]]`
- External URLs and platform-specific content

**Test Cases**:
```rust
#[tokio::test]
async fn test_obsidian_wikilink_processing() { /* ... */ }
#[tokio::test]
async fn test_embedded_content_extraction() { /* ... */ }
#[tokio::test]
async fn test_block_reference_resolution() { /* ... */ }
```

#### 1.3 Content Classification Tests
**File**: `tests/integration/content_classification_tests.rs`

**Test Coverage**:
- Note vs Document distinction (markdown vs PDF/DOC)
- ContentCategory pattern matching for all 12 categories
- File extension handling and MIME type detection
- Platform-specific content classification (YouTube, GitHub, Wikipedia, StackOverflow)

### 2. Document→Note Renaming Verification

#### 2.1 Type Consistency Tests
**File**: `tests/integration/document_note_migration_tests.rs`

**Test Coverage**:
- ParsedDocument → ParsedNote type consistency
- Serialization/deserialization compatibility
- Backward compatibility with legacy data
- Type safety across parsing pipeline

#### 2.2 ContentCategory Validation Tests
**File**: `tests/integration/content_category_tests.rs`

**Test Coverage**:
- All 12 ContentCategory variants: Note, Image, Video, Audio, PDF, Document, Other, Web, YouTube, GitHub, Wikipedia, StackOverflow
- Pattern matching in `as_str()`, `Display`, `FromStr` implementations
- File extension mapping for file-based categories
- Web content detection and classification

### 3. CLI Integration Tests

#### 3.1 Core Command Validation
**File**: `tests/integration/cli_command_tests.rs`

**Test Coverage**:
- **search**: Interactive search with fuzzy finder
- **fuzzy**: Metadata search across tags, properties, content
- **stats**: Kiln statistics and metrics
- **config**: Configuration management
- **diff**: File/directory comparison
- **status**: Storage status reporting
- **storage**: Storage operations and management
- **parse**: File parsing and analysis

**Test Pattern**:
```rust
#[tokio::test]
async fn test_search_command_with_test_kiln() {
    let config = create_test_config_with_test_kiln();
    let result = execute_search_command(config, SearchArgs { query: None, limit: 10 }).await;
    assert!(result.is_ok());
    // Validate search results contain expected files
}
```

#### 3.2 CLI Output Format Tests
**File**: `tests/integration/cli_output_tests.rs`

**Test Coverage**:
- JSON output validation
- Table formatting and structure
- Plain text output consistency
- Configuration-driven output customization

#### 3.3 CLI Error Handling Tests
**File**: `tests/integration/cli_error_handling_tests.rs`

**Test Coverage**:
- Invalid command arguments
- Missing or malformed configuration
- Database connectivity issues
- File permission and access errors
- Graceful degradation scenarios

### 4. End-to-End Pipeline Tests

#### 4.1 Complete Workflow Validation
**File**: `tests/integration/end_to_end_pipeline_tests.rs`

**Test Coverage**:
- Parse → Store → Query complete pipeline
- Complex markdown with mixed content types
- Performance with large files and datasets
- Concurrent processing and database consistency

**Key Test Cases**:
```rust
#[tokio::test]
async fn test_comprehensive_feature_file_processing() {
    // Process the 372-line comprehensive feature test file
    // Validate all content types are handled correctly
    // Verify storage integrity and retrieval accuracy
}

#[tokio::test]
async fn test_mixed_content_pipeline() {
    // Test files with LaTeX, callouts, tasks, footnotes
    // Verify cross-content-type interactions
    // Validate complete EAV graph construction
}
```

#### 4.2 Performance Benchmarking
**File**: `tests/integration/performance_tests.rs`

**Success Criteria**:
- <100ms parsing time per note
- <50ms storage time per note
- <500ms search queries on full dataset
- Memory usage within acceptable limits

### 5. Edge Cases and Error Handling

#### 5.1 Malformed Content Tests
**File**: `tests/integration/malformed_content_tests.rs`

**Test Coverage**:
- Invalid markdown syntax
- Corrupted frontmatter
- Broken wikilinks and references
- Invalid or conflicting metadata

#### 5.2 System Constraint Tests
**File**: `tests/integration/system_constraint_tests.rs`

**Test Coverage**:
- Database connectivity failures
- File system permission errors
- Resource exhaustion scenarios
- Network timeout handling

## Test Execution Plan

### Phase 1: Foundation Setup (Day 1)
1. Create test directory structure
2. Set up test utilities and helpers
3. Verify test-kiln data accessibility
4. Establish baseline measurements

### Phase 2: Core Functionality (Days 1-2)
1. EAV+Graph functionality tests (21 content types)
2. Document→Note migration verification
3. Content classification validation
4. Basic CLI command testing

### Phase 3: Integration and Performance (Days 2-3)
1. CLI integration with real data
2. End-to-end pipeline validation
3. Performance benchmarking
4. Output format validation

### Phase 4: Edge Cases and Robustness (Day 4)
1. Error handling and recovery
2. Malformed content processing
3. System constraint validation
4. Cross-platform compatibility

### Phase 5: Final Validation (Day 5)
1. Complete test suite execution
2. Coverage analysis and gap identification
3. Performance regression testing
4. Quality assurance sign-off

## Success Criteria

### Quantitative Metrics
- **Test Coverage**: 95%+ for new functionality
- **Test Success Rate**: 100% (no flaky tests)
- **Performance Benchmarks**:
  - Parse: <100ms per note
  - Store: <50ms per note
  - Search: <500ms on full dataset
- **Error Handling**: All error scenarios have graceful degradation

### Qualitative Criteria
- **User Experience**: CLI commands provide clear, helpful output
- **Data Integrity**: No data loss or corruption through pipeline
- **Maintainability**: Tests are well-structured and documented
- **Reproducibility**: Tests run consistently across environments

## Test Environment Setup

### Prerequisites
```bash
# Ensure test dependencies
cargo test --package crucible-core
cargo test --package crucible-surrealdb
cargo test --package crucible-parser

# Verify test-kiln availability
ls -la examples/test-kiln/
```

### Test Configuration
```rust
// Test configuration template
let test_config = CliConfig {
    kiln: KilnConfig {
        path: PathBuf::from("examples/test-kiln"),
        ..Default::default()
    },
    database: DatabaseConfig {
        path: Some(":memory:".to_string()),
        ..Default::default()
    },
    ..Default::default()
};
```

## Deliverables

1. **Integration Test Suite**: 50+ comprehensive test cases
2. **Performance Benchmarks**: Baseline measurements and regression tests
3. **CLI Validation Report**: Command functionality and user experience assessment
4. **Quality Assurance Documentation**: Test coverage and validation summary
5. **Final Phase 5 Sign-off**: Complete validation ready for production

## Risk Mitigation

### Technical Risks
- **Database Contamination**: Use in-memory databases with proper isolation
- **Test Data Dependencies**: Create self-contained test fixtures
- **Performance Variability**: Use consistent test environments and baselines

### Timeline Risks
- **Scope Creep**: Maintain focus on Phase 5 requirements
- **Environment Issues**: Have fallback testing strategies
- **Integration Complexity**: Incremental testing approach with early feedback

This comprehensive plan ensures thorough validation of all Phase 5 requirements while leveraging the excellent existing test infrastructure in the Crucible codebase.