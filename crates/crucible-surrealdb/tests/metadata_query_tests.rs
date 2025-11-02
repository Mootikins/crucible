//! Tests for complex metadata and frontmatter queries
//!
//! These tests verify:
//! - Nested property queries (metadata.project.status)
//! - Array property matching
//! - Date range queries
//! - Numeric comparisons
//! - Missing/null metadata handling
//! - Malformed YAML edge cases
//! - Type coercion scenarios

mod common;

use common::*;


#[tokio::test]
async fn test_nested_property_query() {
    let (client, kiln_root) = setup_test_client().await;

    let frontmatter = "project:\n  name: crucible\n  status: active";
    let content = "Content here";

    create_test_note_with_frontmatter(
        &client,
        "nested.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    let query = "SELECT path FROM notes WHERE metadata.project.status = 'active'";
    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note with nested metadata property"
    );
}

#[tokio::test]
async fn test_deeply_nested_property_query() {
    let (client, kiln_root) = setup_test_client().await;

    let frontmatter = "config:\n  database:\n    connection:\n      host: localhost\n      port: 8000";
    let content = "Content";

    create_test_note_with_frontmatter(
        &client,
        "deep.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for deeply nested property
    let query = "SELECT path FROM notes WHERE metadata.config.database.connection.port = 8000";

    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note with deeply nested property"
    );
}

#[tokio::test]
async fn test_array_property_contains() {
    let (client, kiln_root) = setup_test_client().await;

    let frontmatter = "authors:\n  - Alice\n  - Bob\n  - Charlie\ntags:\n  - rust\n  - database";
    let content = "Content";

    create_test_note_with_frontmatter(
        &client,
        "array.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for array containment
    let query = "SELECT path FROM notes WHERE 'Alice' IN metadata.authors";

    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note where array contains 'Alice'"
    );
}

