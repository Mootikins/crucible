use anyhow::Result;
use crucible_cli::commands::search::{search_files_in_kiln, SearchExecutor};
use std::path::Path;
use tempfile::TempDir;
use std::fs;
use unicode_normalization::UnicodeNormalization;

fn normalize_unicode_for_search(text: &str) -> String {
    // Apply NFC normalization first (canonical composition)
    let nfc_text = text.nfc().collect::<String>();
    // Convert to lowercase for case-insensitive matching
    nfc_text.to_lowercase()
}


/// Test that emoji search works correctly with Unicode normalization
/// This test should FAIL initially and PASS after implementing Unicode fixes
#[tokio::test]
async fn test_emoji_search_failing_without_unicode_normalization() {
    let setup = create_test_kiln_with_emoji_content().await;

    // Test emoji search that should work but currently fails
    let results = search_files_in_kiln(&setup.kiln_path, "🎯", 10, false).unwrap();

    // This assertion will FAIL with current implementation
    // because to_lowercase() doesn't handle emoji properly
    assert!(!results.is_empty(), "Should find files containing emoji 🎯");

    // Verify the emoji file is found
    let emoji_file_found = results.iter().any(|r| r.id.contains("emoji_test"));
    assert!(emoji_file_found, "Should find the emoji_test.md file");

    // Test multiple emoji search
    let results_multiple = search_files_in_kiln(&setup.kiln_path, "😀🔍", 10, false).unwrap();
    assert!(!results_multiple.is_empty(), "Should find files containing multiple emojis 😀🔍");
}

/// Test that accented characters work with Unicode normalization
/// This test should FAIL initially and PASS after implementing Unicode fixes
#[tokio::test]
async fn test_accented_characters_failing_without_unicode_normalization() {
    let setup = create_test_kiln_with_unicode_content().await;

    // Test accented character search
    let results = search_files_in_kiln(&setup.kiln_path, "café", 10, false).unwrap();

    // This assertion will FAIL with current implementation
    // because simple string comparison doesn't handle Unicode normalization
    assert!(!results.is_empty(), "Should find files containing accented characters");

    // Test that composed and decomposed forms match
    let results_composed = search_files_in_kiln(&setup.kiln_path, "é", 10, false).unwrap();
    assert!(!results_composed.is_empty(), "Should find both composed (é) and decomposed forms");
}

/// Test that mixed Unicode content is handled correctly
#[tokio::test]
async fn test_mixed_unicode_content() {
    let setup = create_test_kiln_with_mixed_unicode().await;

    // Test search with mixed emoji and text (use actual pattern from content)
    let results = search_files_in_kiln(&setup.kiln_path, "😀 emoji", 10, false).unwrap();
    assert!(!results.is_empty(), "Should find files with mixed emoji and text content");

    // Test search with international characters
    let results = search_files_in_kiln(&setup.kiln_path, "中文", 10, false).unwrap();
    assert!(!results.is_empty(), "Should find files with Chinese characters");
}

/// Test that Unicode characters are preserved in search results
#[tokio::test]
async fn test_unicode_preservation_in_results() {
    let setup = create_test_kiln_with_emoji_content().await;

    let results = search_files_in_kiln(&setup.kiln_path, "🎯", 10, true).unwrap();

    if let Some(first_result) = results.first() {
        // Verify that Unicode characters are preserved in snippets
        let content_contains_emoji = first_result.content.contains('🎯') ||
                                   first_result.content.contains('😀') ||
                                   first_result.content.contains('🔍');
        assert!(content_contains_emoji, "Unicode characters should be preserved in search results");
    }
}

/// Test that Unicode normalization doesn't break regular ASCII searches
#[tokio::test]
async fn test_ascii_search_still_works() {
    let setup = create_test_kiln_with_ascii_content().await;

    let results = search_files_in_kiln(&setup.kiln_path, "markdown", 10, false).unwrap();
    assert!(!results.is_empty(), "ASCII searches should still work after Unicode implementation");
}

