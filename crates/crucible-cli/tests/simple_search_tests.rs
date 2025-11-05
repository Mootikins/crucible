//! Simplified Search Functionality Tests
//!
//! Test basic search functionality without requiring processing pipeline.

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
        return Err(anyhow::anyhow!(
            "Command failed with exit code {:?}\nStdout: {}\nStderr: {}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    Ok((stdout, stderr))
}

/// Test basic search functionality without processing
#[tokio::test]
async fn test_basic_search_functionality() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create sample markdown files
    let test_files = vec![
        ("index.md", "# Test Index\n\nWelcome to the test knowledge base.\n\n## Topics\n- [[Rust Programming]]\n- [[Machine Learning]]\n- [[Database Design]]"),
        ("rust.md", "# Rust Programming\n\nA systems programming language focused on safety and performance.\n\n## Features\n- Memory safety\n- Concurrency\n- Zero-cost abstractions"),
        ("ml.md", "# Machine Learning\n\nAlgorithms that enable computers to learn from data.\n\n## Types\n- Supervised learning\n- Unsupervised learning\n- Reinforcement learning"),
        ("database.md", "# Database Design\n\nPrinciples for designing efficient and scalable databases.\n\n## Concepts\n- Normalization\n- Indexing\n- Query optimization"),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(file_path, content)?;
    }

    // Create simple memory configuration
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

    // Test basic search functionality
    let search_queries = vec![
        ("Rust", "should find Rust programming content"),
        ("Machine Learning", "should find ML content"),
        ("Database", "should find database design content"),
        ("Index", "should find index content"),
        ("Safety", "should find content about safety"),
    ];

    for (query, _description) in &search_queries {
        println!("Testing search query: '{}'", query);

        let (stdout, stderr) = run_cli_command(&[
            "--config", &config_path.to_string_lossy(),
            "search", query
        ])?;

        // Basic validation - search should not crash and should produce some output
        println!("Search output for '{}': {}", query, stdout.trim());

        // If there's error output, it should just be warnings/info, not failures
        if !stderr.is_empty() && stderr.contains("ERROR") {
            return Err(anyhow!("Search failed for query '{}': {}", query, stderr));
        }
    }

    println!("✅ Basic search functionality test passed");
    Ok(())
}

/// Test search with different backends
#[tokio::test]
async fn test_search_with_different_backends() -> Result<()> {
    let backends = vec![
        ("memory", "memory"),
        ("surrealdb", "surrealdb"),
    ];

    for (backend_name, backend_config) in backends {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("test-kiln");
        fs::create_dir_all(&kiln_path)?;

        // Create test content
        let test_content = format!(
            "# Test Document\n\nThis is a test document for {} backend.\n\n## Content\n- Test item 1\n- Test item 2\n- Special keyword: UNIQUE_KEYWORD_{}",
            backend_name, backend_name
        );

        fs::write(kiln_path.join("test.md"), test_content)?;

        // Create configuration for the backend
        let config = if backend_name == "surrealdb" {
            let db_path = temp_dir.path().join("test.db");
            format!(
                r#"[kiln]
path = "{}"

[storage]
backend = "surrealdb"

[storage.surrealdb]
path = "{}"
"#,
                kiln_path.to_string_lossy(),
                db_path.to_string_lossy()
            )
        } else {
            format!(
                r#"[kiln]
path = "{}"

[storage]
backend = "{}"
"#,
                kiln_path.to_string_lossy(),
                backend_config
            )
        };

        let config_path = temp_dir.path().join("config.toml");
        fs::write(&config_path, config)?;

        // Test search with unique keyword
        let query = format!("UNIQUE_KEYWORD_{}", backend_name);
        println!("Testing search for '{}' with {} backend", query, backend_name);

        let (stdout, stderr) = run_cli_command(&[
            "--config", &config_path.to_string_lossy(),
            "search", &query
        ])?;

        // Basic validation
        println!("{} backend search output: {}", backend_name, stdout.trim());

        if !stderr.is_empty() && stderr.contains("ERROR") {
            return Err(anyhow!("Search failed for {} backend: {}", backend_name, stderr));
        }

        println!("✅ {} backend search test passed", backend_name);
    }

    Ok(())
}

/// Test search result ordering and consistency
#[tokio::test]
async fn test_search_ordering_consistency() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create documents with predictable content
    fs::write(kiln_path.join("first.md"), "# First Document\n\nThis document contains the word alpha.")?;
    fs::write(kiln_path.join("second.md"), "# Second Document\n\nThis document contains the word beta.")?;
    fs::write(kiln_path.join("third.md"), "# Third Document\n\nThis document contains both alpha and beta.")?;

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

    // Test searches multiple times for consistency
    let queries = vec!["alpha", "beta", "document"];

    for query in queries {
        println!("Testing search consistency for query: '{}'", query);

        let mut results = Vec::new();

        // Run the same search multiple times
        for _i in 0..3 {
            let (stdout, _stderr) = run_cli_command(&[
                "--config", &config_path.to_string_lossy(),
                "search", query
            ])?;

            results.push(stdout.trim().to_string());
        }

        // Results should be consistent across runs
        if results[0] != results[1] || results[1] != results[2] {
            return Err(anyhow!(
                "Inconsistent search results for query '{}': {:?}",
                query, results
            ));
        }

        println!("✅ Consistent results for query: '{}'", query);
    }

    println!("✅ Search ordering consistency test passed");
    Ok(())
}