#[tokio::test]
async fn test_array_property_containsall() {
    let (client, kiln_root) = setup_test_client().await;

    let note1_frontmatter = "tags: [rust, database, async]";
    let note1_content = "Note 1";

    let note2_frontmatter = "tags: [rust, web]";
    let note2_content = "Note 2";

    create_test_note_with_frontmatter(
        &client,
        "note1.md",
        note1_content,
        note1_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "note2.md",
        note2_content,
        note2_frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for notes containing ALL specified tags
    let query = "SELECT path FROM notes WHERE metadata.tags CONTAINSALL ['rust', 'database']";

    let result = client.query(query, &[]).await.unwrap();

    // Should only find note1 (has both rust and database)
    assert_eq!(
        result.records.len(),
        1,
        "Should find exactly 1 note with both tags"
    );
}

#[tokio::test]
async fn test_array_property_containsany() {
    let (client, kiln_root) = setup_test_client().await;

    let note1_frontmatter = "tags: [python, django]";
    let note1_content = "Note 1";

    let note2_frontmatter = "tags: [rust, actix]";
    let note2_content = "Note 2";

    let note3_frontmatter = "tags: [go, gin]";
    let note3_content = "Note 3";

    create_test_note_with_frontmatter(
        &client,
        "note1.md",
        note1_content,
        note1_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "note2.md",
        note2_content,
        note2_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "note3.md",
        note3_content,
        note3_frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for notes containing ANY of the specified tags
    let query = "SELECT path FROM notes WHERE metadata.tags CONTAINSANY ['rust', 'python']";

    let result = client.query(query, &[]).await.unwrap();

    // Should find note1 (python) and note2 (rust)
    assert_eq!(
        result.records.len(),
        2,
        "Should find 2 notes with either rust or python"
    );
}

#[tokio::test]
async fn test_date_range_query() {
    let (client, kiln_root) = setup_test_client().await;

    let note1_frontmatter = "due_date: \"2025-12-31\"";
    let note1_content = "Future task";

    let note2_frontmatter = "due_date: \"2025-10-15\"";
    let note2_content = "Past task";

    let note3_frontmatter = "due_date: \"2026-06-01\"";
    let note3_content = "Far future";

    create_test_note_with_frontmatter(
        &client,
        "future.md",
        note1_content,
        note1_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "past.md",
        note2_content,
        note2_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "far.md",
        note3_content,
        note3_frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for dates after November 1, 2025
    let query = "SELECT path FROM notes WHERE metadata.due_date > '2025-11-01'";

    let result = client.query(query, &[]).await.unwrap();

    // Should find future.md and far.md (2 notes)
    assert_eq!(
        result.records.len(),
        2,
        "Should find 2 notes with dates after 2025-11-01"
    );
}

#[tokio::test]
async fn test_numeric_comparison_queries() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with numeric metadata
    let notes = [
        ("low.md", "priority: 1"),
        ("medium.md", "priority: 5"),
        ("high.md", "priority: 10"),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Query for priority >= 5
    let query = "SELECT path FROM notes WHERE metadata.priority >= 5";

    let result = client.query(query, &[]).await.unwrap();

    // Should find medium.md and high.md
    assert_eq!(
        result.records.len(),
        2,
        "Should find 2 notes with priority >= 5"
    );
}

#[tokio::test]
async fn test_numeric_range_query() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with scores
    let notes = [
        ("score10.md", "score: 10"),
        ("score50.md", "score: 50"),
        ("score75.md", "score: 75"),
        ("score90.md", "score: 90"),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Query for score between 40 and 80
    let query = "SELECT path FROM notes WHERE metadata.score >= 40 AND metadata.score <= 80";

    let result = client.query(query, &[]).await.unwrap();

    // Should find score50.md and score75.md
    assert_eq!(
        result.records.len(),
        2,
        "Should find 2 notes with score in range [40, 80]"
    );
}

#[tokio::test]
async fn test_missing_metadata_field() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes, some with status, some without
    let note1_frontmatter = "status: active";
    let note1_content = "Has status";

    let note2_frontmatter = "title: Test";
    let note2_content = "No status";

    create_test_note_with_frontmatter(
        &client,
        "with_status.md",
        note1_content,
        note1_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "without_status.md",
        note2_content,
        note2_frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for notes with status field present
    let query = "SELECT path FROM notes WHERE metadata.status != NONE";

    let result = client.query(query, &[]).await.unwrap();

    // Should find only with_status.md
    assert!(
        !result.records.is_empty(),
        "Should find notes with status field present"
    );
}

#[tokio::test]
async fn test_null_metadata_value() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with explicit null value
    let frontmatter = "assignee: null\nstatus: pending";
    let content = "Content";

    create_test_note_with_frontmatter(
        &client,
        "null_value.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for null assignee
    let query = "SELECT path FROM notes WHERE metadata.assignee IS NULL";

    let result = client.query(query, &[]).await.unwrap();

    // Should find the note with null assignee
    assert!(
        !result.records.is_empty(),
        "Should find note with null metadata value"
    );
}

#[tokio::test]
async fn test_empty_frontmatter() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with empty frontmatter
    let frontmatter = "";
    let content = "Content with empty frontmatter";

    create_test_note_with_frontmatter(
        &client,
        "empty_fm.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query should handle empty metadata gracefully
    let query = "SELECT path FROM notes WHERE path = 'empty_fm.md'";

    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note with empty frontmatter"
    );
}

#[tokio::test]
async fn test_no_frontmatter() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note without frontmatter
    let content = "Just plain content, no frontmatter";

    create_test_note(
        &client,
        "no_fm.md",
        content,
        &kiln_root
    ).await.unwrap();

    // Query for any metadata should handle gracefully
    let query = "SELECT path FROM notes WHERE metadata.status = 'active'";

    let result = client.query(query, &[]).await.unwrap();

    // Should return empty (note has no metadata)
    assert!(
        result.records.is_empty() || !result.records.iter().any(|r| r.data.get("path").and_then(|v| v.as_str()) == Some("no_fm.md")),
        "Note without frontmatter should not match metadata queries"
    );
}

#[tokio::test]
async fn test_boolean_metadata_query() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with boolean metadata
    let notes = [
        ("published.md", "published: true"),
        ("draft.md", "published: false"),
        ("unknown.md", "title: Unknown"),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Query for published = true
    let query = "SELECT path FROM notes WHERE metadata.published = true";

    let result = client.query(query, &[]).await.unwrap();

    // Should find only published.md
    assert_eq!(
        result.records.len(),
        1,
        "Should find exactly 1 published note"
    );
}

#[tokio::test]
async fn test_string_comparison_case_sensitive() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with different case
    let note1_frontmatter = "status: Active";
    let note1_content = "Capitalized";

    let note2_frontmatter = "status: active";
    let note2_content = "Lowercase";

    create_test_note_with_frontmatter(
        &client,
        "cap.md",
        note1_content,
        note1_frontmatter,
        &kiln_root
    ).await.unwrap();

    create_test_note_with_frontmatter(
        &client,
        "lower.md",
        note2_content,
        note2_frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query with exact case match
    let query = "SELECT path FROM notes WHERE metadata.status = 'active'";

    let result = client.query(query, &[]).await.unwrap();

    // Should only find lowercase (case-sensitive by default)
    assert_eq!(
        result.records.len(),
        1,
        "Case-sensitive query should find only exact match"
    );
}

#[tokio::test]
async fn test_multiple_metadata_conditions_and() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with various metadata combinations
    let notes = [
        ("match.md", "status: active\npriority: high"),
        ("partial1.md", "status: active\npriority: low"),
        ("partial2.md", "status: done\npriority: high"),
        ("none.md", "status: pending\npriority: medium"),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Query with AND condition
    let query = "SELECT path FROM notes WHERE metadata.status = 'active' AND metadata.priority = 'high'";

    let result = client.query(query, &[]).await.unwrap();

    // Should only find match.md
    assert_eq!(
        result.records.len(),
        1,
        "AND query should find only note matching both conditions"
    );
}

#[tokio::test]
async fn test_multiple_metadata_conditions_or() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with various statuses
    let notes = [
        ("done.md", "status: done"),
        ("archived.md", "status: archived"),
        ("active.md", "status: active"),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Query with OR condition
    let query = "SELECT path FROM notes WHERE metadata.status = 'done' OR metadata.status = 'archived'";

    let result = client.query(query, &[]).await.unwrap();

    // Should find done.md and archived.md
    assert_eq!(
        result.records.len(),
        2,
        "OR query should find notes matching either condition"
    );
}

#[tokio::test]
async fn test_type_coercion_string_to_number() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with numeric value as string
    let frontmatter = "count: \"42\"";
    let content = "Content";

    create_test_note_with_frontmatter(
        &client,
        "string_num.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query with numeric comparison (may or may not coerce)
    // This tests the database's type coercion behavior
    let query = "SELECT path FROM notes WHERE metadata.count = 42";

    let result = client.query(query, &[]).await;

    // Either coerces and finds it, or doesn't coerce and returns empty
    // Both are valid behaviors, we're testing it doesn't crash
    assert!(result.is_ok(), "Type coercion query should not error");
}

#[tokio::test]
async fn test_metadata_with_special_characters() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with special characters in metadata
    let frontmatter = "title: \"Test: Colon & Ampersand\"\ndescription: \"Quote's apostrophe\"\nurl: \"https://example.com/path?query=value\"";
    let content = "Content";

    create_test_note_with_frontmatter(
        &client,
        "special.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query should handle special characters
    let query = r#"SELECT path FROM notes WHERE metadata.title = "Test: Colon & Ampersand""#;

    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note with special characters in metadata"
    );
}

