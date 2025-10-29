# Test Coverage Gap Analysis - Existing Features

> Analysis Date: 2025-10-26
> Current Test Status: 234/236 tests passing (99.2%)
> Phase: Post-Phase 3 Test Restoration

## Executive Summary

This document identifies edge case gaps in test coverage for **existing, already-implemented features** in the Crucible codebase. It does NOT cover TDD tests for unimplemented features (delta processing, auto-start daemon).

### Current State
- **Total Test Files**: 67+ test files across workspace
- **Total Test Functions**: ~350+ tests
- **Lines of Test Code**: ~15,000+ LOC
- **Recent Fixes**: Binary detection (19/19 passing), embedding pipeline (27/27 passing)

### Analysis Scope
Four major areas analyzed:
1. **CLI Commands** - Search, Config, REPL, Rune, Semantic
2. **File Processing** - Kiln parsing, binary detection, markdown processing
3. **Error Handling** - Network errors, database errors, resource exhaustion
4. **Embedding Operations** - Generation, batching, storage, retrieval

---

## 1. CLI Commands - Edge Case Gaps

### 1.1 Search Command (`crates/crucible-cli/src/commands/search.rs`)

**Current Coverage**: Binary detection (19 tests), filesystem security (good), basic error recovery (partial)

#### Critical Gaps (HIGH PRIORITY)

1. **Empty search results** - No test verifying behavior when query returns 0 matches
2. **Query length validation** - MAX_QUERY_LENGTH=1000 exists but no tests for:
   - Queries exactly at limit
   - Queries exceeding limit
   - Unicode characters that expand byte length
3. **Case sensitivity edge cases** - Implementation converts to lowercase, but no tests for:
   - Mixed case in filenames vs content
   - Unicode case folding (Turkish i, German ß)
4. **Special characters in queries** - No tests for:
   - Regex special characters: `[`, `]`, `*`, `?`, `.`, `^`, `$`
   - SQL injection-like patterns
   - Shell metacharacters: `;`, `|`, `&`, `>`
5. **Multiple search terms** - No test for whitespace-separated multi-word queries

#### Medium Priority Gaps

6. Search result ordering for equal scores
7. Snippet extraction at file boundaries
8. File content reading failures (invalid UTF-8, mixed encodings)

#### Example Test Cases

```rust
// HIGH PRIORITY: Empty results test
#[test]
fn test_search_returns_empty_results_gracefully() {
    let kiln = create_test_kiln();
    create_file("test.md", "No matching content");

    let results = search_files_in_kiln(&kiln, "nonexistent", 10, false)?;
    assert_eq!(results.len(), 0);
}

// HIGH PRIORITY: Query length validation
#[test]
fn test_search_rejects_oversized_query() {
    let kiln = create_test_kiln();
    let huge_query = "x".repeat(1001);

    let result = validate_search_query(&huge_query, &validator);
    assert!(result.is_err());
}
```

**Estimated effort**: 10-15 test cases, ~300 LOC

---

### 1.2 Config Command (`crates/crucible-cli/src/commands/config.rs`)

**Current Coverage**: Configuration defaults (15 tests - excellent), file loading (good)

#### Critical Gaps (HIGH PRIORITY)

1. **CLI command integration tests** - ZERO tests for:
   - `config init` with existing file (with/without --force)
   - `config init` with invalid path (no write permissions)
   - `config show` JSON output is valid and parseable
   - `config migrate-env-vars` (dry-run, no vars, conflicts)
2. **Force flag behavior** - Not tested
3. **Path validation edge cases**:
   - Paths with trailing slashes
   - Relative vs absolute path handling
   - Paths with `.` and `..` components

#### Example Test Cases

```rust
#[tokio::test]
async fn test_config_init_respects_force_flag() {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("config.toml");

    // Create initial config
    execute(ConfigCommands::Init { path: Some(config_path.clone()), force: false }).await?;

    // Try without force - should fail
    let result = execute(ConfigCommands::Init { path: Some(config_path.clone()), force: false }).await;
    assert!(result.is_err());

    // Try with force - should succeed
    execute(ConfigCommands::Init { path: Some(config_path.clone()), force: true }).await?;
}
```

**Estimated effort**: 12-15 test cases, ~400 LOC

---

### 1.3 REPL Command (`crates/crucible-cli/src/commands/repl/`)

**Current Coverage**: Tool execution (good), unified registry (good), command parsing (partial)

#### Critical Gaps (HIGH PRIORITY)

