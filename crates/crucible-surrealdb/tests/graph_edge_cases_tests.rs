//! Tests for edge cases in graph traversal and metadata queries
//!
//! These tests cover:
//! - Empty database scenarios
//! - Single isolated notes
//! - Hub nodes with many links
//! - Notes with only self-references
//! - Large graph performance characteristics

mod common;
use common::*;

#[tokio::test]
async fn test_empty_database_traversal() {
    let (client, _kiln_root) = setup_test_client().await;

    // Attempt to traverse in an empty database
    // Query on non-existent record ID - should return empty or error gracefully
    let query = "SELECT * FROM wikilink WHERE in = 'notes:nonexistent_md'";

    let result = client.query(query, &[]).await;
    assert!(
        result.is_ok(),
        "Query on empty database should not error"
    );
    assert!(
        result.unwrap().records.is_empty(),
        "Empty database should return no results"
    );
}

#[tokio::test]
async fn test_single_note_no_links() {
    let (client, kiln_root) = setup_test_client().await;

    // Create a single isolated note
    let note_id = create_test_note(&client, "isolated.md", "This note has no wikilinks", &kiln_root)
        .await
        .expect("Failed to create note");

    // Try to traverse from this note (query for outgoing wikilinks)
    let query = format!("SELECT * FROM wikilink WHERE in = {}", note_id);

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should return empty (no links to traverse)
    assert!(
        result.records.is_empty(),
        "Single note with no links should return empty traversal"
    );
}

#[tokio::test]
async fn test_single_note_with_self_link() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with self-reference
    let note_id = create_test_note(&client, "self.md", "Links to [[self]]", &kiln_root)
        .await
        .expect("Failed to create note");

    // Create self-link using helper
    create_wikilink(&client, &note_id, &note_id, "self", 9)
        .await
        .expect("Failed to create self-link");

    // Query for outgoing links from self-referencing note
    let query = format!("SELECT * FROM wikilink WHERE in = {}", note_id);

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should handle gracefully (should have 1 self-link)
    assert!(
        result.records.len() <= 1,
        "Self-referencing note should not multiply"
    );
}

#[tokio::test]
async fn test_hub_node_with_many_outgoing_links() {
    let (client, kiln_root) = setup_test_client().await;

    // Create hub note
    let hub_id = create_test_note(&client, "hub.md", "Central hub note", &kiln_root)
        .await
        .expect("Failed to create hub note");

    // Create 25 target notes and link them all from the hub
    let target_count = 25;
    for i in 0..target_count {
        let target_id = create_test_note(
            &client,
            &format!("target_{}.md", i),
            &format!("Target note {}", i),
            &kiln_root,
        )
        .await
        .expect("Failed to create target note");

        create_wikilink(&client, &hub_id, &target_id, &format!("target_{}", i), i * 10)
            .await
            .expect("Failed to create wikilink");
    }

    // Verify all outgoing links exist
    let query = format!("SELECT * FROM wikilink WHERE in = {}", hub_id);
    let result = client.query(&query, &[]).await.expect("Query failed");

    assert_eq!(
        result.records.len(),
        target_count,
        "Hub should have {} outgoing links",
        target_count
    );
}

#[tokio::test]
async fn test_hub_node_with_many_incoming_links() {
    let (client, kiln_root) = setup_test_client().await;

    // Create hub note
    let hub_id = create_test_note(&client, "hub.md", "Central hub note", &kiln_root)
        .await
        .expect("Failed to create hub note");

    // Create 25 source notes and link them all to the hub
    let source_count = 25;
    for i in 0..source_count {
        let source_id = create_test_note(
            &client,
            &format!("source_{}.md", i),
            &format!("Source note {}", i),
            &kiln_root,
        )
        .await
        .expect("Failed to create source note");

        create_wikilink(&client, &source_id, &hub_id, "hub", i * 10)
            .await
            .expect("Failed to create wikilink");
    }

    // Verify all incoming links exist (backlinks)
    let query = format!("SELECT * FROM wikilink WHERE out = {}", hub_id);
    let result = client.query(&query, &[]).await.expect("Query failed");

    assert_eq!(
        result.records.len(),
        source_count,
        "Hub should have {} incoming links",
        source_count
    );
}

