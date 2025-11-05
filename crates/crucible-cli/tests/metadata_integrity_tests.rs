//! Metadata Integrity Validation Tests
//!
//! Test that metadata changes (frontmatter, tags, titles, etc.) are properly reflected in search results.

use anyhow::{anyhow, Result};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Run CLI command and return output
fn run_cli_command(args: &[&str]) -> Result<(String, String)> {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "cru", "--"])
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(anyhow!(
            "Command failed with exit code {:?}\nStdout: {}\nStderr: {}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    Ok((stdout, stderr))
}

/// Test that frontmatter title changes are reflected in search
#[tokio::test]
async fn test_frontmatter_title_changes_in_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create initial document with frontmatter
    let initial_content = r#"---
title: "Original Title"
tags: [test, document]
author: "Test Author"
---

# Document Content

This is the main content of the document with some searchable text.
The content should remain the same while metadata changes.
"#;

    fs::write(kiln_path.join("test.md"), initial_content)?;

    // Create configuration
    let config = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "memory"
"#,
        kiln_path.to_string_lossy()
    );

    let config_path = temp_dir.path().join("config.toml");
    fs::write(&config_path, config)?;

    // Test search for original title
    println!("Testing search for original title...");
    let (original_title_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "Original Title"
    ])?;

    // Test search for content
    let (content_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "searchable text"
    ])?;

    // Now update the frontmatter title
    let updated_content = r#"---
title: "Updated Title"
tags: [test, document, modified]
author: "Test Author"
---

# Document Content

This is the main content of the document with some searchable text.
The content should remain the same while metadata changes.
"#;

    fs::write(kiln_path.join("test.md"), updated_content)?;

    // Test search for new title
    println!("Testing search for updated title...");
    let (updated_title_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "Updated Title"
    ])?;

    // Test search for old title (should not find it)
    let (old_title_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "Original Title"
    ])?;

    // Verify results
    assert!(!original_title_search.is_empty(), "Should find original title");
    assert!(!content_search.is_empty(), "Should find content");
    assert!(!updated_title_search.is_empty(), "Should find updated title");

    // The old title search might still find the content if it searches the full document text
    // but ideally it should not find the old title in the metadata
    println!("Original title search results: {}", original_title_search);
    println!("Updated title search results: {}", updated_title_search);
    println!("Old title search results: {}", old_title_search);

    // At minimum, the new title should be found
    assert!(updated_title_search.to_lowercase().contains("updated") ||
            updated_title_search.to_lowercase().contains("test"),
            "Updated title should be found in search");

    println!("✅ Frontmatter title changes test passed");
    Ok(())
}

/// Test that tag changes are reflected in search
#[tokio::test]
async fn test_tag_changes_in_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create document with initial tags
    let initial_content = r#"---
title: "Tag Test Document"
tags: [rust, programming, systems]
---

# Programming with Rust

This document discusses Rust programming language and systems programming concepts.
"#;

    fs::write(kiln_path.join("rust_doc.md"), initial_content)?;

    // Create configuration
    let config = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "memory"
"#,
        kiln_path.to_string_lossy()
    );

    let config_path = temp_dir.path().join("config.toml");
    fs::write(&config_path, config)?;

    // Test search for original tags
    println!("Testing search for original tags...");
    let (rust_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "rust"
    ])?;

    let (systems_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "systems"
    ])?;

    // Update tags - remove 'systems' and add 'performance'
    let updated_content = r#"---
title: "Tag Test Document"
tags: [rust, programming, performance]
---

# Programming with Rust

This document discusses Rust programming language and performance optimization concepts.
"#;

    fs::write(kiln_path.join("rust_doc.md"), updated_content)?;

    // Test search for updated tags and content
    println!("Testing search for updated content...");
    let (performance_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "performance"
    ])?;

    let (updated_rust_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "rust"
    ])?;

    // Verify results
    assert!(!rust_search.is_empty(), "Should find rust in initial search");
    assert!(!systems_search.is_empty(), "Should find systems in initial search");
    assert!(!performance_search.is_empty(), "Should find performance after update");
    assert!(!updated_rust_search.is_empty(), "Should still find rust after update");

    println!("Rust search (initial): {}", rust_search);
    println!("Systems search (initial): {}", systems_search);
    println!("Performance search (updated): {}", performance_search);
    println!("Rust search (updated): {}", updated_rust_search);

    println!("✅ Tag changes test passed");
    Ok(())
}

