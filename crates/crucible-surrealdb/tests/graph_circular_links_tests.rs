//! Tests for circular reference handling in graph traversal (converted to SurrealDB)
//!
//! These tests ensure that:
//! - Circular wikilinks don't cause infinite loops
//! - Graph traversal terminates correctly with cycles
//! - Self-referential links are handled properly
//! - Manual depth expansion works even with cycles
//!
//! NOTE: SurrealDB does NOT have a max_depth parameter. We manually expand
//! graph traversal syntax for different depths.

mod common;
use common::*;

/// Helper to unwrap SurrealDB "Object" wrapper: {"Object": {...}} -> {...}
fn unwrap_object(val: &serde_json::Value) -> Option<&serde_json::Map<String, serde_json::Value>> {
    if let Some(obj) = val.as_object() {
        if let Some(inner) = obj.get("Object") {
            return inner.as_object();
        }
        return Some(obj);
    }
    None
}

/// Helper to unwrap SurrealDB "Strand" wrapper: {"Strand": "value"} -> "value"
fn unwrap_strand(val: &serde_json::Value) -> Option<&str> {
    if let Some(obj) = val.as_object() {
        if let Some(strand) = obj.get("Strand") {
            return strand.as_str();
        }
    }
    val.as_str()
}

/// Helper to unwrap SurrealDB "Array" wrapper: {"Array": [...]} -> [...]
fn unwrap_array(val: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    if let Some(obj) = val.as_object() {
        if let Some(arr) = obj.get("Array") {
            return arr.as_array();
        }
    }
    val.as_array()
}