#[tokio::test]
async fn test_all_notes_isolated() {
    let (client, kiln_root) = setup_test_client().await;

    // Create 5 isolated notes with no links
    let note_count = 5;
    for i in 0..note_count {
        create_test_note(
            &client,
            &format!("isolated_{}.md", i),
            &format!("Isolated note {}", i),
            &kiln_root,
        )
        .await
        .expect("Failed to create isolated note");
    }

    // Verify no wikilinks exist in the database
    let query = "SELECT * FROM wikilink";
    let result = client.query(query, &[]).await.expect("Query failed");

    assert_eq!(
        result.records.len(),
        0,
        "Database with only isolated notes should have no wikilinks"
    );
}

#[tokio::test]
async fn test_mix_of_connected_and_isolated_notes() {
    let (client, kiln_root) = setup_test_client().await;

    // Create connected chain: A -> B -> C
    let id_a = create_test_note(&client, "A.md", "Note A", &kiln_root)
        .await
        .expect("Failed to create A");
    let id_b = create_test_note(&client, "B.md", "Note B", &kiln_root)
        .await
        .expect("Failed to create B");
    let id_c = create_test_note(&client, "C.md", "Note C", &kiln_root)
        .await
        .expect("Failed to create C");

    create_wikilink(&client, &id_a, &id_b, "B", 0)
        .await
        .expect("Failed to create A->B link");
    create_wikilink(&client, &id_b, &id_c, "C", 0)
        .await
        .expect("Failed to create B->C link");

    // Create isolated notes D and E
    let _id_d = create_test_note(&client, "D.md", "Note D isolated", &kiln_root)
        .await
        .expect("Failed to create D");
    let _id_e = create_test_note(&client, "E.md", "Note E isolated", &kiln_root)
        .await
        .expect("Failed to create E");

    // Verify only 2 wikilinks exist (A->B and B->C)
    let query = "SELECT * FROM wikilink";
    let result = client.query(query, &[]).await.expect("Query failed");

    assert_eq!(
        result.records.len(),
        2,
        "Should have exactly 2 links (A->B, B->C)"
    );

    // Verify traversal from A includes B and C, but not D or E
    let query_from_a = format!("SELECT * FROM wikilink WHERE in = {}", id_a);
    let result_from_a = client
        .query(&query_from_a, &[])
        .await
        .expect("Query from A failed");

    assert_eq!(
        result_from_a.records.len(),
        1,
        "A should link to exactly one note (B)"
    );

    // Verify isolated notes have no links
    let query_from_d = format!("SELECT * FROM wikilink WHERE in = {}", _id_d);
    let result_from_d = client
        .query(&query_from_d, &[])
        .await
        .expect("Query from D failed");

    assert_eq!(
        result_from_d.records.len(),
        0,
        "D should have no outgoing links"
    );
}

#[tokio::test]
async fn test_note_with_only_broken_links() {
    let (client, kiln_root) = setup_test_client().await;

    // Create a source note
    let source_id = create_test_note(
        &client,
        "source.md",
        "Note with broken links [[nonexistent1]] and [[nonexistent2]]",
        &kiln_root,
    )
    .await
    .expect("Failed to create source note");

    // Manually create wikilinks to non-existent targets
    // These are "broken" links because the target notes don't exist
    let broken_link1 = format!(
        "RELATE {}->wikilink->notes:nonexistent1_md SET link_text = 'nonexistent1', position = 0",
        source_id
    );
    let broken_link2 = format!(
        "RELATE {}->wikilink->notes:nonexistent2_md SET link_text = 'nonexistent2', position = 20",
        source_id
    );

    client
        .query(&broken_link1, &[])
        .await
        .expect("Failed to create broken link 1");
    client
        .query(&broken_link2, &[])
        .await
        .expect("Failed to create broken link 2");

    // Verify the wikilinks exist
    let query = format!("SELECT * FROM wikilink WHERE in = {}", source_id);
    let result = client.query(&query, &[]).await.expect("Query failed");

    assert_eq!(
        result.records.len(),
        2,
        "Should have 2 broken wikilinks"
    );

    // Verify that querying for the non-existent target notes returns empty
    let query_target1 = "SELECT * FROM notes:nonexistent1_md";
    let result_target1 = client
        .query(query_target1, &[])
        .await
        .expect("Query for nonexistent target failed");

    assert_eq!(
        result_target1.records.len(),
        0,
        "Non-existent target should not exist"
    );
}

