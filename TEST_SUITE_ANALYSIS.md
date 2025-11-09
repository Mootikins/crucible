# Crucible Test Suite Analysis - Phase 1 Planning

**Analysis Date:** November 8, 2025
**Scope:** Test suite structure across crucible-parser and crucible-core

---

## Executive Summary

The Crucible workspace uses a **modular inline testing approach** with comprehensive mock implementations and helper utilities. Tests are organized within source files using Rust's standard `#[test]` and `#[cfg(test)]` patterns, not in separate integration test directories. This document provides guidance for creating consistent, high-quality tests for Phase 1 frontmatter parsing.

---

## 1. Test Organization Overview

### Current Structure
- **55+ test functions** in crucible-parser alone
- **Inline tests** using `#[cfg(test)]` modules within source files
- **No separate `/tests/` directories** - tests live alongside implementation
- **Mock implementations** centralized in `crucible-core/src/test_support/mocks.rs`
- **Test utilities** in `crucible-core/src/test_support/mod.rs`

### Key Files with Tests
```
crucible-parser/src/
├── implementation.rs      (Parser structure and capabilities tests)
├── types.rs              (Type parsing and validation tests)
├── adapter.rs            (Document conversion tests)
├── block_extractor.rs    (Block extraction tests)
├── callouts.rs           (Callout syntax tests)
├── enhanced_tags.rs      (Tag parsing tests)
├── latex.rs              (LaTeX expression tests)
├── footnotes.rs          (Footnote handling tests)
└── ... (other test files)

crucible-core/src/
├── test_support/
│   ├── mod.rs            (Test utilities: kiln creation)
│   └── mocks.rs          (Mock implementations: 1,200+ lines)
├── agent/
│   ├── tests.rs          (65+ comprehensive tests)
│   └── integration_test.rs
└── ... (other tests)
```

---

## 2. Testing Patterns Identified

### Pattern 1: Inline Unit Tests with Assertions
**Location:** `crucible-parser/src/implementation.rs`
**Size:** 15-30 lines per test
**Focus:** Single responsibility

```rust
#[test]
fn test_parse_frontmatter() {
    let parser = CrucibleParser::new();

    // YAML frontmatter
    let content = "---\ntitle: Test\n---\nContent";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_some());
    assert_eq!(content, "Content");
    assert_eq!(format, crate::types::FrontmatterFormat::Yaml);

    // TOML frontmatter
    let content = "+++\ntitle = \"Test\"\n+++\nContent";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_some());
    assert_eq!(content, "Content");
    assert_eq!(format, crate::types::FrontmatterFormat::Toml);

    // No frontmatter
    let content = "Just content";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(content, "Just content");
    assert_eq!(format, crate::types::FrontmatterFormat::None);
}
```

### Pattern 2: Structured Test Modules
**Location:** `crucible-core/src/agent/tests.rs`
**Size:** 865 lines total, individual tests 20-40 lines
**Focus:** Comprehensive coverage with helpers

```rust
#[cfg(test)]
mod tests {
    use crate::agent::{AgentLoader, AgentRegistry};
    use std::fs;
    use tempfile::TempDir;

    // Helper function for test setup
    fn get_sample_agent_frontmatter() -> &'static str {
        r#"---
name: "Test Agent"
version: "1.0.0"
...
---

# System Prompt
..."#
    }

    // Test function with setup/teardown
    #[test]
    fn test_agent_loader_parse_valid_agent() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut loader = AgentLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(result.is_ok(), "Failed to load valid agent file: {:?}", result.err());
        let agent = result.unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.version, "1.0.0");
    }
}
```

### Pattern 3: Async Tests with Tokio
**Location:** `crucible-core/src/test_support/mocks.rs`
**Attribute:** `#[tokio::test]`
**Dependencies:** tokio-test, async-trait

```rust
#[tokio::test]
async fn test_mock_storage_basic_operations() {
    let storage = MockStorage::new();

    // Store block
    storage.store_block("hash1", b"test data").await.unwrap();
    assert_eq!(storage.block_count(), 1);

    // Retrieve block
    let data = storage.get_block("hash1").await.unwrap();
    assert_eq!(data, Some(b"test data".to_vec()));
}
```

### Pattern 4: Test Data with Inline Strings
**Location:** Throughout parser tests
**Approach:** Raw string literals (`r#"..."#`) for multiline content

```rust
#[test]
fn test_complex_frontmatter() {
    let content = r#"---
title: "My Document"
tags:
  - project
  - rust
  - testing
description: "A multi-line\ndescription here"
status: active
---

# Main content here
"#;

    let parser = CrucibleParser::new();
    let (fm, body, format) = parser.parse_frontmatter(content);
    assert!(fm.is_some());
}
```

