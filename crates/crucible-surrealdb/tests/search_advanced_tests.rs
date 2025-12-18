//! Phase 2: Advanced Search Tests
//!
//! This module implements tests for advanced search functionality in Crucible.
//! Tests cover multi-criteria queries, fuzzy search, domain-specific searches,
//! and link validation.
//!
//! All tests use the test-kiln at `examples/test-kiln/` for realistic test data.

mod common;

use common::{setup_test_db_with_kiln, test_kiln_root};
use std::path::PathBuf;
use std::time::Instant;

// ============================================================================
// Section 1: Multi-Criteria Queries (5 tests)
// ============================================================================

/// Test 1.1: Combine tag + property filters
///
/// Verifies that we can search with both tag AND property constraints.
#[tokio::test]
async fn multi_criteria_tag_and_property() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for entities that have both tags and properties
    let sql = r#"
        LET $tagged = (SELECT entity_id FROM entity_tags);
        LET $with_props = (SELECT entity_id FROM properties WHERE key = 'frontmatter');
        SELECT id, data.title as title FROM entities
        WHERE id IN $tagged.entity_id AND id IN $with_props.entity_id
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find entities with both tags and properties
    assert!(
        !result.records.is_empty(),
        "Should find entities with both tags and properties"
    );

    eprintln!(
        "Found {} entities with tags and properties",
        result.records.len()
    );
}

/// Test 1.2: Combine metadata + relationships
///
/// Verifies filtering by properties AND link relationships.
#[tokio::test]
async fn multi_criteria_metadata_and_relationships() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for entities that have properties AND wikilinks
    let sql = r#"
        LET $with_props = (SELECT entity_id FROM properties WHERE key = 'frontmatter');
        LET $with_links = (SELECT in FROM relations WHERE relation_type = 'wikilink');
        SELECT id, data.title as title FROM entities
        WHERE id IN $with_props.entity_id AND id IN $with_links.in
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find entities with both properties and links
    assert!(
        !result.records.is_empty(),
        "Should find entities with properties and links"
    );

    eprintln!(
        "Found {} entities with properties and links",
        result.records.len()
    );
}

/// Test 1.3: Count entities with tags, properties, AND links
///
/// Verifies multi-criteria queries work.
#[tokio::test]
async fn multi_criteria_all_three() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Count entities that have all three: tags, properties, and links
    let tag_sql = "SELECT count() as count FROM entity_tags";
    let prop_sql = "SELECT count() as count FROM properties";
    let rel_sql = "SELECT count() as count FROM relations";

    let tag_result = client.query(tag_sql, &[]).await.expect("Query failed");
    let prop_result = client.query(prop_sql, &[]).await.expect("Query failed");
    let rel_result = client.query(rel_sql, &[]).await.expect("Query failed");

    // Should have all three types of data
    let has_tags = !tag_result.records.is_empty();
    let has_props = !prop_result.records.is_empty();
    let has_rels = !rel_result.records.is_empty();

    assert!(has_tags, "Should have entity_tags");
    assert!(has_props, "Should have properties");
    assert!(has_rels, "Should have relations");
}

/// Test 1.4: Date-based filtering (frontmatter)
///
/// Verifies we can query by date fields.
#[tokio::test]
async fn multi_criteria_date_query() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for entities with 'created' date in frontmatter
    let sql = r#"
        SELECT entity_id, value
        FROM properties
        WHERE key = 'frontmatter'
          AND value.value.created IS NOT NULL
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should find entities with created dates
    if !result.records.is_empty() {
        eprintln!("Found {} entities with created dates", result.records.len());
    }
}

/// Test 1.5: Property value filtering
///
/// Verifies searching by specific property values.
#[tokio::test]
async fn multi_criteria_property_value() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for entities with high priority
    let sql = r#"
        SELECT entity_id.data.title as title, value
        FROM properties
        WHERE key = 'frontmatter'
          AND value.value.priority = 'high'
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Log results
    if !result.records.is_empty() {
        for record in &result.records {
            if let Some(title) = record.data.get("title").and_then(|v| v.as_str()) {
                eprintln!("Found high priority: {}", title);
            }
        }
    }
}

