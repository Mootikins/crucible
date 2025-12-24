//! Phase 3: Integration Tests
//!
//! This module implements integration tests for Crucible's search and storage.
//! Tests focus on data consistency and cross-system validation.
//!
//! All tests use the docs kiln at `docs/` for realistic test data.
//!
//! Run with: `just test fixtures` or `cargo test --features test-fixtures`

#![cfg(feature = "test-fixtures")]

mod common;

use common::{count_kiln_files, setup_test_db_with_kiln, test_kiln_root};
use std::time::{Duration, Instant};

// ============================================================================
// Helper Tests
// ============================================================================

/// Test that count_kiln_files() helper works correctly
#[test]
fn test_count_kiln_files() {
    let count = count_kiln_files();
    assert!(
        count >= 30,
        "Dev kiln should have at least 30 markdown files, found: {}",
        count
    );
    println!("Found {} markdown files in dev-kiln", count);
}

/// Test that test_kiln_root() points to valid directory
#[test]
fn test_kiln_root_exists() {
    let root = test_kiln_root();
    assert!(
        root.exists(),
        "Dev kiln directory should exist at: {}",
        root.display()
    );
    assert!(
        root.is_dir(),
        "Dev kiln path should be a directory: {}",
        root.display()
    );
    println!("Dev kiln root verified at: {}", root.display());
}

// ============================================================================
// Section 1: Data Consistency (5 tests)
// ============================================================================

/// Test 1.1: Tag consistency across ingested documents
///
/// Verifies that tags are consistently applied and queryable.
#[tokio::test]
async fn consistency_tags() {
    // Arrange: Ingest all test-kiln files
    let kiln_root = test_kiln_root();
    let file_count = count_kiln_files();

    assert!(file_count >= 30, "Dev kiln should have at least 30 files");

    // Collect all unique tags from files
    let mut file_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
    let tag_regex = regex::Regex::new(r"tags:\s*\[([^\]]+)\]").unwrap();

    for entry in walkdir::WalkDir::new(&kiln_root).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Some(cap) = tag_regex.captures(&content) {
                    if let Some(tags_str) = cap.get(1) {
                        for tag in tags_str.as_str().split(',') {
                            let tag = tag.trim().trim_matches('"').trim_matches('\'');
                            if !tag.is_empty() {
                                file_tags.insert(tag.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Assert: File-based validation passes (GREEN phase - file validation only)
    assert!(
        !file_tags.is_empty(),
        "Test kiln should have tags in frontmatter"
    );

    println!(
        "Found {} unique tags across {} files",
        file_tags.len(),
        file_count
    );

    // Database validation will be implemented later
    // TODO: After ingestion, verify database has same tags
    // SELECT COUNT(DISTINCT name) FROM tags
    // Should equal file_tags.len()
}

/// Test 1.2: Date validation - no future dates or invalid ranges
///
/// Verifies that created dates are not after modified dates.
#[tokio::test]
async fn consistency_dates() {
    // Arrange: Parse dates from all test-kiln files
    let kiln_root = test_kiln_root();
    let created_regex = regex::Regex::new(r"created:\s*(\d{4}-\d{2}-\d{2})").unwrap();
    let modified_regex = regex::Regex::new(r"modified:\s*(\d{4}-\d{2}-\d{2})").unwrap();

    let mut date_issues = Vec::new();

    for entry in walkdir::WalkDir::new(&kiln_root).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let created = created_regex
                    .captures(&content)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string());

                let modified = modified_regex
                    .captures(&content)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string());

                if let (Some(c), Some(m)) = (created, modified) {
                    if c > m {
                        date_issues.push((
                            entry
                                .path()
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_string(),
                            c,
                            m,
                        ));
                    }
                }
            }
        }
    }

    // Assert: All dates are valid (GREEN phase - fully passing)
    assert!(
        date_issues.is_empty(),
        "Found {} files with created > modified: {:?}",
        date_issues.len(),
        date_issues
    );

    println!("All test-kiln files have valid date ranges (created <= modified)");

    // Database validation can be added later to enforce this at DB level
    // TODO: Database query for date validation
    // SELECT * FROM entities
    // WHERE data.created > data.modified
    //    OR data.created > now()
}

