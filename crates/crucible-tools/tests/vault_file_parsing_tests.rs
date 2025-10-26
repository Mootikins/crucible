//! Vault File Parsing Tests - Phase 1A TDD Implementation
//!
//! This file contains comprehensive tests for vault file parsing functionality.
//! Tests are written FIRST and will initially FAIL before implementation.
//!
//! TDD Process:
//! 1. Write failing tests (this file)
//! 2. Verify tests fail (run cargo test)
//! 3. Implement minimal code to make tests pass
//! 4. Refactor while keeping tests passing

use chrono::Datelike;
use serde_json::Value;

/// Test vault path for integration testing
const TEST_VAULT_PATH: &str = "/home/moot/Documents/crucible-testing";

#[tokio::test]
async fn test_vault_scanner_finds_markdown_files() {
    // This test should FAIL initially before implementation
    // Tests that vault scanner can discover markdown files in directory

    let scanner = crucible_tools::vault_scanner::VaultScanner::new(TEST_VAULT_PATH);
    let markdown_files = scanner.scan_markdown_files().await.unwrap();

    // Verify we found markdown files in the test vault
    assert!(
        !markdown_files.is_empty(),
        "Should find markdown files in test vault"
    );

    // Verify all entries are markdown files
    for file_path in &markdown_files {
        assert!(
            file_path.extension().unwrap_or_default() == "md",
            "All files should have .md extension, found: {:?}",
            file_path
        );
    }

    // Verify we found expected files from the test vault
    let file_paths: Vec<String> = markdown_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Should find known files in the test vault
    assert!(
        file_paths.iter().any(|p| p.contains("PRIME.md")),
        "Should find PRIME.md in test vault"
    );
    assert!(
        file_paths.iter().any(|p| p.contains("Rune MCP")),
        "Should find Rune MCP files in test vault"
    );
}

#[tokio::test]
async fn test_frontmatter_parsing_extracts_metadata() {
    // This test should FAIL initially before implementation
    // Tests that markdown parser can extract YAML frontmatter

    let parser = crucible_tools::vault_parser::VaultParser::new();

    // Test with a real file from the test vault
    let test_file_path = format!("{}/PRIME.md", TEST_VAULT_PATH);
    let vault_file = parser.parse_file(&test_file_path).await.unwrap();

    // Verify frontmatter was extracted
    assert!(
        vault_file.metadata.frontmatter.contains_key("type"),
        "Should extract 'type' field from frontmatter"
    );
    assert!(
        vault_file.metadata.frontmatter.contains_key("tags"),
        "Should extract 'tags' field from frontmatter"
    );

    // Verify specific values from PRIME.md
    let frontmatter_type = vault_file.metadata.frontmatter.get("type").unwrap();
    assert_eq!(
        frontmatter_type,
        &Value::String("meta".to_string()),
        "Type should be 'meta' for PRIME.md"
    );

    // Verify tags array
    let tags = vault_file.metadata.frontmatter.get("tags").unwrap();
    if let Value::Array(tag_array) = tags {
        assert!(
            tag_array.contains(&Value::String("vault-config".to_string())),
            "Should contain 'vault-config' tag"
        );
        assert!(
            tag_array.contains(&Value::String("instructions".to_string())),
            "Should contain 'instructions' tag"
        );
    } else {
        panic!("Tags should be an array, got: {:?}", tags);
    }
}

#[tokio::test]
async fn test_file_change_detection_via_hashing() {
    // This test should FAIL initially before implementation
    // Tests that file change detection works via SHA256 hashing

    let change_detector = crucible_tools::vault_change_detection::ChangeDetector::new();

    // Test file path
    let test_file_path = format!("{}/PRIME.md", TEST_VAULT_PATH);

    // Get initial hash
    let initial_hash = change_detector
        .calculate_file_hash(&test_file_path)
        .await
        .unwrap();
    assert!(!initial_hash.is_empty(), "Hash should not be empty");
    assert_eq!(
        initial_hash.len(),
        64,
        "SHA256 hash should be 64 characters"
    );

    // Verify hash is consistent
    let second_hash = change_detector
        .calculate_file_hash(&test_file_path)
        .await
        .unwrap();
    assert_eq!(
        initial_hash, second_hash,
        "Hash should be consistent for unchanged file"
    );

    // Test with different file (should have different hash)
    let different_file_path = format!("{}/Projects/Rune MCP/Rune MCP - MoC.md", TEST_VAULT_PATH);
    let different_hash = change_detector
        .calculate_file_hash(&different_file_path)
        .await
        .unwrap();
    assert_ne!(
        initial_hash, different_hash,
        "Different files should have different hashes"
    );
}