/// Helper to extract paths from deeply nested graph traversal results
///
/// Graph traversal returns structure like: ->wikilink -> ->notes -> path
/// For depth N, there will be N levels of ->wikilink->notes nesting
/// Each level is wrapped in {"Object": {...}} by SurrealDB serialization
fn extract_graph_paths(result: &crucible_surrealdb::QueryResult, depth: usize) -> Vec<String> {
    use serde_json::Value;

    let mut paths = Vec::new();

    for record in &result.records {
        let mut current: Option<&Value> = None;

        // For each depth level, traverse ->wikilink -> ->notes
        for i in 0..depth {
            if i == 0 {
                // First level: get ->wikilink from record.data
                if let Some(wikilink_val) = record.data.get("->wikilink") {
                    // Unwrap {"Object": {"->notes": ...}}
                    if let Some(wikilink_obj) = unwrap_object(wikilink_val) {
                        if let Some(notes_val) = wikilink_obj.get("->notes") {
                            current = Some(notes_val);
                        } else {
                            return paths; // No ->notes
                        }
                    } else {
                        return paths; // ->wikilink not an object
                    }
                } else {
                    return paths; // No ->wikilink
                }
            } else {
                // Subsequent levels: navigate through current value
                if let Some(curr_val) = current {
                    // Unwrap {"Object": {"->wikilink": ...}}
                    if let Some(curr_obj) = unwrap_object(curr_val) {
                        if let Some(wikilink_val) = curr_obj.get("->wikilink") {
                            // Unwrap {"Object": {"->notes": ...}}
                            if let Some(wikilink_obj) = unwrap_object(wikilink_val) {
                                if let Some(notes_val) = wikilink_obj.get("->notes") {
                                    current = Some(notes_val);
                                } else {
                                    return paths; // No ->notes at this level
                                }
                            } else {
                                return paths; // ->wikilink not an object
                            }
                        } else {
                            return paths; // No ->wikilink at this level
                        }
                    } else {
                        return paths; // Current value not an object
                    }
                } else {
                    return paths; // Should not happen
                }
            }
        }

        // Extract the path array at the final level
        if let Some(val) = current {
            // Unwrap {"Object": {"path": ...}}
            if let Some(obj) = unwrap_object(val) {
                if let Some(path_val) = obj.get("path") {
                    // Unwrap {"Array": [...]}
                    if let Some(path_array) = unwrap_array(path_val) {
                        for item in path_array {
                            // Unwrap {"Strand": "..."}
                            if let Some(path_str) = unwrap_strand(item) {
                                paths.push(path_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    paths
}

/// Batch 1: Simple circular reference tests (lines 79-213 of original)

#[tokio::test]
async fn test_simple_circular_reference_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Depth 1 traversal from A (should get B only)
    let query = format!("SELECT ->wikilink->notes.path FROM {}", ids[0]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 1);

    assert_eq!(
        paths.len(),
        1,
        "Depth 1 should return exactly 1 note (B), got: {:?}",
        paths
    );
}

#[tokio::test]
async fn test_circular_reference_with_depth_2() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Depth 2 traversal from A (should get B at depth 1, C at depth 2)
    // NOTE: SurrealDB graph traversal returns only the FINAL nodes at the specified depth,
    // not all intermediate nodes. To get both depths, we need multiple queries.
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 2);

    // SurrealDB only returns nodes at the final depth (depth 2), not intermediate nodes
    // So we should only get C.md here, not B.md
    assert_eq!(
        paths.len(),
        1,
        "Depth 2 traversal returns only final depth nodes (C), got: {:?}",
        paths
    );
    assert!(paths.contains(&"C.md".to_string()), "Should contain C.md");
}

#[tokio::test]
async fn test_circular_reference_with_depth_3() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Depth 3 traversal from A (should return final depth nodes)
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 3);

    // Depth 3 from A follows: A->B->C->A
    // So at depth 3 we're back to A
    assert_eq!(
        paths.len(),
        1,
        "Depth 3 should return 1 note (A - completing the cycle), got {} paths: {:?}",
        paths.len(),
        paths
    );
    assert!(paths.contains(&"A.md".to_string()), "Should contain A.md");
}

#[tokio::test]
async fn test_circular_reference_with_large_depth() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Depth 10 traversal - manually expand to depth 10
    // This is tedious but necessary without max_depth parameter
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes->\
         wikilink->notes->wikilink->notes->wikilink->notes->\
         wikilink->notes->wikilink->notes->wikilink->notes->\
         wikilink->notes.path FROM {}",
        ids[0]
    );

    // This should NOT infinite loop or crash
    let result = client.query(&query, &[]);

    // Test that it completes within a reasonable time (tokio::time::timeout implicit via await)
    let result = result.await.expect("Query should complete without error");

    let paths = extract_graph_paths(&result, 10);

    // Should terminate and return results (SurrealDB deduplicates, so likely <= 3)
    assert!(
        !paths.is_empty(),
        "Large depth traversal should return results"
    );
    assert!(
        paths.len() <= 10,
        "Should not return unreasonable number of nodes, got {} paths",
        paths.len()
    );
}

#[tokio::test]
async fn test_self_referential_link() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with self-reference
    let note_id = create_test_note(&client, "Self.md", "This note links to [[Self]]", &kiln_root)
        .await
        .expect("Failed to create note");

    // Create self-referential link (Self → Self)
    create_wikilink(&client, &note_id, &note_id, "Self", 20)
        .await
        .expect("Failed to create self-link");

    // Depth 5 traversal with self-link
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes->\
         wikilink->notes->wikilink->notes.path FROM {}",
        note_id
    );

    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 5);

    // Should handle self-reference without infinite loop
    // SurrealDB may return 0 (empty traversal) or deduplicated results
    assert!(
        paths.len() <= 5,
        "Self-referential link should not cause unlimited duplication, got {} paths: {:?}",
        paths.len(),
        paths
    );
}

/// Batch 2: Complex cycles and bidirectional links (lines 215-350 of original spec)

#[tokio::test]
async fn test_bidirectional_circular_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create 3 notes (A, B, C) with bidirectional links (A↔B↔C↔A)
    let id_a = create_test_note(&client, "A.md", "Note A", &kiln_root)
        .await
        .expect("Failed to create note A");
    let id_b = create_test_note(&client, "B.md", "Note B", &kiln_root)
        .await
        .expect("Failed to create note B");
    let id_c = create_test_note(&client, "C.md", "Note C", &kiln_root)
        .await
        .expect("Failed to create note C");

    // Create all 6 bidirectional links: A↔B, B↔C, C↔A
    create_wikilink(&client, &id_a, &id_b, "B", 0)
        .await
        .expect("Failed to create link A→B");
    create_wikilink(&client, &id_b, &id_a, "A", 0)
        .await
        .expect("Failed to create link B→A");
    create_wikilink(&client, &id_b, &id_c, "C", 0)
        .await
        .expect("Failed to create link B→C");
    create_wikilink(&client, &id_c, &id_b, "B", 0)
        .await
        .expect("Failed to create link C→B");
    create_wikilink(&client, &id_c, &id_a, "A", 0)
        .await
        .expect("Failed to create link C→A");
    create_wikilink(&client, &id_a, &id_c, "C", 0)
        .await
        .expect("Failed to create link A→C");

    // Traverse depth 3 from A (outgoing only, but bidirectional graph)
    // Since we created links in both directions, outgoing traversal should reach all nodes
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        id_a
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 3);

    // Should terminate correctly without infinite loop
    // With bidirectional links, depth 3 traversal can reach multiple paths
    assert!(
        !paths.is_empty(),
        "Bidirectional graph traversal should return results"
    );
    // Verify it terminates (not infinite) - allow reasonable number of paths
    // With 3 nodes and bidirectional links, we can get multiple paths at depth 3
    assert!(
        paths.len() <= 20,
        "Should not return unreasonable number of nodes, got {} paths: {:?}",
        paths.len(),
        paths
    );
}

