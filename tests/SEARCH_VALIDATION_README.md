# Search Validation Test Suite

Comprehensive test suite for validating the Crucible knowledge management system's search capabilities across all dimensions.

## Overview

This test suite provides thorough validation of search functionality including:
- **Parsed Metadata Search**: Tags, dates, status, people, custom properties
- **Text Content Search**: Phrases, titles, code blocks, lists, headings
- **Embedding-Based Semantic Search**: Similarity, cross-language, ranking
- **Tool Search Integration**: Discovery, execution, metadata
- **Link Structure Search**: Backlinks, embeds, orphans, graph traversal
- **Interface Parity Testing**: CLI vs REPL vs tool APIs
- **Performance and Validation**: Speed, accuracy, resilience

## Test Structure

### Files

- `search_validation_comprehensive.rs` - Core test harness and metadata/text content tests
- `search_validation_extended.rs` - Semantic, tool, link, parity, and performance tests
- `search_validation_test_runner.rs` - Organized test execution and reporting
- `tests/test-kiln/` - Comprehensive static test kiln with 11 realistic markdown files

### Test Categories

#### 1. Parsed Metadata Search Tests
**Location**: `search_validation_comprehensive.rs` (module: `metadata_search_tests`)

**Tests**:
- `test_tag_based_searches` - Single and multiple tag searches
- `test_date_range_searches` - Creation/modification date queries
- `test_status_priority_searches` - Status and priority filtering
- `test_people_author_searches` - Author and institution searches
- `test_custom_property_searches` - Budget, team size, category searches
- `test_complex_metadata_searches` - Multi-criteria queries
- `test_metadata_search_edge_cases` - Edge cases and error handling

**Coverage**:
- 45+ frontmatter properties
- Tag hierarchies and relationships
- Date range queries and comparisons
- Numeric and textual property searches
- Complex boolean logic

#### 2. Text Content Search Tests
**Location**: `search_validation_comprehensive.rs` (module: `text_content_search_tests`)

**Tests**:
- `test_exact_phrase_matching` - Quoted phrase searches
- `test_title_based_searches` - Title-specific searches
- `test_code_block_searches` - Programming language and code pattern searches
- `test_list_item_searches` - Checklist and task item searches
- `test_heading_searches` - Section and heading level searches
- `test_case_sensitivity_normalization` - Case handling validation
- `test_special_characters_unicode` - Unicode and special character support
- `test_boolean_operators` - AND/OR logic in searches
- `test_proximity_context_search` - Context-aware searching
- `test_content_search_ranking` - Result ordering validation
- `test_content_search_limits` - Result limit handling

**Coverage**:
- Exact and fuzzy matching
- Content type awareness (code, lists, headings)
- Internationalization support
- Search operator functionality

#### 3. Embedding-Based Semantic Search Tests
**Location**: `search_validation_extended.rs` (module: `semantic_search_tests`)

**Tests**:
- `test_content_similarity_across_topics` - Cross-domain similarity
- `test_cross_language_semantic_matching` - Concept translation
- `test_contextual_search_beyond_keywords` - Semantic understanding
- `test_document_recommendation` - Content-based recommendations
- `test_semantic_search_ranking_validation` - Semantic ranking quality
- `test_semantic_search_edge_cases` - Edge cases and limits
- `test_semantic_search_consistency` - Result consistency

**Coverage**:
- Vector similarity computation
- Conceptual relationship understanding
- Ranking algorithm validation
- Provider-agnostic testing

#### 4. Tool Search Integration Tests
**Location**: `search_validation_extended.rs` (module: `tool_search_integration_tests`)

**Tests**:
- `test_tool_discovery_through_search` - Finding available tools
- `test_tool_execution_from_search_results` - Workflow integration
- `test_tool_metadata_searchability` - Tool property searches
- `test_search_tool_workflow_integration` - End-to-end workflows
- `test_search_tool_error_handling` - Error recovery

**Coverage**:
- Tool registry integration
- Search-to-execution workflows
- Metadata indexing
- Error handling and validation

#### 5. Link Structure Search Tests
**Location**: `search_validation_extended.rs` (module: `link_structure_search_tests`)

**Tests**:
- `test_find_documents_linking_to_content` - Backlink discovery
- `test_backlink_analysis_graph_traversal` - Graph relationship analysis
- `test_embed_relationship_discovery` - Embed identification
- `test_orphaned_document_identification` - Link analysis
- `test_link_based_document_ranking` - Link popularity ranking
- `test_wikilink_resolution_validation` - Link validation

**Coverage**:
- 8 different Obsidian link formats
- Graph traversal algorithms
- Link relationship analysis
- Orphan detection

