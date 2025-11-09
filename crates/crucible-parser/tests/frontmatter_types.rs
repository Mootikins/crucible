//! Frontmatter Type Inference Tests
//!
//! Phase 1.1: Test all frontmatter type inference (text, number, bool, date, array, object)
//!
//! Following TDD methodology (RED-GREEN-REFACTOR):
//! 1. Write failing tests for all type inference
//! 2. Implement typed getters
//! 3. Verify all tests pass

use crucible_parser::types::{Frontmatter, FrontmatterFormat};

// ============================================================================
// Phase 1.1.1: YAML Frontmatter with All Types
// ============================================================================

#[test]
fn test_yaml_frontmatter_all_types() {
    let yaml = r#"
title: My Note
count: 42
rating: 4.5
published: true
created: 2024-11-08
tags:
  - rust
  - testing
  - tdd
aliases: ["note", "example"]
author:
  name: John Doe
  email: john@example.com
"#;

    let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

    // Test string property
    assert_eq!(fm.get_string("title"), Some("My Note".to_string()));

    // Test integer number
    assert_eq!(fm.get_number("count"), Some(42.0));

    // Test float number
    assert_eq!(fm.get_number("rating"), Some(4.5));

    // Test boolean
    assert_eq!(fm.get_bool("published"), Some(true));

    // Test date (Phase 1.1.1 - this will fail until we implement get_date)
    assert_eq!(
        fm.get_date("created"),
        Some(chrono::NaiveDate::from_ymd_opt(2024, 11, 8).unwrap())
    );

    // Test array of strings
    assert_eq!(
        fm.get_array("tags"),
        Some(vec![
            "rust".to_string(),
            "testing".to_string(),
            "tdd".to_string()
        ])
    );

    // Test array alternative format
    assert_eq!(
        fm.get_array("aliases"),
        Some(vec!["note".to_string(), "example".to_string()])
    );

    // Test object (Phase 1.1.1 - this will fail until we implement get_object)
    let author = fm.get_object("author").expect("author should exist");
    assert_eq!(author.get("name").and_then(|v| v.as_str()), Some("John Doe"));
    assert_eq!(
        author.get("email").and_then(|v| v.as_str()),
        Some("john@example.com")
    );
}

// ============================================================================
// Phase 1.1.2: TOML Frontmatter Support
// ============================================================================

#[test]
fn test_toml_frontmatter_type_inference() {
    let toml = r#"
title = "My TOML Note"
count = 42
rating = 4.5
published = true
created = "2024-11-08"
tags = ["rust", "testing", "toml"]

[author]
name = "Jane Doe"
email = "jane@example.com"
"#;

    let fm = Frontmatter::new(toml.to_string(), FrontmatterFormat::Toml);

    // Test basic types
    assert_eq!(fm.get_string("title"), Some("My TOML Note".to_string()));
    assert_eq!(fm.get_number("count"), Some(42.0));
    assert_eq!(fm.get_number("rating"), Some(4.5));
    assert_eq!(fm.get_bool("published"), Some(true));

    // Test date
    assert_eq!(
        fm.get_date("created"),
        Some(chrono::NaiveDate::from_ymd_opt(2024, 11, 8).unwrap())
    );

    // Test array
    assert_eq!(
        fm.get_array("tags"),
        Some(vec![
            "rust".to_string(),
            "testing".to_string(),
            "toml".to_string()
        ])
    );

    // Test nested table
    let author = fm.get_object("author").expect("author should exist");
    assert_eq!(author.get("name").and_then(|v| v.as_str()), Some("Jane Doe"));
    assert_eq!(
        author.get("email").and_then(|v| v.as_str()),
        Some("jane@example.com")
    );
}

// ============================================================================
// Phase 1.1.3: Empty Frontmatter Edge Case
// ============================================================================

#[test]
fn test_empty_frontmatter_returns_none() {
    // Empty YAML
    let fm = Frontmatter::new(String::new(), FrontmatterFormat::Yaml);
    assert_eq!(fm.get_string("title"), None);
    assert_eq!(fm.get_number("count"), None);
    assert_eq!(fm.get_bool("published"), None);
    assert_eq!(fm.get_date("created"), None);
    assert_eq!(fm.get_array("tags"), None);
    assert_eq!(fm.get_object("author"), None);

    // Empty TOML
    let fm = Frontmatter::new(String::new(), FrontmatterFormat::Toml);
    assert_eq!(fm.get_string("title"), None);

    // None format
    let fm = Frontmatter::new(String::new(), FrontmatterFormat::None);
    assert_eq!(fm.get_string("title"), None);
}

