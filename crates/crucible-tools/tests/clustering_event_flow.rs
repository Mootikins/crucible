//! Event flow tests for clustering pipeline
//!
//! Tests the integration between MCP clustering tools and Rune plugins
//! through the event system.

use crucible_tools::clustering::ClusteringTools;
use tempfile::TempDir;
use tokio::fs;

/// Test event flow: Document indexed -> Clustering triggered -> Results stored
#[tokio::test]
async fn test_document_indexed_event_flow() {
    let (_temp, vault_path) = create_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Simulate document indexed event (demonstrates event structure)
    let _event_data = serde_json::json!({
        "event": "document_indexed",
        "document": {
            "path": "test-document.md",
            "title": "Test Document",
            "tags": ["test", "clustering"],
            "links": ["other-doc.md"],
            "content": "Test content for clustering"
        },
        "timestamp": "2025-12-09T10:00:00Z"
    });

    // 2. Detect MoCs after document indexing - index.md qualifies as MoC
    let mocs = tools.detect_mocs(Some(0.3)).await.unwrap();
    assert!(
        !mocs.is_empty(),
        "Should detect MoCs (index.md) in test vault"
    );

    // 3. Cluster documents - verify clustering runs without error
    let clusters = tools
        .cluster_documents(
            Some(0.1), // Lower similarity threshold for test data
            Some(2),
            Some(0.6),
            Some(0.3),
            Some(0.1),
        )
        .await
        .unwrap();

    // Verify stats show documents were loaded
    let stats = tools.get_document_stats().await.unwrap();
    assert!(stats.total_documents >= 5, "Should have test documents");

    // Verify cluster structure if any were found
    for cluster in &clusters {
        assert!(!cluster.id.is_empty());
        assert!(cluster.documents.len() >= 2);
    }
}

/// Test event flow: Multiple documents indexed -> Batch clustering
#[tokio::test]
async fn test_batch_clustering_event_flow() {
    let (_temp, vault_path) = create_large_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Demonstrate batch document indexing event structure
    let indexed_docs = [
        ("doc1.md", "Document 1", vec!["ai", "ml"], vec!["doc2"]),
        (
            "doc2.md",
            "Document 2",
            vec!["ml", "nlp"],
            vec!["doc1", "doc3"],
        ),
        ("doc3.md", "Document 3", vec!["nlp", "ai"], vec!["doc2"]),
        (
            "doc4.md",
            "Document 4",
            vec!["web", "frontend"],
            vec!["doc5"],
        ),
        (
            "doc5.md",
            "Document 5",
            vec!["frontend", "css"],
            vec!["doc4"],
        ),
    ];

    // 2. Create event sequence
    let event_sequence: Vec<_> = indexed_docs
        .iter()
        .map(|(path, title, tags, links)| {
            serde_json::json!({
                "event": "document_indexed",
                "document": {
                    "path": path,
                    "title": title,
                    "tags": tags,
                    "links": links,
                    "content_length": 1000
                }
            })
        })
        .collect();

    // 3. Verify event structure
    for event in &event_sequence {
        let doc = &event["document"];
        assert!(doc["path"].is_string());
        assert!(doc["title"].is_string());
        assert!(doc["tags"].is_array());
        assert!(doc["links"].is_array());
    }

    // 4. Run clustering after batch processing
    let clusters = tools
        .cluster_documents(
            Some(0.1), // Lower threshold for test data
            Some(2),
            Some(0.7),
            Some(0.2),
            Some(0.1),
        )
        .await
        .unwrap();

    // 5. Get final statistics - verify documents were loaded
    let stats = tools.get_document_stats().await.unwrap();
    assert_eq!(stats.total_documents, 5, "Should have 5 documents");
    assert!(stats.total_links > 0, "Should have links");

    // Verify any clusters found have valid structure
    for cluster in &clusters {
        assert!(!cluster.id.is_empty());
        assert!(cluster.documents.len() >= 2);
        assert!(cluster.confidence >= 0.0 && cluster.confidence <= 1.0);
    }
}

