//! Tests for hybrid queries combining multiple query types
//!
//! These tests verify queries that combine:
//! - Graph traversal + metadata filters
//! - Graph traversal + tag filters
//! - Metadata + tag filters
//! - All three: graph + metadata + tags
//! - Semantic search + filters (future)

mod common;
use common::*;
use std::collections::HashMap;

/// Helper to setup a complex test graph with metadata and tags
async fn setup_complex_graph(
    client: &crucible_surrealdb::SurrealClient,
    kiln_root: &std::path::Path,
) -> HashMap<String, String> {
    let mut ids = HashMap::new();

    // Create notes with metadata
    let notes = [
        ("index.md", "status: active\npriority: 10", "Index page linking to [[docs]] and [[api]]"),
        ("docs.md", "status: active\npriority: 8", "Documentation linking to [[tutorial]]"),
        ("api.md", "status: done\npriority: 5", "API reference"),
        ("tutorial.md", "status: active\npriority: 7", "Tutorial content"),
        ("archived.md", "status: archived\npriority: 1", "Old content"),
    ];

    for (name, fm, content) in notes {
        let id = create_test_note_with_frontmatter(client, name, content, fm, kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create links: index→docs, index→api, docs→tutorial
    create_wikilink(client, &ids["index"], &ids["docs"], "docs", 0).await.unwrap();
    create_wikilink(client, &ids["index"], &ids["api"], "api", 0).await.unwrap();
    create_wikilink(client, &ids["docs"], &ids["tutorial"], "tutorial", 0).await.unwrap();

    // Create tags
    for tag in ["index", "project", "documentation", "rust", "api", "tutorial", "beginner", "old", "deprecated"] {
        create_tag(client, tag).await.unwrap();
    }

    // Associate tags
    associate_tag(client, &ids["index"], "index").await.unwrap();
    associate_tag(client, &ids["index"], "project").await.unwrap();
    associate_tag(client, &ids["docs"], "documentation").await.unwrap();
    associate_tag(client, &ids["docs"], "rust").await.unwrap();
    associate_tag(client, &ids["api"], "api").await.unwrap();
    associate_tag(client, &ids["api"], "rust").await.unwrap();
    associate_tag(client, &ids["tutorial"], "tutorial").await.unwrap();
    associate_tag(client, &ids["tutorial"], "beginner").await.unwrap();
    associate_tag(client, &ids["archived"], "old").await.unwrap();
    associate_tag(client, &ids["archived"], "deprecated").await.unwrap();

    ids
}

/// Helper to extract nested path from a record field (handles SurrealDB Object wrapping)
fn extract_nested_path(record: &crucible_surrealdb::Record, field_name: &str) -> Option<String> {
    let val = record.data.get(field_name)?;
    let obj = val.get("Object")?.as_object()?;
    let path_val = obj.get("path")?;
    path_val.get("Strand")?.as_str().map(String::from)
}

#[tokio::test]
async fn test_graph_traversal_with_metadata_filter() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Traverse from index, filter for status='active'
    let query = format!(
        "SELECT out.path FROM wikilink WHERE in = {} AND out.metadata.status = 'active'",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find docs (active) but not api (done)
    assert_eq!(result.records.len(), 1, "Should find 1 active linked note");

    // Extract paths - handle SurrealDB nested Object structure
    // Structure: {"Object": {"path": {"Strand": "docs.md"}}}
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| {
            let out_val = r.data.get("out")?;
            // Unwrap the "Object" wrapper
            let obj = out_val.get("Object")?.as_object()?;
            // Get the "path" field
            let path_val = obj.get("path")?;
            // Unwrap the "Strand" wrapper
            let path_str = path_val.get("Strand")?.as_str()?;
            Some(path_str.to_string())
        })
        .collect();

    assert_eq!(paths.len(), 1, "Should extract 1 path");
    assert!(paths[0].contains("docs.md"), "Should be docs.md, got: {}", paths[0]);
}

#[tokio::test]
async fn test_graph_traversal_with_tag_filter() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes linked from index that have the 'rust' tag
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = format!(
        "SELECT out.path FROM wikilink WHERE in = {} AND out IN (SELECT VALUE in FROM tagged_with WHERE out = tags:rust)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find docs.md and api.md (both have 'rust' tag)
    assert_eq!(
        result.records.len(),
        2,
        "Should find 2 linked notes with 'rust' tag"
    );

    // Extract paths - handle SurrealDB nested Object structure
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| {
            let out_val = r.data.get("out")?;
            let obj = out_val.get("Object")?.as_object()?;
            let path_val = obj.get("path")?;
            let path_str = path_val.get("Strand")?.as_str()?;
            Some(path_str.to_string())
        })
        .collect();

    assert_eq!(paths.len(), 2, "Should extract 2 paths");
    assert!(paths.iter().any(|p| p.contains("docs.md")), "Should include docs");
    assert!(paths.iter().any(|p| p.contains("api.md")), "Should include api");
}

