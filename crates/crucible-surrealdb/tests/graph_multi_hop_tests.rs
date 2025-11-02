//! Tests for multi-hop graph traversal (converted to SurrealDB)
//!
//! These tests verify that graph traversal correctly handles:
//! - 2-hop, 3-hop, 4-hop, and 5-hop traversals
//! - Max depth enforcement
//! - Both outgoing and incoming (backlink) multi-hop traversal
//! - Bidirectional traversal
//! - Path counting and verification
//!
//! NOTE: SurrealDB does NOT have a max_depth parameter. We manually expand
//! graph traversal syntax for different depths.

mod common;
use common::*;
use crucible_core::QueryResult;

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
fn extract_graph_paths(result: &QueryResult, depth: usize) -> Vec<String> {
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

/// Helper to extract paths from backlink (incoming) graph traversal results
///
/// Backlink traversal uses <-wikilink<-notes syntax (incoming direction)
/// Structure is similar to forward traversal but with <- prefix
fn extract_backlink_paths(result: &QueryResult, depth: usize) -> Vec<String> {
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

/// Batch 1: Linear chain traversal tests (2-5 hops)

#[tokio::test]
async fn test_two_hop_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 2-hop traversal from A: A→B→C
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 2);

    // Should reach C only (at depth 2)
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 2");
    assert!(paths[0].contains("C.md"), "Should reach C, got: {:?}", paths);
}

#[tokio::test]
async fn test_three_hop_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 3-hop traversal from A: A→B→C→D
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 3);

    // Should reach D only (at depth 3)
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 3");
    assert!(paths[0].contains("D.md"), "Should reach D, got: {:?}", paths);
}

#[tokio::test]
async fn test_four_hop_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 4-hop traversal from A: A→B→C→D→E
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 4);

    // Should reach E only (at depth 4)
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 4");
    assert!(paths[0].contains("E.md"), "Should reach E, got: {:?}", paths);
}

#[tokio::test]
async fn test_five_hop_traversal_reaches_end() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain (only 4 links)
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 5-hop traversal from A (exceeds chain length)
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 5);

    // Should return empty - can't go beyond E
    assert_eq!(paths.len(), 0, "5-hop traversal should return no results (chain only has 4 links)");
}

#[tokio::test]
async fn test_max_depth_enforcement() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 3-hop traversal from A (should only reach D, not E)
    let query = format!(
        "SELECT ->wikilink->notes->wikilink->notes->wikilink->notes.path FROM {}",
        ids[0]
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 3);

    // Should reach D only (at depth 3), NOT E
    assert_eq!(paths.len(), 1, "Should reach exactly 1 note at depth 3");
    assert!(paths[0].contains("D.md"), "Should reach D, got: {:?}", paths);
    assert!(!paths.iter().any(|p| p.contains("E.md")), "Should NOT reach E");
}

/// Batch 2: Backlinks and bidirectional traversal tests

#[tokio::test]
async fn test_backlinks_two_hop() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 2-hop backlinks from C (C←B←A)
    let query = format!(
        "SELECT <-wikilink<-notes<-wikilink<-notes.path FROM {}",
        ids[2]  // C is at index 2
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 2);

    // Should reach A at depth 2 backwards
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 2 backwards");
    assert!(paths[0].contains("A.md"), "Should reach A, got: {:?}", paths);
}

#[tokio::test]
async fn test_backlinks_three_hop() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 3-hop backlinks from D (D←C←B←A)
    let query = format!(
        "SELECT <-wikilink<-notes<-wikilink<-notes<-wikilink<-notes.path FROM {}",
        ids[3]  // D is at index 3
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 3);

    // Should reach A at depth 3 backwards
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 3 backwards");
    assert!(paths[0].contains("A.md"), "Should reach A, got: {:?}", paths);
}

#[tokio::test]
async fn test_backlinks_from_end_of_chain() {
    let (client, kiln_root) = setup_test_client().await;

    // Create A→B→C→D→E linear chain
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // 4-hop backlinks from E (E←D←C←B←A)
    let query = format!(
        "SELECT <-wikilink<-notes<-wikilink<-notes<-wikilink<-notes<-wikilink<-notes.path FROM {}",
        ids[4]  // E is at index 4
    );
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 4);

    // Should reach A at depth 4 backwards (entire chain backwards)
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 4 backwards");
    assert!(paths[0].contains("A.md"), "Should reach A (entire chain backwards), got: {:?}", paths);
}

#[tokio::test]
async fn test_bidirectional_traversal_depth_1() {
    let (client, kiln_root) = setup_test_client().await;

    // Create bidirectional chain A↔B↔C↔D↔E
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // Add reverse links to make it bidirectional
    create_wikilink(&client, &ids[1], &ids[0], "A", 0).await.unwrap(); // B→A
    create_wikilink(&client, &ids[2], &ids[1], "B", 0).await.unwrap(); // C→B
    create_wikilink(&client, &ids[3], &ids[2], "C", 0).await.unwrap(); // D→C
    create_wikilink(&client, &ids[4], &ids[3], "D", 0).await.unwrap(); // E→D

    // 1-hop from C (should reach both B and D via outgoing links)
    let query = format!("SELECT ->wikilink->notes.path FROM {}", ids[2]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 1);

    // Should reach 2 neighbors: B and D
    assert_eq!(paths.len(), 2, "Should reach 2 neighbors in bidirectional graph");
    let paths_str = paths.join(", ");
    assert!(paths_str.contains("B.md") && paths_str.contains("D.md"),
        "Should reach B and D, got: {:?}", paths);
}

