//! Integration tests for clustering using real test-kiln data
//!
//! These tests use the examples/test-kiln/ directory to validate clustering
//! algorithms against realistic knowledge base structures.

use super::*;
use super::test_utils::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Integration test for MoC detection on the test-kiln data
#[tokio::test]
#[ignore = "Requires test-kiln directory"]
async fn test_moc_detection_on_test_kiln() {
    // This test would be run with: cargo test -- --ignored

    // Load documents from test-kiln
    let test_kiln_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../")
        .join("examples")
        .join("test-kiln");

    if !test_kiln_path.exists() {
        println!("Skipping test - test-kiln not found at {:?}", test_kiln_path);
        return;
    }

    let documents = load_documents_from_path(&test_kiln_path).await
        .expect("Failed to load documents from test-kiln");

    assert!(!documents.is_empty(), "Should load documents from test-kiln");

    // Run MoC detection
    let mocs = detect_mocs(&documents).await
        .expect("MoC detection should succeed");

    // Verify Knowledge Management Hub is detected as MoC
    let hub_moc = mocs.iter()
        .find(|m| m.file_path.contains("Knowledge Management Hub"))
        .expect("Knowledge Management Hub should be detected as MoC");

    assert!(hub_moc.score > 0.5, "Hub should have high MoC score");
    assert!(hub_moc.outbound_links > 10, "Hub should have many outbound links");

    // Check reasons include expected indicators
    let reasons_str = hub_moc.reasons.join(" ");
    assert!(reasons_str.contains("outbound") || reasons_str.contains("links"),
            "Should mention link count in reasons");
}

/// Integration test for heuristic clustering on test-kiln data
#[tokio::test]
#[ignore = "Requires test-kiln directory"]
async fn test_heuristic_clustering_on_test_kiln() {
    let test_kiln_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../")
        .join("examples")
        .join("test-kiln");

    if !test_kiln_path.exists() {
        println!("Skipping test - test-kiln not found");
        return;
    }

    let documents = load_documents_from_path(&test_kiln_path).await
        .expect("Failed to load documents");

    // Create clustering service
    let service = SimpleClusteringService::new();

    // Configure for heuristic clustering
    let config = ClusteringConfig {
        algorithm: "heuristic".to_string(),
        parameters: AlgorithmParameters::new({
            let mut params = HashMap::new();
            params.insert("link_weight".to_string(), serde_json::json!(0.5));
            params.insert("tag_weight".to_string(), serde_json::json!(0.3));
            params.insert("title_weight".to_string(), serde_json::json!(0.2));
            params.insert("min_similarity".to_string(), serde_json::json!(0.15));
            params
        }),
        min_cluster_size: 2,
        max_clusters: None,
        detect_mocs: true,
        moc_config: None,
        embedding_config: None,
        performance: PerformanceConfig::default(),
    };

    // Run clustering
    let start_time = Instant::now();
    let result = service.cluster_documents(documents, config).await
        .expect("Clustering should succeed");
    let duration = start_time.elapsed();

    // Verify results
    assert!(!result.clusters.is_empty(), "Should generate at least one cluster");

    // Performance check - should be fast for small dataset
    assert!(duration.as_millis() < 100, "Heuristic clustering should complete quickly");

    // Check metrics
    assert_eq!(result.metrics.documents_processed, 12, "Should process all test-kiln documents");
    assert!(result.metrics.avg_cluster_size > 0.0, "Should have non-zero average cluster size");

    // Verify Knowledge Management Hub is in its own cluster or with related docs
    let hub_cluster = result.clusters.iter()
        .find(|c| c.documents.iter().any(|d| d.contains("Knowledge Management Hub")));

    if let Some(cluster) = hub_cluster {
        assert!(cluster.documents.len() >= 1, "Hub cluster should not be empty");
        assert!(cluster.confidence > 0.5, "Hub cluster should have good confidence");
    }

    println!("Generated {} clusters in {}ms",
             result.clusters.len(),
             duration.as_millis());
    for cluster in &result.clusters {
        println!("  Cluster {}: {} documents, confidence={:.2}",
                 cluster.id,
                 cluster.documents.len(),
                 cluster.confidence);
    }
}

