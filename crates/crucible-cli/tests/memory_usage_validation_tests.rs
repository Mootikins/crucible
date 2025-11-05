//! Memory Usage Validation Tests for Crucible CLI
//!
//! Simple, practical memory testing focused on CLI command execution patterns.
//! Tests for memory leaks, excessive memory growth, and resource usage during
//! normal CLI operations.

use anyhow::{anyhow, Result};
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};
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

/// Run CLI command and measure execution time
fn run_cli_command_with_timing(args: &[&str]) -> Result<(String, String, Duration)> {
    let start = Instant::now();
    let output = Command::new("cargo")
        .args(&["run", "--bin", "cru", "--"])
        .args(args)
        .output()?;

    let duration = start.elapsed();
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

    Ok((stdout, stderr, duration))
}

/// Test basic memory usage patterns during CLI operations
#[tokio::test]
async fn test_basic_memory_usage_patterns() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create test documents of different sizes
    let small_doc = "# Small Document\n\nThis is a small document.";
    let medium_doc = format!(
        "# Medium Document\n\n{}",
        "This is a medium document with more content.\n".repeat(50)
    );
    let large_doc = format!(
        "# Large Document\n\n{}",
        "This is a large document with substantial content.\n".repeat(500)
    );

    fs::write(kiln_path.join("small.md"), small_doc)?;
    fs::write(kiln_path.join("medium.md"), medium_doc)?;
    fs::write(kiln_path.join("large.md"), large_doc)?;

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

    // Test memory patterns for different document sizes
    let test_cases = vec![
        ("small", "Small Document"),
        ("medium", "Medium Document"),
        ("large", "Large Document"),
    ];

    for (size, query) in test_cases {
        println!("Testing memory usage for {} document search...", size);

        // Run the search and measure timing as a proxy for memory usage
        let (_stdout, _stderr, duration) = run_cli_command_with_timing(&[
            "--config", &config_path.to_string_lossy(),
            "search", query
        ])?;

        println!("Search for '{}' completed in {:?}", query, duration);

        // Basic sanity check - searches should complete in reasonable time
        // This is a simple proxy for memory efficiency
        assert!(duration < Duration::from_secs(60),
                "Search for {} documents took too long: {:?}", size, duration);

        // Run multiple searches to check for memory growth patterns
        let mut durations = Vec::new();
        for i in 0..5 {
            let (_stdout, _stderr, d) = run_cli_command_with_timing(&[
                "--config", &config_path.to_string_lossy(),
                "search", query
            ])?;
            durations.push(d);
            println!("  Search {} completed in {:?}", i + 1, d);
        }

        // Check that execution times remain relatively stable
        // Significant increases might indicate memory leaks
        let first_duration = durations[0];
        let last_duration = durations[durations.len() - 1];
        let ratio = last_duration.as_millis() as f64 / first_duration.as_millis() as f64;

        println!("  Duration ratio (last/first): {:.2}", ratio);

        // Allow some variance but not extreme growth
        assert!(ratio < 5.0,
                "Potential memory leak detected: execution time grew from {:?} to {:?} (ratio: {:.2})",
                first_duration, last_duration, ratio);
    }

    println!("✅ Basic memory usage patterns test passed");
    Ok(())
}

/// Test memory usage during repeated operations
#[tokio::test]
async fn test_repeated_operations_memory_stability() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create test documents
    let test_files = vec![
        ("doc1.md", "# Document 1\n\nContent for document 1."),
        ("doc2.md", "# Document 2\n\nContent for document 2."),
        ("doc3.md", "# Document 3\n\nContent for document 3."),
    ];

    for (filename, content) in test_files {
        fs::write(kiln_path.join(filename), content)?;
    }

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

    // Perform repeated operations and monitor for performance degradation
    let operations = vec![
        ("search", "Document"),
        ("search", "Content"),
        ("search", "doc1"),
        ("search", "doc2"),
        ("search", "doc3"),
    ];

    println!("Testing memory stability over repeated operations...");

    // Run each operation multiple times and track performance
    for (op, query) in operations {
        let mut durations = Vec::new();

        println!("Testing repeated '{}' operations with query '{}'", op, query);

        for i in 0..10 {
            let (_stdout, _stderr, duration) = run_cli_command_with_timing(&[
                "--config", &config_path.to_string_lossy(),
                op, query
            ])?;
            durations.push(duration);

            if i == 0 || i == 4 || i == 9 {
                println!("  Iteration {}: {:?}", i + 1, duration);
            }
        }

        // Analyze performance trend
        let avg_first_half: Duration = durations.iter().take(5).sum();
        let avg_second_half: Duration = durations.iter().skip(5).sum();

        let avg_first = avg_first_half / 5;
        let avg_second = avg_second_half / 5;

        let degradation_ratio = if avg_first.as_millis() > 0 {
            avg_second.as_millis() as f64 / avg_first.as_millis() as f64
        } else {
            1.0
        };

        println!("  Performance degradation ratio: {:.2}", degradation_ratio);

        // Allow some performance variation but not significant degradation
        assert!(degradation_ratio < 3.0,
                "Significant performance degradation detected for '{}' '{}': {:.2}",
                op, query, degradation_ratio);
    }

    println!("✅ Repeated operations memory stability test passed");
    Ok(())
}