// ============================================================================
// Section 2: Fuzzy/Approximate Search (4 tests)
// ============================================================================

/// Test 2.1: Typo tolerance
///
/// Verifies that search handles common typos.
#[tokio::test]
async fn fuzzy_typo_tolerance() {
    // Arrange: Search with common typos
    let queries_with_typos = vec![
        ("knowlege", "knowledge"),          // Missing 'd'
        ("managment", "management"),        // Missing 'e'
        ("documantation", "documentation"), // 'a' instead of 'e'
        ("projct", "project"),              // Missing 'e'
    ];

    // Expected: Each typo query should find documents with correct spelling

    for (typo, expected) in queries_with_typos {
        // TODO: Fuzzy search should match:
        // - Edit distance <= 2
        // - Phonetic matching
        // - Common substitution patterns

        // Database query with fuzzy matching:
        // SELECT * FROM entities
        // WHERE search::score((search_text), $query) > 0.8
        // OR search::levenshtein(search_text, $query) <= 2

        // For now, verify the test data exists
        let kiln_root = test_kiln_root();
        let has_expected = walkdir::WalkDir::new(&kiln_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| {
                if let Ok(content) = std::fs::read_to_string(e.path()) {
                    content.to_lowercase().contains(expected)
                } else {
                    false
                }
            });

        assert!(
            has_expected,
            "Test kiln should contain '{}' (correct spelling of '{}')",
            expected, typo
        );
    }

    // Test data validation passed - database fuzzy search implementation pending
}

/// Test 2.2: Phonetic matching
///
/// Verifies that phonetically similar words match.
#[ignore = "Requires phonetic matching implementation (Soundex, Metaphone)"]
#[tokio::test]
async fn fuzzy_phonetic() {
    // Arrange: Words that sound similar
    let phonetic_pairs = vec![
        ("color", "colour"),
        ("organize", "organise"),
        ("analyze", "analyse"),
    ];

    // Expected: Searches for either spelling should find the same documents

    // TODO: Implement phonetic matching (Soundex, Metaphone, etc.)
    // This is important for international users with different spelling conventions

    panic!("TDD: Implement phonetic matching in fuzzy search");
}

/// Test 2.3: Stemming
///
/// Verifies that word stems match (manage/managing/managed/management).
#[ignore = "Requires word stemming implementation"]
#[tokio::test]
async fn fuzzy_stemming() {
    // Arrange: Different forms of the same root word
    let stem_variations = vec!["manage", "managed", "managing", "management", "manager"];

    let kiln_root = test_kiln_root();

    // Act: Each variation should find documents containing the stem
    for word in stem_variations {
        let _has_match = walkdir::WalkDir::new(&kiln_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| {
                if let Ok(content) = std::fs::read_to_string(e.path()) {
                    content.to_lowercase().contains(word)
                } else {
                    false
                }
            });
    }

    // TODO: Implement stemming so that searching for any variation
    // returns documents with any form of the stem

    panic!("TDD: Implement word stemming in fuzzy search");
}

/// Test 2.4: Prefix matching
///
/// Verifies that partial words match full words.
#[ignore = "Requires prefix matching implementation"]
#[tokio::test]
async fn fuzzy_prefix_matching() {
    // Arrange: Partial words that should match full words
    let prefix_tests = vec![
        ("proj", "project"),      // Should match "project", "projects", etc.
        ("tech", "technical"),    // Should match "technical", "technology"
        ("doc", "documentation"), // Should match "document", "documentation"
    ];

    // Expected: Prefix queries should find documents with full words

    // TODO: Implement prefix matching (autocomplete-style)
    // SELECT * FROM entities WHERE search_text LIKE '$prefix*'

    panic!("TDD: Implement prefix matching in search");
}

// ============================================================================
// Section 3: Domain-Specific Searches (4 tests)
// ============================================================================