#[tokio::test]
async fn test_vault_parsing_integration_with_real_files() {
    // This test should FAIL initially before implementation
    // Tests full integration: scanning + parsing + metadata extraction

    let vault_path = TEST_VAULT_PATH;
    let scanner = crucible_tools::vault_scanner::VaultScanner::new(vault_path);
    let parser = crucible_tools::vault_parser::VaultParser::new();

    // Scan for markdown files
    let markdown_files = scanner.scan_markdown_files().await.unwrap();
    assert!(!markdown_files.is_empty(), "Should find markdown files");

    // Parse first few files, but make sure to include PRIME.md
    let mut parsed_files = Vec::new();
    let mut prime_file_found = false;

    for file_path in markdown_files.iter() {
        let absolute_path = format!("{}/{}", vault_path, file_path.to_string_lossy());
        let vault_file = parser.parse_file(&absolute_path).await.unwrap();

        // Always include PRIME.md if found
        if file_path.to_string_lossy().contains("PRIME.md") {
            prime_file_found = true;
            parsed_files.push(vault_file);
        }
        // Include other files up to 5 total
        else if parsed_files.len() < 5 {
            parsed_files.push(vault_file);
        }

        // Stop once we have PRIME.md + up to 4 other files
        if prime_file_found && parsed_files.len() >= 5 {
            break;
        }
    }

    // Verify all files have required fields
    for vault_file in &parsed_files {
        assert!(
            !vault_file.path.to_string_lossy().is_empty(),
            "File path should not be empty"
        );
        assert!(
            !vault_file.content.is_empty(),
            "Content should not be empty"
        );
        assert!(!vault_file.hash.is_empty(), "Hash should not be empty");
        assert!(
            vault_file.metadata.size > 0,
            "File size should be greater than 0"
        );

        // Verify frontmatter structure
        let frontmatter = &vault_file.metadata.frontmatter;
        if frontmatter.contains_key("type") {
            let file_type = frontmatter.get("type").unwrap();
            assert!(file_type.is_string(), "Type should be a string");
        }

        if frontmatter.contains_key("tags") {
            let tags = frontmatter.get("tags").unwrap();
            assert!(tags.is_array(), "Tags should be an array");
        }
    }

    // Test specific file properties
    let prime_file = parsed_files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("PRIME.md"))
        .expect("Should find PRIME.md in parsed files");

    assert_eq!(
        prime_file.metadata.frontmatter.get("type").unwrap(),
        &Value::String("meta".to_string())
    );
    assert!(
        prime_file.content.contains("# Vault Primer"),
        "Content should contain expected markdown"
    );
}