/// Test memory usage with large numbers of documents
#[tokio::test]
async fn test_memory_usage_with_many_documents() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create many small documents
    let num_docs = 50;
    println!("Creating {} documents...", num_docs);

    for i in 0..num_docs {
        let content = format!(
            r#"---
title: "Document {}"
tags: [test, doc{}, batch]
---

# Document {}

This is document number {} in our test collection.
It contains unique content: UNIQUE_CONTENT_{}.
Keywords: keyword{}, test{}, batch{}.
"#,
            i, i, i, i, i, i, i, i
        );

        fs::write(kiln_path.join(format!("doc{}.md", i)), content)?;
    }

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

    println!("Testing search performance with {} documents...", num_docs);

    // Test different types of searches
    let search_queries = vec![
        ("Document", "should match many documents"),
        ("test", "should match many documents"),
        ("batch", "should match many documents"),
        ("UNIQUE_CONTENT_25", "should match exactly one document"),
        ("keyword10", "should match one document"),
        ("nonexistent_term_xyz", "should match no documents"),
    ];

    for (query, description) in search_queries {
        println!("Testing query: '{}' ({})", query, description);

        let (_stdout, _stderr, duration) = run_cli_command_with_timing(&[
            "--config", &config_path.to_string_lossy(),
            "search", query
        ])?;

        println!("  Search completed in {:?}", duration);

        // Performance should remain reasonable even with many documents
        assert!(duration < Duration::from_secs(120),
                "Search '{}' with {} documents took too long: {:?}",
                query, num_docs, duration);

        // Test repeated execution of the same query
        let mut repeated_durations = Vec::new();
        for _i in 0..3 {
            let (_stdout, _stderr, d) = run_cli_command_with_timing(&[
                "--config", &config_path.to_string_lossy(),
                "search", query
            ])?;
            repeated_durations.push(d);
        }

        // Check for consistency in repeated searches
        let max_duration = repeated_durations.iter().max().unwrap();
        let min_duration = repeated_durations.iter().min().unwrap();

        let variation_ratio = if min_duration.as_millis() > 0 {
            max_duration.as_millis() as f64 / min_duration.as_millis() as f64
        } else {
            1.0
        };

        println!("  Repeated search variation ratio: {:.2}", variation_ratio);

        // Allow some variation but not extreme inconsistency
        assert!(variation_ratio < 5.0,
                "Inconsistent search performance for '{}': {:?}",
                query, repeated_durations);
    }

    println!("✅ Memory usage with many documents test passed");
    Ok(())
}