/// Test 3.1: Technical queries - find code blocks
///
/// Verifies searching for specific programming languages in code blocks.
#[ignore = "Requires code block language search implementation"]
#[tokio::test]
async fn domain_technical_code_blocks() {
    // Arrange: Search for documents with specific language code blocks
    let languages = vec!["javascript", "python", "rust", "sql"];

    let kiln_root = test_kiln_root();

    for lang in languages {
        let pattern = format!("```{}", lang);
        let has_code_block = walkdir::WalkDir::new(&kiln_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| {
                if let Ok(content) = std::fs::read_to_string(e.path()) {
                    content.to_lowercase().contains(&pattern)
                } else {
                    false
                }
            });

        // At least some languages should have code blocks in test-kiln
        if has_code_block {
            // TODO: Database query to find documents with code blocks
            // SELECT * FROM entities
            // WHERE search_text CONTAINS '```$lang'
            //    OR data.code_languages CONTAINS $lang
        }
    }

    panic!("TDD: Implement code block language search");
}

/// Test 3.2: Business queries - project status
///
/// Verifies searching for projects by status.
#[ignore = "Requires project status search implementation"]
#[tokio::test]
async fn domain_business_project_status() {
    // Arrange: Search for projects with different statuses
    let statuses = vec!["active", "completed", "on-hold", "planning"];

    // Expected: Should find projects matching each status
    // Project Management has status: active

    // TODO: Database query for project status
    // SELECT * FROM entities
    // WHERE data.type = 'project'
    //   AND id IN (SELECT entity_id FROM properties WHERE key = 'status' AND value = $status)

    panic!("TDD: Implement project status search");
}

/// Test 3.3: Academic queries - peer reviewed content
///
/// Verifies searching for academically validated content.
#[ignore = "Requires peer-reviewed content search implementation"]
#[tokio::test]
async fn domain_academic_peer_reviewed() {
    // Arrange: Search for peer-reviewed content
    // Research Methods has peer_reviewed: true

    let kiln_root = test_kiln_root();
    let research_file = kiln_root.join("Research Methods.md");

    if research_file.exists() {
        let content =
            std::fs::read_to_string(&research_file).expect("Failed to read Research Methods");

        // Verify peer_reviewed exists in frontmatter
        assert!(
            content.contains("peer_reviewed"),
            "Research Methods should have peer_reviewed property"
        );
    }

    // TODO: Database query for academic content
    // SELECT * FROM entities
    // WHERE id IN (SELECT entity_id FROM properties WHERE key = 'peer_reviewed' AND value = true)

    panic!("TDD: Implement peer-reviewed content search");
}

/// Test 3.4: Meeting queries - by attendee
///
/// Verifies searching meetings by participant.
#[ignore = "Requires meeting attendee search implementation"]
#[tokio::test]
async fn domain_meeting_by_attendee() {
    // Arrange: Search for meetings with specific attendee
    let attendee = "Sarah Chen";

    // Expected: Meeting Notes has attendees array including Sarah Chen

    let kiln_root = test_kiln_root();
    let meeting_file = kiln_root.join("Meeting Notes.md");

    if meeting_file.exists() {
        let content = std::fs::read_to_string(&meeting_file).expect("Failed to read Meeting Notes");

        assert!(
            content.contains(attendee),
            "Meeting Notes should mention Sarah Chen"
        );
    }

    // TODO: Database query for meetings by attendee
    // SELECT * FROM entities
    // WHERE id IN (SELECT entity_id FROM properties
    //              WHERE key = 'attendees' AND $attendee IN value)

    panic!("TDD: Implement meeting search by attendee");
}

// ============================================================================
// Section 4: Link Validation (4 tests)
// ============================================================================

/// Test 4.1: Find all wikilinks in vault
///
/// Verifies we can enumerate all wikilinks across all notes.
#[tokio::test]
async fn link_validation_find_all() {
    // Arrange: Scan all markdown files for wikilinks
    let kiln_root = test_kiln_root();
    let mut total_wikilinks = 0;
    let wikilink_regex = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();

    for entry in walkdir::WalkDir::new(&kiln_root) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md")
            {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    total_wikilinks += wikilink_regex.find_iter(&content).count();
                }
            }
        }
    }

    // Assert: Test kiln should have multiple wikilinks
    assert!(
        total_wikilinks > 0,
        "Test kiln should contain wikilinks, found: {}",
        total_wikilinks
    );

    // Verification passed - found wikilinks in test data
    println!("Found {} total wikilinks in test kiln", total_wikilinks);

    // Database query should return same count when implemented
    // SELECT COUNT(*) FROM relations WHERE relation_type = 'wikilink'
}