// ============================================================================
// Phase 1.1.4: Invalid YAML Error Handling
// ============================================================================

#[test]
fn test_invalid_yaml_does_not_panic() {
    // Invalid YAML (unbalanced brackets)
    let invalid_yaml = r#"
title: "My Note
tags: [incomplete
    "#;

    let fm = Frontmatter::new(invalid_yaml.to_string(), FrontmatterFormat::Yaml);

    // Should not panic, should return None or empty
    assert_eq!(fm.properties().len(), 0); // Should be empty due to parse error
}

#[test]
fn test_invalid_toml_does_not_panic() {
    // Invalid TOML (syntax error)
    let invalid_toml = r#"
title = "My Note
[broken table
    "#;

    let fm = Frontmatter::new(invalid_toml.to_string(), FrontmatterFormat::Toml);

    // Should not panic, should return empty
    assert_eq!(fm.properties().len(), 0); // Should be empty due to parse error
}

// ============================================================================
// Phase 1.1.5: Unicode Support in Frontmatter
// ============================================================================

#[test]
fn test_unicode_in_frontmatter_values() {
    let yaml = r#"
title: "日本語のタイトル"
author: "François Müller"
emoji: "🦀 Rust 🔥"
chinese: "中文测试"
arabic: "مرحبا"
mixed: "Hello 世界 🌍"
tags: ["日本語", "中文", "🎯"]
"#;

    let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

    // Test Unicode string values
    assert_eq!(
        fm.get_string("title"),
        Some("日本語のタイトル".to_string())
    );
    assert_eq!(
        fm.get_string("author"),
        Some("François Müller".to_string())
    );
    assert_eq!(fm.get_string("emoji"), Some("🦀 Rust 🔥".to_string()));
    assert_eq!(fm.get_string("chinese"), Some("中文测试".to_string()));
    assert_eq!(fm.get_string("arabic"), Some("مرحبا".to_string()));
    assert_eq!(
        fm.get_string("mixed"),
        Some("Hello 世界 🌍".to_string())
    );

    // Test Unicode in arrays
    assert_eq!(
        fm.get_array("tags"),
        Some(vec![
            "日本語".to_string(),
            "中文".to_string(),
            "🎯".to_string()
        ])
    );
}

#[test]
fn test_unicode_in_toml_frontmatter() {
    let toml = r#"
title = "日本語のタイトル"
author = "François Müller"
tags = ["日本語", "Rust 🦀"]
"#;

    let fm = Frontmatter::new(toml.to_string(), FrontmatterFormat::Toml);

    assert_eq!(
        fm.get_string("title"),
        Some("日本語のタイトル".to_string())
    );
    assert_eq!(
        fm.get_string("author"),
        Some("François Müller".to_string())
    );
    assert_eq!(
        fm.get_array("tags"),
        Some(vec!["日本語".to_string(), "Rust 🦀".to_string()])
    );
}

// ============================================================================
// Additional Edge Cases
// ============================================================================

#[test]
fn test_type_coercion_edge_cases() {
    let yaml = r#"
string_number: "42"
number_string: 42
bool_string: "true"
string_bool: true
"#;

    let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

    // String that looks like number should return as string
    assert_eq!(fm.get_string("string_number"), Some("42".to_string()));
    assert_eq!(fm.get_number("string_number"), None); // Not a number

    // Number should not return as string
    assert_eq!(fm.get_string("number_string"), None); // Not a string
    assert_eq!(fm.get_number("number_string"), Some(42.0));

    // String that looks like bool should return as string
    assert_eq!(fm.get_string("bool_string"), Some("true".to_string()));
    assert_eq!(fm.get_bool("bool_string"), None); // Not a boolean

    // Bool should not return as string
    assert_eq!(fm.get_string("string_bool"), None); // Not a string
    assert_eq!(fm.get_bool("string_bool"), Some(true));
}

#[test]
fn test_missing_keys_return_none() {
    let yaml = r#"
title: "My Note"
"#;

    let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

    assert_eq!(fm.get_string("nonexistent"), None);
    assert_eq!(fm.get_number("nonexistent"), None);
    assert_eq!(fm.get_bool("nonexistent"), None);
    assert_eq!(fm.get_date("nonexistent"), None);
    assert_eq!(fm.get_array("nonexistent"), None);
    assert_eq!(fm.get_object("nonexistent"), None);
}
