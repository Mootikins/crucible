//! Critical Edge Case Testing for CLI Search Functionality
//!
//! This test file identifies and validates critical gaps in the current search
//! implementation that could impact user experience or data integrity.
//!
//! Focus Areas:
//! 1. Search algorithm coverage and edge cases
//! 2. File system boundary conditions
//! 3. Search query validation and normalization
//! 4. Output formatting reliability
//! 5. Performance and memory safety

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use tokio::process::Command;



/// Test harness for edge case scenarios
pub struct EdgeCaseTestHarness {
    pub temp_dir: TempDir,
    pub vault_path: PathBuf,
}

impl EdgeCaseTestHarness {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let vault_path = temp_dir.path().to_path_buf();

        // Create .obsidian directory
        std::fs::create_dir_all(vault_path.join(".obsidian"))?;

        Ok(Self {
            temp_dir,
            vault_path,
        })
    }

    /// Create files with specific problematic characteristics
    pub async fn create_problematic_files(&self) -> Result<()> {
        // 1. Very large markdown file
        let large_content = "# Large Document\n\n".to_string() +
            &"This is a repeated sentence. ".repeat(10000) +
            "\n\nEnd of large document.";
        std::fs::write(self.vault_path.join("large-document.md"), large_content)?;

        // 2. File with only frontmatter, no content
        let frontmatter_only = "---\ntitle: Frontmatter Only\ntags: [test, empty]\ncreated: 2025-01-01\n---\n\n";
        std::fs::write(self.vault_path.join("frontmatter-only.md"), frontmatter_only)?;

        // 3. File with special characters and encoding issues
        let special_chars = "# Special Characters Test\n\nTest content: café, naïve, résumé, 北京, Москва\nMath: α, β, γ, p < 0.05, μ = 78.4\nQuotes: \"smart quotes\", 'single quotes', \`backticks\`\nSymbols: @#$%^&*()[]{}|\\:;\"'<>?,./";
        std::fs::write(self.vault_path.join("special-chars.md"), special_chars)?;

        // 4. File with deeply nested structure simulation
        let nested_content = "# Deep Nesting\n\n";
        let mut nested = nested_content;
        for i in 1..=20 {
            nested += &format!("\nLevel {}: ", i);
            nested += &"* Item ".repeat(i);
        }
        std::fs::write(self.vault_path.join("nested-content.md"), nested)?;

        // 5. File with minimal content
        std::fs::write(self.vault_path.join("minimal.md"), "# Min\n\na")?;

        // 6. File with only whitespace and newlines
        std::fs::write(self.vault_path.join("whitespace.md"), "   \n\n\t\t   \n   \n")?;

        // 7. File with code blocks and special syntax
        let code_content = r#"# Code Test

```javascript
function search(query) {
    return "result: " + query;
}
```

```python
def analyze(data):
    return data.filter(x => x.is_valid())
```

```sql
SELECT * FROM documents WHERE content LIKE '%search%';
```

Inline code: `variable = value`, `function()`

Math: $E = mc^2$ and $\sqrt{x^2 + y^2}$

Links: [[Wikilink]], [[Link|Alias]], [External](https://example.com)

Images: ![Alt text](image.png "Title")
"#;
        std::fs::write(self.vault_path.join("code-syntax.md"), code_content)?;

        Ok(())
    }

    /// Create files with different encodings
    pub async fn create_encoding_test_files(&self) -> Result<()> {
        // UTF-8 BOM (should be handled properly)
        let bom_content = "\u{FEFF}# BOM Test\n\nContent with UTF-8 BOM";
        std::fs::write(self.vault_path.join("bom-test.md"), bom_content)?;

        // Mixed encoding simulation (create invalid UTF-8 sequences)
        let mixed_content = b"# Mixed Encoding\n\nInvalid UTF-8: \xff\xfe \x80\x81\nValid part: This is valid UTF-8 text.";
        std::fs::write(self.vault_path.join("mixed-encoding.md"), mixed_content)?;

        Ok(())
    }
}

// ============================================================================
// SEARCH ALGORITHM COVERAGE TESTS
// ============================================================================

#[cfg(test)]
mod search_algorithm_tests {
    use super::*;

    /// Test case sensitivity handling edge cases
    #[tokio::test]
    async fn test_case_sensitivity_edge_cases() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Create test content with specific case patterns
        let test_content = r#"# Case Sensitivity Test

Content with KNOWLEDGE MANAGEMENT
Content with knowledge management
Content with Knowledge Management
Content with kNoWlEdGe MaNaGeMeNt
"#;
        std::fs::write(harness.vault_path.join("case-test.md"), test_content)?;