#[tokio::test]
async fn test_metadata_with_unicode() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with Unicode metadata
    let frontmatter = "title: \"日本語タイトル\"\nauthor: \"François Müller\"\nemoji: \"🦀🚀\"";
    let content = "Content with Unicode metadata";

    create_test_note_with_frontmatter(
        &client,
        "unicode.md",
        content,
        frontmatter,
        &kiln_root
    ).await.unwrap();

    // Query for Unicode value
    let query = r#"SELECT path FROM notes WHERE metadata.title = "日本語タイトル""#;

    let result = client.query(query, &[]).await.unwrap();

    assert!(
        !result.records.is_empty(),
        "Should find note with Unicode metadata"
    );
}

#[tokio::test]
async fn test_complex_combined_metadata_query() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with complex metadata
    let notes = [
        (
            "match.md",
            "status: active\npriority: 5\ntags: [rust, database]\ndue_date: \"2025-12-01\"",
        ),
        (
            "no_match1.md",
            "status: done\npriority: 8\ntags: [rust, web]\ndue_date: \"2025-12-01\"",
        ),
        (
            "no_match2.md",
            "status: active\npriority: 3\ntags: [python]\ndue_date: \"2024-01-01\"",
        ),
    ];

    for (name, meta) in notes.iter() {
        let content = "Content";
        create_test_note_with_frontmatter(
            &client,
            name,
            content,
            meta,
            &kiln_root
        ).await.unwrap();
    }

    // Complex query: active, priority >= 5, has rust tag, due after Nov 2025
    let query = "SELECT path FROM notes WHERE metadata.status = 'active' AND metadata.priority >= 5 AND 'rust' IN metadata.tags AND metadata.due_date > '2025-11-01'";

    let result = client.query(query, &[]).await.unwrap();

    // Should only find match.md
    assert_eq!(
        result.records.len(),
        1,
        "Complex combined query should find exactly 1 matching note"
    );
}