#[tokio::test]
async fn test_bidirectional_traversal_depth_2() {
    let (client, kiln_root) = setup_test_client().await;

    // Create bidirectional chain A↔B↔C↔D↔E
    let ids = create_linear_chain(&client, &["A.md", "B.md", "C.md", "D.md", "E.md"], &kiln_root)
        .await
        .expect("Failed to create chain");

    // Add reverse links to make it bidirectional
    create_wikilink(&client, &ids[1], &ids[0], "A", 0).await.unwrap(); // B→A
    create_wikilink(&client, &ids[2], &ids[1], "B", 0).await.unwrap(); // C→B
    create_wikilink(&client, &ids[3], &ids[2], "C", 0).await.unwrap(); // D→C
    create_wikilink(&client, &ids[4], &ids[3], "D", 0).await.unwrap(); // E→D

    // 2-hop from C (should reach A and E via outgoing links in bidirectional graph)
    let query = format!("SELECT ->wikilink->notes->wikilink->notes.path FROM {}", ids[2]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 2);

    // In a bidirectional graph, from C we can reach:
    // - C→D→E (forward 2 hops)
    // - C→B→A (backward 2 hops, but using outgoing links)
    // Should reach at least 2 nodes (A and E)
    assert!(paths.len() >= 2, "Should reach at least 2 notes at depth 2 in bidirectional graph, got: {:?}", paths);
    let paths_str = paths.join(", ");
    assert!(paths_str.contains("A.md") || paths_str.contains("E.md"),
        "Should reach A and/or E, got: {:?}", paths);
}

/// Batch 3: Branching tree and diamond graph tests