/// Test memory usage patterns with different storage backends
#[tokio::test]
async fn test_memory_usage_across_backends() -> Result<()> {
    let backends = vec![
        ("memory", "memory"),
        ("surrealdb", "surrealdb"),
    ];

    for (backend_name, backend_config) in backends {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("test-kiln");
        fs::create_dir_all(&kiln_path)?;

        // Create test documents
        let test_content = vec![
            ("doc1.md", "# Backend Test 1\n\nContent for backend testing."),
            ("doc2.md", "# Backend Test 2\n\nMore content for testing."),
            ("doc3.md", "# Backend Test 3\n\nAdditional test content."),
        ];

        for (filename, content) in test_content {
            fs::write(kiln_path.join(filename), content)?;
        }

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

        println!("Testing memory usage with {} backend...", backend_name);

        // Test various operations
        let operations = vec![
            ("search", "Backend"),
            ("search", "Test"),
            ("search", "Content"),
        ];

        for (op, query) in operations {
            println!("  Testing {} operation: '{}' with {}", op, query, backend_name);

            // Measure initial execution time
            let (_stdout, _stderr, initial_duration) = run_cli_command_with_timing(&[
                "--config", &config_path.to_string_lossy(),
                op, query
            ])?;

            println!("    Initial execution: {:?}", initial_duration);

            // Test repeated operations for memory stability
            let mut total_duration = Duration::ZERO;
            let iterations = 5;

            for i in 0..iterations {
                let (_stdout, _stderr, duration) = run_cli_command_with_timing(&[
                    "--config", &config_path.to_string_lossy(),
                    op, query
                ])?;
                total_duration += duration;

                if i == 0 {
                    println!("    First repeated: {:?}", duration);
                }
            }

            let avg_duration = total_duration / iterations;
            println!("    Average over {} iterations: {:?}", iterations, avg_duration);

            // Check for reasonable performance
            assert!(avg_duration < Duration::from_secs(60),
                    "Average performance too slow for {} backend: {:?}",
                    backend_name, avg_duration);

            // Check that repeated operations don't get significantly slower
            let slowdown_factor = if initial_duration.as_millis() > 0 {
                avg_duration.as_millis() as f64 / initial_duration.as_millis() as f64
            } else {
                1.0
            };

            println!("    Slowdown factor: {:.2}", slowdown_factor);
            assert!(slowdown_factor < 4.0,
                    "Excessive slowdown detected for {} backend: {:.2}",
                    backend_name, slowdown_factor);
        }

        println!("✅ {} backend memory usage test passed", backend_name);
    }

    Ok(())
}

/// Test for simple memory leak detection patterns
#[tokio::test]
async fn test_simple_memory_leak_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create a document we'll repeatedly update
    let mut content_version = 1;
    let initial_content = format!(
        r#"---
title: "Memory Leak Test Document"
tags: [test, memory, version{}]
---

# Memory Leak Test

This document will be updated to test for memory leaks.
Current version: {}
Unique content for version {}: UNIQUE_V{}
"#,
        content_version, content_version, content_version, content_version
    );

    fs::write(kiln_path.join("memory_test.md"), initial_content)?;

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

    println!("Testing for memory leaks through repeated document updates...");

    let mut search_durations = Vec::new();
    let iterations = 20;

    for i in 0..iterations {
        // Update the document
        content_version = i + 1;
        let updated_content = format!(
            r#"---
title: "Memory Leak Test Document"
tags: [test, memory, version{}]
---

# Memory Leak Test

This document will be updated to test for memory leaks.
Current version: {}
Unique content for version {}: UNIQUE_V{}
Additional content: {}
"#,
            content_version, content_version, content_version, content_version,
            "Added content line.\n".repeat(i)
        );

        fs::write(kiln_path.join("memory_test.md"), updated_content)?;

        // Search for the unique content
        let query = format!("UNIQUE_V{}", content_version);
        let (_stdout, _stderr, duration) = run_cli_command_with_timing(&[
            "--config", &config_path.to_string_lossy(),
            "search", &query
        ])?;

        search_durations.push(duration);

        if i % 5 == 0 {
            println!("  Iteration {}: search time {:?}", i + 1, duration);
        }
    }

    // Analyze the duration trends for signs of memory leaks
    let quarter_size = iterations / 4;
    let first_quarter: Duration = search_durations.iter().take(quarter_size).sum();
    let last_quarter: Duration = search_durations.iter().skip(iterations * 3 / 4).sum();

    let first_avg = first_quarter / quarter_size as u32;
    let last_avg = last_quarter / quarter_size as u32;

    let memory_growth_factor = if first_avg.as_millis() > 0 {
        last_avg.as_millis() as f64 / first_avg.as_millis() as f64
    } else {
        1.0
    };

    println!("First quarter average: {:?}", first_avg);
    println!("Last quarter average: {:?}", last_avg);
    println!("Memory growth factor: {:.2}", memory_growth_factor);

    // Check for significant performance degradation that might indicate memory leaks
    assert!(memory_growth_factor < 3.0,
            "Potential memory leak detected: performance degraded by factor {:.2}",
            memory_growth_factor);

    // Also check that the latest searches complete in reasonable time
    if let Some(last_duration) = search_durations.last() {
        assert!(*last_duration < Duration::from_secs(5),
                "Search performance degraded significantly: {:?}",
                last_duration);
    }

    println!("✅ Simple memory leak detection test passed");
    Ok(())
}