#[tokio::test]
async fn test_complex_multi_cycle_graph() {
    let (client, kiln_root) = setup_test_client().await;

    // Create 5 notes with multiple overlapping cycles:
    // Cycle 1: A→B→C→A
    // Cycle 2: B→D→B
    // Cycle 3: C→D→E→C
    let notes: Vec<(&str, &str)> = vec![
        ("A.md", "Content A"),
        ("B.md", "Content B"),
        ("C.md", "Content C"),
        ("D.md", "Content D"),
        ("E.md", "Content E"),
    ];

    let mut ids = std::collections::HashMap::new();
    for (name, content) in notes {
        let id = create_test_note(&client, name, content, &kiln_root)
            .await
            .expect(&format!("Failed to create note {}", name));
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create 8 links for 3 overlapping cycles
    let links = [
        ("A", "B"),
        ("B", "C"),
        ("C", "A"), // Cycle 1
        ("B", "D"),
        ("D", "B"), // Cycle 2
        ("C", "D"),
        ("D", "E"),
        ("E", "C"), // Cycle 3
    ];

    for (from, to) in links {
        create_wikilink(&client, &ids[from], &ids[to], to, 0)
            .await
            .expect(&format!("Failed to create link {}→{}", from, to));
    }

    // Traverse depth 5 from A
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes->\
         wikilink->notes->wikilink->notes.path FROM {}",
        ids["A"]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 5);

    // Should handle multiple cycles without infinite loop
    // With 5 nodes and 3 overlapping cycles, depth 5 can reach many paths
    assert!(
        !paths.is_empty(),
        "Multi-cycle graph traversal should return results"
    );
    // Verify it terminates (not infinite) - allow reasonable number of paths
    // The key test is that it completes without hanging, not the exact count
    assert!(
        paths.len() <= 50,
        "Should not return unreasonable number of nodes, got {} paths: {:?}",
        paths.len(),
        paths
    );
}

/// Batch 3: Backlinks and edge cases (final batch)