#### 6. Interface Parity Testing
**Location**: `search_validation_extended.rs` (module: `interface_parity_tests`)

**Tests**:
- `test_cli_vs_repl_search_consistency` - Interface consistency
- `test_tool_api_vs_cli_search_consistency` - API consistency
- `test_result_formatting_consistency` - Format standardization
- `test_parameter_handling_consistency` - Parameter validation
- `test_error_handling_consistency` - Error behavior

**Coverage**:
- Multiple interface consistency
- Result format standardization
- Parameter handling validation
- Error behavior uniformity

#### 7. Performance and Validation Tests
**Location**: `search_validation_extended.rs` (module: `performance_validation_tests`)

**Tests**:
- `test_search_performance_large_dataset` - Performance benchmarks
- `test_search_accuracy_completeness` - Result accuracy validation
- `test_search_ranking_quality` - Ranking quality assessment
- `test_search_system_resilience` - Error recovery and stability

**Coverage**:
- Performance benchmarking
- Accuracy validation
- Ranking quality
- System resilience

## Test Kiln

The test suite uses a comprehensive static test kiln located in `tests/test-kiln/` containing:

### Documents (11 files)
- **README - Test Kiln Structure.md** - Meta documentation and testing guide
- **Knowledge Management Hub.md** - Central linking node
- **Project Management.md** - Tasks, timelines, project tracking
- **Research Methods.md** - Academic content and methodology
- **Technical Documentation.md** - Code examples and specifications
- **Contact Management.md** - People and relationships
- **Meeting Notes.md** - Dates, action items, decisions
- **Reading List.md** - Books, articles, learning resources
- **Ideas & Brainstorming.md** - Innovation and concept development
- **API Documentation.md** - Technical specifications
- **Book Review.md** - Detailed literary analysis

### Frontmatter Properties (45+ types)
- **Standard**: type, tags, created, modified, status, priority, aliases, related
- **Extended**: author, category, version, license, rating, location, organization
- **Technical**: api_version, language, framework, endpoints_count, complexity
- **Academic**: institution, peer_reviewed, citation_count, doi, methodology

### Link Types (8 formats)
- Basic wikilinks: `[[Document]]`
- Alias links: `[[Document|Display Text]]`
- Heading links: `[[Document#Header]]`
- Block references: `[[Document^block-id]]`
- Full note embeds: `![[Document]]`
- Section embeds: `![[Document#Section]]`
- Block embeds: `![[Document#^block-id]]`
- External links: `[Text](URL)`

## Running Tests

### Individual Test Categories

```bash
# Run specific test modules
cargo test -p crucible-daemon metadata_search_tests
cargo test -p crucible-daemon text_content_search_tests
cargo test -p crucible-daemon semantic_search_tests
cargo test -p crucible-daemon tool_search_integration_tests
cargo test -p crucible-daemon link_structure_search_tests
cargo test -p crucible-daemon interface_parity_tests
cargo test -p crucible-daemon performance_validation_tests
```

### Using the Test Runner

```rust
use crate::search_validation_test_runner::{SearchTestRunner, SearchTestRunnerConfig, SearchTestCategory};

// Run all tests
let config = SearchTestRunnerConfig::default();
let runner = SearchTestRunner::new(config);
let results = runner.run_all_tests().await?;

// Run specific categories
let config = SearchTestRunnerConfig {
    categories: vec![SearchTestCategory::Semantic, SearchTestCategory::Performance],
    ..Default::default()
};
let runner = SearchTestRunner::new(config);
let results = runner.run_all_tests().await?;
```

### Performance Testing

```bash
# Run with performance benchmarks enabled
RUST_LOG=debug cargo test -p crucible-daemon performance_validation_tests -- --nocapture

# Run semantic search performance tests
cargo test -p crucible-daemon test_semantic_search_performance -- --nocapture
```

## Expected Outcomes

### Success Criteria

#### Accuracy Targets
- **Precision**: >90% relevant results for domain-specific queries
- **Recall**: >85% coverage of relevant documents
- **Ranking**: Most relevant results appear first

#### Performance Benchmarks
- **Simple Queries**: <100ms response time
- **Complex Queries**: <500ms response time
- **Semantic Search**: <5s response time
- **Index Operations**: <10s for full rebuild

#### Consistency Requirements
- **Interface Parity**: 100% result consistency across interfaces
- **Deterministic Results**: Same query produces same results
- **Error Handling**: Graceful degradation for edge cases

### Test Coverage

#### Search Dimensions
- ✅ Parsed metadata (tags, dates, properties)
- ✅ Text content (phrases, code, lists, headings)
- ✅ Semantic similarity (embedding-based)
- ✅ Tool integration (discovery, execution)
- ✅ Link structure (backlinks, relationships)
- ✅ Interface consistency (CLI, REPL, APIs)

