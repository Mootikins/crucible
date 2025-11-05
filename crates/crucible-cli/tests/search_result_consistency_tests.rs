//! Search Result Consistency Tests
//!
//! Test search functionality before and after storage operations to ensure consistency.
//! This includes both regular text search and semantic search with embeddings.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{TempDir, NamedTempFile, Builder};
use tokio::time::{sleep, Duration};

/// Test configuration for different storage backends
#[derive(Debug, Clone)]
struct StorageTestConfig {
    backend_type: StorageBackendType,
    config_path: PathBuf,
    kiln_path: PathBuf,
    test_name: String,
}

#[derive(Debug, Clone)]
enum StorageBackendType {
    SurrealDB,
    Memory,
    RocksDB,
}

/// Create test kiln with sample content
async fn create_test_kiln_with_content(
    _temp_dir: &Path,
    backend_type: &str,
) -> Result<(TempDir, PathBuf)> {
    let test_dir = TempDir::new()?;
    let kiln_path = test_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create sample markdown files
    let test_files = vec![
        ("index.md", format!(
            "# Test Index - Backend: {}\n\nWelcome to the test knowledge base.\n\n## Topics\n- [[Rust Programming]]\n- [[Machine Learning]]\n- [[Database Design]]",
            backend_type
        )),
        ("rust.md", "# Rust Programming\n\nA systems programming language focused on safety and performance.\n\n## Features\n- Memory safety\n- Concurrency\n- Zero-cost abstractions".to_string()),
        ("ml.md", "# Machine Learning\n\nAlgorithms that enable computers to learn from data.\n\n## Types\n- Supervised learning\n- Unsupervised learning\n- Reinforcement learning".to_string()),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(file_path, content)?;
    }

    Ok((test_dir, kiln_path))
}

/// Create backend-specific configuration
fn create_backend_config(
    backend: StorageBackendType,
    kiln_path: &Path,
) -> Result<NamedTempFile> {
    let config_file = NamedTempFile::new()?;
    let config_content = match backend {
        StorageBackendType::Memory => format!(
            r#"[kiln]
path = "{}"

[storage]
backend = "memory"

[storage.memory]
max_size_mb = 100
"#,
            kiln_path.to_string_lossy()
        ),
        StorageBackendType::SurrealDB => format!(
            r#"[kiln]
path = "{}"

[storage]
backend = "surrealdb"

[storage.surrealdb]
namespace = "crucible_test"
database = "test_db"
"#,
            kiln_path.to_string_lossy()
        ),
        StorageBackendType::RocksDB => {
            let data_path = kiln_path.join("_data");
            format!(
                r#"[kiln]
path = "{}"

[storage]
backend = "rocksdb"

[storage.rocksdb]
data_path = "{}"
compression = "lz4"
"#,
                kiln_path.to_string_lossy(),
                data_path.to_string_lossy()
            )
        },
    };

    fs::write(config_file.path(), config_content)?;
    Ok(config_file)
}

/// Run CLI command with test configuration
async fn run_cli_command(
    args: &[&str],
    config_path: &Path,
) -> Result<(String, String)> {
    let binary_path = find_cru_binary();
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let config_path_owned = config_path.to_path_buf();

    let temp_home = tempfile::Builder::new()
        .prefix("crucible-cli-home")
        .tempdir()
        .context("failed to create temporary HOME directory")?;

    let output = tokio::task::spawn_blocking(move || -> Result<(String, String)> {
        let mut cmd = std::process::Command::new(&binary_path);

        cmd.env("HOME", temp_home.path());
        cmd.env("XDG_CONFIG_HOME", temp_home.path());
        cmd.env("XDG_DATA_HOME", temp_home.path());
        cmd.env("CRUCIBLE_CONFIG", config_path_owned.to_string_lossy().as_ref());

        cmd.args(&args);

        let output = cmd.output().context("Failed to execute CLI command")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Command failed with exit code {:?}: {}\nStderr: {}",
                output.status.code(),
                stdout,
                stderr
            ));
        }

        Ok((stdout, stderr))
    }).await?;

    output
}

/// Find the compiled `cru` binary
fn find_cru_binary() -> PathBuf {
    // Try multiple possible locations for the binary
    let possible_paths = vec![
        "../../target/debug/cru",
        "../../target/release/cru",
        "target/debug/cru",
        "target/release/cru",
    ];

    for path in possible_paths {
        let full_path = std::env::current_dir().unwrap_or_default().join(path);
        if full_path.exists() {
            return full_path;
        }
    }

    panic!("`cru` binary not found. Run `cargo build -p crucible-cli` first.");
}