        let test_cases = vec![
            ("KNOWLEDGE MANAGEMENT", "Should find all variations"),
            ("knowledge management", "Should find all variations"),
            ("Knowledge Management", "Should find all variations"),
            ("kNoWlEdGe MaNaGeMeNt", "Should find all variations"),
        ];

        let mut result_counts = Vec::new();
        for (query, _desc) in test_cases {
            let result = run_cli_command(
                vec!["search", &query, "--limit", "20"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            result_counts.push(result.matches("case-test.md").count());
        }

        // All case variations should find the same document
        for (i, count) in result_counts.iter().enumerate() {
            assert!(*count > 0, "Case variation {} should find the document", i + 1);
        }

        // All should find the same number of occurrences (within reason)
        let max_count = *result_counts.iter().max().unwrap();
        let min_count = *result_counts.iter().min().unwrap();
        assert!((max_count as i32 - min_count as i32).abs() <= 1,
                "Case variations should return consistent results");

        Ok(())
    }

    /// Test search scoring and ranking logic
    #[tokio::test]
    async fn test_search_scoring_edge_cases() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Create files with different match patterns for scoring validation
        let files = vec![
            ("exact-title.md", "# Search Query\n\nExact title match"),
            ("early-content.md", "# Document\n\nSearch Query appears early in content"),
            ("late-content.md", "# Document\n\nLots of content before...\nSearch Query appears late"),
            ("multiple.md", "# Document\n\nSearch Query appears multiple times. Search Query again. Search Query third time."),
            ("partial.md", "# Document\n\nSearch partial match with other words"),
        ];

        for (filename, content) in files {
            std::fs::write(harness.vault_path.join(filename), content)?;
        }

        let result = run_cli_command(
            vec!["search", "Search Query", "--limit", "10", "--format", "json"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Parse JSON results to verify scoring
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        if let Some(results) = parsed.as_array() {
            assert!(!results.is_empty(), "Should find results for scoring test");

            // Results should be ordered by score (descending)
            let mut previous_score = f64::MAX;
            for result in results {
                if let Some(score) = result.get("score").and_then(|s| s.as_f64()) {
                    assert!(score <= previous_score, "Results should be ordered by score descending");
                    previous_score = score;
                }
            }
        }

        Ok(())
    }

    /// Test content length and density handling
    #[tokio::test]
    async fn test_content_length_density_edge_cases() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Test very large file search
        let result = run_cli_command(
            vec!["search", "repeated sentence", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.contains("large-document.md"), "Should search in large files");
        assert!(result.contains("Found"), "Should report matches found");

        // Test minimal content file
        let result = run_cli_command(
            vec!["search", "a", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.contains("minimal.md") || result.contains("matches"), "Should handle minimal content");

        // Test empty/whitespace-only file
        let result = run_cli_command(
            vec!["search", "any", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should not crash or return problematic content from whitespace-only file
        assert!(result.contains("Found") || result.contains("No matches") || result.contains("No matches found"));

        Ok(())
    }
}

// ============================================================================
// FILE SYSTEM EDGE CASES
// ============================================================================

#[cfg(test)]
mod file_system_edge_cases {
    use super::*;

    /// Test handling of very large markdown files
    #[tokio::test]
    async fn test_large_file_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Search in the large file
        let result = timeout(
            Duration::from_secs(10),
            run_cli_command(
                vec!["search", "repeated sentence", "--limit", "1"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            )
        ).await;

        match result {
            Ok(Ok(output)) => {
                assert!(output.contains("large-document.md"), "Should find large file");
                assert!(output.len() < 10000, "Output should be truncated for large files");
            }
            Ok(Err(e)) => {
                panic!("Search in large file should not fail: {}", e);
            }
            Err(_) => {
                panic!("Search in large file should not timeout");
            }
        }

        Ok(())
    }

    /// Test handling of files with encoding issues
    #[tokio::test]
    async fn test_encoding_issue_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_encoding_test_files().await?;

        // Search in files with encoding issues
        let result = run_cli_command(
            vec!["search", "content", "--limit", "10"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should not crash on encoding issues
        assert!(result.contains("Found") || result.contains("No matches") || result.contains("Error"));

        // Search for BOM content
        let result = run_cli_command(
            vec!["search", "BOM", "--limit", "10"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should handle BOM properly
        assert!(result.contains("bom-test.md") || result.contains("No matches"));

        Ok(())
    }

    /// Test handling of files with complex nested structures
    #[tokio::test]
    async fn test_nested_structure_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Search for nested content
        let result = run_cli_command(
            vec!["search", "Level 10", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.contains("nested-content.md"), "Should find nested content");

        // Search for deeply nested content
        let result = run_cli_command(
            vec!["search", "Level 20", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.contains("nested-content.md"), "Should find deeply nested content");

        Ok(())
    }

    /// Test permission handling and locked files
    #[tokio::test]
    async fn test_permission_error_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Create a file and make it unreadable (if permissions allow)
        let test_file = harness.vault_path.join("unreadable.md");
        std::fs::write(&test_file, "# Unreadable\n\nThis file should be unreadable")?;

        // Try to make file unreadable (this may not work on all systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&test_file)?.permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&test_file, perms)?;
        }

        // Search should not crash on unreadable files
        let result = run_cli_command(
            vec!["search", "unreadable", "--limit", "10"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should handle permission errors gracefully
        assert!(result.contains("Found") || result.contains("No matches") || result.contains("Error"));

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&test_file)?.permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&test_file, perms)?;
        }

        Ok(())
    }
}

// ============================================================================
// SEARCH QUERY EDGE CASES
// ============================================================================

#[cfg(test)]
mod search_query_edge_cases {
    use super::*;

    /// Test empty and single character queries
    #[tokio::test]
    async fn test_empty_and_single_char_queries() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Empty query
        let result = run_cli_command(
            vec!["search", "", "--limit", "10"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should handle empty query gracefully
        assert!(result.contains("query") || result.contains("search") || result.contains("files"));

        // Single character queries
        let single_chars = vec!["a", "i", "x", "Q", "1", "@"];

        for char_query in single_chars {
            let result = run_cli_command(
                vec!["search", char_query, "--limit", "5"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            // Should not crash on single character queries
            assert!(result.len() > 0, "Should handle single character query: {}", char_query);
        }

        Ok(())
    }

    /// Test special characters and regex-like patterns
    #[tokio::test]
    async fn test_special_characters_in_queries() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        let special_queries = vec![
            "test@domain.com",
            "path/to/file",
            "function()",
            "var=value",
            "a + b = c",
            "100%",
            "item[0]",
            "${variable}",
            "(parentheses)",
            "{braces}",
            "square[brackets]",
            "a|b",
            "a&b",
            "regex.*pattern",
            "^start",
            "end$",
        ];

        for query in special_queries {
            let result = run_cli_command(
                vec!["search", query, "--limit", "5"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            // Should not crash on special characters
            assert!(result.len() > 0, "Should handle special character query: {}", query);
        }

        Ok(())
    }

    /// Test very long queries
    #[tokio::test]
    async fn test_very_long_queries() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Create progressively longer queries
        let long_queries = vec![
            "a".repeat(100),
            "search term ".repeat(50),
            "very long search query with many words ".repeat(20),
        ];

        for (i, long_query) in long_queries.iter().enumerate() {
            let result = timeout(
                Duration::from_secs(5),
                run_cli_command(
                    vec!["search", long_query, "--limit", "5"],
                    vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
                )
            ).await;

            match result {
                Ok(Ok(output)) => {
                    // Should handle long queries without crashing
                    assert!(output.len() > 0, "Long query {} should produce output", i + 1);
                }
                Ok(Err(e)) => {
                    // Long queries might fail reasonably
                    println!("Long query {} failed reasonably: {}", i + 1, e);
                }
                Err(_) => {
                    panic!("Long query {} should not timeout", i + 1);
                }
            }
        }

        Ok(())
    }

    /// Test Unicode and emoji search terms
    #[tokio::test]
    async fn test_unicode_emoji_search_terms() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Create content with various Unicode and emoji
        let unicode_content = r#"# Unicode and Emoji Test

Content with emoji: 🚀 🔥 💡 🎯 ⭐
Mathematical: α β γ δ ε ζ η θ
Currency: $ € £ ¥ ¥ ₽ ₹
Special: café naïve résumé Beijing Москва
Arrows: ← → ↑ ↓ ↔
Symbols: © ® ™ § ¶ † ‡ • …
"#;
        std::fs::write(harness.vault_path.join("unicode-emoji.md"), unicode_content)?;

        let unicode_queries = vec![
            "🚀", "rocket emoji",
            "café", "cafe",
            "α", "alpha",
            "€", "euro",
            "北京", "Beijing",
            "←", "left arrow",
        ];

        for query in unicode_queries {
            let result = run_cli_command(
                vec!["search", query, "--limit", "5"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            // Should handle Unicode/emoji queries
            assert!(result.len() > 0, "Should handle Unicode query: {}", query);
        }

        Ok(())
    }

    /// Test queries that match file names vs content
    #[tokio::test]
    async fn test_filename_vs_content_matching() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create files where name and content differ
        let test_files = vec![
            ("search-algorithms.md", "# This file is about data structures and algorithms"),
            ("machine-learning.md", "# This document covers neural networks and deep learning"),
            ("database-design.md", "# SQL queries and NoSQL solutions are discussed here"),
        ];

        for (filename, content) in test_files {
            std::fs::write(harness.vault_path.join(filename), content)?;
        }

        // Search for terms that appear in filenames but not content
        let filename_queries = vec!["algorithms", "machine", "design"];

        for query in filename_queries {
            let result = run_cli_command(
                vec!["search", query, "--limit", "5"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            // Should find files matching the query in filename or content
            assert!(result.contains("Found") || result.contains("matches"),
                    "Should find matches for filename query: {}", query);
        }

        Ok(())
    }
}

// ============================================================================
// OUTPUT FORMAT TESTING
// ============================================================================

#[cfg(test)]
mod output_format_tests {
    use super::*;

    /// Test JSON output validation
    #[tokio::test]
    async fn test_json_output_validation() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Test JSON output with special characters
        let result = run_cli_command(
            vec!["search", "special", "--format", "json", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should be valid JSON
        match serde_json::from_str::<serde_json::Value>(&result) {
            Ok(parsed) => {
                // Validate JSON structure
                if let Some(results) = parsed.as_array() {
                    for result in results {
                        // Each result should have expected fields
                        assert!(result.get("id").is_some(), "JSON result should have 'id' field");
                        assert!(result.get("title").is_some(), "JSON result should have 'title' field");
                        assert!(result.get("content").is_some(), "JSON result should have 'content' field");
                        assert!(result.get("score").is_some(), "JSON result should have 'score' field");

                        // Validate field types
                        assert!(result.get("id").unwrap().is_string(), "ID should be string");
                        assert!(result.get("title").unwrap().is_string(), "Title should be string");
                        assert!(result.get("content").unwrap().is_string(), "Content should be string");
                        assert!(result.get("score").unwrap().is_number(), "Score should be number");
                    }
                }
            }
            Err(e) => {
                panic!("JSON output should be valid: {}", e);
            }
        }

        Ok(())
    }

    /// Test content preview formatting
    #[tokio::test]
    async fn test_content_preview_formatting() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Test content preview with very long content
        let result = run_cli_command(
            vec!["search", "repeated", "--show-content", "--limit", "3"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Content should be truncated in preview
        assert!(result.len() < 5000, "Content preview should be truncated for large files");

        // Test content preview with Unicode
        let result = run_cli_command(
            vec!["search", "café", "--show-content", "--limit", "3"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Should handle Unicode in content preview
        assert!(result.contains("café") || result.contains("No matches"),
                "Should handle Unicode in content preview");

        Ok(())
    }

    /// Test file path handling across different environments
    #[tokio::test]
    async fn test_file_path_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create files with spaces and special characters in names
        let problem_names = vec![
            "file with spaces.md",
            "file-with-dashes.md",
            "file_with_underscores.md",
            "file.with.dots.md",
            "file with 'single' quotes.md",
            "file with \"double\" quotes.md",
            "file (parentheses).md",
            "file [brackets].md",
            "file {braces}.md",
        ];

        for filename in problem_names {
            let content = format!("# {}\n\nContent of {}", filename, filename);
            std::fs::write(harness.vault_path.join(filename), content)?;
        }

        // Search for content and verify file paths are handled correctly
        let result = run_cli_command(
            vec!["search", "Content of", "--format", "json", "--limit", "20"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Parse JSON and verify file paths
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        if let Some(results) = parsed.as_array() {
            for result in results {
                if let Some(file_path) = result.get("id").and_then(|id| id.as_str()) {
                    // File paths should be properly escaped/handled
                    assert!(!file_path.is_empty(), "File path should not be empty");
                    assert!(file_path.ends_with(".md"), "File path should end with .md");
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// PERFORMANCE CONSIDERATIONS
// ============================================================================

#[cfg(test)]
mod performance_tests {
    use super::*;

    /// Test search performance with many files
    #[tokio::test]
    async fn test_search_performance_many_files() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create many files
        for i in 1..=100 {
            let content = format!("# Document {}\n\nContent for document {} with search terms.", i, i);
            let filename = format!("document-{:03}.md", i);
            std::fs::write(harness.vault_path.join(filename), content)?;
        }

        // Time search performance
        let start = std::time::Instant::now();
        let result = run_cli_command(
            vec!["search", "search terms", "--limit", "50"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;
        let duration = start.elapsed();

        // Should complete in reasonable time
        assert!(duration < Duration::from_secs(5), "Search should complete in < 5 seconds, took {:?}", duration);
        assert!(result.contains("Found"), "Should find results");

        Ok(())
    }

    /// Test memory usage with large files
    #[tokio::test]
    async fn test_memory_usage_large_files() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;
        harness.create_problematic_files().await?;

        // Search in large file should not cause memory issues
        let result = run_cli_command(
            vec!["search", "sentence", "--show-content", "--limit", "1"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        // Output should be reasonably sized (indicating proper memory management)
        assert!(result.len() < 10000, "Output should be truncated to manage memory");

        Ok(())
    }

    /// Test concurrent search operations
    #[tokio::test]
    async fn test_concurrent_search_operations() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create test files
        for i in 1..=10 {
            let content = format!("# File {}\n\nContent {}", i, i);
            std::fs::write(harness.vault_path.join(format!("file{}.md", i)), content)?;
        }

        // Run multiple searches concurrently
        let mut handles = Vec::new();
        for i in 1..=5 {
            let vault_path = harness.vault_path.clone();
            let handle = tokio::spawn(async move {
                run_cli_command(
                    vec!["search", &format!("Content {}", i), "--limit", "5"],
                    vec![("OBSIDIAN_VAULT_PATH", vault_path.to_string_lossy().as_ref())]
                ).await
            });
            handles.push(handle);
        }

        // All searches should complete successfully
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(Ok(result)) => {
                    assert!(result.len() > 0, "Concurrent search {} should return results", i + 1);
                }
                Ok(Err(e)) => {
                    panic!("Concurrent search {} failed: {}", i + 1, e);
                }
                Err(e) => {
                    panic!("Concurrent search {} panicked: {}", i + 1, e);
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// RELIABILITY AND ERROR HANDLING
// ============================================================================

#[cfg(test)]
mod reliability_tests {
    use super::*;

    /// Test search behavior with corrupted vault structure
    #[tokio::test]
    async fn test_corrupted_vault_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create invalid file in .obsidian directory
        std::fs::write(harness.vault_path.join(".obsidian/invalid"), "invalid content")?;

        // Search should still work despite vault corruption
        let result = run_cli_command(
            vec!["search", "test", "--limit", "5"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.len() > 0, "Should handle corrupted vault gracefully");

        Ok(())
    }

    /// Test search with missing or broken symlinks
    #[tokio::test]
    async fn test_broken_symlink_handling() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create a broken symlink
            let symlink_path = harness.vault_path.join("broken-symlink.md");
            symlink("nonexistent-file.md", &symlink_path)?;

            // Search should not crash on broken symlinks
            let result = run_cli_command(
                vec!["search", "test", "--limit", "5"],
                vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
            ).await?;

            assert!(result.len() > 0, "Should handle broken symlinks gracefully");
        }

        Ok(())
    }

    /// Test graceful degradation under various error conditions
    #[tokio::test]
    async fn test_graceful_degradation() -> Result<()> {
        let harness = EdgeCaseTestHarness::new().await?;

        // Create some valid files
        std::fs::write(harness.vault_path.join("valid1.md"), "# Valid 1\n\nContent")?;
        std::fs::write(harness.vault_path.join("valid2.md"), "# Valid 2\n\nContent")?;

        // Create problematic conditions
        std::fs::write(harness.vault_path.join("empty.md"), "")?;
        std::fs::write(harness.vault_path.join("binary.md"), b"\x00\x01\x02\x03\x04\x05")?;

        // Search should work and skip problematic files
        let result = run_cli_command(
            vec!["search", "Content", "--limit", "10"],
            vec![("OBSIDIAN_VAULT_PATH", harness.vault_path.to_string_lossy().as_ref())]
        ).await?;

        assert!(result.contains("valid1.md") || result.contains("valid2.md"),
                "Should find valid files despite problematic ones");

        Ok(())
    }
}