//! TDD Tests for Interactive Fuzzy Search
//!
//! This test module follows Test-Driven Development principles.
//! Tests are written BEFORE implementation and guide the development process.

mod common;

use anyhow::Result;
use crucible_core::test_support::{create_kiln_with_files, kiln_path_str};
use tempfile::TempDir;

/// Helper function to create a test kiln with specific files for fuzzy search testing.
fn create_fuzzy_test_kiln() -> Result<TempDir> {
    create_kiln_with_files(&[
        ("note1.md", "# Note 1\n\nThis is the first note about Rust programming."),
        ("note2.md", "# Note 2\n\nAnother note discussing algorithms."),
        ("todo.md", "# TODO List\n\n- Task 1\n- Task 2"),
        ("project/design.md", "# Design Document\n\nSystem architecture with Rust."),
        ("project/implementation.md", "# Implementation\n\nCoding details here."),
    ])
}

// ============================================================================
// Phase 3: TDD Cycle 1 - Basic File Listing Tests
// ============================================================================

/// Test: Picker should list all markdown files in the kiln
/// Expected: PASS after basic implementation
#[tokio::test]
async fn test_basic_file_listing() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Expected behavior:
    // 1. Call list_files_in_kiln with kiln path
    // 2. Should discover all 5 .md files
    // 3. Files should be returned as a list

    let files = fuzzy_interactive::list_files_in_kiln(kiln_path)
        .expect("should list files successfully");

    // We created 5 markdown files in create_fuzzy_test_kiln
    assert_eq!(files.len(), 5, "should find exactly 5 markdown files");

    // Verify expected files are present (order may vary)
    let file_names: Vec<String> = files
        .iter()
        .map(|f| f.split('/').last().unwrap_or(f).to_string())
        .collect();

    assert!(file_names.contains(&"note1.md".to_string()));
    assert!(file_names.contains(&"note2.md".to_string()));
    assert!(file_names.contains(&"todo.md".to_string()));
    assert!(file_names.contains(&"design.md".to_string()));
    assert!(file_names.contains(&"implementation.md".to_string()));

    // TDD: Verify all paths are relative (don't start with '/')
    for file in &files {
        assert!(
            !file.starts_with('/'),
            "Path should be relative, but got: {}",
            file
        );
    }

    // TDD: Verify paths don't contain the absolute kiln directory path
    let kiln_path_str = kiln_path.to_string_lossy();
    for file in &files {
        assert!(
            !file.contains(kiln_path_str.as_ref()),
            "Path should not contain kiln directory, but got: {}",
            file
        );
    }
}

/// Test: Verify that listed paths are relative to kiln root
/// This test ensures privacy and correct fuzzy matching on file structure
#[tokio::test]
async fn test_paths_are_relative() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    let files = fuzzy_interactive::list_files_in_kiln(kiln_path)
        .expect("should list files successfully");

    // All paths should be relative (not absolute)
    for file in &files {
        assert!(!file.starts_with('/'), "Path '{}' should be relative", file);
        assert!(!file.starts_with('\\'), "Path '{}' should be relative (Windows)", file);

        // Verify we can reconstruct full path and it exists
        let full_path = kiln_path.join(file);
        assert!(
            full_path.exists(),
            "Reconstructed path should exist: {:?} (from relative: {})",
            full_path,
            file
        );
    }

    // Verify nested paths work correctly (should have project/design.md format)
    let nested = files.iter().find(|f| f.contains('/'));
    assert!(
        nested.is_some(),
        "Should have at least one nested file path like 'project/design.md'"
    );

    // Verify the nested path format is correct
    if let Some(nested_file) = nested {
        assert!(
            nested_file.starts_with("project/"),
            "Nested file should start with 'project/', got: {}",
            nested_file
        );
    }
}

/// Test: Picker initialization with empty kiln should not panic
/// Expected: PASS - should return empty list gracefully
#[tokio::test]
async fn test_empty_kiln_initialization() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_kiln_with_files(&[]).unwrap();
    let kiln_path = kiln.path();

    // Expected behavior:
    // 1. Should not panic with empty kiln
    // 2. Returns empty list
    // 3. No errors

    let files = fuzzy_interactive::list_files_in_kiln(kiln_path)
        .expect("should handle empty kiln without error");

    assert_eq!(files.len(), 0, "empty kiln should return zero files");
}