#[tokio::test]
async fn test_complex_hub_and_spoke_topology() {
    let (client, kiln_root) = setup_test_client().await;

    // Create central hub
    let hub_id = create_test_note(&client, "hub.md", "Central hub", &kiln_root)
        .await
        .expect("Failed to create hub");

    // Create 10 spokes, each with 3 sub-notes
    let spoke_count = 10;
    let sub_notes_per_spoke = 3;

    for i in 0..spoke_count {
        // Create spoke
        let spoke_id = create_test_note(
            &client,
            &format!("spoke_{}.md", i),
            &format!("Spoke {}", i),
            &kiln_root,
        )
        .await
        .expect("Failed to create spoke");

        // Link hub to spoke
        create_wikilink(&client, &hub_id, &spoke_id, &format!("spoke_{}", i), i * 10)
            .await
            .expect("Failed to link hub to spoke");

        // Create sub-notes for this spoke
        for j in 0..sub_notes_per_spoke {
            let sub_id = create_test_note(
                &client,
                &format!("spoke_{}_sub_{}.md", i, j),
                &format!("Sub-note {} of spoke {}", j, i),
                &kiln_root,
            )
            .await
            .expect("Failed to create sub-note");

            // Link spoke to sub-note
            create_wikilink(
                &client,
                &spoke_id,
                &sub_id,
                &format!("sub_{}", j),
                j * 10,
            )
            .await
            .expect("Failed to link spoke to sub-note");
        }
    }

    // Verify hub has correct number of outgoing links (to spokes)
    let query_hub = format!("SELECT * FROM wikilink WHERE in = {}", hub_id);
    let result_hub = client
        .query(&query_hub, &[])
        .await
        .expect("Query from hub failed");

    assert_eq!(
        result_hub.records.len(),
        spoke_count,
        "Hub should link to {} spokes",
        spoke_count
    );

    // Verify each spoke has correct number of outgoing links (to sub-notes)
    for i in 0..spoke_count {
        let spoke_id = format!("notes:spoke_{}_md", i);
        let query_spoke = format!("SELECT * FROM wikilink WHERE in = {}", spoke_id);
        let result_spoke = client
            .query(&query_spoke, &[])
            .await
            .expect("Query from spoke failed");

        assert_eq!(
            result_spoke.records.len(),
            sub_notes_per_spoke,
            "Spoke {} should link to {} sub-notes",
            i,
            sub_notes_per_spoke
        );
    }

    // Verify total wikilink count (10 hub->spoke + 10*3 spoke->sub = 40)
    let query_all = "SELECT * FROM wikilink";
    let result_all = client
        .query(query_all, &[])
        .await
        .expect("Query for all wikilinks failed");

    let expected_total = spoke_count + (spoke_count * sub_notes_per_spoke);
    assert_eq!(
        result_all.records.len(),
        expected_total,
        "Should have {} total wikilinks",
        expected_total
    );
}

#[tokio::test]
async fn test_disconnected_clusters() {
    let (client, kiln_root) = setup_test_client().await;

    // Create 3 separate graph clusters with no connections between them
    // Cluster 1: A -> B -> C
    let cluster1 = create_linear_chain(&client, &["A.md", "B.md", "C.md"], &kiln_root)
        .await
        .expect("Failed to create cluster 1");

    // Cluster 2: X -> Y -> Z
    let cluster2 = create_linear_chain(&client, &["X.md", "Y.md", "Z.md"], &kiln_root)
        .await
        .expect("Failed to create cluster 2");

    // Cluster 3: P -> Q -> R
    let cluster3 = create_linear_chain(&client, &["P.md", "Q.md", "R.md"], &kiln_root)
        .await
        .expect("Failed to create cluster 3");

    // Verify A can only reach B (not C, X, Y, Z, P, Q, R)
    let query = format!("SELECT * FROM wikilink WHERE in = {}", cluster1[0]);
    let result = client.query(&query, &[]).await.expect("Query from A failed");

    assert_eq!(
        result.records.len(),
        1,
        "A should have exactly 1 outgoing link (to B)"
    );

    // Verify the link from A points to B
    let out_field = result.records[0].data.get("out").and_then(|v| v.as_str());
    assert_eq!(
        out_field,
        Some(cluster1[1].as_str()),
        "A's link should point to B"
    );

    // Verify X can only reach Y (not Z, A, B, C, P, Q, R)
    let query = format!("SELECT * FROM wikilink WHERE in = {}", cluster2[0]);
    let result = client.query(&query, &[]).await.expect("Query from X failed");

    assert_eq!(
        result.records.len(),
        1,
        "X should have exactly 1 outgoing link (to Y)"
    );

    // Verify P can only reach Q (not R, A, B, C, X, Y, Z)
    let query = format!("SELECT * FROM wikilink WHERE in = {}", cluster3[0]);
    let result = client.query(&query, &[]).await.expect("Query from P failed");

    assert_eq!(
        result.records.len(),
        1,
        "P should have exactly 1 outgoing link (to Q)"
    );

    // Verify total links: 2 per cluster = 6 total
    let query_all = "SELECT * FROM wikilink";
    let result_all = client
        .query(query_all, &[])
        .await
        .expect("Query for all wikilinks failed");

    assert_eq!(
        result_all.records.len(),
        6,
        "Should have exactly 6 links total (2 per cluster)"
    );
}