### Pattern 5: Mock Implementation Pattern
**Location:** `crucible-core/src/test_support/mocks.rs`
**Size:** 1,200+ lines
**Key Features:**
- Observable state tracking (stats, operation counts)
- Error injection capabilities
- Thread-safe with Arc<Mutex<>>
- Async-trait implementations

```rust
#[derive(Debug, Clone)]
pub struct MockContentHasher {
    state: Arc<Mutex<MockContentHasherState>>,
    algorithm: MockHashingAlgorithm,
}

impl MockContentHasher {
    pub fn new() -> Self { ... }
    pub fn set_file_hash(&self, path: &str, hash: Vec<u8>) { ... }
    pub fn operation_counts(&self) -> (usize, usize) { ... }
    pub fn reset(&self) { ... }
}

#[async_trait]
impl ContentHasher for MockContentHasher {
    async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError> { ... }
}
```

---

## 3. Test Utilities and Helpers

### Available Test Support Infrastructure

#### A. Kiln Creation Helpers (`crucible-core/src/test_support/mod.rs`)
```rust
/// Create a temporary kiln with specific files
pub fn create_kiln_with_files(files: &[(&str, &str)]) -> Result<TempDir>

/// Create a basic kiln with 5 standard markdown files
pub fn create_basic_kiln() -> Result<TempDir>

/// Convert kiln path to string
pub fn kiln_path_str(path: &Path) -> String
```

**Usage:**
```rust
#[test]
fn test_parsing_multiple_documents() {
    let kiln = create_kiln_with_files(&[
        ("document1.md", "---\ntitle: Doc1\n---\nContent1"),
        ("document2.md", "---\ntitle: Doc2\n---\nContent2"),
    ]).unwrap();

    // Test operations on kiln files
}
```

#### B. Mock Implementations Available
```rust
// In crucible-core/src/test_support/mocks.rs

MockHashingAlgorithm      // Deterministic simple hash
MockStorage               // In-memory block/tree storage
MockContentHasher         // Configurable file/block hashing
MockHashLookupStorage     // Hash lookup with operation tracking
MockChangeDetector        // Complete change detection mock
```

#### C. Test Dependencies in Workspace
```toml
# crucible-parser/Cargo.toml
[dev-dependencies]
tokio-test = "0.4"

# crucible-core/Cargo.toml
[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "full"] }
tempfile = "3.0"
serial_test = "3.0"
```

---

## 4. Existing Frontmatter Tests

### Current Frontmatter Coverage
**File:** `crucible-parser/src/implementation.rs`

```rust
#[test]
fn test_parse_frontmatter() {
    let parser = CrucibleParser::new();

    // YAML test (passes)
    let content = "---\ntitle: Test\n---\nContent";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_some());
    assert_eq!(content, "Content");
    assert_eq!(format, crate::types::FrontmatterFormat::Yaml);

    // TOML test (passes)
    let content = "+++\ntitle = \"Test\"\n+++\nContent";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_some());
    assert_eq!(content, "Content");
    assert_eq!(format, crate::types::FrontmatterFormat::Toml);

    // No frontmatter test (passes)
    let content = "Just content";
    let (fm, content, format) = parser.parse_frontmatter(content);
    assert!(fm.is_none());
    assert_eq!(content, "Just content");
    assert_eq!(format, crate::types::FrontmatterFormat::None);
}
```

### Gap Analysis

**Currently Tested:**
- Basic YAML parsing (delimiter detection)
- Basic TOML parsing (delimiter detection)
- No frontmatter case
- Content extraction after delimiters

**NOT Currently Tested:**
- YAML validation (malformed syntax)
- TOML validation (malformed syntax)
- Edge cases (missing closing delimiter, nested delimiters)
- Frontmatter value extraction and type conversion
- Special characters in frontmatter
- Multiline string handling
- Comment syntax in YAML/TOML
- YAML array/object structures
- Case sensitivity in format detection
- Whitespace handling around delimiters
- Empty frontmatter blocks
- Mixed delimiter styles (error cases)

---

## 5. Recommended Test Structure for Phase 1

### Location
**File:** `/home/moot/crucible/crates/crucible-parser/src/frontmatter.rs`

Create a dedicated module with:
1. Frontmatter parsing implementation
2. Validation logic
3. Comprehensive inline test module

### Template Structure