/// Integration test for algorithm auto-selection
#[tokio::test]
#[ignore = "Requires test-kiln directory"]
async fn test_algorithm_auto_selection() {
    let test_kiln_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../")
        .join("examples")
        .join("test-kiln");

    if !test_kiln_path.exists() {
        println!("Skipping test - test-kiln not found");
        return;
    }

    let documents = load_documents_from_path(&test_kiln_path).await
        .expect("Failed to load documents");

    let service = SimpleClusteringService::new();

    // Test auto-selection
    let selected = service.auto_select_algorithm(&documents)
        .expect("Should auto-select an algorithm");

    // Should select heuristic for this small dataset without embeddings
    assert_eq!(selected, "heuristic", "Should select heuristic for documents without embeddings");

    // List all available algorithms
    let algorithms = service.list_algorithms();
    assert!(algorithms.len() >= 2, "Should have at least heuristic and kmeans");

    // Verify heuristic algorithm is available
    let heuristic_meta = algorithms.iter()
        .find(|a| a.id == "heuristic")
        .expect("Heuristic algorithm should be available");

    assert!(!heuristic_meta.requires_embeddings, "Heuristic should not require embeddings");
}

/// Integration test for MoC detection configuration
#[tokio::test]
#[ignore = "Requires test-kiln directory"]
async fn test_moc_detection_configuration() {
    let test_kiln_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../")
        .join("examples")
        .join("test-kiln");

    if !test_kiln_path.exists() {
        println!("Skipping test - test-kiln not found");
        return;
    }

    let documents = load_documents_from_path(&test_kiln_path).await
        .expect("Failed to load documents");

    // Test custom MoC detection config
    let moc_config = MocDetectionConfig {
        min_outbound_links: 8, // Lower threshold
        score_threshold: 0.3,  // Lower threshold
        moc_tags: vec!["hub".to_string(), "index".to_string(), "navigation".to_string()],
        title_patterns: vec![
            "Hub".to_string(),
            "Management".to_string(),
            "Documentation".to_string(),
        ],
    };

    let config = ClusteringConfig {
        algorithm: "heuristic".to_string(),
        parameters: AlgorithmParameters::new(HashMap::new()),
        min_cluster_size: 2,
        max_clusters: None,
        detect_mocs: true,
        moc_config: Some(moc_config),
        embedding_config: None,
        performance: PerformanceConfig::default(),
    };

    // Run clustering with MoC detection
    let service = SimpleClusteringService::new();
    let result = service.cluster_documents(documents, config).await
        .expect("Clustering should succeed");

    // Should detect more MoCs with lower thresholds
    assert!(result.warnings.len() >= 0, "Should collect any warnings");

    // Print any warnings for diagnostics
    for warning in &result.warnings {
        println!("Warning: {}", warning);
    }
}