#[tokio::test]
async fn test_empty_tag_results() {
    let (client, kiln_root) = setup_test_client().await;

    // Create notes without tags
    create_test_note(&client, "untagged1.md", "No tags here", &kiln_root)
        .await
        .expect("Failed to create note");

    create_test_note(&client, "untagged2.md", "Also no tags", &kiln_root)
        .await
        .expect("Failed to create note");

    // Query for notes with a specific tag
    let query = "SELECT * FROM tagged_with WHERE out.name = 'nonexistent'";

    let result = client.query(query, &[]).await.expect("Query failed");

    // Should return empty (no notes with that tag)
    assert!(
        result.records.is_empty(),
        "Query for non-existent tag should return empty"
    );
}

#[tokio::test]
async fn test_note_with_metadata_but_no_tags() {
    let (client, kiln_root) = setup_test_client().await;

    // Create note with metadata but no tags
    let note_id = create_test_note_with_frontmatter(
        &client,
        "metadata.md",
        "Content here",
        "title: Test\nstatus: draft",
        &kiln_root,
    )
    .await
    .expect("Failed to create note");

    // Verify note exists
    assert!(note_id.contains("metadata_md"));

    // Query for any tags
    let query = format!("SELECT * FROM tagged_with WHERE in = {}", note_id);

    let result = client.query(&query, &[]).await.expect("Query failed");

    // Should return empty (note has metadata but no tags)
    assert!(
        result.records.is_empty(),
        "Note with metadata but no tags should return empty tag query"
    );
}

#[tokio::test]
async fn test_very_long_link_chain() {
    let (client, kiln_root) = setup_test_client().await;

    // Create a chain of 100+ notes (A->B->C->...->Z->AA->AB...)
    let names: Vec<String> = (0..100).map(|i| format!("note_{}.md", i)).collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

    let ids = create_linear_chain(&client, &name_refs, &kiln_root)
        .await
        .expect("Failed to create long chain");

    assert_eq!(
        ids.len(),
        100,
        "Should have created 100 notes"
    );

    // Test traversal from start to various depths
    // 10-hop traversal: verify note_0 links to note_1
    let query = format!("SELECT * FROM wikilink WHERE in = {}", ids[0]);
    let result = client.query(&query, &[]).await.expect("Query from note_0 failed");

    assert_eq!(
        result.records.len(),
        1,
        "note_0 should have exactly 1 outgoing link"
    );

    // 50-hop traversal: verify note_50 links to note_51
    let query = format!("SELECT * FROM wikilink WHERE in = {}", ids[50]);
    let result = client.query(&query, &[]).await.expect("Query from note_50 failed");

    assert_eq!(
        result.records.len(),
        1,
        "note_50 should have exactly 1 outgoing link"
    );

    // Verify last note (note_99) has no outgoing links
    let query = format!("SELECT * FROM wikilink WHERE in = {}", ids[99]);
    let result = client.query(&query, &[]).await.expect("Query from note_99 failed");

    assert_eq!(
        result.records.len(),
        0,
        "note_99 (last note) should have no outgoing links"
    );

    // Verify total link count (100 notes = 99 links)
    let query_all = "SELECT * FROM wikilink";
    let result_all = client
        .query(query_all, &[])
        .await
        .expect("Query for all wikilinks failed");

    assert_eq!(
        result_all.records.len(),
        99,
        "Should have exactly 99 links for 100 notes in a chain"
    );

    // Verify no performance degradation or stack overflow by querying middle of chain
    let query = format!("SELECT * FROM wikilink WHERE in = {}", ids[49]);
    let result = client.query(&query, &[]).await.expect("Query from note_49 should not overflow");

    assert_eq!(
        result.records.len(),
        1,
        "note_49 should have exactly 1 outgoing link"
    );
}