#[tokio::test]
async fn test_branching_traversal_depth_1() {
    let (client, kiln_root) = setup_test_client().await;

    // Create branching tree: A→{B,C}, B→{D,E}, C→F
    //     A
    //    / \
    //   B   C
    //  / \   \
    // D   E   F
    let mut ids = std::collections::HashMap::new();

    for name in &["A.md", "B.md", "C.md", "D.md", "E.md", "F.md"] {
        let id = create_test_note(&client, name, &format!("Content {}", name), &kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create tree links
    create_wikilink(&client, &ids["A"], &ids["B"], "B", 0).await.unwrap();
    create_wikilink(&client, &ids["A"], &ids["C"], "C", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["D"], "D", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["E"], "E", 0).await.unwrap();
    create_wikilink(&client, &ids["C"], &ids["F"], "F", 0).await.unwrap();

    // 1-hop from A (should reach B and C)
    let query = format!("SELECT ->wikilink->notes.path FROM {}", ids["A"]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 1);

    assert_eq!(paths.len(), 2, "Should reach 2 children at depth 1");
    assert!(paths.iter().any(|p| p.contains("B.md")), "Should include B");
    assert!(paths.iter().any(|p| p.contains("C.md")), "Should include C");
}

#[tokio::test]
async fn test_branching_traversal_depth_2() {
    let (client, kiln_root) = setup_test_client().await;

    // Create branching tree: A→{B,C}, B→{D,E}, C→F
    //     A
    //    / \
    //   B   C
    //  / \   \
    // D   E   F
    let mut ids = std::collections::HashMap::new();

    for name in &["A.md", "B.md", "C.md", "D.md", "E.md", "F.md"] {
        let id = create_test_note(&client, name, &format!("Content {}", name), &kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create tree links
    create_wikilink(&client, &ids["A"], &ids["B"], "B", 0).await.unwrap();
    create_wikilink(&client, &ids["A"], &ids["C"], "C", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["D"], "D", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["E"], "E", 0).await.unwrap();
    create_wikilink(&client, &ids["C"], &ids["F"], "F", 0).await.unwrap();

    // 2-hop from A (should reach D, E, F - all leaves at depth 2)
    let query = format!("SELECT ->wikilink->notes->wikilink->notes.path FROM {}", ids["A"]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 2);

    assert_eq!(paths.len(), 3, "Should reach 3 leaves at depth 2");
    assert!(paths.iter().any(|p| p.contains("D.md")), "Should include D");
    assert!(paths.iter().any(|p| p.contains("E.md")), "Should include E");
    assert!(paths.iter().any(|p| p.contains("F.md")), "Should include F");
}

#[tokio::test]
async fn test_branching_backlinks_depth_2() {
    let (client, kiln_root) = setup_test_client().await;

    // Create branching tree: A→{B,C}, B→{D,E}, C→F
    //     A
    //    / \
    //   B   C
    //  / \   \
    // D   E   F
    let mut ids = std::collections::HashMap::new();

    for name in &["A.md", "B.md", "C.md", "D.md", "E.md", "F.md"] {
        let id = create_test_note(&client, name, &format!("Content {}", name), &kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create tree links
    create_wikilink(&client, &ids["A"], &ids["B"], "B", 0).await.unwrap();
    create_wikilink(&client, &ids["A"], &ids["C"], "C", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["D"], "D", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["E"], "E", 0).await.unwrap();
    create_wikilink(&client, &ids["C"], &ids["F"], "F", 0).await.unwrap();

    // 2-hop backlinks from D (should reach A via D←B←A)
    let query = format!("SELECT <-wikilink<-notes<-wikilink<-notes.path FROM {}", ids["D"]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 2);

    assert_eq!(paths.len(), 1, "Should reach A via 2-hop backlinks");
    assert!(paths[0].contains("A.md"), "Should reach A");
}

#[tokio::test]
async fn test_diamond_graph_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create diamond: A→B→D, A→C→D
    //   A
    //  / \
    // B   C
    //  \ /
    //   D
    let mut ids = std::collections::HashMap::new();

    for name in &["A.md", "B.md", "C.md", "D.md"] {
        let id = create_test_note(&client, name, &format!("Content {}", name), &kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create diamond links
    create_wikilink(&client, &ids["A"], &ids["B"], "B", 0).await.unwrap();
    create_wikilink(&client, &ids["A"], &ids["C"], "C", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["D"], "D", 0).await.unwrap();
    create_wikilink(&client, &ids["C"], &ids["D"], "D", 0).await.unwrap();

    // 2-hop from A (reaches D via 2 paths: A→B→D and A→C→D)
    let query = format!("SELECT ->wikilink->notes->wikilink->notes.path FROM {}", ids["A"]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 2);

    // SurrealDB does NOT deduplicate - D appears twice (once from each path)
    // This is expected behavior for graph traversal syntax
    assert_eq!(paths.len(), 2, "Should reach D twice (once via each path)");
    assert!(paths.iter().all(|p| p.contains("D.md")), "Both paths should reach D");
}

#[tokio::test]
async fn test_diamond_backlinks() {
    let (client, kiln_root) = setup_test_client().await;

    // Create diamond: A→B→D, A→C→D
    //   A
    //  / \
    // B   C
    //  \ /
    //   D
    let mut ids = std::collections::HashMap::new();

    for name in &["A.md", "B.md", "C.md", "D.md"] {
        let id = create_test_note(&client, name, &format!("Content {}", name), &kiln_root)
            .await
            .expect("Failed to create note");
        ids.insert(name.strip_suffix(".md").unwrap().to_string(), id);
    }

    // Create diamond links
    create_wikilink(&client, &ids["A"], &ids["B"], "B", 0).await.unwrap();
    create_wikilink(&client, &ids["A"], &ids["C"], "C", 0).await.unwrap();
    create_wikilink(&client, &ids["B"], &ids["D"], "D", 0).await.unwrap();
    create_wikilink(&client, &ids["C"], &ids["D"], "D", 0).await.unwrap();

    // 2-hop backlinks from D (reaches A via D←B←A and D←C←A)
    let query = format!("SELECT <-wikilink<-notes<-wikilink<-notes.path FROM {}", ids["D"]);
    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_backlink_paths(&result, 2);

    // SurrealDB does NOT deduplicate - A appears twice (once via each backlink path)
    // This is expected behavior for graph traversal syntax
    assert_eq!(paths.len(), 2, "Should reach A twice (once via each backlink path)");
    assert!(paths.iter().all(|p| p.contains("A.md")), "Both paths should reach A");
}

/// Batch 4: Very deep traversal test (10+ hops)

#[tokio::test]
async fn test_very_deep_traversal() {
    let (client, kiln_root) = setup_test_client().await;

    // Create a 20-node linear chain: note_0 → note_1 → ... → note_19
    let note_count = 20;
    let names: Vec<String> = (0..note_count)
        .map(|i| format!("note_{}.md", i))
        .collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

    let ids = create_linear_chain(&client, &name_refs, &kiln_root)
        .await
        .expect("Failed to create long chain");

    // 10-hop traversal from note_0
    // Each hop: ->wikilink->notes
    let traversal = "->wikilink->notes".repeat(10);
    let query = format!("SELECT {}.path FROM {}", traversal, ids[0]);

    let result = client.query(&query, &[]).await.expect("Query failed");

    let paths = extract_graph_paths(&result, 10);

    // Should reach note_10 at depth 10
    assert_eq!(paths.len(), 1, "Should reach 1 note at depth 10");
    assert!(
        paths[0].contains("note_10.md"),
        "Should reach note_10 at depth 10, got: {}",
        paths[0]
    );

    // The test passing is evidence that:
    // 1. No stack overflow occurred
    // 2. Query completed in reasonable time
    // 3. Deep graph traversal works correctly
}