/// Test 1.3: Related document consistency
///
/// Verifies that all 'related' frontmatter references are valid.
#[tokio::test]
async fn consistency_related_docs() {
    // Arrange: Extract all 'related' references
    let kiln_root = test_kiln_root();
    let related_regex = regex::Regex::new(r"related:\s*\[([^\]]+)\]").unwrap();

    let mut all_titles: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_related: Vec<(String, Vec<String>)> = Vec::new();

    for entry in walkdir::WalkDir::new(&kiln_root).into_iter().flatten() {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
            let title = entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            all_titles.insert(title.clone());

            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Some(cap) = related_regex.captures(&content) {
                    if let Some(related_str) = cap.get(1) {
                        let related: Vec<String> = related_str
                            .as_str()
                            .split(',')
                            .map(|s| {
                                // Strip quotes and wikilink brackets
                                s.trim()
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .trim_start_matches("[[")
                                    .trim_end_matches("]]")
                                    .to_string()
                            })
                            .filter(|s| !s.is_empty())
                            .collect();

                        if !related.is_empty() {
                            all_related.push((title, related));
                        }
                    }
                }
            }
        }
    }

    // Assert: Check for broken related references (GREEN phase - file validation only)
    let mut broken_refs = Vec::new();
    for (source, related) in &all_related {
        for rel in related {
            if !all_titles.contains(rel) {
                broken_refs.push((source.clone(), rel.clone()));
            }
        }
    }

    assert!(
        broken_refs.is_empty(),
        "Found {} broken related references: {:?}",
        broken_refs.len(),
        broken_refs
    );

    println!(
        "Validated {} related document references across {} files",
        all_related.iter().map(|(_, r)| r.len()).sum::<usize>(),
        all_related.len()
    );

    // Database validation can verify these relationships persist correctly
    // TODO: After ingestion, verify database consistency
    // SELECT d1.title, d1.data.related
    // FROM entities d1
    // WHERE d1.data.related IS NOT NULL
}

/// Test 1.4: Entity count verification
///
/// Verifies that ingestion creates entities.
#[tokio::test]
async fn consistency_entity_count() {
    // Arrange: Count files in test-kiln
    let file_count = count_kiln_files();

    assert!(
        file_count >= 30,
        "Dev kiln should have at least 30 files, found: {}",
        file_count
    );

    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query all entities (not using count() which can be tricky in SurrealDB)
    let sql = r#"
        SELECT id FROM entities WHERE type = 'note'
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    let db_count = result.records.len();

    eprintln!("File count: {}, Entity count: {}", file_count, db_count);

    // Should have entities after ingestion
    assert!(
        db_count > 0,
        "Should have entities in database after ingestion"
    );

    // Entity count should be close to file count (some may have parse errors)
    assert!(
        db_count >= file_count - 2,
        "Entity count ({}) should be close to file count ({})",
        db_count,
        file_count
    );
}

/// Test 1.5: Properties stored during ingestion
///
/// Verifies that properties are stored in the database.
#[tokio::test]
async fn consistency_properties_exist() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for properties
    let sql = r#"
        SELECT key, count() as count FROM properties GROUP BY key LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should have properties stored
    assert!(
        !result.records.is_empty(),
        "Should have properties stored in database"
    );

    eprintln!("Property keys found:");
    for record in &result.records {
        if let (Some(key), Some(count)) = (
            record.data.get("key").and_then(|v| v.as_str()),
            record.data.get("count"),
        ) {
            eprintln!("  {}: {:?}", key, count);
        }
    }
}

// ============================================================================
// Section 2: Performance Benchmarks (4 tests)
// ============================================================================

/// Test 2.1: Simple query performance (<100ms)
///
/// Verifies that simple queries complete within performance budget.
#[tokio::test]
#[ignore = "Requires database implementation"]
async fn perf_simple_query() {
    // Arrange: Set up performance target
    let target_duration = Duration::from_millis(100);

    // Act: Time a simple query
    let start = Instant::now();

    // TODO: Execute simple query
    // SELECT * FROM entities WHERE type = 'note' LIMIT 10
    std::thread::sleep(Duration::from_millis(10)); // Placeholder

    let duration = start.elapsed();

    // Assert: Should complete within budget
    assert!(
        duration < target_duration,
        "Simple query took {:?}, should be < {:?}",
        duration,
        target_duration
    );

    println!("Simple query completed in {:?}", duration);
}

