//! Comprehensive Frontmatter Enhancement Tests
//!
//! This test file validates the enhanced frontmatter extraction functionality
//! including edge cases, security features, and performance characteristics.

use crucible_parser::frontmatter_extractor::*;
use crucible_parser::types::FrontmatterFormat;

#[tokio::test]
async fn test_mixed_line_ending_comprehensive() {
    let test_cases = vec![
        // Unix line endings
        ("---\ntitle: Test\n---\n# Content", LineEndingStyle::Unix),
        // Windows line endings
        ("---\r\ntitle: Test\r\n---\r\n# Content", LineEndingStyle::Windows),
        // Old Mac line endings
        ("---\rtitle: Test\r---\r# Content", LineEndingStyle::OldMac),
        // Mixed line endings (converted to Unix)
        ("---\n\rtitle: Test\r\n---\n# Content", LineEndingStyle::Mixed),
    ];

    for (content, expected_style) in test_cases {
        let result = extract_frontmatter(content).unwrap();
        assert!(result.frontmatter.is_some());
        assert_eq!(result.stats.line_ending_style, expected_style);
        assert_eq!(result.format, FrontmatterFormat::Yaml);
    }
}

#[tokio::test]
async fn test_toml_comprehensive_features() {
    let toml_content = r#"+++
title = "Advanced TOML Test"
tags = ["rust", "toml", "frontmatter"]

[author]
name = "John Doe"
email = "john@example.com"

[metadata]
created = 2024-11-20
version = "1.0.0"

[[references]]
title = "Paper 1"
authors = ["Alice", "Bob"]

[[references]]
title = "Paper 2"
authors = ["Charlie"]
+++
# Content
"#;

    let result = extract_frontmatter(toml_content).unwrap();

    assert!(result.frontmatter.is_some());
    assert_eq!(result.format, FrontmatterFormat::Toml);

    let frontmatter = result.frontmatter.unwrap();

    // Test various TOML features
    assert_eq!(frontmatter.get_string("title"), Some("Advanced TOML Test".to_string()));
    assert_eq!(
        frontmatter.get_array("tags"),
        Some(vec!["rust".to_string(), "toml".to_string(), "frontmatter".to_string()])
    );

    // Test nested object
    let author = frontmatter.get_object("author").unwrap();
    assert_eq!(author.get("name").and_then(|v| v.as_str()), Some("John Doe"));
    assert_eq!(author.get("email").and_then(|v| v.as_str()), Some("john@example.com"));

    // Test array of objects
    let references = frontmatter.properties().get("references");
    assert!(references.is_some());

    // Should be an array with 2 objects
    let references_array = references.unwrap().as_array().unwrap();
    assert_eq!(references_array.len(), 2);

    // Check first reference object
    let ref1 = &references_array[0];
    assert_eq!(ref1.get("title").and_then(|v| v.as_str()), Some("Paper 1"));
    let ref1_authors = ref1.get("authors").and_then(|v| v.as_array()).unwrap();
    assert_eq!(ref1_authors[0].as_str(), Some("Alice"));
    assert_eq!(ref1_authors[1].as_str(), Some("Bob"));

    // Check second reference object
    let ref2 = &references_array[1];
    assert_eq!(ref2.get("title").and_then(|v| v.as_str()), Some("Paper 2"));
    let ref2_authors = ref2.get("authors").and_then(|v| v.as_array()).unwrap();
    assert_eq!(ref2_authors[0].as_str(), Some("Charlie"));
}



#[tokio::test]
async fn test_edge_case_delimiters() {
    let test_cases = vec![
        // Valid YAML
        ("---\ntitle: test\n---\n# Content", true, FrontmatterFormat::Yaml),
        // Valid TOML
        ("+++\ntitle = \"test\"\n+++\n# Content", true, FrontmatterFormat::Toml),
        // Extra dashes (invalid)
        ("----\ntitle: test\n----\n# Content", false, FrontmatterFormat::None),
        // Extra pluses (invalid)
        ("++++\ntitle = \"test\"\n++++\n# Content", false, FrontmatterFormat::None),
        // Whitespace around delimiters (invalid)
        ("--- \ntitle: test\n--- \n# Content", false, FrontmatterFormat::None),
        // No content after opening delimiter
        ("---\n# Content", false, FrontmatterFormat::None),
        // No closing delimiter
        ("---\ntitle: test\n# content", false, FrontmatterFormat::None),
    ];

    for (content, should_parse, expected_format) in test_cases {
        let result = extract_frontmatter(content).unwrap();

        if should_parse {
            assert!(result.frontmatter.is_some(), "Should parse frontmatter: {}", content);
            assert_eq!(result.format, expected_format);
        } else {
            assert!(result.frontmatter.is_none(), "Should NOT parse frontmatter: {}", content);
            assert_eq!(result.format, FrontmatterFormat::None);
        }
    }
}