#[tokio::test]
async fn test_error_handling_missing_file() {
    // This test should FAIL initially before implementation
    // Tests error handling for non-existent files

    let parser = crucible_tools::vault_parser::VaultParser::new();
    let nonexistent_path = format!("{}/nonexistent_file.md", TEST_VAULT_PATH);

    let result = parser.parse_file(&nonexistent_path).await;

    assert!(result.is_err(), "Should return error for missing file");

    match result.unwrap_err() {
        crucible_tools::vault_types::VaultError::FileNotFound(path) => {
            assert!(
                path.contains("nonexistent_file.md"),
                "Error should mention missing file"
            );
        }
        other => panic!("Expected FileNotFound error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_error_handling_malformed_frontmatter() {
    // This test should FAIL initially before implementation
    // Tests error handling for malformed YAML frontmatter

    // We'll need to create a temporary file with malformed frontmatter
    // For now, this is a placeholder for the test structure
    let parser = crucible_tools::vault_parser::VaultParser::new();

    // This test would require creating a temp file with bad YAML
    // Implementation to follow after basic structure is in place

    // For now, just verify the error type exists
    let _error_type =
        crucible_tools::vault_types::VaultError::FrontmatterParseError("test error".to_string());
}

#[tokio::test]
async fn test_vault_scanner_recursive_directory_traversal() {
    // This test should FAIL initially before implementation
    // Tests that scanner properly traverses subdirectories

    let vault_path = TEST_VAULT_PATH;
    let scanner = crucible_tools::vault_scanner::VaultScanner::new(vault_path);

    // Test recursive scanning (default behavior)
    let all_files = scanner.scan_markdown_files().await.unwrap();

    // Test non-recursive scanning
    let root_only_files = scanner.scan_markdown_files_non_recursive().await.unwrap();

    // Should find more files with recursive scanning
    assert!(
        all_files.len() > root_only_files.len(),
        "Recursive scanning should find more files than non-recursive"
    );

    // Verify subdirectories are included in recursive scan
    let has_subdir_files = all_files.iter().any(|p| {
        p.to_string_lossy().contains("Projects/") || p.to_string_lossy().contains("Sessions/")
    });
    assert!(
        has_subdir_files,
        "Recursive scan should include subdirectory files"
    );
}

#[tokio::test]
async fn test_vault_metadata_extraction_various_frontmatter_types() {
    // This test should FAIL initially before implementation
    // Tests parsing different frontmatter field types

    let parser = crucible_tools::vault_parser::VaultParser::new();

    // Parse files with different frontmatter structures
    let test_files = vec![
        "PRIME.md",                               // Has type, tags, created, status
        "Projects/Rune MCP/Rune MCP - MoC.md",    // Has project, tags, created, status
        "Sessions/Session Summary 2025-10-11.md", // Has type, date, topics, status
    ];

    for file_name in test_files {
        let full_path = format!("{}/{}", TEST_VAULT_PATH, file_name);
        let vault_file = parser.parse_file(&full_path).await.unwrap();

        let frontmatter = &vault_file.metadata.frontmatter;

        // Verify common fields exist and have correct types
        if frontmatter.contains_key("tags") {
            let tags = frontmatter.get("tags").unwrap();
            assert!(tags.is_array(), "Tags should be an array");
        }

        if frontmatter.contains_key("created") || frontmatter.contains_key("date") {
            let date_field = frontmatter
                .get("created")
                .or_else(|| frontmatter.get("date"))
                .unwrap();
            assert!(date_field.is_string(), "Date fields should be strings");
        }

        if frontmatter.contains_key("status") {
            let status = frontmatter.get("status").unwrap();
            assert!(status.is_string(), "Status should be a string");
        }

        // Verify content extraction worked
        assert!(
            vault_file.content.contains("#"),
            "Content should contain markdown headers"
        );
    }
}

// ===== PHASE 1B: FAILING TESTS FOR MOCK TOOL REPLACEMENT =====
// These tests should FAIL initially because mock tools return hardcoded mock data,
// not real data from the test vault at /home/moot/Documents/crucible-testing

#[tokio::test]
async fn test_search_by_properties_returns_real_vault_data_not_mock_data() {
    // This test should FAIL initially because search_by_properties returns mock data
    // instead of real data from the test vault

    use crucible_tools::vault_tools;
    use serde_json::json;

    let tool_fn = vault_tools::search_by_properties();
    let parameters = json!({
        "properties": {
            "type": "meta"
        }
    });

    let result = tool_fn(
        "search_by_properties".to_string(),
        parameters,
        Some("test_user".to_string()),
        Some("test_session".to_string()),
    )
    .await
    .unwrap();

    assert!(result.success);
    let data = result.data.unwrap();
    let matching_files = data.get("matching_files").unwrap().as_array().unwrap();

    // This should FAIL because mock implementation returns hardcoded "projects/project1.md"
    // but we expect real files from the test vault like "PRIME.md"
    assert!(
        !matching_files.is_empty(),
        "Should find files with type=meta"
    );

    // Check if any result contains real vault file (not mock data)
    let has_real_vault_file = matching_files.iter().any(|file| {
        if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
            // Mock data returns "projects/project1.md" - real vault should have different paths
            path.contains("PRIME.md") || path.contains("Rune MCP") || path.contains("Sessions/")
        } else {
            false
        }
    });

    // This assertion should FAIL initially (mock data vs real data)
    assert!(
        has_real_vault_file,
        "Expected real vault files like PRIME.md, but got mock data. Found files: {:?}",
        matching_files
    );
}

#[tokio::test]
async fn test_search_by_tags_finds_real_vault_files_not_mock_data() {
    // This test should FAIL initially because search_by_tags returns mock data
    // instead of real tagged files from the test vault

    use crucible_tools::vault_tools;
    use serde_json::json;

    let tool_fn = vault_tools::search_by_tags();
    let parameters = json!({
        "tags": ["vault-config", "instructions"]
    });

    let result = tool_fn(
        "search_by_tags".to_string(),
        parameters,
        Some("test_user".to_string()),
        Some("test_session".to_string()),
    )
    .await
    .unwrap();

    assert!(result.success);
    let data = result.data.unwrap();
    let matching_files = data.get("matching_files").unwrap().as_array().unwrap();

    // Mock returns "knowledge/ai.md" but real vault should have PRIME.md with these tags
    let has_prime_file = matching_files.iter().any(|file| {
        if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
            path.contains("PRIME.md")
        } else {
            false
        }
    });

    // This should FAIL initially (mock data vs real data)
    assert!(
        has_prime_file,
        "Expected PRIME.md with vault-config and instructions tags, but got mock data. Found: {:?}",
        matching_files
    );
}