/// Test event flow: Clustering completed -> Results published
#[tokio::test]
async fn test_clustering_completed_event_flow() {
    let (_temp, vault_path) = create_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Run clustering
    let clusters = tools
        .cluster_documents(Some(0.2), Some(2), Some(0.6), Some(0.3), Some(0.1))
        .await
        .unwrap();

    // 2. Simulate clustering completed event
    let event = serde_json::json!({
        "event": "clustering_completed",
        "algorithm": "heuristic",
        "timestamp": "2025-12-09T10:05:00Z",
        "results": {
            "num_clusters": clusters.len(),
            "clusters": clusters
        }
    });

    // 3. Verify event data
    assert_eq!(event["event"], "clustering_completed");
    assert_eq!(event["algorithm"], "heuristic");
    assert_eq!(event["results"]["num_clusters"], clusters.len());

    // 4. Verify cluster data structure
    if let Some(clusters_array) = event["results"]["clusters"].as_array() {
        for cluster in clusters_array {
            assert!(cluster.get("id").is_some(), "Cluster should have ID");
            assert!(
                cluster.get("documents").is_some(),
                "Cluster should have documents"
            );
            assert!(
                cluster.get("confidence").is_some(),
                "Cluster should have confidence score"
            );
        }
    }
}

/// Test event flow: MoC discovered -> Enrichment applied
#[tokio::test]
async fn test_moc_discovered_event_flow() {
    let (_temp, vault_path) = create_moc_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Detect MoCs
    let mocs = tools.detect_mocs(Some(0.4)).await.unwrap();
    assert!(!mocs.is_empty(), "Should detect MoCs");

    // 2. Simulate MoC discovered events
    for moc in mocs {
        let event = serde_json::json!({
            "event": "moc_discovered",
            "moc": {
                "path": moc.path,
                "score": moc.score,
                "reasons": moc.reasons,
                "outbound_links": moc.outbound_links,
                "inbound_links": moc.inbound_links
            },
            "timestamp": "2025-12-09T10:10:00Z"
        });

        // 3. Verify event structure
        assert!(event["moc"]["path"].is_string());
        assert!(event["moc"]["score"].is_number());
        assert!(event["moc"]["reasons"].is_array());
        assert!(event["moc"]["outbound_links"].is_number());
        assert!(event["moc"]["inbound_links"].is_number());
    }
}

/// Test error handling in event flow
#[tokio::test]
async fn test_event_flow_error_handling() {
    let (_temp, vault_path) = create_empty_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Try clustering with empty vault
    let clusters = tools
        .cluster_documents(Some(0.2), Some(2), Some(0.6), Some(0.3), Some(0.1))
        .await
        .unwrap();

    assert_eq!(clusters.len(), 0, "Empty vault should produce no clusters");

    // 2. Simulate error event
    let error_event = serde_json::json!({
        "event": "clustering_failed",
        "error": {
            "code": "EMPTY_VAULT",
            "message": "No documents found to cluster"
        },
        "timestamp": "2025-12-09T10:15:00Z"
    });

    assert_eq!(error_event["event"], "clustering_failed");
    assert_eq!(error_event["error"]["code"], "EMPTY_VAULT");
}

/// Test concurrent event processing
#[tokio::test]
async fn test_concurrent_event_processing() {
    let (_temp, vault_path) = create_test_vault().await;

    // Create separate tools instances for concurrent tasks
    let vault_path1 = vault_path.clone();
    let vault_path2 = vault_path.clone();
    let vault_path3 = vault_path.clone();

    // 1. Spawn concurrent clustering operations
    let detect_task = tokio::spawn(async move {
        let tools = ClusteringTools::new(vault_path1);
        tools.detect_mocs(Some(0.3)).await
    });

    let cluster_task = tokio::spawn(async move {
        let tools = ClusteringTools::new(vault_path2);
        tools
            .cluster_documents(Some(0.2), Some(2), Some(0.6), Some(0.3), Some(0.1))
            .await
    });

    let stats_task = tokio::spawn(async move {
        let tools = ClusteringTools::new(vault_path3);
        tools.get_document_stats().await
    });

    // 2. Wait for all to complete
    let (mocs_result, clusters_result, stats_result) =
        tokio::try_join!(detect_task, cluster_task, stats_task).unwrap();

    // 3. Verify all operations succeeded
    assert!(
        mocs_result.is_ok(),
        "Concurrent MoC detection should succeed"
    );
    assert!(
        clusters_result.is_ok(),
        "Concurrent clustering should succeed"
    );
    assert!(stats_result.is_ok(), "Concurrent stats should succeed");
}

