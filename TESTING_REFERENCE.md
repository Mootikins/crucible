# Testing Reference Guide

A quick lookup guide for common testing scenarios in the Crucible workspace.

---

## Running Tests

### Basic Commands
```bash
# Run all tests in crucible-parser
cargo test -p crucible-parser

# Run specific test
cargo test -p crucible-parser test_parse_frontmatter

# Run with output
cargo test -p crucible-parser -- --nocapture --test-threads=1

# Run with backtrace on failure
RUST_BACKTRACE=1 cargo test -p crucible-parser

# Run only unit tests (no integration tests)
cargo test --lib -p crucible-parser
```

### Test Output Interpretation
```
running 10 tests

test tests::test_parse_valid_yaml ... ok
test tests::test_validate_invalid_yaml_unclosed_quote ... ok
test tests::test_yaml_multiline_strings ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out
```

---

## Test Template Structure

### Minimal Test
```rust
#[test]
fn test_feature_works() {
    let result = do_something();
    assert!(result.is_ok());
}
```

### Test with Helper Data
```rust
#[test]
fn test_with_helper() {
    let data = sample_data();
    let result = parser.process(data);
    assert_eq!(result, expected);
}

fn sample_data() -> &'static str {
    "test content"
}
```

### Async Test
```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Test with Error Handling
```rust
#[test]
fn test_error_case() {
    let result = invalid_operation();
    
    assert!(result.is_err(), "Should return error for invalid input");
    
    match result {
        Err(ParseError::FrontmatterError(msg)) => {
            assert!(msg.contains("expected"), "Error message should mention expectation");
        }
        _ => panic!("Wrong error type"),
    }
}
```

### Test with Multiple Assertions
```rust
#[test]
fn test_complex_scenario() {
    let parser = Parser::new();
    
    // Setup
    let doc = "---\ntitle: Test\n---\nContent";
    
    // Execute
    let (fm, body, format) = parser.extract(doc).unwrap();
    
    // Assert - multiple conditions
    assert!(fm.is_some(), "Should extract frontmatter");
    assert_eq!(format, FrontmatterFormat::Yaml, "Should detect YAML");
    assert!(!body.is_empty(), "Body should not be empty");
    assert!(body.contains("Content"), "Body should contain content");
}
```

---

## Common Assertion Patterns

### Existence and Type Checks
```rust
assert!(value.is_some(), "Option should contain value");
assert!(value.is_err(), "Result should be error");
assert!(vec.is_empty(), "Vector should be empty");
```

### Equality and Value Checks
```rust
assert_eq!(actual, expected, "Values should match");
assert_ne!(actual, other, "Values should differ");
assert!(actual > threshold, "Value {} should be greater than {}", actual, threshold);
```

### String Checks
```rust
assert!(content.contains("substring"), "Should contain substring");
assert!(content.starts_with("prefix"), "Should start with prefix");
assert_eq!(content.len(), expected_len, "String length mismatch");
```

### Collection Checks
```rust
assert_eq!(vec.len(), 3, "Should have 3 items");
assert!(map.contains_key("key"), "Should contain key");
let values: Vec<_> = vec.iter().filter(|x| x > 5).collect();
assert_eq!(values.len(), 2, "Should have 2 values > 5");
```

---

## Error Handling in Tests

### Pattern 1: Expect
```rust
#[test]
fn test_with_expect() {
    let result = parser.parse(content)
        .expect("Should parse successfully");
    
    assert_eq!(result.title, "Test");
}
```

### Pattern 2: Unwrap with Message
```rust
#[test]
fn test_with_unwrap_message() {
    let result = parser.parse(content);
    let parsed = result.unwrap_or_else(|e| {
        panic!("Failed to parse: {:?}", e)
    });
    
    assert_eq!(parsed.title, "Test");
}
```

### Pattern 3: is_err Assertion
```rust
#[test]
fn test_error_handling() {
    let result = parser.parse(invalid);
    
    assert!(result.is_err(), "Should fail on invalid input");
    
    if let Err(e) = result {
        match e {
            ParserError::FrontmatterError(_) => {}, // Expected
            _ => panic!("Wrong error type"),
        }
    }
}
```

---

## Test Data Patterns

### Raw String Literals (Preferred)
```rust
#[test]
fn test_with_raw_string() {
    let content = r#"---
title: "My Document"
tags: [rust, testing]
---

# Content"#;

    // test implementation
}
```

### Escaped Strings
```rust
#[test]
fn test_with_escaped_string() {
    let yaml = "title: \"Quoted \\\"Title\\\"\"\nauthor: John";
    // test
}
```

### Multi-line Formatted
```rust
#[test]
fn test_multi_line() {
    let content = "\
---
title: Test
---
Content";

    // test
}
```

### Helper Function
```rust
fn frontmatter_yaml() -> &'static str {
    r#"title: Test Document
author: Jane Doe
tags: [rust, testing]"#
}