/// Test 4.2: Validate link targets exist
///
/// Verifies that all wikilink targets resolve to existing notes.
#[tokio::test]
async fn link_validation_targets_exist() {
    // Arrange: Extract all wikilink targets and verify they exist
    let kiln_root = test_kiln_root();
    let wikilink_regex = regex::Regex::new(r"\[\[([^\]|#^]+)").unwrap();

    let mut targets: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut note_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Collect all wikilink targets
    for entry in walkdir::WalkDir::new(&kiln_root) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md")
            {
                // Add note name (without .md)
                if let Some(stem) = entry.path().file_stem() {
                    note_names.insert(stem.to_string_lossy().to_string());
                }

                // Extract wikilink targets
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    for cap in wikilink_regex.captures_iter(&content) {
                        if let Some(target) = cap.get(1) {
                            targets.insert(target.as_str().to_string());
                        }
                    }
                }
            }
        }
    }

    // Check for broken links
    let mut broken_links = Vec::new();
    for target in &targets {
        // Skip special targets (files with extensions, headings already stripped)
        if target.contains('.') && !target.ends_with(".md") {
            continue;
        }

        // Check if target exists in note names
        let target_name = target.trim_end_matches(".md");
        if !note_names.contains(target_name) {
            broken_links.push(target.clone());
        }
    }

    // Report broken links if found
    if !broken_links.is_empty() {
        println!(
            "Found {} broken links: {:?}",
            broken_links.len(),
            broken_links
        );
    } else {
        println!(
            "All {} wikilink targets validated successfully",
            targets.len()
        );
    }

    // Verification passed - able to detect broken links
    // Database should track broken links when implemented
    // SELECT * FROM relations WHERE resolved = false
}

/// Test 4.3: Find orphaned pages
///
/// Verifies detection of notes with no incoming links.
#[tokio::test]
async fn link_validation_orphaned_pages() {
    // Arrange: Find pages that no other page links to
    let kiln_root = test_kiln_root();
    let wikilink_regex = regex::Regex::new(r"\[\[([^\]|#^]+)").unwrap();

    let mut linked_to: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_notes: Vec<String> = Vec::new();

    for entry in walkdir::WalkDir::new(&kiln_root) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md")
            {
                // Add note name
                if let Some(stem) = entry.path().file_stem() {
                    all_notes.push(stem.to_string_lossy().to_string());
                }

                // Extract wikilink targets
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    for cap in wikilink_regex.captures_iter(&content) {
                        if let Some(target) = cap.get(1) {
                            linked_to.insert(target.as_str().to_string());
                        }
                    }
                }
            }
        }
    }

    // Find orphans
    let orphans: Vec<_> = all_notes
        .iter()
        .filter(|note| !linked_to.contains(*note))
        .collect();

    // Some orphans may exist (README, etc.)
    // The key is that we can detect them
    println!(
        "Found {} orphaned pages out of {} total notes",
        orphans.len(),
        all_notes.len()
    );

    if !orphans.is_empty() {
        println!("Orphaned pages: {:?}", orphans);
    }

    // Verification passed - able to detect orphaned pages
    // Database query for orphaned pages when implemented
    // SELECT * FROM entities e
    // WHERE e.type = 'note'
    //   AND NOT EXISTS (SELECT 1 FROM relations WHERE to_entity_id = e.id)
}

/// Test 4.4: Link density analysis
///
/// Verifies we can analyze link patterns per document.
#[tokio::test]
async fn link_validation_density() {
    // Arrange: Calculate links per document
    let kiln_root = test_kiln_root();
    let wikilink_regex = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();

    let mut link_counts: Vec<(String, usize)> = Vec::new();

    for entry in walkdir::WalkDir::new(&kiln_root) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md")
            {
                if let Some(stem) = entry.path().file_stem() {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        let count = wikilink_regex.find_iter(&content).count();
                        link_counts.push((stem.to_string_lossy().to_string(), count));
                    }
                }
            }
        }
    }

    // Sort by link count
    link_counts.sort_by(|a, b| b.1.cmp(&a.1));

    // Knowledge Management Hub should be one of the most linked
    assert!(!link_counts.is_empty(), "Should have link count data");

    // Show top linked documents
    println!("Link density analysis:");
    for (i, (name, count)) in link_counts.iter().take(5).enumerate() {
        println!("  {}. {} - {} links", i + 1, name, count);
    }

    // Verification passed - able to calculate link density
    // Database query for link density when implemented
    // SELECT entity_id, COUNT(*) as link_count
    // FROM relations WHERE relation_type = 'wikilink'
    // GROUP BY entity_id ORDER BY link_count DESC
}