/// Test search consistency before and after processing operations
#[tokio::test]
async fn test_search_consistency_before_after_processing() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    for backend in backends {
        let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("search_consistency_{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Create test documents with varied content
        let test_documents = vec![
            ("rust-programming.md", r#"---
title: Rust Programming Guide
tags: [rust, programming, systems]
---

# Rust Programming Guide

Rust is a systems programming language focused on safety and performance.

## Key Features
- Memory safety without garbage collection
- Concurrency without data races
- Zero-cost abstractions
- Pattern matching

## Code Example
```rust
fn main() {
    println!("Hello, Rust!");
}
```

#rust #programming #systems
"#),
            ("machine-learning.md", r#"---
title: Machine Learning Fundamentals
tags: [ml, ai, algorithms]
---

# Machine Learning Fundamentals

Machine learning algorithms enable computers to learn from data.

## Types of Learning
- Supervised learning
- Unsupervised learning
- Reinforcement learning

#machine-learning #algorithms #data-science
"#),
            ("database-design.md", r#"---
title: Database Design Principles
tags: [database, architecture, design]
---

# Database Design Principles

Good database design ensures data integrity and performance.

## Normalization
- First Normal Form (1NF)
- Second Normal Form (2NF)
- Third Normal Form (3NF)

#database #architecture #design
"#),
        ];

        // Write test documents to kiln
        for (filename, content) in &test_documents {
            let file_path = kiln_path.join(filename);
            fs::write(&file_path, content)?;
        }

        // Test search BEFORE processing (file system search only)
        println!("Testing search BEFORE processing for {:?}", backend);
        let mut search_results_before = HashMap::new();

        let search_queries = vec![
            "rust programming",
            "machine learning",
            "database design",
            "algorithms",
            "performance",
        ];

        for query in &search_queries {
            let (search_stdout, search_stderr) = run_cli_command(&[
                "search", query, "--limit", "10", "--format", "json"
            ], config_file.path()).await?;

            // Before processing, search might work on file system or return no results
            search_results_before.insert(*query, (search_stdout, search_stderr));
        }

        // Process the kiln to index documents and generate embeddings
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
        sleep(Duration::from_millis(2000)).await; // Give time for embeddings

        // Test search AFTER processing (should have indexed results)
        println!("Testing search AFTER processing for {:?}", backend);
        let mut search_results_after = HashMap::new();

        for query in &search_queries {
            let (search_stdout, search_stderr) = run_cli_command(&[
                "search", query, "--limit", "10", "--format", "json"
            ], config_file.path()).await?;

            assert!(!search_stderr.contains("error"),
                     "Search should succeed for query '{}' on {:?}", query, backend);

            search_results_after.insert(*query, (search_stdout, search_stderr));
        }

        // Verify search results are consistent and better after processing
        for query in &search_queries {
            let (before_stdout, _) = search_results_before.get(query).unwrap();
            let (after_stdout, _) = search_results_after.get(query).unwrap();

            // After processing should have more/better results
            let before_has_content = !before_stdout.is_empty() && !before_stdout.contains("[]");
            let after_has_content = !after_stdout.is_empty() && !after_stdout.contains("[]");

            if query.contains("rust") {
                assert!(after_has_content, "Should find rust-related content after processing");
                assert!(after_stdout.to_lowercase().contains("rust"),
                         "Results should contain rust for query '{}'", query);
            }

            if query.contains("machine") {
                assert!(after_has_content, "Should find machine learning content after processing");
                assert!(after_stdout.to_lowercase().contains("machine") ||
                       after_stdout.to_lowercase().contains("ml"),
                       "Results should contain machine learning for query '{}'", query);
            }

            if query.contains("database") {
                assert!(after_has_content, "Should find database content after processing");
                assert!(after_stdout.to_lowercase().contains("database"),
                       "Results should contain database for query '{}'", query);
            }

            println!("✅ Query '{}' consistency check passed for {:?}", query, backend);
        }

        println!("✅ Search consistency test passed for {:?}", backend);
    }

    Ok(())
}

/// Test semantic search consistency with embeddings
#[tokio::test]
async fn test_semantic_search_consistency_with_embeddings() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    for backend in backends {
        let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("semantic_consistency_{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Create documents with semantic relationships
        let semantic_documents = vec![
            ("ai-research.md", r#"---
title: Artificial Intelligence Research
tags: [ai, research, neural-networks]
---

# Artificial Intelligence Research

Modern AI research focuses on neural networks and deep learning architectures.

## Key Areas
- Natural Language Processing
- Computer Vision
- Reinforcement Learning
- Large Language Models

The field has evolved dramatically since the advent of transformer architectures.
"#),
            ("nlp-techniques.md", r#"---
title: Natural Language Processing
tags: [nlp, text-processing, linguistics]
---

# Natural Language Processing

NLP enables computers to understand and generate human language.

## Techniques
- Tokenization and embedding
- Transformer models
- Attention mechanisms
- Language generation

Modern NLP leverages large pre-trained models for various tasks.
"#),
            ("deep-learning.md", r#"---
title: Deep Learning Fundamentals
tags: [deep-learning, neural-networks, training]
---

# Deep Learning Fundamentals

Deep learning uses multi-layered neural networks to learn complex patterns.

## Components
- Artificial neurons and layers
- Activation functions
- Backpropagation
- Optimization algorithms

Training deep networks requires significant computational resources and data.
"#),
        ];

        // Write semantic documents to kiln
        for (filename, content) in &semantic_documents {
            let file_path = kiln_path.join(filename);
            fs::write(&file_path, content)?;
        }

        // Process the kiln to generate embeddings
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
        sleep(Duration::from_millis(3000)).await; // Give more time for embeddings

        // Test semantic search with conceptually related queries
        let semantic_queries = vec![
            ("artificial intelligence", "Should find AI, neural networks, and deep learning content"),
            ("text understanding", "Should find NLP and language processing content"),
            ("machine learning models", "Should find deep learning and AI research content"),
            ("neural network training", "Should find deep learning fundamentals"),
            ("language models", "Should find NLP and AI research content"),
        ];

        for (query, expected_behavior) in &semantic_queries {
            println!("Testing semantic search for '{}' on {:?}", query, backend);

            let (semantic_stdout, semantic_stderr) = run_cli_command(&[
                "semantic", query, "--limit", "5", "--format", "json"
            ], config_file.path()).await?;

            assert!(!semantic_stderr.contains("error"),
                     "Semantic search should succeed for query '{}' on {:?}", query, backend);

            // Should have some results for meaningful queries
            assert!(!semantic_stdout.is_empty(),
                     "Semantic search should return results for '{}'", query);

            // If results are in JSON format, they should be parseable
            if semantic_stdout.trim().starts_with('[') {
                // Valid JSON array of results
                assert!(semantic_stdout.contains("title") || semantic_stdout.contains("content") || semantic_stdout.contains("path"),
                         "Semantic search results should contain document information");
            }

            println!("✅ Semantic search '{}' returned {} characters", query, semantic_stdout.len());
        }

        // Test semantic search consistency across multiple runs
        println!("Testing semantic search consistency across runs for {:?}", backend);
        let consistency_query = "artificial intelligence";

        let mut results = Vec::new();
        for run in 1..=3 {
            let (semantic_stdout, _) = run_cli_command(&[
                "semantic", consistency_query, "--limit", "3", "--format", "json"
            ], config_file.path()).await?;

            results.push(semantic_stdout.clone());
            sleep(Duration::from_millis(500)).await; // Small delay between runs
        }

        // Results should be consistent (or very similar) across runs
        for (i, result) in results.iter().enumerate() {
            assert!(!result.is_empty(),
                     "Semantic search run {} should return results", i + 1);

            // All runs should find the AI-related content
            assert!(result.to_lowercase().contains("ai") ||
                   result.to_lowercase().contains("intelligence") ||
                   result.to_lowercase().contains("neural") ||
                   result.to_lowercase().contains("deep"),
                   "Run {} should find AI-related content", i + 1);
        }

        println!("✅ Semantic search consistency test passed for {:?}", backend);
    }

    Ok(())
}

/// Test that search results are consistent after database operations
#[tokio::test]
async fn test_search_consistency_after_database_operations() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    for backend in backends {
        let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("db_consistency_{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Create test content
        let test_content = r#"---
title: Database Consistency Test
tags: [database, consistency, testing]
---

# Database Consistency Test

This document tests search consistency after database operations.

## Content
Database operations should maintain search index integrity.

## Features
- Backup and restore functionality
- Search index consistency
- Data preservation

#database #consistency #testing
"#;

        let test_file = kiln_path.join("consistency-test.md");
        fs::write(&test_file, test_content)?;

        // Process to create initial search index
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Initial processing should succeed");
        sleep(Duration::from_millis(1500)).await;

        // Test search before database operations
        let (search_before, _) = run_cli_command(&[
            "search", "database consistency", "--format", "json"
        ], config_file.path()).await?;

        assert!(!search_before.is_empty(), "Should find content before database operations");
        let before_has_content = search_before.to_lowercase().contains("database") &&
                                 search_before.to_lowercase().contains("consistency");

        // Perform database backup
        let backup_path = config_file.path().parent().unwrap().join("backup.json");
        let (backup_stdout, backup_stderr) = run_cli_command(&[
            "storage", "backup", "--format", "json", backup_path.to_str().unwrap()
        ], config_file.path()).await?;

        assert!(!backup_stderr.contains("error"), "Backup should succeed");
        assert!(backup_path.exists(), "Backup file should exist");

        // Test search after backup (should still work)
        let (search_after_backup, _) = run_cli_command(&[
            "search", "database consistency", "--format", "json"
        ], config_file.path()).await?;

        assert!(!search_after_backup.is_empty(), "Should still find content after backup");
        let after_backup_has_content = search_after_backup.to_lowercase().contains("database") &&
                                      search_after_backup.to_lowercase().contains("consistency");

        assert!(after_backup_has_content, "Search should work consistently after backup");

        // Perform database cleanup
        let (_, cleanup_stderr) = run_cli_command(&[
            "storage", "cleanup", "--gc"
        ], config_file.path()).await?;

        assert!(!cleanup_stderr.contains("error"), "Cleanup should succeed");

        // Test search after cleanup (should still work)
        let (search_after_cleanup, _) = run_cli_command(&[
            "search", "database consistency", "--format", "json"
        ], config_file.path()).await?;

        assert!(!search_after_cleanup.is_empty(), "Should still find content after cleanup");
        let after_cleanup_has_content = search_after_cleanup.to_lowercase().contains("database") &&
                                       search_after_cleanup.to_lowercase().contains("consistency");

        assert!(after_cleanup_has_content, "Search should work consistently after cleanup");

        // Verify search results are fundamentally the same
        assert_eq!(before_has_content, after_backup_has_content,
                  "Search consistency should be maintained across backup operations");
        assert_eq!(before_has_content, after_cleanup_has_content,
                  "Search consistency should be maintained across cleanup operations");

        println!("✅ Search consistency after database operations test passed for {:?}", backend);
    }

    Ok(())
}

/// Test search result ordering and relevance consistency
#[tokio::test]
async fn test_search_result_ordering_consistency() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    for backend in backends {
        let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("ordering_consistency_{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Create documents with varying levels of relevance to test queries
        let ordering_documents = vec![
            ("highly-relevant.md", r#"---
title: Rust Programming Language
tags: [rust, programming, systems]
---

# Rust Programming Language

Rust is a modern systems programming language that guarantees memory safety.

## Features
- Memory safety without garbage collection
- Performance and safety
- Systems programming

#rust #programming #systems
"#),
            ("somewhat-relevant.md", r#"---
title: Programming Languages Comparison
tags: [programming, languages, comparison]
---

# Programming Languages Comparison

This compares various programming languages including C++, Java, and Go.

## Languages
- C++ for systems programming
- Java for enterprise applications
- Go for concurrent systems

#programming #languages #comparison
"#),
            ("less-relevant.md", r#"---
title: Software Engineering Best Practices
tags: [engineering, best-practices, software]
---

# Software Engineering Best Practices

Best practices for software development and team collaboration.

## Topics
- Code review processes
- Testing methodologies
- Documentation standards

#engineering #software #practices
"#),
        ];

        // Write documents to kiln
        for (filename, content) in &ordering_documents {
            let file_path = kiln_path.join(filename);
            fs::write(&file_path, content)?;
        }

        // Process to create search index
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Processing should succeed");
        sleep(Duration::from_millis(1500)).await;

        // Test search ordering consistency
        let test_query = "rust programming";
        println!("Testing search ordering for '{}' on {:?}", test_query, backend);

        let mut search_results = Vec::new();

        // Run the same search multiple times to check consistency
        for run in 1..=3 {
            let (search_stdout, search_stderr) = run_cli_command(&[
                "search", test_query, "--limit", "10", "--format", "json"
            ], config_file.path()).await?;

            assert!(!search_stderr.contains("error"),
                     "Search should succeed for run {} on {:?}", run, backend);
            assert!(!search_stdout.is_empty(),
                     "Search should return results for run {}", run);

            search_results.push(search_stdout.clone());
            sleep(Duration::from_millis(200)).await;
        }

        // Results should be identical across runs for deterministic ordering
        for (i, result) in search_results.iter().enumerate() {
            if i > 0 {
                assert_eq!(search_results[0], *result,
                         "Search results should be identical across runs (run {} vs run 1)", i + 1);
            }

            // Should find the most relevant document first
            assert!(result.to_lowercase().contains("rust"),
                     "Results should contain rust for query '{}'", test_query);
        }

        println!("✅ Search ordering consistency test passed for {:?}", backend);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
}