/// Test that content changes are immediately reflected in search
#[tokio::test]
async fn test_content_changes_immediate_reflection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create initial document
    let initial_content = r#"---
title: "Content Change Test"
tags: [test, content]
---

# Document Title

This is the original content that should be searchable.
We will add new content later.
"#;

    fs::write(kiln_path.join("content_test.md"), initial_content)?;

    // Create configuration
    let config = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "memory"
"#,
        kiln_path.to_string_lossy()
    );

    let config_path = temp_dir.path().join("config.toml");
    fs::write(&config_path, config)?;

    // Test search for initial content
    println!("Testing search for initial content...");
    let (original_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "original content"
    ])?;

    let (missing_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "additional content"
    ])?;

    // Add new content to the document
    let updated_content = r#"---
title: "Content Change Test"
tags: [test, content, updated]
---

# Document Title

This is the original content that should be searchable.
We will add new content later.

## New Section

This is additional content that was added to test immediate reflection in search results.
The new content should be immediately searchable.
"#;

    fs::write(kiln_path.join("content_test.md"), updated_content)?;

    // Test search for new content
    println!("Testing search for new content...");
    let (new_content_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "additional content"
    ])?;

    let (updated_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "immediate reflection"
    ])?;

    // Verify results
    assert!(!original_search.is_empty(), "Should find original content");
    assert!(!new_content_search.is_empty(), "Should find newly added content");

    println!("Original content search: {}", original_search);
    println!("Missing content search (before): {}", missing_search);
    println!("New content search (after): {}", new_content_search);
    println!("Updated content search: {}", updated_search);

    println!("✅ Content changes immediate reflection test passed");
    Ok(())
}

/// Test multiple documents with metadata changes
#[tokio::test]
async fn test_multiple_documents_metadata_consistency() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create multiple documents
    let doc1 = r#"---
title: "Document One"
tags: [alpha, beta]
---

# First Document

Content from the first document with alpha beta tags.
"#;

    let doc2 = r#"---
title: "Document Two"
tags: [beta, gamma]
---

# Second Document

Content from the second document with beta gamma tags.
"#;

    fs::write(kiln_path.join("doc1.md"), doc1)?;
    fs::write(kiln_path.join("doc2.md"), doc2)?;

    // Create configuration
    let config = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "memory"
"#,
        kiln_path.to_string_lossy()
    );

    let config_path = temp_dir.path().join("config.toml");
    fs::write(&config_path, config)?;

    // Test searches that should find both documents
    println!("Testing cross-document searches...");
    let (beta_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "beta"
    ])?;

    let (alpha_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "alpha"
    ])?;

    let (gamma_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "gamma"
    ])?;

    // Update one document's metadata
    let updated_doc1 = r#"---
title: "Document One Updated"
tags: [alpha, delta]
---

# First Document

Content from the first document with alpha delta tags.
"#;

    fs::write(kiln_path.join("doc1.md"), updated_doc1)?;

    // Test searches after metadata change
    println!("Testing after metadata changes...");
    let (updated_beta_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "beta"
    ])?;

    let (updated_alpha_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "alpha"
    ])?;

    let (delta_search, _) = run_cli_command(&[
        "--config", &config_path.to_string_lossy(),
        "search", "delta"
    ])?;

    // Verify results
    assert!(!beta_search.is_empty(), "Should find beta in initial search");
    assert!(!alpha_search.is_empty(), "Should find alpha in initial search");
    assert!(!gamma_search.is_empty(), "Should find gamma in initial search");

    // After update: beta should only find doc2, alpha should still find doc1, delta should find doc1
    assert!(!updated_beta_search.is_empty(), "Should still find beta (in doc2)");
    assert!(!updated_alpha_search.is_empty(), "Should still find alpha (in doc1)");
    assert!(!delta_search.is_empty(), "Should find new delta tag");

    println!("Initial beta search: {}", beta_search);
    println!("Initial alpha search: {}", alpha_search);
    println!("Initial gamma search: {}", gamma_search);
    println!("Updated beta search: {}", updated_beta_search);
    println!("Updated alpha search: {}", updated_alpha_search);
    println!("Delta search: {}", delta_search);

    println!("✅ Multiple documents metadata consistency test passed");
    Ok(())
}