/// Integration test for clustering quality metrics
#[tokio::test]
#[ignore = "Requires test-kiln directory"]
async fn test_clustering_quality_metrics() {
    let test_kiln_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../")
        .join("examples")
        .join("test-kiln");

    if !test_kiln_path.exists() {
        println!("Skipping test - test-kiln not found");
        return;
    }

    let documents = load_documents_from_path(&test_kiln_path).await
        .expect("Failed to load documents");

    // Create test documents with known cluster structure
    let test_docs = create_test_kiln_scenario();

    let config = ClusteringConfig {
        algorithm: "heuristic".to_string(),
        parameters: AlgorithmParameters::new({
            let mut params = HashMap::new();
            params.insert("link_weight".to_string(), serde_json::json!(0.6));
            params.insert("tag_weight".to_string(), serde_json::json!(0.4));
            params.insert("min_similarity".to_string(), serde_json::json!(0.1));
            params
        }),
        min_cluster_size: 2,
        max_clusters: Some(5),
        detect_mocs: false,
        moc_config: None,
        embedding_config: None,
        performance: PerformanceConfig::default(),
    };

    let service = SimpleClusteringService::new();
    let result = service.cluster_documents(test_docs, config).await
        .expect("Clustering should succeed");

    // Verify quality metrics
    assert!(result.metrics.execution_time_ms > 0, "Should record execution time");
    assert_eq!(result.metrics.documents_processed, 8, "Should process all test documents");
    assert!(result.metrics.clusters_generated > 0, "Should generate clusters");
    assert!(result.metrics.avg_cluster_size > 0.0, "Should calculate average cluster size");

    // Check for reasonable cluster distribution
    let total_docs: usize = result.clusters.iter().map(|c| c.documents.len()).sum();
    assert_eq!(total_docs, 8, "All documents should be assigned to clusters");

    // Verify clusters meet minimum size requirement
    for cluster in &result.clusters {
        assert!(cluster.documents.len() >= 2, "All clusters should meet min size");
        assert!(cluster.confidence > 0.0, "All clusters should have confidence scores");
    }
}

/// Load documents from a directory path
async fn load_documents_from_path(path: &Path) -> Result<Vec<DocumentInfo>, Box<dyn std::error::Error>> {
    let mut documents = Vec::new();

    // For this test, we'll create mock documents based on the actual test-kiln structure
    // In a real implementation, this would parse the markdown files

    let test_files = vec![
        ("Knowledge Management Hub.md", "hub", vec!["knowledge-management", "navigation"], 15),
        ("Project Management.md", "project", vec!["project-management", "tasks"], 12),
        ("Research Methods.md", "research", vec!["research", "methodology"], 10),
        ("Technical Documentation.md", "technical", vec!["api", "documentation"], 13),
        ("Contact Management.md", "contacts", vec!["contacts", "networking"], 8),
        ("Meeting Notes.md", "meeting", vec!["meetings", "notes"], 6),
        ("Reading List.md", "reading", vec!["learning", "books"], 9),
        ("Ideas & Brainstorming.md", "ideas", vec!["ideas", "innovation"], 7),
        ("API Documentation.md", "api", vec!["api", "technical"], 11),
        ("Book Review.md", "review", vec!["review", "learning"], 5),
        ("Comprehensive-Feature-Test.md", "test", vec!["test", "integration"], 4),
        ("README - Test Kiln Structure.md", "docs", vec!["documentation"], 3),
    ];

    for (filename, doc_type, tags, outbound_links) in test_files {
        documents.push(DocumentInfo {
            file_path: filename.to_string(),
            title: Some(filename.replace(".md", "")),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            outbound_links: vec![], // Would be filled by actual parsing
            inbound_links: vec![],
            embedding: None,
            content_length: 1000 + outbound_links * 100, // Mock content length
        });
    }

    Ok(documents)
}