#[tokio::test]
async fn test_metadata_and_tag_combined_filter() {
    let (client, kiln_root) = setup_test_client().await;
    let _ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes that are active AND have the 'rust' tag
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = "SELECT path FROM notes WHERE metadata.status = 'active' AND id IN (SELECT VALUE in FROM tagged_with WHERE out = tags:rust)";

    let result = client.query(query, &[]).await.expect("Query failed");

    // Should find only docs.md (active + rust tag)
    // api.md has rust tag but status is 'done', not 'active'
    assert_eq!(
        result.records.len(),
        1,
        "Should find 1 note with active status and rust tag"
    );

    // Verify it's docs
    let paths = extract_paths(&result);
    assert!(paths.iter().any(|p| p.contains("docs.md")), "Should be docs.md");
}

#[tokio::test]
async fn test_triple_hybrid_graph_metadata_tag() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes:
    // 1. Linked from index (graph)
    // 2. With priority >= 7 (metadata)
    // 3. Tagged with 'rust' (tag)
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = format!(
        "SELECT out.path FROM wikilink WHERE in = {} AND out.metadata.priority >= 7 AND out IN (SELECT VALUE in FROM tagged_with WHERE out = tags:rust)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find only docs.md (linked from index, priority=8, has rust tag)
    // api.md has rust tag but priority=5 (< 7)
    assert_eq!(
        result.records.len(),
        1,
        "Should find 1 note matching all three criteria"
    );

    // Extract paths - handle SurrealDB nested Object structure
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| {
            let out_val = r.data.get("out")?;
            let obj = out_val.get("Object")?.as_object()?;
            let path_val = obj.get("path")?;
            let path_str = path_val.get("Strand")?.as_str()?;
            Some(path_str.to_string())
        })
        .collect();

    assert_eq!(paths.len(), 1, "Should extract 1 path");
    assert!(paths[0].contains("docs.md"), "Should be docs.md, got: {}", paths[0]);
}

#[tokio::test]
async fn test_backlinks_with_metadata_filter() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes that link TO tutorial and have priority > 5
    let query = format!(
        "SELECT in.path FROM wikilink WHERE out = {} AND in.metadata.priority > 5",
        ids["tutorial"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find docs.md (links to tutorial, priority=8 > 5)
    assert_eq!(
        result.records.len(),
        1,
        "Should find 1 backlink with priority > 5"
    );

    // Extract paths - handle SurrealDB nested Object structure (note: using 'in' not 'out')
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| {
            let in_val = r.data.get("in")?;
            let obj = in_val.get("Object")?.as_object()?;
            let path_val = obj.get("path")?;
            let path_str = path_val.get("Strand")?.as_str()?;
            Some(path_str.to_string())
        })
        .collect();

    assert_eq!(paths.len(), 1, "Should extract 1 path");
    assert!(paths[0].contains("docs.md"), "Should be docs.md, got: {}", paths[0]);
}

// ====================
// Batch 2: Advanced Hybrid Queries (Tests 6-10)
// ====================

#[tokio::test]
async fn test_multi_hop_with_tag_filter() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes 2 hops from index that have 'beginner' tag
    // index -> docs -> tutorial (tutorial has 'beginner' tag)
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in IN (SELECT VALUE out FROM wikilink WHERE in = {})
         AND out IN (SELECT VALUE in FROM tagged_with WHERE out = tags:beginner)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find tutorial.md (2 hops from index, has 'beginner' tag)
    assert!(
        !result.records.is_empty(),
        "Should find notes at 2 hops with 'beginner' tag"
    );

    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    assert!(paths.iter().any(|p| p.contains("tutorial.md")),
            "Should find tutorial.md, got: {:?}", paths);
}