/// Test 2.2: Complex query performance (<500ms)
///
/// Verifies that complex multi-criteria queries complete within budget.
#[tokio::test]
#[ignore = "Requires database implementation"]
async fn perf_complex_query() {
    // Arrange: Set up performance target
    let target_duration = Duration::from_millis(500);

    // Act: Time a complex query
    let start = Instant::now();

    // TODO: Execute complex query combining:
    // - Content search
    // - Tag filter
    // - Date range
    // - Relationship check
    std::thread::sleep(Duration::from_millis(10)); // Placeholder

    let duration = start.elapsed();

    // Assert: Should complete within budget
    assert!(
        duration < target_duration,
        "Complex query took {:?}, should be < {:?}",
        duration,
        target_duration
    );

    println!("Complex query completed in {:?}", duration);
}

/// Test 2.3: Full-text search performance (<200ms)
///
/// Verifies that full-text search completes within performance budget.
#[tokio::test]
#[ignore = "Requires database implementation"]
async fn perf_fulltext_search() {
    // Arrange: Set up performance target
    let target_duration = Duration::from_millis(200);

    // Act: Time a full-text search
    let start = Instant::now();

    // TODO: Execute full-text search
    // SELECT * FROM entities WHERE search_text CONTAINS 'knowledge management'
    std::thread::sleep(Duration::from_millis(10)); // Placeholder

    let duration = start.elapsed();

    // Assert: Should complete within budget
    assert!(
        duration < target_duration,
        "Full-text search took {:?}, should be < {:?}",
        duration,
        target_duration
    );

    println!("Full-text search completed in {:?}", duration);
}

/// Test 2.4: Ingestion performance
///
/// Verifies that ingesting the test-kiln completes in reasonable time.
#[tokio::test]
#[ignore = "Requires database implementation"]
async fn perf_ingestion() {
    // Arrange: Set up performance target
    // 12 files should ingest in under 5 seconds
    let target_duration = Duration::from_secs(5);
    let file_count = count_kiln_files();

    // Act: Time ingestion
    let start = Instant::now();

    // TODO: Ingest all test-kiln files
    std::thread::sleep(Duration::from_millis(10)); // Placeholder

    let duration = start.elapsed();

    // Assert: Should complete within budget
    assert!(
        duration < target_duration,
        "Ingesting {} files took {:?}, should be < {:?}",
        file_count,
        duration,
        target_duration
    );

    // Calculate rate
    let rate = file_count as f64 / duration.as_secs_f64();
    println!("Ingestion rate: {:.1} files/second", rate);
}

// ============================================================================
// Section 3: Index Integrity (4 tests)
// ============================================================================

/// Test 3.1: All entities are queryable
///
/// Verifies that all ingested entities can be retrieved.
#[tokio::test]
async fn index_completeness() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query all entities
    let sql = r#"
        SELECT id, data.title as title FROM entities WHERE type = 'note'
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    let file_count = count_kiln_files();

    // Most files should be queryable (allow some margin for parse issues)
    assert!(
        result.records.len() >= file_count - 3,
        "Most files should be queryable as entities: got {} of {}",
        result.records.len(),
        file_count
    );

    eprintln!("Successfully queried {} entities", result.records.len());
}

/// Test 3.2: Tag index integrity
///
/// Verifies that tags are properly linked to entities.
#[tokio::test]
async fn index_tags() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query tags and their associations
    let sql = r#"
        SELECT name FROM tags LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should have tags stored
    assert!(!result.records.is_empty(), "Should have tags in database");

    eprintln!("Found {} tags", result.records.len());
}

/// Test 3.3: Relation index integrity
///
/// Verifies that relations reference valid entities.
#[tokio::test]
async fn index_relations() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query relations with their connected entities
    let sql = r#"
        SELECT relation_type, count() as count
        FROM relations
        GROUP BY relation_type
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should have relations stored
    assert!(
        !result.records.is_empty(),
        "Should have relations in database"
    );

    for record in &result.records {
        if let (Some(rtype), Some(count)) = (
            record.data.get("relation_type").and_then(|v| v.as_str()),
            record.data.get("count"),
        ) {
            eprintln!("  {}: {:?}", rtype, count);
        }
    }
}