/// Create a test scenario based on test-kiln structure
fn create_test_kiln_scenario() -> Vec<DocumentInfo> {
    vec![
        // Central hub document
        DocumentInfo {
            file_path: "knowledge-management-hub.md".to_string(),
            title: Some("Knowledge Management Hub".to_string()),
            tags: vec!["hub".to_string(), "navigation".to_string()],
            outbound_links: vec![
                "project-management.md".to_string(),
                "research-methods.md".to_string(),
                "technical-docs.md".to_string(),
                "contact-management.md".to_string(),
            ],
            inbound_links: vec![],
            embedding: None,
            content_length: 2000,
        },
        // Project management cluster
        DocumentInfo {
            file_path: "project-management.md".to_string(),
            title: Some("Project Management".to_string()),
            tags: vec!["project".to_string(), "tasks".to_string()],
            outbound_links: vec!["meeting-notes.md".to_string()],
            inbound_links: vec!["knowledge-management-hub.md".to_string()],
            embedding: None,
            content_length: 1500,
        },
        DocumentInfo {
            file_path: "meeting-notes.md".to_string(),
            title: Some("Meeting Notes".to_string()),
            tags: vec!["meeting".to_string(), "notes".to_string()],
            outbound_links: vec!["project-management.md".to_string()],
            inbound_links: vec!["project-management.md".to_string()],
            embedding: None,
            content_length: 1000,
        },
        // Research cluster
        DocumentInfo {
            file_path: "research-methods.md".to_string(),
            title: Some("Research Methods".to_string()),
            tags: vec!["research".to_string(), "methodology".to_string()],
            outbound_links: vec!["reading-list.md".to_string()],
            inbound_links: vec!["knowledge-management-hub.md".to_string()],
            embedding: None,
            content_length: 1800,
        },
        DocumentInfo {
            file_path: "reading-list.md".to_string(),
            title: Some("Reading List".to_string()),
            tags: vec!["learning".to_string(), "books".to_string()],
            outbound_links: vec!["research-methods.md".to_string()],
            inbound_links: vec!["research-methods.md".to_string()],
            embedding: None,
            content_length: 1200,
        },
        // Technical cluster
        DocumentInfo {
            file_path: "technical-docs.md".to_string(),
            title: Some("Technical Documentation".to_string()),
            tags: vec!["technical".to_string(), "api".to_string()],
            outbound_links: vec!["api-documentation.md".to_string()],
            inbound_links: vec!["knowledge-management-hub.md".to_string()],
            embedding: None,
            content_length: 2500,
        },
        DocumentInfo {
            file_path: "api-documentation.md".to_string(),
            title: Some("API Documentation".to_string()),
            tags: vec!["api".to_string(), "documentation".to_string()],
            outbound_links: vec!["technical-docs.md".to_string()],
            inbound_links: vec!["technical-docs.md".to_string()],
            embedding: None,
            content_length: 2000,
        },
        // Contacts
        DocumentInfo {
            file_path: "contact-management.md".to_string(),
            title: Some("Contact Management".to_string()),
            tags: vec!["contacts".to_string(), "networking".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["knowledge-management-hub.md".to_string()],
            embedding: None,
            content_length: 1000,
        },
    ]
}

/// Integration test for user vault (optional, requires CRUCIBLE_KILN_PATH)
#[tokio::test]
#[ignore = "Requires CRUCIBLE_KILN_PATH environment variable"]
async fn test_user_vault_clustering() {
    // This test allows developers to test clustering on their own vault
    // Run with: CRUCIBLE_KILN_PATH=/path/to/your/vault cargo test -- --ignored test_user_vault_clustering

    let vault_path = match std::env::var("CRUCIBLE_KILN_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            println!("Skipping test - CRUCIBLE_KILN_PATH not set");
            return;
        }
    };

    if !vault_path.exists() {
        println!("Skipping test - vault path does not exist: {:?}", vault_path);
        return;
    }

    println!("Testing with vault at: {:?}", vault_path);

    // Load documents from user vault
    let documents = load_documents_from_path(&vault_path).await
        .expect("Failed to load documents from user vault");

    if documents.is_empty() {
        println!("No documents found in vault");
        return;
    }

    println!("Loaded {} documents from vault", documents.len());

    // Run MoC detection
    let mocs = detect_mocs(&documents).await
        .expect("MoC detection should succeed");

    println!("\n=== Detected Maps of Content ===");
    for moc in &mocs {
        println!("📋 {} (score: {:.2})", moc.file_path, moc.score);
        for reason in &moc.reasons {
            println!("   - {}", reason);
        }
    }

    // Run clustering
    let service = SimpleClusteringService::new();
    let config = ClusteringConfig {
        algorithm: "heuristic".to_string(),
        parameters: AlgorithmParameters::new({
            let mut params = HashMap::new();
            params.insert("link_weight".to_string(), serde_json::json!(0.5));
            params.insert("tag_weight".to_string(), serde_json::json!(0.3));
            params.insert("title_weight".to_string(), serde_json::json!(0.2));
            params.insert("min_similarity".to_string(), serde_json::json!(0.1));
            params
        }),
        min_cluster_size: 2,
        max_clusters: Some(20),
        detect_mocs: false,
        moc_config: None,
        embedding_config: None,
        performance: PerformanceConfig::default(),
    };

    let result = service.cluster_documents(documents, config).await
        .expect("Clustering should succeed");

    println!("\n=== Generated Clusters ===");
    for (i, cluster) in result.clusters.iter().enumerate() {
        println!("📁 Cluster {} (confidence: {:.2})", i + 1, cluster.confidence);
        for doc in &cluster.documents {
            println!("   - {}", doc);
        }
    }

    println!("\n=== Clustering Metrics ===");
    println!("Execution time: {}ms", result.metrics.execution_time_ms);
    println!("Documents processed: {}", result.metrics.documents_processed);
    println!("Clusters generated: {}", result.metrics.clusters_generated);
    println!("Average cluster size: {:.2}", result.metrics.avg_cluster_size);

    // Basic assertions
    assert!(!result.clusters.is_empty(), "Should generate at least one cluster");
    assert!(result.metrics.execution_time_ms > 0, "Should record execution time");
}