```rust
// File: crates/crucible-parser/src/frontmatter.rs

use std::collections::HashMap;
use serde_yaml::Value;
use crate::types::FrontmatterFormat;
use crate::error::{ParserError, ParserResult};

/// Frontmatter parser for YAML and TOML formats
pub struct FrontmatterParser {
    strict_mode: bool,  // Enforce strict validation
}

impl FrontmatterParser {
    pub fn new() -> Self {
        Self { strict_mode: true }
    }

    /// Parse YAML frontmatter
    pub fn parse_yaml(&self, content: &str) -> ParserResult<HashMap<String, Value>> {
        // Implementation
    }

    /// Parse TOML frontmatter
    pub fn parse_toml(&self, content: &str) -> ParserResult<toml::Value> {
        // Implementation
    }

    /// Validate frontmatter structure
    pub fn validate(&self, content: &str, format: FrontmatterFormat) -> ParserResult<()> {
        // Implementation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: Create test data
    fn sample_yaml_frontmatter() -> &'static str {
        r#"title: Test Document
tags:
  - rust
  - testing
status: active"#
    }

    // Test: Basic YAML parsing
    #[test]
    fn test_parse_valid_yaml_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = sample_yaml_frontmatter();

        let result = parser.parse_yaml(content);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.get("title").and_then(|v| v.as_str()), Some("Test Document"));
    }

    // Test: Invalid YAML syntax
    #[test]
    fn test_parse_invalid_yaml() {
        let parser = FrontmatterParser::new();
        let invalid = r#"title: Unclosed string
description: "this is unclosed
tags: [1, 2"#;

        let result = parser.parse_yaml(invalid);
        assert!(result.is_err());
    }

    // Test: YAML with special characters
    #[test]
    fn test_parse_yaml_special_characters() {
        let parser = FrontmatterParser::new();
        let content = r#"title: "Special: Characters & Symbols!"
description: "Line 1\nLine 2"
url: "https://example.com?q=test&v=1""#;

        let result = parser.parse_yaml(content);
        assert!(result.is_ok());
    }

    // Test: Empty frontmatter
    #[test]
    fn test_parse_empty_yaml() {
        let parser = FrontmatterParser::new();
        let result = parser.parse_yaml("");

        // Should either succeed with empty dict or fail gracefully
        assert!(result.is_ok() || result.is_err());
    }

    // Test: TOML parsing
    #[test]
    fn test_parse_valid_toml_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = r#"title = "Test"
tags = ["rust", "testing"]
[metadata]
author = "Test Author""#;

        let result = parser.parse_toml(content);
        assert!(result.is_ok());
    }

    // Test: Case sensitivity
    #[test]
    fn test_yaml_case_sensitivity() {
        let parser = FrontmatterParser::new();
        let content = r#"Title: "Capitalized"
title: "Lowercase""#;

        let result = parser.parse_yaml(content);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        // Both should exist as separate keys
        assert!(parsed.contains_key("Title"));
        assert!(parsed.contains_key("title"));
    }

    // Test: Validation with strict mode
    #[test]
    fn test_frontmatter_validation_strict_mode() {
        let mut parser = FrontmatterParser::new();
        parser.strict_mode = true;

        let valid = sample_yaml_frontmatter();
        assert!(parser.validate(valid, FrontmatterFormat::Yaml).is_ok());
    }

    // Test: Multiline values
    #[test]
    fn test_yaml_multiline_strings() {
        let parser = FrontmatterParser::new();
        let content = r#"description: |
  This is a multiline
  string description
  with multiple lines"#;

        let result = parser.parse_yaml(content);
        assert!(result.is_ok());
    }
}
```

### Integration with Existing Code
1. Add to `/home/moot/crucible/crates/crucible-parser/src/lib.rs`:
   ```rust
   pub mod frontmatter;
   pub use frontmatter::FrontmatterParser;
   ```

2. Update `implementation.rs` to use new parser:
   ```rust
   use crate::frontmatter::FrontmatterParser;
   ```

---

## 6. Test Execution Guidelines

### Running Tests
```bash
# All tests in crucible-parser
cargo test -p crucible-parser

# Specific test
cargo test -p crucible-parser test_parse_frontmatter

# With output
cargo test -p crucible-parser -- --nocapture

# Test coverage (if tarpaulin installed)
cargo tarpaulin -p crucible-parser --out Html
```

### Quality Checklist for Phase 1 Tests
- [ ] Each test has single, clear purpose
- [ ] Test data uses raw string literals for readability
- [ ] Helper functions extract common setup
- [ ] Error cases explicitly tested
- [ ] Edge cases documented with comments
- [ ] Assertions have descriptive messages: `assert!(x, "Expected {} but got {}", a, b)`
- [ ] No external dependencies (use mocks)
- [ ] Tests run in < 100ms each
- [ ] Test names clearly describe what's tested
- [ ] Coverage: normal cases, edge cases, error cases

---

## 7. Mock Storage Testing Pattern

For Phase 1, if frontmatter tests need storage interaction:

```rust
#[tokio::test]
async fn test_frontmatter_with_storage() {
    use crucible_core::test_support::mocks::MockStorage;

    let storage = MockStorage::new();
    let parser = FrontmatterParser::new();

    // Parse and validate with mock storage
    let content = "---\ntitle: Test\n---\nBody";
    let (fm, body, format) = parser.extract_frontmatter(content).unwrap();

    // Store parsed frontmatter metadata
    let metadata = format!("{:?}", fm);
    storage.store_block("meta_key", metadata.as_bytes()).await.unwrap();

    // Verify storage was used
    let stats = storage.stats();
    assert_eq!(stats.store_count, 1);
}
```

---

## 8. Test Data Patterns

### Recommended Approaches

**Approach 1: Inline Raw Strings (Preferred for Phase 1)**
```rust
#[test]
fn test_with_inline_data() {
    let content = r#"---
title: Test
tags: [a, b]
---
Content"#;

    // Test implementation
}
```

**Approach 2: Helper Functions**
```rust
fn sample_yaml() -> &'static str {
    r#"---
title: Test
---"#
}

#[test]
fn test_with_helper() {
    let content = sample_yaml();
    // Test implementation
}
```

**Approach 3: Builder Pattern (for complex data)**
```rust
struct FrontmatterBuilder {
    pairs: Vec<(String, String)>,
}

impl FrontmatterBuilder {
    fn new() -> Self { /* ... */ }
    fn with_field(mut self, key: &str, value: &str) -> Self {
        self.pairs.push((key.to_string(), value.to_string()));
        self
    }
    fn build(&self) -> String {
        // Format as YAML
    }
}

#[test]
fn test_with_builder() {
    let fm = FrontmatterBuilder::new()
        .with_field("title", "Test")
        .with_field("status", "active")
        .build();
}
```

---

## 9. Summary and Recommendations

### Phase 1 Testing Strategy

1. **Location:** `crates/crucible-parser/src/frontmatter.rs` (new file)

2. **Module Structure:**
   - Public API: `FrontmatterParser` struct with methods for YAML/TOML parsing
   - Validation logic: Input validation and format detection
   - Error handling: Detailed error messages using `ParserError`
   - Inline `#[cfg(test)]` module with 20-30 tests

3. **Test Organization:**
   ```
   frontmatter.rs tests/
   ├── Basic parsing
   │   ├── test_parse_valid_yaml_frontmatter
   │   ├── test_parse_valid_toml_frontmatter
   │   └── test_parse_empty_frontmatter
   ├── Validation & Error Handling
   │   ├── test_parse_invalid_yaml
   │   ├── test_parse_invalid_toml
   │   └── test_malformed_delimiters
   ├── Edge Cases
   │   ├── test_yaml_special_characters
   │   ├── test_yaml_multiline_strings
   │   ├── test_nested_structures
   │   └── test_case_sensitivity
   └── Integration
       ├── test_frontmatter_extraction_full_document
       └── test_frontmatter_with_storage_mock
   ```

4. **Code Quality Standards:**
   - Follow existing inline test patterns from `types.rs` and `implementation.rs`
   - Use helper functions for common test data
   - Inline raw strings for multiline content
   - Descriptive assertion messages
   - No external test dependencies beyond workspace
   - Target: >80% code coverage for frontmatter module

5. **Dependencies to Leverage:**
   - `serde_yaml` (already in workspace)
   - `toml` (already in workspace)
   - `tempfile` (already in dev-dependencies)
   - `thiserror` (already in workspace)

---

## 10. File References

### Key Files for Phase 1 Development
- **Parser implementation:** `/home/moot/crucible/crates/crucible-parser/src/implementation.rs` (lines 1-200)
- **Existing test patterns:** `/home/moot/crucible/crates/crucible-parser/src/types.rs` (test module)
- **Agent tests (reference):** `/home/moot/crucible/crates/crucible-core/src/agent/tests.rs` (865 lines)
- **Mock implementations:** `/home/moot/crucible/crates/crucible-core/src/test_support/mocks.rs` (1,200+ lines)
- **Test utilities:** `/home/moot/crucible/crates/crucible-core/src/test_support/mod.rs` (87 lines)

### Configuration Files
- **Parser Cargo.toml:** `/home/moot/crucible/crates/crucible-parser/Cargo.toml`
- **Core Cargo.toml:** `/home/moot/crucible/crates/crucible-core/Cargo.toml`

---

## 11. Next Steps

1. Create `/home/moot/crucible/crates/crucible-parser/src/frontmatter.rs`
2. Implement `FrontmatterParser` with YAML and TOML parsing
3. Write inline test module with 25-30 test cases
4. Run test suite: `cargo test -p crucible-parser`
5. Verify coverage and fix any gaps
6. Update library exports in `lib.rs`
7. Document API with rustdoc comments

---

*End of Test Suite Analysis Document*