1. **Command parsing edge cases** - No tests for:
   - Commands with excessive whitespace: `:   run    tool`
   - Quoted arguments with spaces: `:run tool "arg with spaces"`
   - Escaped quotes in arguments
   - Unicode in command names or args
2. **Invalid tool names** - No tests for special characters, very long names
3. **History edge cases** - No tests for limit=0, non-numeric limit, very large history

#### Medium Priority Gaps

4. Invalid log levels, format switching, command aliases

#### Example Test Cases

```rust
#[test]
fn test_command_parse_whitespace_normalization() {
    let cmd = Command::parse(":   run    tool    arg1    arg2")?;
    assert_eq!(cmd, Command::RunTool {
        tool_name: "tool".to_string(),
        args: vec!["arg1".to_string(), "arg2".to_string()],
    });
}

#[test]
fn test_command_parse_quoted_arguments() {
    let cmd = Command::parse(":run tool \"arg with spaces\"")?;
    assert_eq!(cmd.args[0], "arg with spaces");
}
```

**Estimated effort**: 15-20 test cases, ~350 LOC

---

### 1.4 Rune Command (`crates/crucible-cli/src/commands/rune.rs`)

**Current Coverage**: 22 tests recently restored (excellent)

#### Critical Gaps (HIGH PRIORITY)

1. **Script path resolution precedence** - Multiple locations exist, but which takes precedence?
   - Script in `~/.config/crucible/commands/` vs `.crucible/commands/` vs local
2. **Script syntax errors** - Incomplete syntax, unclosed braces
3. **Script permissions** - Scripts without read permission

#### Example Test Cases

```rust
#[tokio::test]
async fn test_script_path_resolution_precedence() {
    let context = create_test_context();

    // Create same-named script in multiple locations
    create_script("~/.config/crucible/commands/tool.rn", "pub fn main() { 1 }");
    create_script(".crucible/commands/tool.rn", "pub fn main() { 2 }");

    let result = execute(config, "tool".to_string(), None).await?;
    // Which one wins? Should be documented and tested
}
```

**Estimated effort**: 8-10 test cases, ~250 LOC

---

### 1.5 Semantic Search (`crates/crucible-cli/src/commands/semantic.rs`)

**Current Coverage**: Real integration (excellent), JSON output (good), daemonless mode (good)

#### Critical Gaps (HIGH PRIORITY)

1. **Empty query string** - No validation
2. **top_k parameter edge cases** - No tests for:
   - top_k=0, negative values, extremely large values
3. **Empty embeddings database** - Corrupted table scenarios

#### Example Test Cases

```rust
#[tokio::test]
async fn test_semantic_search_empty_query() {
    let config = create_test_config();
    let result = execute(config, "".to_string(), 10, "text".to_string(), false).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_semantic_search_top_k_zero() {
    let config = create_test_config();
    setup_embeddings(&db).await;

    let result = execute(config, "test query".to_string(), 0, "json".to_string(), false).await?;
    let json: Value = serde_json::from_str(&result)?;
    assert_eq!(json["total_results"], 0);
}
```

**Estimated effort**: 8-10 test cases, ~200 LOC

---

## 2. File Processing - Edge Case Gaps

### 2.1 Kiln/Kiln File Processing (`crates/crucible-tools/src/kiln_*.rs`)

**Current Test File**: `/home/moot/crucible/crates/crucible-tools/tests/kiln_file_parsing_tests.rs` (724 lines)

#### Critical Gaps (HIGH PRIORITY)

1. **Different File Encodings** (CRITICAL)
   - Current: Only UTF-8 tested
   - Missing: UTF-16 (common on Windows), UTF-16 LE/BE, Latin-1, Windows-1252
   - Impact: Files from Windows users may fail silently
   - Implementation: `kiln_parser.rs:42` uses `fs::read_to_string()` (UTF-8 only)

2. **Large Files Edge Cases**
   - Missing: Files near 10MB limit, files >100MB, very long lines (>10,000 chars)

3. **Corrupted/Malformed Frontmatter**
   - Missing: Invalid YAML syntax, circular references, very deep nesting, multiple frontmatter blocks

4. **Special Characters in Paths**
   - Missing: Unicode filenames (emoji), spaces, special chars (`#`, `%`, `&`), very long filenames (>255)

#### Medium Priority Gaps

5. **Symlink Handling** - Circular symlinks, symlinks outside kiln
6. **Empty/Minimal Files** - Zero-byte files, only frontmatter
7. **Mixed Line Endings** - CRLF/LF/CR mixed