#[tokio::test]
async fn test_performance_characteristics() {
    let medium_content = format!("---\ntitle: test\n---\n{}", "# Content\n".repeat(100));
    let large_content = format!("---\ntitle: test\n---\n{}", "# Content\n".repeat(10_000));

    let test_cases = vec![
        // Small content
        ("# No frontmatter", "small_no_frontmatter"),
        // Small frontmatter
        ("---\ntitle: test\n---\n# Content", "small_frontmatter"),
        // Medium content
        (&medium_content.as_str(), "medium_content"),
        // Large content
        (&large_content.as_str(), "large_content"),
    ];

    for (content, case_name) in test_cases {
        let start = std::time::Instant::now();
        let result = extract_frontmatter(content).unwrap();
        let duration = start.elapsed();

        // Should complete quickly even for large content
        assert!(duration.as_millis() < 100, "Case {} should complete quickly: {:?}", case_name, duration);

        // Stats should be reasonable
        assert!(result.stats.extraction_time_us > 0, "Should measure extraction time for {}", case_name);
        assert_eq!(result.stats.body_size, content.len() - result.stats.frontmatter_size);
    }
}

#[tokio::test]
async fn test_error_recovery_and_warnings() {
    let test_cases = vec![
        // Empty frontmatter
        ("---\n\n---", "Empty frontmatter should warn"),
        // Malformed YAML but recoverable
        ("---\ntitle: \"unclosed string\ntags: [valid, items]\n---", "Should warn about quotes but still parse"),
        // Mixed valid and invalid content
        ("---\nvalid: true\n  invalid_indent: value\nalso_valid: true\n---", "Should warn about indentation but parse valid parts"),
    ];

    for (content, description) in test_cases {
        let result = extract_frontmatter(content).unwrap();

        match description {
            desc if desc.contains("warn") => {
                assert!(!result.warnings.is_empty(), "Should generate warnings: {}", description);
            }
            _ => {}
        }

        // Should still be able to create frontmatter object
        if content.starts_with("---") {
            assert!(result.frontmatter.is_some() || !result.warnings.is_empty(),
                   "Should either parse successfully or provide warnings: {}", description);
        }
    }
}

#[tokio::test]
async fn test_nested_structures_comprehensive() {
    let yaml_nested = r#"---
author:
  name: "John Doe"
  email: "john@example.com"
  address:
    street: "123 Main St"
    city: "Anytown"
    coordinates:
      lat: 40.7128
      lng: -74.0060
metadata:
  created: "2024-11-20"
  tags:
    - personal
    - important
  config:
    theme: dark
    notifications: true
    experimental:
      feature_a: true
      feature_b: false
---

# Test content here
"#;

    let result = extract_frontmatter(yaml_nested).unwrap();
    assert!(result.frontmatter.is_some());

    let frontmatter = result.frontmatter.unwrap();

    // Test deeply nested object access
    let author = frontmatter.get_object("author").unwrap();
    let address = author.get("address").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        address.get("street").and_then(|v| v.as_str()),
        Some("123 Main St")
    );

    let coordinates = address.get("coordinates").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        coordinates.get("lat").and_then(|v| v.as_f64()),
        Some(40.7128)
    );

    // Test metadata with nested arrays and objects
    let metadata = frontmatter.get_object("metadata").unwrap();
    let tags = metadata.get("tags").and_then(|v| v.as_array()).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].as_str(), Some("personal"));

    let config = metadata.get("config").and_then(|v| v.as_object()).unwrap();
    let experimental = config.get("experimental").and_then(|v| v.as_object()).unwrap();
    assert_eq!(experimental.get("feature_a").and_then(|v| v.as_bool()), Some(true));
}

#[tokio::test]
async fn test_compatibility_with_existing_types() {
    // Test that our new extractor produces results compatible with existing Frontmatter type
    let yaml_content = "---\ntitle: Test\ntags: [rust, testing]\ncreated: 2024-11-20\n---\n# Content";
    let result = extract_frontmatter(yaml_content).unwrap();

    assert!(result.frontmatter.is_some());
    let frontmatter = result.frontmatter.unwrap();

    // Test all the existing getter methods work
    assert_eq!(frontmatter.get_string("title"), Some("Test".to_string()));
    assert_eq!(frontmatter.get_array("tags"), Some(vec!["rust".to_string(), "testing".to_string()]));
    assert_eq!(
        frontmatter.get_date("created"),
        Some(chrono::NaiveDate::from_ymd_opt(2024, 11, 20).unwrap())
    );

    // Test missing keys
    assert_eq!(frontmatter.get_string("nonexistent"), None);
    assert_eq!(frontmatter.get_number("nonexistent"), None);
    assert_eq!(frontmatter.get_bool("nonexistent"), None);
}

#[test]
fn test_early_exit_optimization() {
    // Test that content without frontmatter delimiters is quickly rejected
    let content_no_frontmatter = "# Just a regular markdown file\n\nNo frontmatter here.\n\nJust content.";

    let start = std::time::Instant::now();
    let result = extract_frontmatter(content_no_frontmatter).unwrap();
    let duration = start.elapsed();

    // Should be very fast (< 1ms) since it does early exit
    assert!(duration.as_millis() < 1, "Early exit should be very fast: {:?}", duration);
    assert!(result.frontmatter.is_none());
    assert_eq!(result.format, FrontmatterFormat::None);
    assert!(result.warnings.is_empty());
}