#[test]
fn test_using_helper() {
    let yaml = frontmatter_yaml();
    // test
}
```

---

## Mock Usage Examples

### Using MockStorage
```rust
#[tokio::test]
async fn test_with_mock_storage() {
    let storage = MockStorage::new();
    
    // Perform operations
    storage.store_block("key", b"data").await.unwrap();
    
    // Verify operations were called
    let stats = storage.stats();
    assert_eq!(stats.store_count, 1);
    
    // Verify data
    let data = storage.get_block("key").await.unwrap();
    assert_eq!(data, Some(b"data".to_vec()));
}
```

### Using MockContentHasher
```rust
#[tokio::test]
async fn test_with_mock_hasher() {
    let hasher = MockContentHasher::new();
    
    // Configure expected hash
    hasher.set_file_hash("test.md", vec![1u8; 32]);
    
    // Use hasher
    let hash = hasher.hash_file(Path::new("test.md"))
        .await
        .unwrap();
    
    // Verify
    assert_eq!(hash.as_bytes(), &[1u8; 32]);
    
    // Check operation counts
    let (count, _) = hasher.operation_counts();
    assert_eq!(count, 1);
}
```

### Error Injection
```rust
#[tokio::test]
async fn test_error_handling_with_mock() {
    let storage = MockStorage::new();
    
    // Enable error simulation
    storage.set_simulate_errors(true, "Storage full");
    
    // Operations should fail
    let result = storage.store_block("key", b"data").await;
    assert!(result.is_err());
    
    // Disable errors
    storage.set_simulate_errors(false, "");
    let result = storage.store_block("key", b"data").await;
    assert!(result.is_ok());
}
```

---

## Common Test Issues and Solutions

### Issue: Test Fails Locally but Passes in CI
**Solution:** Ensure no hardcoded paths or system-specific assumptions
```rust
// Bad: Hardcoded path
let path = "/home/user/test/file.txt";

// Good: Use TempDir
let temp = TempDir::new().unwrap();
let path = temp.path().join("file.txt");
```

### Issue: Tests Interfere with Each Other
**Solution:** Use TempDir or reset mocks
```rust
#[test]
fn test_isolation() {
    let storage = MockStorage::new();  // Fresh instance
    // Each test gets its own instance
}

// Or reset between operations
#[test]
fn test_with_reset() {
    let storage = MockStorage::new();
    
    storage.store_block("key", b"data").await.unwrap();
    assert_eq!(storage.stats().store_count, 1);
    
    storage.reset();  // Clear state
    assert_eq!(storage.stats().store_count, 0);
}
```

### Issue: Async Test Hangs
**Solution:** Set timeout or check for deadlocks
```rust
#[tokio::test]
async fn test_with_timeout() {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        async_operation()
    ).await;
    
    assert!(result.is_ok(), "Operation should complete");
}
```

### Issue: Test Output is Confusing
**Solution:** Add descriptive messages to assertions
```rust
// Bad: No context
assert_eq!(a, b);

// Good: Clear message
assert_eq!(a, b, "Expected parsed value '{}' to equal '{}'", a, b);
```

---

## Test Organization Best Practices

### Group Related Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper functions
    fn sample_data() -> &'static str { ... }

    // Group 1: Basic functionality
    #[test]
    fn test_basic_case_1() { ... }

    #[test]
    fn test_basic_case_2() { ... }

    // Group 2: Error handling
    #[test]
    fn test_error_case_1() { ... }

    // Group 3: Edge cases
    #[test]
    fn test_edge_case_1() { ... }
}
```

### Use Descriptive Names
```rust
// Good: Clear what's tested
#[test]
fn test_parse_yaml_with_nested_objects() { }

#[test]
fn test_validate_invalid_toml_unclosed_string() { }

// Bad: Unclear or too generic
#[test]
fn test_parsing() { }

#[test]
fn test_error() { }
```

---

## Performance Testing

### Basic Timing
```rust
#[test]
fn test_performance() {
    let start = std::time::Instant::now();
    
    for _ in 0..1000 {
        parser.parse(content).unwrap();
    }
    
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 1000, "Should parse 1000 items in < 1s");
}
```

### Memory Usage (for mocks)
```rust
#[test]
fn test_memory_usage() {
    let storage = MockStorage::new();
    
    for i in 0..1000 {
        storage.store_block(&format!("key{}", i), b"data").await.ok();
    }
    
    let stats = storage.stats();
    assert!(stats.total_bytes_stored < 100_000_000, "Should use < 100MB");
}
```

---

## Debugging Tests

### Print Debug Information
```rust
#[test]
fn test_with_debug_output() {
    let value = parser.parse(content).unwrap();
    
    eprintln!("Parsed value: {:?}", value);  // Visible with --nocapture
    
    assert_eq!(value.title, "Expected");
}
```

### Run with Backtrace
```bash
RUST_BACKTRACE=1 cargo test test_name -- --nocapture
```

### Run Single Test Repeatedly
```bash
# Run test 10 times to catch intermittent failures
for i in {1..10}; do 
    cargo test test_name || break
done
```

---

## Useful Test Utilities

### From Workspace
```rust
use crucible_core::test_support::{
    create_kiln_with_files,
    create_basic_kiln,
    mocks::{MockStorage, MockContentHasher, MockHashingAlgorithm}
};
```

### Standard Rust Testing
```rust
#[test]
#[should_panic]  // Test should panic
fn test_panic() { panic!("Expected"); }

#[test]
#[ignore]  // Skip test
fn test_skip_for_now() { /* ... */ }

#[test]
#[should_panic(expected = "specific message")]
fn test_panic_message() { /* ... */ }
```

---

*End of Testing Reference Guide*