#### Example Test Cases

```rust
#[tokio::test]
async fn test_parse_utf16_encoded_file() -> Result<()> {
    let parser = KilnParser::new();

    // Create temp file with UTF-16 content
    let content_utf16: Vec<u16> = "---\ntitle: UTF-16 Test\n---\n# Content".encode_utf16().collect();
    let bytes: Vec<u8> = content_utf16.iter().flat_map(|&c| c.to_le_bytes()).collect();

    // Write to temp file and test
    // Expected: Should detect encoding and handle appropriately
}

#[tokio::test]
async fn test_parse_malformed_yaml_frontmatter() -> Result<()> {
    let parser = KilnParser::new();

    let content = r#"---
title: "Unclosed quote
tags: [test
created: 2025-13-45
---
# Content"#;

    let result = parser.parse_content("test.md".into(), content.into()).await;
    assert!(matches!(result.unwrap_err(), KilnError::FrontmatterParseError(_)));
}
```

**Estimated effort**: 15-20 test cases, ~500 LOC

---

### 2.2 Binary Detection (`crates/crucible-cli/src/commands/secure_filesystem.rs`)

**Current Coverage**: 19/19 tests passing (recently fixed - EXCELLENT)

#### Medium Priority Gaps

1. **Additional Binary Formats** - RAR, encrypted files, database files, font files
2. **Very Small Files** - Binary files <4 bytes (can't have full signature)
3. **Polyglot Files** - Files with multiple valid signatures

#### Example Test Cases

```rust
#[test]
fn test_detect_rar_archive() -> Result<()> {
    let harness = BinarySafetyTestHarness::new()?;
    let rar_header = vec![0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];
    let file_path = harness.create_binary_file("archive.md", &rar_header)?;

    let result = get_file_content(&file_path);
    assert!(result.is_err());
    Ok(())
}
```

**Estimated effort**: 5-8 test cases, ~150 LOC

---

### 2.3 Markdown Parsing (`crates/crucible-core/src/parser/`)

**Current Test Files**: `integration_parser_pipeline.rs` (463 lines), inline tests (60 lines)

#### Critical Gaps (HIGH PRIORITY)

1. **Malformed Markdown**
   - Unclosed code blocks (``` without closing)
   - Invalid wikilinks: `[[broken`, `]]broken`
   - Very deep nesting (100+ levels of quotes)

2. **Obsidian-Specific Syntax** (HIGH IMPACT)
   - Callouts: `> [!note] Title`
   - Block references: `[[file#^blockid]]`
   - Embeds with sections: `![[file#heading]]`
   - Math blocks: `$$LaTeX$$`
   - Mermaid diagrams

3. **HTML in Markdown**
   - Raw HTML blocks
   - Script tags (security concern)
   - Malformed HTML

4. **Tables with Edge Cases**
   - Varying column counts per row
   - Empty cells, cells with wikilinks

#### Example Test Cases

```rust
#[test]
fn test_parse_unclosed_code_block() {
    let parser = PulldownParser::new();
    let content = "# Title\n\n```rust\nfn main() {\n// No closing";

    let result = parser.parse_content(content, &path);
    assert!(result.is_ok()); // Should not panic
}

#[test]
fn test_parse_obsidian_callout() {
    let parser = PulldownParser::new();
    let content = r#"> [!note] Important Note
> This is callout content"#;

    let doc = parser.parse_content(content, &path).unwrap();
    // Should extract callout or parse as blockquote
}
```

**Estimated effort**: 20-25 test cases, ~600 LOC

---

### 2.4 Frontmatter Processing

#### Critical Gaps (HIGH PRIORITY)

1. **Invalid YAML Structures**
   - Syntax errors, type mismatches, circular references, very deep nesting

2. **Special YAML Values**
   - `null` vs `~` vs empty
   - Boolean variations: `yes`/`no`, `on`/`off`, `True`/`False`
   - Large numbers, scientific notation
   - Multi-line strings: `|`, `>`

3. **Frontmatter Edge Cases**
   - Whitespace-only frontmatter
   - Multiple blocks in one file
   - Windows line endings

#### Example Test Cases

```rust
#[tokio::test]
async fn test_frontmatter_boolean_variations() {
    let variations = vec![
        ("published: yes", true),
        ("published: no", false),
        ("published: True", true),
        ("published: FALSE", false),
    ];

    for (yaml, expected) in variations {
        let content = format!("---\n{}\n---\n# Content", yaml);
        let result = parser.parse_content("test.md".into(), content).await.unwrap();
        assert_eq!(result.metadata.frontmatter.get("published"), Some(expected));
    }
}
```

**Estimated effort**: 12-15 test cases, ~350 LOC

---

### 2.5 File System Operations (`crates/crucible-watch/`)

**Current Coverage**: **ZERO TEST FILES FOUND** - CRITICAL GAP

#### Critical Gaps (URGENT)

1. **File Watching Edge Cases** - ALL UNTESTED
   - Rapid file changes (>100 events/sec)
   - File rename chains (A → B → C)
   - Directory operations
   - Permission changes mid-watch

2. **Debouncing and Event Filtering** - No tests for `src/utils/debouncer.rs`

3. **Case-Insensitive Filesystems** - macOS/Windows behavior

**Recommendation**: Create comprehensive integration tests BEFORE addressing other gaps.

**Estimated effort**: 25-30 test cases, ~800 LOC (NEW TEST FILE NEEDED)

---

## 3. Error Handling - Edge Case Gaps

### 3.1 Error Recovery Tests

**Current File**: `/home/moot/crucible/crates/crucible-cli/tests/error_recovery_tdd.rs` (27 tests, 864 LOC)

#### Critical Gaps (HIGH PRIORITY)

1. **Network Timeout Edge Cases**
   - Timeout during response streaming (partial data)
   - Slow responses (>30s but <timeout)
   - Multiple consecutive timeouts

2. **Concurrent Configuration Updates**
   - Two processes writing config simultaneously
   - Config reload during active operations

3. **Partial Recovery Scenarios**
   - Circuit breaker reopens during recovery
   - Service partially recovers (degraded state)

#### Example Test Cases

```rust
#[tokio::test]
async fn test_timeout_during_streaming_response() {
    // Mock server that starts sending then stops mid-stream
    // Verify timeout detection and cleanup
}
```

**Estimated effort**: 10-12 test cases, ~400 LOC

---

### 3.2 Network/External Service Errors

**Current Coverage**: Error types defined, basic HTTP errors tested

#### Critical Gaps (HIGH PRIORITY - NEW TEST FILE NEEDED)

1. **Connection-Level Errors** (ZERO tests)
   - DNS resolution failures
   - Connection refused vs connection timeout
   - Network unreachable
   - SSL/TLS certificate errors

2. **Partial Response Handling** (ZERO tests)
   - Connection drops mid-JSON response
   - Truncated response body
   - Invalid content-type header

3. **Rate Limiting Variations**
   - 429 without retry-after header
   - Retry-after with date format

4. **Slow but Non-Timeout Responses**
   - Response taking 29s (timeout=30s)

#### Example Test Cases

```rust
#[tokio::test]
async fn test_dns_resolution_failure() {
    let config = EmbeddingConfig::ollama(
        Some("http://nonexistent-domain-xyz123.local".to_string()),
        Some("model".to_string()),
    );
    let provider = OllamaProvider::new(config).unwrap();

    let result = provider.embed("test").await;
    assert!(result.is_err());
    assert!(err.is_retryable());
}

#[tokio::test]
async fn test_truncated_json_response() {
    // Mock server returns incomplete JSON
    // Should return InvalidResponse, not panic
}
```

**Estimated effort**: 18-20 test cases, ~600 LOC (NEW FILE: `embedding_network_errors.rs`)

---

### 3.3 Database Error Handling

**Current Coverage**: In-memory storage (no real DB errors), basic checks

#### Critical Gaps (HIGH PRIORITY)

1. **Database Corruption Scenarios** (ZERO tests)
   - Corrupted database file on disk
   - Partial write during disk full
   - Recovery from corrupted state

2. **Lock Contention** (ZERO tests)
   - Simultaneous writes to same document
   - Long-running query blocking writes
   - Current code uses `.lock().unwrap()` - will panic on poison!

3. **Transaction Conflicts** (ZERO tests)
   - Optimistic locking failures
   - Partial batch operation failures

4. **Query Timeouts** (ZERO tests)

#### Example Test Cases

```rust
#[tokio::test]
async fn test_concurrent_updates_same_document() {
    let client = SurrealClient::new_memory().await.unwrap();

    // Spawn 10 concurrent updates to same document
    let handles: Vec<_> = (0..10).map(|i| {
        let client = client.clone();
        tokio::spawn(async move {
            update_metadata(&client, "doc1", HashMap::new()).await
        })
    }).collect();

    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
}
```

**Estimated effort**: 15-18 test cases, ~500 LOC (NEW FILE: `database_concurrency_tests.rs`)

---

### 3.4 File System Error Scenarios

**Current Coverage**: Deep nesting (good), symlinks (good), permissions (good)

#### Critical Gaps (HIGH PRIORITY)

1. **Disk Full During Write** (ZERO tests)
   - Write starts, disk fills mid-write
   - Atomic write guarantees
   - Cleanup of partial writes

2. **File Locked by Another Process** (ZERO tests)

3. **File Deleted Between Check and Open** (TOCTOU) (ZERO tests)

4. **I/O Errors (Hardware)** (ZERO tests)

#### Example Test Cases

```rust
#[test]
fn test_disk_full_during_config_write() {
    // Simulate disk full using quota limits or mock filesystem
    // Verify config file is not corrupted
    // Verify old config is still valid
}
```

**Estimated effort**: 10-12 test cases, ~350 LOC

---

### 3.5 Concurrent Operation Errors

**Current Coverage**: Some concurrent tests exist but not focused on races

#### Critical Gaps (HIGH PRIORITY)

1. **Race Conditions** (MINIMAL testing)
   - Two threads updating same config simultaneously
   - Read-modify-write races in database

2. **Deadlock Detection** (ZERO tests)
   - Lock order violations (A→B vs B→A)
   - Current code uses `.lock().unwrap()` - will poison on panic

3. **Channel Buffer Overflow** (ZERO tests)
   - mpsc channel full, sender blocks
   - Backpressure handling

4. **Thread Pool Exhaustion** (ZERO tests)

#### Example Test Cases

```rust
#[tokio::test]
async fn test_no_deadlock_circuit_breaker_and_health() {
    let manager = ErrorRecoveryManager::new(&config);

    // Thread 1: Update health while checking circuit breaker
    // Thread 2: Update circuit breaker while checking health

    tokio::time::timeout(Duration::from_secs(5), async {
        handle1.await.unwrap();
        handle2.await.unwrap();
    }).await.expect("Deadlock detected");
}
```

**Estimated effort**: 12-15 test cases, ~450 LOC (NEW FILE: `concurrency_edge_cases.rs`)

---

### 3.6 Resource Exhaustion

**Current Coverage**: Resource monitoring utilities exist, but no exhaustion tests

#### Critical Gaps (HIGH PRIORITY)

1. **Out of Memory** (ZERO tests)
   - Large batch embedding exceeding memory
   - Embedding cache growing without bounds

2. **Connection Pool Exhaustion** (ZERO tests)

3. **File Descriptor Limits** (ZERO tests)

4. **Disk Space Exhaustion** (ZERO tests)

#### Example Test Cases

```rust
#[tokio::test]
#[should_panic] // Or should handle gracefully?
async fn test_embedding_batch_oom() {
    let provider = MockEmbeddingProvider::with_dimensions(1536);

    // Try to embed 1 million documents at once
    let texts: Vec<String> = (0..1_000_000).map(|i| "x".repeat(1000)).collect();

    let result = provider.embed_batch(texts).await;
    assert!(result.is_err());
}
```

**Estimated effort**: 10-12 test cases, ~300 LOC (NEW FILE: `resource_exhaustion_tests.rs`)

---

### 3.7 Graceful Degradation

**Current Coverage**: Fallback to defaults (good), partial config merging (good)

#### Critical Gaps (HIGH PRIORITY)

1. **Partial Index Corruption** (ZERO tests)
   - Some embeddings corrupted, others valid
   - Can we search unaffected documents?

2. **One Feature Broken, Others Work** (MINIMAL)
   - Embedding service down, can we still browse files?
   - Database down, can we still parse files?

#### Example Test Cases

```rust
#[tokio::test]
async fn test_degraded_mode_embedding_service_down() {
    let manager = ErrorRecoveryManager::new(&config);

    // Simulate embedding service failure
    manager.health_monitor()
        .update_health("embedding_service", ServiceHealth::Unhealthy)
        .await;

    // Semantic search should fail or fallback
    // But file operations should still work
    let files = list_files(&config).await;
    assert!(files.is_ok());
}
```

**Estimated effort**: 8-10 test cases, ~250 LOC

---

## 4. Embedding Operations - Edge Case Gaps

### Summary Statistics

**Current Coverage**:
- Total Test Files: 12 embedding-related test files
- Total Test Functions: 67 async tests
- Lines of Test Code: ~3,807 LOC

### 4.1 Embedding Generation

**Current Coverage**: Empty content, minimal content, Unicode, large (~10KB), special chars - GOOD

#### Critical Gaps (HIGH PRIORITY)

1. **Extremely Long Text (>32k tokens)**
   - Current: Tests ~10KB (~2.5k tokens)
   - Missing: 100k+ characters

2. **Text with Only Whitespace/Newlines**
   - Missing: `"   \n\n\t\t   "`

3. **Malformed UTF-8 Sequences**
   - Could crash embedding provider

4. **Very Short Inputs (1 word, 1 character)**

5. **Text with Null Bytes/Control Characters**
   - Could break JSON serialization

#### Example Test Cases

```rust
#[tokio::test]
async fn test_embedding_extremely_long_text() {
    let provider = MockEmbeddingProvider::with_dimensions(768);
    let very_long_text = "Lorem ipsum ".repeat(4000); // >100k chars

    let result = provider.embed(&very_long_text).await;
    // Should truncate or error, not panic
}

#[tokio::test]
async fn test_embedding_whitespace_only() {
    let whitespace_texts = vec!["   ", "\n\n\n", "\t\t\t"];
    for text in whitespace_texts {
        let result = provider.embed(text).await;
        assert!(result.is_ok() || result.is_err()); // No panic
    }
}
```

**Estimated effort**: 10-12 test cases, ~300 LOC

---

### 4.2 Batch Processing

**Current Coverage**: Basic batch (5-10 items), large batch (50-100), varied content - GOOD

#### Critical Gaps (HIGH PRIORITY)

1. **Batch size = 0** - Not tested: `embed_batch(vec![])`
2. **Batch size = 1** - Not explicitly tested
3. **Partial batch failures** - Items 1-3 succeed, item 4 fails
4. **Batch result ordering** - Do results match input order?
5. **Very large batches (1000+ items)** - Tests up to 100

#### Example Test Cases

```rust
#[tokio::test]
async fn test_batch_empty() {
    let provider = MockEmbeddingProvider::with_dimensions(768);
    let empty_batch: Vec<String> = vec![];

    let result = provider.embed_batch(empty_batch).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[tokio::test]
async fn test_batch_result_ordering() {
    let texts = vec!["First".to_string(), "Second".to_string(), "Third".to_string()];
    let results = provider.embed_batch(texts).await.unwrap();

    // Results should be in same order as input
    assert_eq!(results.len(), 3);
}
```

**Estimated effort**: 8-10 test cases, ~250 LOC

---

### 4.3 Embedding Storage

**Current Coverage**: Single doc, chunked, mixed, concurrent, large (4096 dims) - EXCELLENT

#### Critical Gaps (HIGH PRIORITY)

1. **Duplicate embeddings** - Store same doc_id twice without clearing
2. **Vector dimension mismatches** - Store 384-dim, then 768-dim for same doc
3. **Orphaned embeddings** - Delete doc, check embedding cleanup
4. **Concurrent writes to same document** - Tests different docs, not same
5. **Million+ embeddings** - Performance at scale

#### Example Test Cases

```rust
#[tokio::test]
async fn test_duplicate_embedding_storage() {
    let client = SurrealClient::new_memory().await.unwrap();
    let doc_id = "duplicate-test";

    // Store first embedding
    store_document_embedding(&client, &embedding1).await.unwrap();

    // Store duplicate WITHOUT clearing
    store_document_embedding(&client, &embedding2).await.unwrap();

    // Should either replace or reject, not duplicate
    let retrieved = get_document_embeddings(&client, doc_id).await.unwrap();
    assert!(retrieved.len() <= 1);
}
```

**Estimated effort**: 10-12 test cases, ~350 LOC

---

### 4.4 Provider-Specific Edge Cases

**Current Coverage**: Basic generation, batch, error handling, health check - GOOD

#### Critical Gaps - Ollama (HIGH PRIORITY)

1. **Model not loaded/pulled** - Ollama auto-pulls (slow) or errors
2. **Model loading timeout** - Very large model first load
3. **Ollama service restart during operation**
4. **Model name typos** - `"nomic-embed-txet"`
5. **Very slow responses (>60s)**

#### Critical Gaps - OpenAI (HIGH PRIORITY)

1. **Invalid API key** - Wrong format, revoked key
2. **API key rotation mid-request**
3. **Model deprecation** - Use deprecated model
4. **Rate limiting** - Exceed RPM/TPM limits
5. **Different dimension parameters** - Custom dims

#### Example Test Cases

```rust
#[tokio::test]
async fn test_ollama_model_not_found() {
    std::env::set_var("EMBEDDING_MODEL", "nonexistent-model-xyz");
    let result = pool.process_document_with_retry("test", "content").await.unwrap();

    assert!(!result.succeeded);
    assert!(result.final_error.unwrap().error_message.contains("model"));
}

#[tokio::test]
async fn test_openai_invalid_api_key() {
    std::env::set_var("OPENAI_API_KEY", "invalid-key-123");
    let result = provider.embed("test").await;

    assert!(result.is_err());
    assert!(error.to_string().contains("401") || error.to_string().contains("Unauthorized"));
}
```

**Estimated effort**: 15-18 test cases, ~500 LOC (SPLIT: `ollama_edge_cases.rs` + `openai_edge_cases.rs`)

---

### 4.5 Re-embedding Scenarios

**Current Coverage**: MINIMAL - No specific test file (was archived)

#### Critical Gaps (HIGH PRIORITY)

1. **When to trigger re-embedding** - Content changed, model changed, schema changed
2. **Partial re-embedding** - Only changed docs, incremental updates
3. **Re-embedding failures** - Some docs fail, track retry state
4. **Re-embedding large corpus** - 10k+ docs, progress tracking

#### Example Test Cases

```rust
#[tokio::test]
async fn test_detect_content_change_for_reembedding() {
    // Store initial embedding
    store_document_embedding(&client, &embedding_v1).await.unwrap();

    // Simulate content change
    let content_v2 = "Updated content";
    let needs_reembed = should_reembed(&client, doc_id, content_v2).await.unwrap();

    assert!(needs_reembed);
}
```

**Estimated effort**: 8-10 test cases, ~300 LOC (NEW FILE: `re_embedding_tests.rs`)

---

### 4.6 Content Type Variations

**Current Coverage**: Code blocks, mixed content, Unicode - GOOD

#### Critical Gaps (HIGH PRIORITY)

1. **Frontmatter handling** - Should it be embedded?
2. **Wikilinks in content** - `[[Link]]` formatting
3. **Tables** - Markdown table structure
4. **Images (alt text)** - `![alt](image.png)`
5. **Block-level vs document-level** - Granularity trade-offs

**Estimated effort**: 10-12 test cases, ~300 LOC

---

### 4.7 Embedding Retrieval/Search

**Current Coverage**: Basic similarity, top-k, empty DB, cosine similarity - GOOD

#### Critical Gaps (HIGH PRIORITY)

1. **Query embedding fails but search requested** - Provider down
2. **No matches above threshold** - All similarities < 0.3
3. **Identical embeddings (similarity = 1.0)** - Multiple docs same embedding
4. **Zero vectors** - Division by zero in cosine
5. **Negative similarity scores** - Opposite vectors

#### Example Test Cases

```rust
#[tokio::test]
async fn test_search_with_zero_vector() {
    let zero_vector = vec![0.0f32; 768];
    let results = semantic_search(&client, "test", 5).await;

    // Should handle gracefully
    match results {
        Ok(res) => println!("Handled zero vector"),
        Err(e) => println!("Zero vector search errored: {}", e),
    }
}
```

**Estimated effort**: 10-12 test cases, ~300 LOC

---

## Prioritized Implementation Plan

### Phase 1: Critical Gaps (Implement First)

**Total Estimated Effort**: ~5,500 LOC, ~180 test cases

1. **File System Watch Testing** (ZERO coverage) - 800 LOC, 30 tests
   - NEW FILE: `crates/crucible-watch/tests/file_watch_integration.rs`

2. **Connection-Level Network Errors** (ZERO coverage) - 600 LOC, 20 tests
   - NEW FILE: `crates/crucible-llm/tests/embedding_network_errors.rs`

3. **Database Lock Contention** (ZERO coverage) - 500 LOC, 18 tests
   - NEW FILE: `crates/crucible-surrealdb/tests/database_concurrency_tests.rs`

4. **File Encoding Support** (UTF-16, etc.) - 300 LOC, 10 tests
   - ENHANCE: `crates/crucible-tools/tests/kiln_file_parsing_tests.rs`

5. **Disk Full During Write** - 350 LOC, 12 tests
   - ENHANCE: `crates/crucible-cli/tests/filesystem_edge_case_tdd.rs`

6. **CLI Command Integration** (config init/show/migrate) - 400 LOC, 15 tests
   - ENHANCE: `crates/crucible-cli/tests/configuration_tests.rs`

7. **Search Command Edge Cases** - 300 LOC, 12 tests
   - NEW FILE: `crates/crucible-cli/tests/search_edge_cases.rs`

8. **Concurrent Operation Errors** - 450 LOC, 15 tests
   - NEW FILE: `crates/crucible-cli/tests/concurrency_edge_cases.rs`

9. **Embedding Generation Extremes** - 300 LOC, 12 tests
   - ENHANCE: `crates/crucible-daemon/tests/embedding_pipeline.rs`

10. **Provider-Specific Edge Cases** - 500 LOC, 18 tests
    - NEW FILE: `crates/crucible-llm/tests/ollama_edge_cases.rs`
    - NEW FILE: `crates/crucible-llm/tests/openai_edge_cases.rs`

11. **Embedding Storage Edge Cases** - 350 LOC, 12 tests
    - ENHANCE: `crates/crucible-surrealdb/tests/embedding_storage_tests.rs`

12. **Re-embedding Scenarios** - 300 LOC, 10 tests
    - NEW FILE: `crates/crucible-daemon/tests/re_embedding_tests.rs`

13. **Resource Exhaustion** - 300 LOC, 12 tests
    - NEW FILE: `crates/crucible-watch/tests/resource_exhaustion_tests.rs`

14. **Obsidian-Specific Syntax** - 350 LOC, 14 tests
    - ENHANCE: `crates/crucible-core/tests/integration_parser_pipeline.rs`

---

### Phase 2: High Priority Gaps

**Total Estimated Effort**: ~2,800 LOC, ~95 test cases

1. **REPL Command Parsing** - 350 LOC, 15 tests
2. **Partial Response Handling** - 300 LOC, 10 tests
3. **Malformed Frontmatter** - 300 LOC, 12 tests
4. **Batch Processing Edge Cases** - 250 LOC, 10 tests
5. **Graceful Degradation** - 250 LOC, 10 tests
6. **Rune Script Edge Cases** - 250 LOC, 10 tests
7. **Semantic Search Parameters** - 200 LOC, 8 tests
8. **Embedding Retrieval Edge Cases** - 300 LOC, 12 tests
9. **Content Type Variations** - 300 LOC, 12 tests
10. **Markdown Malformed Input** - 300 LOC, 12 tests

---

### Phase 3: Medium Priority Gaps

**Total Estimated Effort**: ~1,500 LOC, ~50 test cases

1. **Binary Detection Extensions** - 150 LOC, 8 tests
2. **Frontmatter YAML Edge Cases** - 350 LOC, 15 tests
3. **Mixed Line Endings** - 150 LOC, 6 tests
4. **Empty/Minimal Files** - 150 LOC, 6 tests
5. **REPL Command Aliases** - 150 LOC, 6 tests
6. **Rate Limiting Variations** - 200 LOC, 8 tests
7. **Miscellaneous** - 350 LOC, ~1 test each

---

## Summary Statistics

### Current State
- **Passing Tests**: 234/236 (99.2%)
- **Total Test LOC**: ~15,000+
- **Total Test Files**: 67+

### Identified Gaps
- **Critical Gaps**: ~180 test cases, ~5,500 LOC
- **High Priority Gaps**: ~95 test cases, ~2,800 LOC
- **Medium Priority Gaps**: ~50 test cases, ~1,500 LOC
- **Total New Tests Needed**: ~325 test cases, ~9,800 LOC

### Projected Final State
- **Total Tests**: ~675 tests (234 + 325 + existing)
- **Total Test LOC**: ~25,000+ LOC
- **Test Coverage**: Comprehensive edge case coverage for all existing features

---

## Notes

1. This analysis focuses ONLY on existing, implemented features
2. Does NOT include TDD tests for unimplemented features (delta processing, auto-start daemon)
3. Estimates are conservative - actual implementation may vary
4. Priority is based on:
   - Impact (data corruption, crashes, security)
   - Likelihood (common user scenarios)
   - Current test coverage gaps

---

## Next Steps

1. Review and prioritize this plan
2. Create tracking issues for each phase
3. Implement Phase 1 (Critical) first
4. Run full test suite after each phase
5. Update this document as gaps are filled