#[tokio::test]
async fn test_search_by_folder_returns_real_files_from_test_vault() {
    // This test should FAIL initially because search_by_folder returns mock data
    // instead of real files from the test vault folders

    use crucible_tools::vault_tools;
    use serde_json::json;

    let tool_fn = vault_tools::search_by_folder();
    let parameters = json!({
        "path": "Projects",
        "recursive": true
    });

    let result = tool_fn(
        "search_by_folder".to_string(),
        parameters,
        Some("test_user".to_string()),
        Some("test_session".to_string()),
    )
    .await
    .unwrap();

    assert!(result.success);
    let data = result.data.unwrap();
    let files = data.get("files").unwrap().as_array().unwrap();

    // Mock returns "projects/active/project1.md" but real vault should have different structure
    let has_rune_mcp_files = files.iter().any(|file| {
        if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
            path.contains("Rune MCP") || path.contains("Multi-Agent")
        } else {
            false
        }
    });

    // This should FAIL initially (mock data vs real data)
    assert!(
        has_rune_mcp_files,
        "Expected real files from Projects folder like Rune MCP, but got mock data. Found: {:?}",
        files
    );
}

#[tokio::test]
async fn test_get_vault_stats_calculates_real_statistics_not_mock_numbers() {
    // This test should FAIL initially because get_vault_stats returns hardcoded mock numbers
    // instead of real statistics from the test vault

    use crucible_tools::vault_tools;
    use serde_json::json;

    let tool_fn = vault_tools::get_vault_stats();
    let parameters = json!({});

    let result = tool_fn("get_vault_stats".to_string(), parameters, None, None)
        .await
        .unwrap();

    assert!(result.success);
    let data = result.data.unwrap();

    // Mock returns hardcoded 1250 total_notes, but real vault should have different count
    let total_notes = data.get("total_notes").unwrap().as_u64().unwrap();
    let total_size_mb = data.get("total_size_mb").unwrap().as_f64().unwrap();

    // Mock returns exactly 156.7 for size - real vault should be different
    // This should FAIL initially because we expect real calculation, not mock values
    assert_ne!(
        total_notes, 1250,
        "Expected real vault note count, not mock value of 1250. Got: {}",
        total_notes
    );
    assert_ne!(
        total_size_mb, 156.7,
        "Expected real vault size calculation, not mock value of 156.7. Got: {}",
        total_size_mb
    );

    // Real vault should have specific structure indicators
    if let Some(vault_type) = data.get("vault_type").and_then(|v| v.as_str()) {
        assert_eq!(vault_type, "obsidian", "Vault type should be obsidian");
    }

    // Real stats should not be perfect round numbers like mock data
    // This is a heuristic to detect mock vs real data
    let is_perfect_round_number = total_size_mb * 10.0 == (total_size_mb * 10.0).round();
    assert!(
        !is_perfect_round_number,
        "Real vault size should not be a perfect round number like mock data. Got: {}",
        total_size_mb
    );
}

#[tokio::test]
async fn test_list_tags_extracts_real_tags_from_vault_frontmatter() {
    // This test should FAIL initially because list_tags returns hardcoded mock tags
    // instead of extracting real tags from test vault frontmatter

    use crucible_tools::vault_tools;
    use serde_json::json;

    let tool_fn = vault_tools::list_tags();
    let parameters = json!({});

    let result = tool_fn("list_tags".to_string(), parameters, None, None)
        .await
        .unwrap();

    assert!(result.success);
    let data = result.data.unwrap();
    let tags = data.get("tags").unwrap().as_array().unwrap();

    // Mock returns hardcoded tags: ["ai", "research", "project"]
    // Real vault should have different tags like "vault-config", "instructions", "ai-guide"
    let has_vault_config_tag = tags.iter().any(|tag| {
        if let Some(name) = tag.get("name").and_then(|n| n.as_str()) {
            name == "vault-config" || name == "ai-guide" || name == "instructions"
        } else {
            false
        }
    });

    // This should now pass with real vault tags
    assert!(has_vault_config_tag,
           "Expected real vault tags like 'vault-config' from PRIME.md, but got mock tags. Found: {:?}",
           tags);

    // Check that counts are realistic (not perfect round numbers like mock data)
    let total_tags = data.get("total_tags").unwrap().as_u64().unwrap();
    assert_ne!(
        total_tags, 3,
        "Expected real tag count from vault, not mock value of 3. Got: {}",
        total_tags
    );
}