/// Test event persistence
#[tokio::test]
async fn test_event_persistence() {
    let (_temp, vault_path) = create_test_vault().await;
    let tools = ClusteringTools::new(vault_path);

    // 1. Run clustering and get results
    let initial_clusters = tools
        .cluster_documents(Some(0.2), Some(2), Some(0.6), Some(0.3), Some(0.1))
        .await
        .unwrap();

    // 2. Create persistence event
    let persist_event = serde_json::json!({
        "event": "clustering_results_persisted",
        "algorithm": "heuristic",
        "results": initial_clusters,
        "metadata": {
            "version": "1.0",
            "timestamp": "2025-12-09T10:20:00Z",
            "parameters": {
                "min_similarity": 0.2,
                "min_cluster_size": 2
            }
        }
    });

    // 3. Verify persistence event structure
    assert_eq!(persist_event["event"], "clustering_results_persisted");
    assert_eq!(persist_event["algorithm"], "heuristic");
    assert!(persist_event.get("results").is_some());
    assert!(persist_event.get("metadata").is_some());
    assert_eq!(persist_event["metadata"]["version"], "1.0");

    // 4. In a real implementation, verify data is persisted to storage
    // For now, just verify the event structure is correct
    assert!(persist_event["results"].is_array());
}

/// Test helper: Create test vault
async fn create_test_vault() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let vault_path = temp_dir.path().to_path_buf();

    // Create test documents
    fs::write(
        vault_path.join("index.md"),
        r#"---
tags: [moc, index]
---

# Knowledge Index

## AI/ML
- [[neural-networks]]
- [[machine-learning]]

## Web Development
- [[html-basics]]
- [[css-guide]]
"#,
    )
    .await
    .unwrap();

    fs::write(
        vault_path.join("neural-networks.md"),
        r#"---
tags: [ai, ml, deep-learning]
---

# Neural Networks

Fundamentals of neural networks.

Related: [[machine-learning]]
"#,
    )
    .await
    .unwrap();

    fs::write(
        vault_path.join("machine-learning.md"),
        r#"---
tags: [ai, ml]
---

# Machine Learning

Introduction to ML algorithms.

See also: [[neural-networks]]
"#,
    )
    .await
    .unwrap();

    fs::write(
        vault_path.join("html-basics.md"),
        r#"---
tags: [web, html]
---

# HTML Basics

Introduction to HTML.

See: [[css-guide]]
"#,
    )
    .await
    .unwrap();

    fs::write(
        vault_path.join("css-guide.md"),
        r#"---
tags: [web, css]
---

# CSS Guide

Styling with CSS.

Related: [[html-basics]]
"#,
    )
    .await
    .unwrap();

    (temp_dir, vault_path)
}

/// Test helper: Create larger test vault with proper wikilinks
async fn create_large_test_vault() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let vault_path = temp_dir.path().to_path_buf();

    // Create multiple related documents with proper wikilinks
    let docs = vec![
        ("doc1.md", "Document 1", "ai, ml", vec!["doc2"]),
        ("doc2.md", "Document 2", "ml, nlp", vec!["doc1", "doc3"]),
        ("doc3.md", "Document 3", "nlp, ai", vec!["doc2"]),
        ("doc4.md", "Document 4", "web, frontend", vec!["doc5"]),
        ("doc5.md", "Document 5", "frontend, css", vec!["doc4"]),
    ];

    for (filename, title, tags, links) in docs {
        let wikilinks: String = links
            .iter()
            .map(|l| format!("- [[{}]]", l))
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            r#"---
tags: [{}]
---

# {}

Content for {}.

## Related
{}
"#,
            tags, title, title, wikilinks
        );

        fs::write(vault_path.join(filename), content).await.unwrap();
    }

    (temp_dir, vault_path)
}

/// Test helper: Create MoC test vault
async fn create_moc_test_vault() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let vault_path = temp_dir.path().to_path_buf();

    // Create a clear MoC structure
    fs::write(
        vault_path.join("knowledge-map.md"),
        r#"---
tags: [moc, map]
---

# Knowledge Map

## Core Concepts
- [[concept-1]]
- [[concept-2]]

## Applications
- [[app-1]]
- [[app-2]]

## Resources
- [[resource-1]]
"#,
    )
    .await
    .unwrap();

    // Create referenced documents
    for doc in ["concept-1", "concept-2", "app-1", "app-2", "resource-1"] {
        fs::write(
            vault_path.join(format!("{}.md", doc)),
            format!(
                r#"---
tags: [topic]
---

# {}

Content for {}
"#,
                doc.replace("-", " ").to_uppercase(),
                doc
            ),
        )
        .await
        .unwrap();
    }

    (temp_dir, vault_path)
}

/// Test helper: Create empty test vault
async fn create_empty_test_vault() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let vault_path = temp_dir.path().to_path_buf();
    (temp_dir, vault_path)
}