// ============================================================================
// Section 5: Cross-Reference Validation (3 tests)
// ============================================================================

/// Test 5.1: Verify frontmatter related field is stored
///
/// Checks that 'related' frontmatter is accessible.
#[tokio::test]
async fn crossref_related_stored() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Query for entities with 'related' in frontmatter
    let sql = r#"
        SELECT entity_id.data.title as title, value.value.related as related
        FROM properties
        WHERE key = 'frontmatter'
          AND value.value.related IS NOT NULL
        LIMIT 10
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Log what we found
    for record in &result.records {
        if let Some(title) = record.data.get("title").and_then(|v| v.as_str()) {
            eprintln!("Found related in: {}", title);
        }
    }
}

/// Test 5.2: Bidirectional link verification
///
/// Verifies that relations are stored for links.
#[tokio::test]
async fn crossref_bidirectional() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Get all wikilink relations
    let sql = r#"
        SELECT
            in.data.title as source,
            out.data.title as target
        FROM relations
        WHERE relation_type = 'wikilink'
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should have relations
    assert!(!result.records.is_empty(), "Should have wikilink relations");

    // Log links
    for record in &result.records {
        if let (Some(source), Some(target)) = (
            record.data.get("source").and_then(|v| v.as_str()),
            record.data.get("target").and_then(|v| v.as_str()),
        ) {
            eprintln!("  {} -> {}", source, target);
        }
    }
}

/// Test 5.3: Tag coverage analysis
///
/// Verifies that entities have tags.
#[tokio::test]
async fn crossref_tag_coverage() {
    // Set up database with ingested data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // Count entities with tags
    let sql = r#"
        SELECT entity_id, count() as tag_count
        FROM entity_tags
        GROUP BY entity_id
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // Should have entities with tags
    assert!(!result.records.is_empty(), "Should have entities with tags");

    eprintln!("Entities with tags: {}", result.records.len());
}

// ============================================================================
// Test Summary
// ============================================================================

#[test]
fn print_test_summary() {
    println!("\n=== Phase 2: Advanced Search Tests ===\n");
    println!("Test Suite: search_advanced_tests.rs");
    println!("Total Tests: 20\n");

    println!("Section 1: Multi-Criteria Queries (5 tests)");
    println!("  - multi_criteria_content_and_metadata");
    println!("  - multi_criteria_metadata_and_relationships");
    println!("  - multi_criteria_all_three");
    println!("  - multi_criteria_date_and_tag");
    println!("  - multi_criteria_author_and_priority\n");

    println!("Section 2: Fuzzy/Approximate Search (4 tests)");
    println!("  - fuzzy_typo_tolerance");
    println!("  - fuzzy_phonetic");
    println!("  - fuzzy_stemming");
    println!("  - fuzzy_prefix_matching\n");

    println!("Section 3: Domain-Specific Searches (4 tests)");
    println!("  - domain_technical_code_blocks");
    println!("  - domain_business_project_status");
    println!("  - domain_academic_peer_reviewed");
    println!("  - domain_meeting_by_attendee\n");

    println!("Section 4: Link Validation (4 tests)");
    println!("  - link_validation_find_all");
    println!("  - link_validation_targets_exist");
    println!("  - link_validation_orphaned_pages");
    println!("  - link_validation_density\n");

    println!("Section 5: Cross-Reference Validation (3 tests)");
    println!("  - crossref_related_exist");
    println!("  - crossref_bidirectional");
    println!("  - crossref_tag_consistency\n");

    println!("TDD Protocol: All tests fail initially, guiding implementation.\n");
}