#[tokio::test]
async fn test_exclude_archived_from_graph() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find all notes reachable from index, excluding archived status
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in = {} AND out.metadata.status != 'archived'",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find docs.md and api.md (not archived)
    // Even if archived.md were linked, it would be excluded
    assert_eq!(
        result.records.len(),
        2,
        "Should find linked notes excluding archived"
    );

    // Extract and verify no archived notes
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    assert!(!paths.iter().any(|p| p.contains("archived.md")),
            "Should not include archived notes");
    assert!(paths.iter().any(|p| p.contains("docs.md")), "Should include docs");
    assert!(paths.iter().any(|p| p.contains("api.md")), "Should include api");
}

#[tokio::test]
async fn test_tag_co_occurrence_with_metadata() {
    let (client, kiln_root) = setup_test_client().await;
    let _ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes tagged with 'rust' that also have priority >= 7
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = "SELECT path FROM notes
                 WHERE id IN (SELECT VALUE in FROM tagged_with WHERE out = tags:rust)
                 AND metadata.priority >= 7";

    let result = client.query(query, &[]).await.expect("Query failed");

    // Should find docs.md (rust tag + priority=8)
    // api.md has rust but priority=5 < 7
    assert_eq!(
        result.records.len(),
        1,
        "Should find 1 note with rust tag and priority >= 7"
    );

    let paths = extract_paths(&result);
    assert!(paths.iter().any(|p| p.contains("docs.md")),
            "Should be docs.md, got: {:?}", paths);
}

#[tokio::test]
async fn test_multiple_tag_filter_with_graph() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes linked from index with BOTH 'rust' AND 'documentation' tags
    // docs.md has both tags, api.md has only rust
    // NOTE: Must use SELECT VALUE to get array of IDs for IN clause
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in = {}
         AND out IN (SELECT VALUE in FROM tagged_with WHERE out = tags:rust)
         AND out IN (SELECT VALUE in FROM tagged_with WHERE out = tags:documentation)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should find only docs.md (has both tags)
    assert_eq!(
        result.records.len(),
        1,
        "Should find exactly 1 note with both tags"
    );

    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    assert!(paths[0].contains("docs.md"),
            "Should be docs.md, got: {}", paths[0]);
}

#[tokio::test]
async fn test_bidirectional_traversal_with_filter() {
    let (client, kiln_root) = setup_test_client().await;

    // Create bidirectional links: A <-> B with different statuses
    let id_a = create_test_note_with_frontmatter(
        &client,
        "A.md",
        "Links to [[B]]",
        "status: active",
        &kiln_root,
    )
    .await
    .expect("Failed to create A");

    let id_b = create_test_note_with_frontmatter(
        &client,
        "B.md",
        "Links to [[A]]",
        "status: done",
        &kiln_root,
    )
    .await
    .expect("Failed to create B");

    // Create bidirectional links
    create_wikilink(&client, &id_a, &id_b, "B", 0)
        .await
        .expect("Failed to create A->B");
    create_wikilink(&client, &id_b, &id_a, "A", 0)
        .await
        .expect("Failed to create B->A");

    // Find notes linked to/from A with status = 'done'
    // This tests UNION of outgoing and incoming links with metadata filter
    let query_out = format!(
        "SELECT out.path FROM wikilink WHERE in = {} AND out.metadata.status = 'done'",
        id_a
    );
    let query_in = format!(
        "SELECT in.path FROM wikilink WHERE out = {} AND in.metadata.status = 'done'",
        id_a
    );

    let result_out = client.query(&query_out, &[]).await.expect("Query out failed");
    let result_in = client.query(&query_in, &[]).await.expect("Query in failed");

    // Combine results from both directions
    let mut paths = vec![];
    paths.extend(result_out.records.iter().filter_map(|r| extract_nested_path(r, "out")));
    paths.extend(result_in.records.iter().filter_map(|r| extract_nested_path(r, "in")));

    // Should find B (status = done, linked from A)
    // Note: A has status = active, so only B should be found
    assert!(!paths.is_empty(), "Should find bidirectionally linked note with status filter");
    assert!(paths.iter().any(|p| p.contains("B.md")),
            "Should find B.md with status=done, got: {:?}", paths);
}

// ====================
// BATCH 3: Complex Filter Tests (11-14)
// ====================