#### Edge Cases
- ✅ Empty queries and results
- ✅ Special characters and Unicode
- ✅ Very long queries
- ✅ Non-existent content
- ✅ Invalid parameters
- ✅ Network failures (for external providers)

#### Performance Scenarios
- ✅ Large document sets
- ✅ Concurrent access
- ✅ Memory usage validation
- ✅ Index performance
- ✅ Search accuracy under load

## Test Data and Assertions

### Validation Approach

#### Deterministic Testing
- All tests use deterministic data from the static test kiln
- Expected results are validated against known document properties
- Search scores are checked for valid ranges and ordering

#### Property-Based Testing
- Test variations across all document types and properties
- Validate search behavior with different input combinations
- Check edge cases and boundary conditions

#### Performance Testing
- Measure actual response times against benchmarks
- Validate memory usage patterns
- Test concurrent access scenarios

### Common Assertions

#### Result Validation
```rust
// Path validation
assert!(!result.path.is_empty(), "Result paths should not be empty");

// Score validation
assert!(result.score >= 0.0 && result.score <= 1.0, "Scores must be in valid range");

// Relevance validation
assert!(doc.content.to_lowercase().contains(&query.to_lowercase()) ||
       doc.title.to_lowercase().contains(&query.to_lowercase()),
       "Results should be relevant to query");
```

#### Performance Validation
```rust
// Response time validation
assert!(duration.as_millis() < 1000, "Search should complete within 1 second");

// Memory usage validation
assert!(memory_increase < 100 * 1024 * 1024, "Memory increase should be reasonable");

// Concurrency validation
assert!(concurrent_results.is_ok(), "Concurrent search should succeed");
```

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Search Validation Tests
on: [push, pull_request]

jobs:
  search-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run search validation tests
        run: |
          cargo test -p crucible-daemon search_validation_comprehensive
          cargo test -p crucible-daemon search_validation_extended
      - name: Run performance benchmarks
        run: |
          cargo test -p crucible-daemon performance_validation_tests --release
```

### Test Reports

The test runner generates comprehensive reports including:
- **Category Results**: Pass/fail counts per test category
- **Performance Metrics**: Response times and memory usage
- **Error Details**: Detailed failure information
- **Coverage Analysis**: Test coverage across search dimensions

## Contributing

### Adding New Tests

1. **Identify Test Category**: Choose appropriate module or create new one
2. **Follow Naming Convention**: Use descriptive test names
3. **Include Assertions**: Validate both positive and negative cases
4. **Add Documentation**: Document test purpose and expected outcomes
5. **Update Test Runner**: Add to appropriate test function list

### Test Implementation Pattern

```rust
#[tokio::test]
async fn test_descriptive_name() -> Result<()> {
    let harness = SearchTestHarness::new().await?;

    // Setup test data if needed
    // harness.setup_test_scenario().await?;

    // Execute test
    let results = harness.search_cli("test query", 5).await?;

    // Validate results
    assert!(!results.is_empty(), "Should find results for test query");

    for result in &results {
        // Specific validations
        assert!(result.score > 0.0, "Results should have meaningful scores");
    }

    Ok(())
}
```

### Performance Test Guidelines

- Use realistic query patterns
- Measure multiple iterations
- Include warm-up periods
- Validate against established benchmarks
- Report both average and worst-case performance

## Troubleshooting

### Common Issues

#### Test Failures
1. **Missing Test Data**: Ensure test kiln is properly set up
2. **Index Issues**: Rebuild search indexes before running tests
3. **Timeout Issues**: Increase timeout values for slow systems
4. **Environment Issues**: Check for required dependencies

#### Performance Issues
1. **Slow Tests**: Consider reducing test data size or complexity
2. **Memory Issues**: Monitor memory usage during test execution
3. **Concurrent Issues**: Reduce parallelism if system resources are limited

#### Semantic Search Issues
1. **Provider Issues**: Check embedding provider availability
2. **Score Validation**: Ensure scores are in valid [0.0, 1.0] range
3. **Consistency Issues**: Verify deterministic behavior for same inputs

### Debug Mode

Enable debug output for detailed test execution:

```bash
RUST_LOG=debug cargo test -p crucible-daemon search_validation -- --nocapture
```

### Isolated Testing

Run individual tests to isolate issues:

```bash
cargo test -p crucible-daemon test_specific_function_name -- --exact --nocapture
```

---

*This comprehensive search validation test suite ensures the reliability, accuracy, and performance of the Crucible knowledge management system's search functionality across all dimensions.*