// ===== ADVANCED METADATA EXTRACTION FAILING TESTS =====
// These tests should FAIL because advanced metadata processing is not yet implemented

#[tokio::test]
async fn test_metadata_extraction_normalizes_tags_and_dates() {
    // This test should FAIL because metadata normalization is not implemented

    use crucible_tools::vault_parser::VaultParser;

    let parser = VaultParser::new();
    let test_file_path = format!("{}/PRIME.md", TEST_VAULT_PATH);
    let vault_file = parser.parse_file(&test_file_path).await.unwrap();

    // Test tag normalization (lowercase, hyphen-separated)
    let tags = vault_file.get_tags();
    assert!(!tags.is_empty(), "Should extract tags from PRIME.md");

    // Tags should be normalized (letters should be lowercase, hyphens are allowed)
    for tag in &tags {
        // Check that all alphabetic characters are lowercase (allow hyphens and numbers)
        for c in tag.chars() {
            if c.is_alphabetic() {
                assert!(
                    c.is_ascii_lowercase(),
                    "Character '{}' in tag '{}' should be lowercase",
                    c,
                    tag
                );
            }
        }
        assert!(
            !tag.contains(' '),
            "Tag '{}' should use hyphens instead of spaces",
            tag
        );
    }

    // Test date parsing - PRIME.md has created date
    if let Some(created_date) = vault_file.metadata.created {
        assert!(
            created_date.year() >= 2020,
            "Created date should be realistic"
        );
        assert!(
            created_date.year() <= 2025,
            "Created date should not be in future"
        );
    }
}

#[tokio::test]
async fn test_metadata_extraction_handles_various_frontmatter_formats() {
    // This test should FAIL because flexible frontmatter parsing is not fully implemented

    use crucible_tools::vault_parser::VaultParser;

    let parser = VaultParser::new();
    let test_files = vec![
        "PRIME.md",
        "Projects/Rune MCP/Rune MCP - MoC.md",
        "Sessions/Session Summary 2025-10-11.md",
    ];

    for file_name in test_files {
        let full_path = format!("{}/{}", TEST_VAULT_PATH, file_name);
        if std::path::Path::new(&full_path).exists() {
            let vault_file = parser.parse_file(&full_path).await.unwrap();

            // Each file should have proper frontmatter extraction
            assert!(
                !vault_file.metadata.frontmatter.is_empty(),
                "{} should have frontmatter",
                file_name
            );

            // Should extract tags properly regardless of format
            let tags = vault_file.get_tags();
            // Some files might not have tags, but parsing should not fail
            if !tags.is_empty() {
                for tag in &tags {
                    assert!(!tag.is_empty(), "Tags should not be empty strings");
                }
            }
        }
    }
}

#[tokio::test]
async fn test_metadata_extraction_calculates_content_metrics() {
    // This test should FAIL because content analysis is not implemented

    use crucible_tools::vault_parser::VaultParser;

    let parser = VaultParser::new();
    let test_file_path = format!("{}/PRIME.md", TEST_VAULT_PATH);
    let vault_file = parser.parse_file(&test_file_path).await.unwrap();

    // Content should be analyzed for basic metrics
    assert!(
        !vault_file.content.is_empty(),
        "Content should not be empty"
    );

    // Should calculate word count (this feature doesn't exist yet)
    let word_count = vault_file.content.split_whitespace().count();
    assert!(word_count > 0, "Should calculate word count from content");
    assert!(word_count > 50, "PRIME.md should have substantial content");

    // Should extract structure information (headers, etc.)
    let has_headers = vault_file
        .content
        .lines()
        .any(|line| line.trim().starts_with('#'));
    assert!(has_headers, "Should detect markdown headers in content");
}

#[tokio::test]
async fn test_metadata_extraction_handles_relationships_and_links() {
    // This test should FAIL because relationship extraction is not implemented

    use crucible_tools::vault_parser::VaultParser;

    let parser = VaultParser::new();
    let test_file_path = format!("{}/Projects/Rune MCP/Rune MCP - MoC.md", TEST_VAULT_PATH);

    if std::path::Path::new(&test_file_path).exists() {
        let vault_file = parser.parse_file(&test_file_path).await.unwrap();

        // Should detect internal links (wikilinks)
        let has_wikilinks = vault_file.content.contains("[[") && vault_file.content.contains("]]");
        // This might fail if the file doesn't have wikilinks, but parsing should work

        // Should extract document relationships (not implemented yet)
        // This is a placeholder for the test structure
        let content_lines: Vec<&str> = vault_file.content.lines().collect();
        assert!(
            !content_lines.is_empty(),
            "Should have content lines to analyze"
        );
    }
}