/// Test the SearchExecutor directly with Unicode content
#[tokio::test]
async fn test_search_executor_unicode() {
    let setup = create_test_kiln_with_emoji_content().await;
    let executor = SearchExecutor::new();

    // Test direct executor usage with Unicode
    let results = executor.search_with_query(&setup.kiln_path, "🎯", 10, false).unwrap();

    // This will fail initially due to Unicode handling issues
    assert!(!results.is_empty(), "SearchExecutor should handle Unicode correctly");
}

// Helper functions to create test data

struct TestKilnSetup {
    kiln_path: std::path::PathBuf,
    _temp_dir: TempDir,
}

async fn create_test_kiln_with_emoji_content() -> TestKilnSetup {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path();

    // Create test file with emoji content
    let emoji_content = r#"# Emoji Test File

This file contains various emojis for testing search functionality:

🎯 Target emoji for searching
😀 Smile emoji
🔍 Search emoji
📊 Chart emoji

## Content with emojis

Here's some markdown content with embedded emojis:

- **Important point**: 🎯 This is important!
- **Happy moment**: 😀 We found it!
- **Search process**: 🔍 Looking for content
- **Data visualization**: 📊 Charts and graphs

The search should be able to find this content when searching for emojis.
"#;

    fs::write(kiln_path.join("emoji_test.md"), emoji_content).unwrap();

    // Create another file with different emoji content
    let other_content = r#"# Another Emoji File

This file has different emojis: 🚀🌟💡

Mixed content: React components with 😀 emoji support.

## Multiple Emojis Together
Here are some emojis together: 😀🔍🎯
Search for multiple emojis in sequence.
"#;

    fs::write(kiln_path.join("other_emoji.md"), other_content).unwrap();

    TestKilnSetup {
        kiln_path: kiln_path.to_path_buf(),
        _temp_dir: temp_dir,
    }
}

async fn create_test_kiln_with_unicode_content() -> TestKilnSetup {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path();

    // Create file with accented characters (NFC normalized form)
    let unicode_content = r#"# Accented Characters Test

Café: café in composed form
Résumé: professional document
Naïve: innocent or naive
Éclair: French pastry

## Testing Unicode Normalization

This file tests that searches for both composed (é) and decomposed (e + ´) forms work correctly.
"#;

    fs::write(kiln_path.join("accented_test.md"), unicode_content).unwrap();

    // Create file with decomposed Unicode content (simulate different normalization)
    let decomposed_content = "Cafe\u{0301}: decomposed form\n"; // café in NFD form
    fs::write(kiln_path.join("decomposed_test.md"), decomposed_content).unwrap();

    TestKilnSetup {
        kiln_path: kiln_path.to_path_buf(),
        _temp_dir: temp_dir,
    }
}

async fn create_test_kiln_with_mixed_unicode() -> TestKilnSetup {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path();

    let mixed_content = r#"# Mixed Unicode Content

## English with Emojis
React development with 😀 emoji support! 🎯 Target achieved.

## International Content
中文: Chinese characters for testing
العربية: Arabic text content
España: contenido en español

## Mixed Patterns
- React + 😀 = Happy coding
- 中文 + 🎯 = Chinese target
- العربية + 🔍 = Arabic search
"#;

    fs::write(kiln_path.join("mixed_unicode.md"), mixed_content).unwrap();

    TestKilnSetup {
        kiln_path: kiln_path.to_path_buf(),
        _temp_dir: temp_dir,
    }
}

async fn create_test_kiln_with_ascii_content() -> TestKilnSetup {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path();

    let ascii_content = r#"# ASCII Content Test

This is a regular markdown file with ASCII content only.

## Topics

- Markdown formatting
- Search functionality
- File processing
- Basic text operations

The search should work normally for ASCII content.
"#;

    fs::write(kiln_path.join("ascii_test.md"), ascii_content).unwrap();

    TestKilnSetup {
        kiln_path: kiln_path.to_path_buf(),
        _temp_dir: temp_dir,
    }
}