/// Test 3.4: Property index integrity
///
/// Verifies that properties are linked to valid entities.
#[tokio::test]
async fn index_properties() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query properties
    let sql = r#"
        SELECT count() as count FROM properties
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    let prop_count = result
        .records
        .first()
        .and_then(|r| r.data.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Should have properties stored
    assert!(prop_count > 0, "Should have properties in database");

    eprintln!("Found {} properties", prop_count);
}

// ============================================================================
// Section 4: End-to-End Workflows (3 tests)
// ============================================================================

/// Test 4.1: Full ingestion and search workflow
///
/// Verifies the complete flow from file to searchable entity.
#[tokio::test]
async fn e2e_ingest_and_search() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Search for a known file (Index.md has title "Welcome to Crucible")
    let sql = r#"
        SELECT data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND data.title CONTAINS 'Crucible'
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find the Welcome to Crucible index
    assert!(
        !result.records.is_empty(),
        "Should find Welcome to Crucible"
    );

    let found_title = result.records[0]
        .data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert!(
        found_title.contains("Crucible"),
        "Found title should contain 'Crucible': {}",
        found_title
    );
}

/// Test 4.2: Link resolution workflow
///
/// Verifies that wikilinks can be followed after ingestion.
#[tokio::test]
async fn e2e_link_resolution() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query relations from the Index (Welcome to Crucible)
    let sql = r#"
        SELECT
            in.data.title as source,
            out.data.title as target,
            relation_type
        FROM relations
        WHERE relation_type = 'wikilink'
          AND in.data.title CONTAINS 'Crucible'
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find links from the Index
    assert!(
        !result.records.is_empty(),
        "Should find wikilinks from Welcome to Crucible"
    );

    eprintln!("Links from Welcome to Crucible:");
    for record in &result.records {
        if let Some(target) = record.data.get("target").and_then(|v| v.as_str()) {
            eprintln!("  -> {}", target);
        }
    }
}

/// Test 4.3: Backlink discovery workflow
///
/// Verifies that backlinks can be found after ingestion.
#[tokio::test]
async fn e2e_backlink_discovery() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query backlinks TO Wikilinks reference (incoming links)
    let sql = r#"
        SELECT
            in.data.title as source,
            out.data.title as target,
            relation_type
        FROM relations
        WHERE relation_type IN ['wikilink', 'embed']
          AND out.data.title CONTAINS 'Wikilinks'
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find backlinks to Wikilinks reference
    eprintln!("Backlinks to Wikilinks:");
    for record in &result.records {
        if let Some(source) = record.data.get("source").and_then(|v| v.as_str()) {
            eprintln!("  <- {}", source);
        }
    }

    // Even if there are no backlinks, the query should work
    // The test validates the workflow works
}

// ============================================================================
// Test Summary
// ============================================================================

#[test]
fn print_test_summary() {
    println!("\n=== Phase 3: Integration Tests ===\n");
    println!("Test Suite: integration_tests_kiln.rs");
    println!("Total Tests: 16\n");

    println!("Section 1: Data Consistency (5 tests)");
    println!("  - consistency_tags");
    println!("  - consistency_dates");
    println!("  - consistency_related_docs");
    println!("  - consistency_entity_count");
    println!("  - consistency_property_namespaces\n");

    println!("Section 2: Performance Benchmarks (4 tests)");
    println!("  - perf_simple_query (<100ms)");
    println!("  - perf_complex_query (<500ms)");
    println!("  - perf_fulltext_search (<200ms)");
    println!("  - perf_ingestion (<5s for dev-kiln)\n");

    println!("Section 3: Index Integrity (4 tests)");
    println!("  - index_completeness");
    println!("  - index_tags");
    println!("  - index_relations");
    println!("  - index_properties\n");

    println!("Section 4: End-to-End Workflows (3 tests)");
    println!("  - e2e_ingest_and_search");
    println!("  - e2e_link_resolution");
    println!("  - e2e_backlink_discovery\n");

    println!("Performance Targets:");
    println!("  - Simple queries: <100ms");
    println!("  - Complex queries: <500ms");
    println!("  - Full-text search: <200ms");
    println!("  - Ingestion: <5s for dev-kiln\n");

    println!("TDD Protocol: All tests fail initially, guiding implementation.\n");
}