/// Test: Filter files with an initial query
/// Expected: PASS after filtering implementation
#[tokio::test]
async fn test_filter_with_initial_query() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Expected behavior:
    // 1. Filter files with query "note"
    // 2. Should match note1.md and note2.md only
    // 3. Uses fuzzy matching

    let files = fuzzy_interactive::filter_files_by_query(kiln_path, "note")
        .expect("should filter files successfully");

    // Should find 2 files matching "note"
    assert_eq!(files.len(), 2, "should find 2 files matching 'note'");

    let file_names: Vec<String> = files
        .iter()
        .map(|f| f.split('/').last().unwrap_or(f).to_string())
        .collect();

    assert!(file_names.contains(&"note1.md".to_string()));
    assert!(file_names.contains(&"note2.md".to_string()));
}

// ============================================================================
// Phase 4: TDD Cycle 2 - Filename Filtering Tests
// ============================================================================

/// Test: Filter files by different queries
/// Expected: PASS after filtering implementation
#[tokio::test]
async fn test_filter_by_different_queries() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Test 1: Filter by "todo"
    let files = fuzzy_interactive::filter_files_by_query(kiln_path, "todo")
        .expect("should filter by 'todo'");
    assert_eq!(files.len(), 1, "should find 1 file matching 'todo'");
    assert!(files[0].contains("todo.md"));

    // Test 2: Filter by "design"
    let files = fuzzy_interactive::filter_files_by_query(kiln_path, "design")
        .expect("should filter by 'design'");
    assert_eq!(files.len(), 1, "should find 1 file matching 'design'");
    assert!(files[0].contains("design.md"));
}

/// Test: Fuzzy matching works (non-exact substring)
/// Expected: PASS with nucleo-matcher fuzzy matching
#[tokio::test]
async fn test_fuzzy_matching() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Test fuzzy matching: "dsn" should match "design.md"
    let files = fuzzy_interactive::filter_files_by_query(kiln_path, "dsn")
        .expect("should handle fuzzy query");

    // Should match design.md via fuzzy matching
    assert!(!files.is_empty(), "fuzzy query 'dsn' should match something");
    assert!(
        files.iter().any(|f| f.contains("design.md")),
        "should fuzzy match design.md with query 'dsn'"
    );

    // Test fuzzy matching: "impl" should match "implementation.md"
    let files = fuzzy_interactive::filter_files_by_query(kiln_path, "impl")
        .expect("should handle fuzzy query");

    assert!(
        files.iter().any(|f| f.contains("implementation.md")),
        "should fuzzy match implementation.md with query 'impl'"
    );
}

// ============================================================================
// Phase 5: TDD Cycle 3 - Content Search Tests
// ============================================================================

/// Test: Search file contents for query
/// Expected: PASS after content search implementation
#[tokio::test]
async fn test_search_file_contents() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Expected behavior:
    // 1. Query "Rust" should match note1.md and project/design.md
    // 2. Content is searched, not just filenames
    // 3. Results show files containing the query text

    let results = fuzzy_interactive::search_files_by_content(kiln_path, "Rust")
        .expect("should search file contents");

    // Should find 2 files containing "Rust"
    // note1.md: "This is the first note about Rust programming."
    // project/design.md: "System architecture with Rust."
    assert_eq!(results.len(), 2, "should find 2 files containing 'Rust'");

    let file_names: Vec<String> = results
        .iter()
        .map(|r| r.path.split('/').last().unwrap_or(&r.path).to_string())
        .collect();

    assert!(file_names.contains(&"note1.md".to_string()));
    assert!(file_names.contains(&"design.md".to_string()));
}

/// Test: Content search shows snippets
/// Expected: PASS with snippet extraction
#[tokio::test]
async fn test_content_search_shows_snippets() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_fuzzy_test_kiln().unwrap();
    let kiln_path = kiln.path();

    // Expected behavior:
    // 1. Query "Rust"
    // 2. Results should include context snippets
    // 3. Snippets show where the match occurred in the file

    let results = fuzzy_interactive::search_files_by_content(kiln_path, "Rust")
        .expect("should search file contents");

    assert!(!results.is_empty(), "should find files containing 'Rust'");

    // Check that snippets are provided
    for result in &results {
        assert!(!result.snippet.is_empty(), "result should have a snippet");
        assert!(
            result.snippet.to_lowercase().contains("rust"),
            "snippet should contain the query term"
        );
    }
}

// ============================================================================
// Phase 6: TDD Cycle 4 - Search Mode Toggle Tests
// ============================================================================