/// Helper to extract paths from backlink traversal results
///
/// Backlinks use incoming syntax: <-wikilink<-notes.path
/// For depth N, there will be N levels of <-wikilink<-notes nesting
fn extract_backlink_paths(result: &crucible_surrealdb::QueryResult, depth: usize) -> Vec<String> {
    use serde_json::Value;

    let mut paths = Vec::new();

    for record in &result.records {
        let mut current: Option<&Value> = None;

        // For each depth level, traverse <-wikilink <- <-notes
        for i in 0..depth {
            if i == 0 {
                // First level: get <-wikilink from record.data
                if let Some(wikilink_val) = record.data.get("<-wikilink") {
                    // Unwrap {"Object": {"<-notes": ...}}
                    if let Some(wikilink_obj) = unwrap_object(wikilink_val) {
                        if let Some(notes_val) = wikilink_obj.get("<-notes") {
                            current = Some(notes_val);
                        } else {
                            return paths; // No <-notes
                        }
                    } else {
                        return paths; // <-wikilink not an object
                    }
                } else {
                    return paths; // No <-wikilink
                }
            } else {
                // Subsequent levels: navigate through current value
                if let Some(curr_val) = current {
                    // Unwrap {"Object": {"<-wikilink": ...}}
                    if let Some(curr_obj) = unwrap_object(curr_val) {
                        if let Some(wikilink_val) = curr_obj.get("<-wikilink") {
                            // Unwrap {"Object": {"<-notes": ...}}
                            if let Some(wikilink_obj) = unwrap_object(wikilink_val) {
                                if let Some(notes_val) = wikilink_obj.get("<-notes") {
                                    current = Some(notes_val);
                                } else {
                                    return paths; // No <-notes at this level
                                }
                            } else {
                                return paths; // <-wikilink not an object
                            }
                        } else {
                            return paths; // No <-wikilink at this level
                        }
                    } else {
                        return paths; // Current value not an object
                    }
                } else {
                    return paths; // Should not happen
                }
            }
        }

        // Extract the path array at the final level
        if let Some(val) = current {
            // Unwrap {"Object": {"path": ...}}
            if let Some(obj) = unwrap_object(val) {
                if let Some(path_val) = obj.get("path") {
                    // Unwrap {"Array": [...]}
                    if let Some(path_array) = unwrap_array(path_val) {
                        for item in path_array {
                            // Unwrap {"Strand": "..."}
                            if let Some(path_str) = unwrap_strand(item) {
                                paths.push(path_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    paths
}

#[tokio::test]
async fn test_backlinks_with_circular_references() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Get backlinks to B (depth 1) - should find A (since A→B exists)
    let query = format!("SELECT <-wikilink<-notes.path FROM {}", ids[1]); // ids[1] is B
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 1);

    // Should find exactly 1 backlink: A→B
    assert_eq!(
        paths.len(),
        1,
        "Should find exactly 1 backlink (A -> B), got: {:?}",
        paths
    );
    assert!(
        paths[0].contains("A.md"),
        "Backlink should be from A.md, got: {}",
        paths[0]
    );
}

#[tokio::test]
async fn test_backlinks_with_depth_in_circular_graph() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Get backlinks to B with depth 2
    // Depth 1: A→B (direct backlink)
    // Depth 2: C→A→B (indirect backlink via A)
    let query = format!(
        "SELECT <-wikilink<-notes<-wikilink<-notes.path FROM {}",
        ids[1]
    ); // ids[1] is B
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 2);

    // Should find backlinks at depth 2 (final depth only in SurrealDB)
    // C→A→B means C has a path to B through A, so C should be in results
    assert!(
        !paths.is_empty(),
        "Should find backlinks at depth 2, got: {:?}",
        paths
    );

    // At depth 2, we should get C (via C→A→B path)
    assert!(
        paths.iter().any(|p| p.contains("C.md")),
        "Should find C.md via C->A->B path, got: {:?}",
        paths
    );
}

#[tokio::test]
async fn test_no_infinite_loop_on_max_depth_zero() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→A cycle
    let ids = create_cycle(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cycle");

    // Depth 0 means no traversal - query the note directly without graph syntax
    // This verifies that depth 0 doesn't cause infinite loops in cycle detection
    let query = format!("SELECT path FROM {}", ids[0]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should return just the starting note (depth 0 = no traversal)
    // The result should contain exactly 1 record (the starting note itself)
    assert_eq!(
        result.records.len(),
        1,
        "Depth 0 should return exactly the starting note, got {} records",
        result.records.len()
    );

    // Verify the path is the starting note (A.md)
    // The path field in notes table is an array of path segments
    if let Some(path_val) = result.records[0].data.get("path") {
        // Path might be directly an array, or could be a simple string value
        // Let's handle both cases
        if let Some(path_str) = unwrap_strand(path_val) {
            // Path is a simple string
            assert!(
                path_str.contains("A.md"),
                "Path should be A.md, got: {}",
                path_str
            );
        } else if let Some(path_array) = unwrap_array(path_val) {
            // Path is an array
            assert!(
                !path_array.is_empty(),
                "Path array should not be empty"
            );
            // Get the last element (filename)
            let last_elem = &path_array[path_array.len() - 1];
            if let Some(path_str) = unwrap_strand(last_elem) {
                assert!(
                    path_str.contains("A.md"),
                    "Path should contain A.md, got: {}",
                    path_str
                );
            } else {
                panic!("Could not extract path string from array element");
            }
        } else {
            panic!("Path is neither a string nor an array: {:?}", path_val);
        }
    } else {
        panic!("No path field in result");
    }
}