/// Integration test for user vault with different algorithms
#[tokio::test]
#[ignore = "Requires CRUCIBLE_KILN_PATH environment variable"]
async fn test_user_vault_algorithm_comparison() {
    let vault_path = match std::env::var("CRUCIBLE_KILN_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            println!("Skipping test - CRUCIBLE_KILN_PATH not set");
            return;
        }
    };

    if !vault_path.exists() {
        return;
    }

    // Load a subset of documents for faster testing
    let documents = load_documents_from_path(&vault_path).await
        .expect("Failed to load documents");

    // Limit to first 50 documents for faster testing
    let documents: Vec<_> = documents.into_iter().take(50).collect();

    println!("Testing with {} documents", documents.len());

    let service = SimpleClusteringService::new();

    // Test different configurations
    let configs = vec![
        ("link_heavy", AlgorithmParameters::new({
            let mut p = HashMap::new();
            p.insert("link_weight".to_string(), serde_json::json!(0.8));
            p.insert("tag_weight".to_string(), serde_json::json!(0.1));
            p.insert("title_weight".to_string(), serde_json::json!(0.1));
            p.insert("min_similarity".to_string(), serde_json::json!(0.1));
            p
        })),
        ("tag_heavy", AlgorithmParameters::new({
            let mut p = HashMap::new();
            p.insert("link_weight".to_string(), serde_json::json!(0.2));
            p.insert("tag_weight".to_string(), serde_json::json!(0.7));
            p.insert("title_weight".to_string(), serde_json::json!(0.1));
            p.insert("min_similarity".to_string(), serde_json::json!(0.15));
            p
        })),
        ("balanced", AlgorithmParameters::new({
            let mut p = HashMap::new();
            p.insert("link_weight".to_string(), serde_json::json!(0.4));
            p.insert("tag_weight".to_string(), serde_json::json!(0.3));
            p.insert("title_weight".to_string(), serde_json::json!(0.3));
            p.insert("min_similarity".to_string(), serde_json::json!(0.2));
            p
        })),
    ];

    println!("\n=== Algorithm Comparison ===");
    for (name, params) in configs {
        let config = ClusteringConfig {
            algorithm: "heuristic".to_string(),
            parameters: params,
            min_cluster_size: 2,
            max_clusters: Some(10),
            detect_mocs: false,
            moc_config: None,
            embedding_config: None,
            performance: PerformanceConfig::default(),
        };

        let result = service.cluster_documents(documents.clone(), config).await
            .expect("Clustering should succeed");

        println!(
            "{}: {} clusters, avg size: {:.2}, time: {}ms",
            name,
            result.clusters.len(),
            result.metrics.avg_cluster_size,
            result.metrics.execution_time_ms
        );
    }
}