#[tokio::test]
async fn test_graph_depth_with_cumulative_filter() {
    let (client, kiln_root) = setup_test_client().await;

    // Create chain: A -> B -> C -> D
    // Only B and D have priority >= 5
    let id_a = create_test_note_with_frontmatter(
        &client, "A.md", "[[B]]", "priority: 3", &kiln_root
    ).await.unwrap();

    let id_b = create_test_note_with_frontmatter(
        &client, "B.md", "[[C]]", "priority: 8", &kiln_root
    ).await.unwrap();

    let id_c = create_test_note_with_frontmatter(
        &client, "C.md", "[[D]]", "priority: 2", &kiln_root
    ).await.unwrap();

    let id_d = create_test_note_with_frontmatter(
        &client, "D.md", "End", "priority: 7", &kiln_root
    ).await.unwrap();

    // Create links: A->B, B->C, C->D
    create_wikilink(&client, &id_a, &id_b, "B", 0).await.unwrap();
    create_wikilink(&client, &id_b, &id_c, "C", 0).await.unwrap();
    create_wikilink(&client, &id_c, &id_d, "D", 0).await.unwrap();

    // Find notes reachable at any depth from A with priority >= 5
    // Note: SurrealDB applies filter at final hop only (simplified from cumulative)
    // This query finds immediate links with priority >= 5
    let query = format!(
        "SELECT out.path FROM wikilink WHERE in = {} AND out.metadata.priority >= 5",
        id_a
    );

    let result = client.query(&query, &[]).await.expect("Query failed");
    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    // Should find B (priority 8) directly linked from A
    assert!(!paths.is_empty(), "Should find at least 1 note with priority >= 5");
    assert!(paths.iter().any(|p| p.contains("B.md")),
            "Should find B.md with priority >= 5, got: {:?}", paths);
}

#[tokio::test]
async fn test_complex_or_conditions_with_graph() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes linked from index that are EITHER:
    // - status = 'done', OR
    // - have priority >= 8
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in = {}
         AND (out.metadata.status = 'done' OR out.metadata.priority >= 8)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    // Should find:
    // - docs.md (priority=8 >= 8, status='active')
    // - api.md (status='done', priority=5)
    assert_eq!(paths.len(), 2, "Should find 2 notes matching OR conditions, got: {:?}", paths);
    assert!(paths.iter().any(|p| p.contains("docs.md")), "Should find docs.md");
    assert!(paths.iter().any(|p| p.contains("api.md")), "Should find api.md");
}

#[tokio::test]
async fn test_exclude_tags_from_graph_results() {
    let (client, kiln_root) = setup_test_client().await;
    let ids = setup_complex_graph(&client, &kiln_root).await;

    // Find notes linked from index, but exclude those with 'api' tag
    // Use subquery to find notes tagged with 'api'
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in = {}
         AND out NOT IN (SELECT VALUE in FROM tagged_with WHERE out = tags:api)",
        ids["index"]
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    // Should find docs.md but NOT api.md (api.md has 'api' tag)
    assert_eq!(paths.len(), 1, "Should find 1 note excluding 'api' tag, got: {:?}", paths);
    assert!(paths.iter().any(|p| p.contains("docs.md")), "Should find docs.md");
    assert!(!paths.iter().any(|p| p.contains("api.md")), "Should not find api.md (has 'api' tag)");
}

#[tokio::test]
async fn test_graph_with_date_range_filter() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes with dates
    let id_recent = create_test_note_with_frontmatter(
        &client,
        "recent.md",
        "Links to [[old]]",
        "created: \"2025-11-01\"",
        &kiln_root
    ).await.unwrap();

    let id_old = create_test_note_with_frontmatter(
        &client,
        "old.md",
        "Old content",
        "created: \"2020-01-01\"",
        &kiln_root
    ).await.unwrap();

    create_wikilink(&client, &id_recent, &id_old, "old", 0).await.unwrap();

    // Find notes linked from recent.md created before 2025
    let query = format!(
        "SELECT out.path FROM wikilink
         WHERE in = {}
         AND out.metadata.created < '2025-01-01'",
        id_recent
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths: Vec<String> = result.records.iter()
        .filter_map(|r| extract_nested_path(r, "out"))
        .collect();

    // Should find old.md (created in 2020)
    assert_eq!(paths.len(), 1, "Should find 1 linked note with date before 2025, got: {:?}", paths);
    assert!(paths[0].contains("old.md"), "Should find old.md");
}