/// Test: Toggle between search modes with Ctrl-M
/// Expected: FAIL (mode toggle not implemented)
#[tokio::test]
#[ignore]
async fn test_search_mode_toggle() {
    let kiln = create_fuzzy_test_kiln().unwrap();
    let _kiln_path = kiln_path_str(kiln.path());

    // Expected behavior:
    // 1. Start in "Both" mode (searches filename and content)
    // 2. Press Ctrl-M → switch to "Filename Only" mode
    // 3. Press Ctrl-M → switch to "Content Only" mode
    // 4. Press Ctrl-M → cycle back to "Both" mode
    // 5. Results update based on current mode

    panic!("Test not yet implemented - awaiting search mode toggle implementation");
}

/// Test: Filename-only mode ignores content matches
/// Expected: FAIL (mode filtering not implemented)
#[tokio::test]
#[ignore]
async fn test_filename_only_mode() {
    let kiln = create_fuzzy_test_kiln().unwrap();
    let _kiln_path = kiln_path_str(kiln.path());

    // Expected behavior:
    // 1. Switch to "Filename Only" mode
    // 2. Query "Rust" should NOT match note1.md (content match)
    // 3. Query "note" SHOULD match note1.md (filename match)

    panic!("Test not yet implemented - awaiting filename-only mode implementation");
}

/// Test: Content-only mode ignores filename matches
/// Expected: FAIL (mode filtering not implemented)
#[tokio::test]
#[ignore]
async fn test_content_only_mode() {
    let kiln = create_fuzzy_test_kiln().unwrap();
    let _kiln_path = kiln_path_str(kiln.path());

    // Expected behavior:
    // 1. Switch to "Content Only" mode
    // 2. Query "note" in content should match note1.md if "note" appears in content
    // 3. Query "todo" should NOT match todo.md if only in filename

    panic!("Test not yet implemented - awaiting content-only mode implementation");
}

// ============================================================================
// Phase 7: TDD Cycle 5 - Multi-Select Tests
// ============================================================================
// Phase 9: Edge Cases & Error Handling Tests
// ============================================================================

/// Test: Handle binary files gracefully (should skip)
#[tokio::test]
async fn test_binary_files_skipped() {
    use std::fs;
    use crucible_cli::commands::fuzzy_interactive::list_files_in_kiln;

    let kiln = create_kiln_with_files(&[
        ("text.md", "# Text File\n\nRegular markdown."),
    ]).unwrap();

    // Add a binary file with .md extension (edge case)
    let binary_path = kiln.path().join("binary.md");
    fs::write(&binary_path, &[0u8, 1u8, 2u8, 255u8]).unwrap();

    // Add a non-.md binary file (should be skipped by extension filter)
    let bin_path = kiln.path().join("binary.bin");
    fs::write(&bin_path, &[0u8, 1u8, 2u8, 255u8]).unwrap();

    // List files in kiln
    let files = list_files_in_kiln(kiln.path()).unwrap();

    // Verify only text.md appears (binary.md should be filtered by binary detection)
    assert_eq!(files.len(), 1, "Should only find 1 file (text.md)");
    assert!(
        files[0].contains("text.md"),
        "Should find text.md, got: {:?}",
        files
    );

    // Verify binary files are not in the list
    assert!(
        !files.iter().any(|f| f.contains("binary.md")),
        "binary.md should be skipped"
    );
    assert!(
        !files.iter().any(|f| f.contains("binary.bin")),
        "binary.bin should be skipped"
    );
}

/// Test: Handle invalid UTF-8 gracefully
#[tokio::test]
async fn test_invalid_utf8_handling() {
    use std::fs;
    use crucible_cli::commands::fuzzy_interactive::list_files_in_kiln;

    let kiln = create_kiln_with_files(&[
        ("valid.md", "# Valid UTF-8\n\nAll good here."),
    ]).unwrap();

    // Add file with invalid UTF-8 bytes
    let invalid_path = kiln.path().join("invalid.md");
    fs::write(&invalid_path, &[0xFFu8, 0xFEu8, 0xFDu8]).unwrap();

    // List files in kiln - should not panic
    let files = list_files_in_kiln(kiln.path()).unwrap();

    // Verify valid.md appears
    assert!(
        files.iter().any(|f| f.contains("valid.md")),
        "Should find valid.md"
    );

    // Invalid UTF-8 file should be skipped (treated as binary due to invalid bytes)
    // Note: The file listing itself doesn't read content, so invalid.md may appear.
    // The real test is that reading it later won't crash.
    // For now, we just verify no panic occurred during listing.

    // Additional verification: Try to read the invalid file through search
    // This should gracefully handle the error
    use crucible_cli::commands::fuzzy_interactive::search_files_by_content;
    let results = search_files_by_content(kiln.path(), "test");
    assert!(results.is_ok(), "Search should not panic on invalid UTF-8");
}
