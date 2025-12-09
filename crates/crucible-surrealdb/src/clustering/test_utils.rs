use super::*;
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;
use rand::SeedableRng;

/// Create a test directory with sample documents
pub fn create_test_kiln() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Create a simple MoC
    std::fs::write(
        path.join("Knowledge Hub.md"),
        r#"# Knowledge Management Hub

## Projects
- [[Project Alpha]]
- [[Project Beta]]

## Research
- [[AI Research]]
- [[Machine Learning Basics]]

## Meeting Notes
- [[Weekly Sync]]
- [[Project Retrospective]]
"#,
    ).unwrap();

    // Create linked documents
    std::fs::write(
        path.join("Project Alpha.md"),
        r#"---
tags: [project, active]
---

# Project Alpha

Started on 2024-01-01.

Related: [[Project Beta]], [[AI Research]]
"#,
    ).unwrap();

    std::fs::write(
        path.join("AI Research.md"),
        r#"---
tags: [research, ai]
---

# AI Research

Notes on artificial intelligence.

See also: [[Machine Learning Basics]]
"#,
    ).unwrap();

    // Create a regular note (not a MoC)
    std::fs::write(
        path.join("Daily Note 2024-01-01.md"),
        r#"---
tags: [daily]
---

# Daily Note - 2024-01-01

Worked on Project Alpha today. Made good progress on the implementation.

Also reviewed [[AI Research]] notes.
"#,
    ).unwrap();

    temp_dir
}

/// Create a temporary test kiln with specified structure
pub fn create_test_kiln_with_structure(
    moc_count: usize,
    content_count: usize,
    links_per_moc: usize,
) -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Create MoCs
    for i in 0..moc_count {
        let moc_file = path.join(format!("moc_{}.md", i));
        let moc_content = format!(
            r#"# MoC {}

## Content
{}
"#,
            i,
            (0..links_per_moc.min(content_count))
                .map(|j| format!("- [[content_{}.md]]", j))
                .collect::<Vec<_>>()
                .join("\n")
        );
        std::fs::write(moc_file, moc_content).unwrap();
    }

    // Create content documents
    for i in 0..content_count {
        let content_file = path.join(format!("content_{}.md", i));
        let moc_index = i % moc_count.max(1);
        let content_content = format!(
            r#"---
tags: [content, category_{}]
---

# Content {}

This is content document number {}.

Linked from [[moc_{}.md]]
"#,
            i % 3,
            i,
            i,
            moc_index
        );
        std::fs::write(content_file, content_content).unwrap();
    }

    temp_dir
}

/// Create a test directory with complex link structure
pub fn create_complex_test_kiln() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Central hub
    std::fs::write(
        path.join("README.md"),
        r#"# Knowledge Base

## Areas
- [[Projects]]
- [[Research]]
- [[Learning]]

## Daily Notes
- [[2024-01-01]]
- [[2024-01-02]]
"#,
    ).unwrap();

    // Area MoCs
    std::fs::write(
        path.join("Projects.md"),
        r#"---
tags: [moc, projects]
---

# Projects

## Active
- [[Project Alpha]]
- [[Project Beta]]

## Completed
- [[Project Legacy]]
"#,
    ).unwrap();

    std::fs::write(
        path.join("Research.md"),
        r#"---
tags: [moc, research]
---

# Research Topics

## Machine Learning
- [[ML Basics]]
- [[Neural Networks]]

## Databases
- [[SurrealDB Notes]]
- [[Query Optimization]]
"#,
    ).unwrap();

    std::fs::write(
        path.join("Learning.md"),
        r#"---
tags: [moc, learning]
---

# Learning Resources

## Books
- [[System Design]]
- [[Algorithms]]

## Courses
- [[ML Course]]
- [[Rust Mastery]]
"#,
    ).unwrap();

    // Content documents
    let content_docs = vec![
        ("Project Alpha.md", "project", "active", vec!["Project Beta.md", "2024-01-01.md"]),
        ("Project Beta.md", "project", "active", vec!["Project Alpha.md"]),
        ("Project Legacy.md", "project", "completed", vec!["README.md"]),
        ("ML Basics.md", "ml", "learning", vec!["Neural Networks.md", "ML Course.md"]),
        ("Neural Networks.md", "ml", "advanced", vec!["ML Basics.md"]),
        ("SurrealDB Notes.md", "database", "notes", vec!["Query Optimization.md"]),
        ("Query Optimization.md", "database", "optimization", vec!["SurrealDB Notes.md"]),
        ("2024-01-01.md", "daily", "note", vec!["Project Alpha.md"]),
        ("2024-01-02.md", "daily", "note", vec!["README.md"]),
    ];

    for (filename, category, status, links) in content_docs {
        let content = format!(
            r#"---
tags: [{}, {}, {}]
---

# {}

This is a {} document with status: {}.

## Related
{}
"#,
            category,
            status,
            if links.len() > 2 { "linked" } else { "isolated" },
            filename.replace(".md", ""),
            category,
            status,
            links.iter().map(|l| format!("- [[{}]]", l)).collect::<Vec<_>>().join("\n")
        );
        std::fs::write(path.join(filename), content).unwrap();
    }

    temp_dir
}

/// Create mock document info for testing
pub fn create_mock_documents() -> Vec<DocumentInfo> {
    vec![
        DocumentInfo {
            file_path: "Knowledge Hub.md".to_string(),
            title: Some("Knowledge Management Hub".to_string()),
            tags: vec!["hub".to_string(), "index".to_string()],
            outbound_links: vec![
                "Project Alpha.md".to_string(),
                "Project Beta.md".to_string(),
                "AI Research.md".to_string(),
                "Machine Learning Basics.md".to_string(),
                "Weekly Sync.md".to_string(),
                "Project Retrospective.md".to_string(),
            ],
            inbound_links: vec!["Project Alpha.md".to_string(), "AI Research.md".to_string()],
            embedding: None,
            content_length: 500,
        },
        DocumentInfo {
            file_path: "Project Alpha.md".to_string(),
            title: Some("Project Alpha".to_string()),
            tags: vec!["project".to_string(), "active".to_string()],
            outbound_links: vec!["Project Beta.md".to_string(), "AI Research.md".to_string()],
            inbound_links: vec!["Knowledge Hub.md".to_string()],
            embedding: None,
            content_length: 1500,
        },
        DocumentInfo {
            file_path: "AI Research.md".to_string(),
            title: Some("AI Research".to_string()),
            tags: vec!["research".to_string(), "ai".to_string()],
            outbound_links: vec!["Machine Learning Basics.md".to_string()],
            inbound_links: vec!["Knowledge Hub.md".to_string(), "Project Alpha.md".to_string()],
            embedding: None,
            content_length: 2000,
        },
        DocumentInfo {
            file_path: "Daily Note 2024-01-01.md".to_string(),
            title: Some("Daily Note - 2024-01-01".to_string()),
            tags: vec!["daily".to_string()],
            outbound_links: vec!["Project Alpha.md".to_string(), "AI Research.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 300,
        },
    ]
}

/// Generate a set of documents with known link structure
pub fn generate_test_document_set(
    num_docs: usize,
    links_per_doc: usize,
) -> Vec<DocumentInfo> {
    let mut docs = Vec::new();
    let mut all_paths = HashSet::new();

    // Generate document paths
    for i in 0..num_docs {
        let path = format!("document_{}.md", i);
        all_paths.insert(path.clone());
    }

    let all_paths: Vec<String> = all_paths.into_iter().collect();

    // Create documents with links
    for (i, path) in all_paths.iter().enumerate() {
        let mut outbound_links = Vec::new();

        // Add some links (avoiding self-links)
        for j in 1..=links_per_doc.min(num_docs - 1) {
            let link_index = (i + j) % num_docs;
            outbound_links.push(all_paths[link_index].clone());
        }

        docs.push(DocumentInfo {
            file_path: path.clone(),
            title: Some(format!("Document {}", i)),
            tags: if i == 0 { vec!["moc".to_string()] } else { vec![] },
            outbound_links,
            inbound_links: vec![],
            embedding: None,
            content_length: 1000,
        });
    }

    // Calculate inbound links
    // Collect all links first, then update
    let mut inbound_updates: Vec<(String, String)> = Vec::new(); // (target_path, source_path)

    for i in 0..docs.len() {
        for link in &docs[i].outbound_links {
            inbound_updates.push((link.clone(), docs[i].file_path.clone()));
        }
    }

    // Now apply the updates
    for (target_path, source_path) in inbound_updates {
        if let Some(doc) = docs.iter_mut().find(|d| d.file_path == target_path) {
            doc.inbound_links.push(source_path);
        }
    }

    docs
}

/// Create a realistic test knowledge base with domains and structure
pub fn generate_realistic_test_kiln(num_docs: usize) -> Vec<DocumentInfo> {
    let mut documents = Vec::new();
    let mut all_paths = HashSet::new();

    // Define content domains
    let domains = vec![
        ("project-management", vec!["project", "tasks", "timeline", "milestone"]),
        ("research", vec!["research", "methodology", "literature", "analysis"]),
        ("technical", vec!["api", "documentation", "code", "architecture"]),
        ("learning", vec!["learning", "notes", "books", "courses"]),
        ("meetings", vec!["meeting", "notes", "action-items", "decisions"]),
        ("contacts", vec!["contact", "network", "profile", "collaboration"]),
    ];

    // Generate central hub document
    let hub_path = "knowledge-hub.md".to_string();
    all_paths.insert(hub_path.clone());

    let hub = DocumentInfo {
        file_path: hub_path.clone(),
        title: Some("Knowledge Management Hub".to_string()),
        tags: vec!["hub".to_string(), "navigation".to_string(), "index".to_string()],
        outbound_links: (0..num_docs.min(6))
            .map(|i| format!("doc_{}.md", i + 1))
            .collect(),
        inbound_links: vec![],
        embedding: None,
        content_length: 3000,
    };
    documents.push(hub);

    // Generate remaining documents
    for i in 1..num_docs {
        let domain_idx = i % domains.len();
        let (domain_name, domain_tags) = &domains[domain_idx];

        let doc_path = format!("doc_{}.md", i);
        all_paths.insert(doc_path.clone());

        // Mix of inbound and outbound links
        let mut outbound_links = Vec::new();
        let inbound_links = Vec::new();

        // Link to hub with 80% probability
        if i % 10 != 0 {
            outbound_links.push(hub_path.clone());
        }

        // Links to other documents (circular pattern)
        let next_doc = format!("doc_{}.md", (i % (num_docs - 1)) + 1);
        if next_doc != doc_path {
            outbound_links.push(next_doc);
        }

        let doc = DocumentInfo {
            file_path: doc_path,
            title: Some(format!("Document {} ({})", i, domain_name.replace("-", " "))),
            tags: domain_tags.iter().take(2).map(|t| t.to_string()).collect(),
            outbound_links,
            inbound_links,
            embedding: None,
            content_length: 500 + (i * 123) % 2000,
        };

        documents.push(doc);
    }

    // Calculate inbound links
    let mut link_updates: Vec<(String, String)> = Vec::new();
    for doc in &documents {
        for link in &doc.outbound_links {
            link_updates.push((link.clone(), doc.file_path.clone()));
        }
    }

    // Apply the updates
    for (target_path, source_path) in link_updates {
        if let Some(target) = documents.iter_mut().find(|d| d.file_path == target_path) {
            target.inbound_links.push(source_path);
        }
    }

    documents
}

/// Create a known MoC structure for testing
pub fn create_moc_structure() -> Vec<DocumentInfo> {
    vec![
        // Main MoC
        DocumentInfo {
            file_path: "index.md".to_string(),
            title: Some("Main Index".to_string()),
            tags: vec!["moc".to_string(), "index".to_string()],
            outbound_links: vec![
                "projects/index.md".to_string(),
                "research/index.md".to_string(),
                "learnings/index.md".to_string(),
            ],
            inbound_links: vec![],
            embedding: None,
            content_length: 800,
        },
        // Project MoC
        DocumentInfo {
            file_path: "projects/index.md".to_string(),
            title: Some("Projects".to_string()),
            tags: vec!["moc".to_string(), "projects".to_string()],
            outbound_links: vec![
                "projects/project-alpha.md".to_string(),
                "projects/project-beta.md".to_string(),
            ],
            inbound_links: vec!["index.md".to_string()],
            embedding: None,
            content_length: 600,
        },
        // Research MoC
        DocumentInfo {
            file_path: "research/index.md".to_string(),
            title: Some("Research".to_string()),
            tags: vec!["moc".to_string(), "research".to_string()],
            outbound_links: vec![
                "research/topic-a.md".to_string(),
                "research/topic-b.md".to_string(),
            ],
            inbound_links: vec!["index.md".to_string()],
            embedding: None,
            content_length: 600,
        },
        // Learnings MoC
        DocumentInfo {
            file_path: "learnings/index.md".to_string(),
            title: Some("Learnings".to_string()),
            tags: vec!["moc".to_string(), "learning".to_string()],
            outbound_links: vec![
                "learnings/concept-x.md".to_string(),
                "learnings/concept-y.md".to_string(),
            ],
            inbound_links: vec!["index.md".to_string()],
            embedding: None,
            content_length: 600,
        },
        // Regular content documents
        DocumentInfo {
            file_path: "projects/project-alpha.md".to_string(),
            title: Some("Project Alpha".to_string()),
            tags: vec!["project".to_string(), "active".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["projects/index.md".to_string()],
            embedding: None,
            content_length: 1200,
        },
        DocumentInfo {
            file_path: "projects/project-beta.md".to_string(),
            title: Some("Project Beta".to_string()),
            tags: vec!["project".to_string(), "completed".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["projects/index.md".to_string()],
            embedding: None,
            content_length: 800,
        },
        DocumentInfo {
            file_path: "research/topic-a.md".to_string(),
            title: Some("Research Topic A".to_string()),
            tags: vec!["research".to_string(), "ongoing".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["research/index.md".to_string()],
            embedding: None,
            content_length: 1500,
        },
        DocumentInfo {
            file_path: "research/topic-b.md".to_string(),
            title: Some("Research Topic B".to_string()),
            tags: vec!["research".to_string(), "draft".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["research/index.md".to_string()],
            embedding: None,
            content_length: 900,
        },
    ]
}

/// Measure clustering quality against ground truth
pub fn measure_clustering_quality(
    result: &ClusteringResult,
    ground_truth: &HashMap<String, String>, // doc_path -> expected_cluster_id
) -> QualityMetrics {
    let mut correct_assignments = 0;
    let mut total_assignments = 0;

    // Create mapping from doc to assigned cluster
    let mut doc_to_cluster: HashMap<String, String> = HashMap::new();
    for cluster in &result.clusters {
        for doc in &cluster.documents {
            doc_to_cluster.insert(doc.clone(), cluster.id.clone());
        }
    }

    // Count correct assignments
    for (doc_path, expected_cluster) in ground_truth {
        if let Some(actual_cluster) = doc_to_cluster.get(doc_path) {
            if actual_cluster == expected_cluster {
                correct_assignments += 1;
            }
            total_assignments += 1;
        }
    }

    // Calculate precision, recall, f1
    let precision = if total_assignments > 0 {
        correct_assignments as f64 / total_assignments as f64
    } else {
        0.0
    };

    // For recall, we need to count how many expected items were found
    let recall = if ground_truth.len() > 0 {
        correct_assignments as f64 / ground_truth.len() as f64
    } else {
        1.0
    };

    let f1_score = if precision + recall > 0.0 {
        2.0 * (precision * recall) / (precision + recall)
    } else {
        0.0
    };

    QualityMetrics {
        precision,
        recall,
        f1_score,
        cluster_count: result.clusters.len(),
    }
}

/// Quality metrics for clustering evaluation
#[derive(Debug, Clone)]
pub struct QualityMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub cluster_count: usize,
}

/// Generate embeddings for documents (for testing semantic clustering)
pub fn generate_embeddings(documents: &mut [DocumentInfo], dimensions: usize) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use rand::Rng;

    for doc in documents.iter_mut() {
        // Generate deterministic embeddings based on content
        let mut hasher = DefaultHasher::new();
        doc.file_path.hash(&mut hasher);
        for tag in &doc.tags {
            tag.hash(&mut hasher);
        }

        let seed = hasher.finish() as u64;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let mut embedding = Vec::with_capacity(dimensions);
        for _ in 0..dimensions {
            embedding.push(rng.gen_range(-1.0..1.0));
        }

        doc.embedding = Some(embedding);
    }
}

/// Create a clustering scenario with known ground truth
pub fn create_clustering_scenario() -> (Vec<DocumentInfo>, HashMap<String, String>) {
    let documents = vec![
        // Cluster 1: Project Management
        create_realistic_document(
            "project-overview.md",
            Some("Project Overview".to_string()),
            vec!["project".to_string(), "management".to_string()],
            vec!["project-tasks.md".to_string(), "project-timeline.md".to_string()],
            1500,
        ),
        create_realistic_document(
            "project-tasks.md",
            Some("Project Tasks".to_string()),
            vec!["project".to_string(), "tasks".to_string()],
            vec!["project-overview.md".to_string()],
            1000,
        ),
        create_realistic_document(
            "project-timeline.md",
            Some("Project Timeline".to_string()),
            vec!["project".to_string(), "timeline".to_string()],
            vec!["project-overview.md".to_string()],
            800,
        ),
        // Cluster 2: Research
        create_realistic_document(
            "research-plan.md",
            Some("Research Plan".to_string()),
            vec!["research".to_string(), "plan".to_string()],
            vec!["literature-review.md".to_string()],
            1200,
        ),
        create_realistic_document(
            "literature-review.md",
            Some("Literature Review".to_string()),
            vec!["research".to_string(), "review".to_string()],
            vec!["research-plan.md".to_string()],
            2000,
        ),
        // Cluster 3: Technical
        create_realistic_document(
            "api-spec.md",
            Some("API Specification".to_string()),
            vec!["api".to_string(), "technical".to_string()],
            vec!["implementation-guide.md".to_string()],
            3000,
        ),
        create_realistic_document(
            "implementation-guide.md",
            Some("Implementation Guide".to_string()),
            vec!["guide".to_string(), "technical".to_string()],
            vec!["api-spec.md".to_string()],
            2500,
        ),
    ];

    // Ground truth mapping
    let mut ground_truth = HashMap::new();

    // Project Management cluster
    ground_truth.insert("project-overview.md".to_string(), "cluster_1".to_string());
    ground_truth.insert("project-tasks.md".to_string(), "cluster_1".to_string());
    ground_truth.insert("project-timeline.md".to_string(), "cluster_1".to_string());

    // Research cluster
    ground_truth.insert("research-plan.md".to_string(), "cluster_2".to_string());
    ground_truth.insert("literature-review.md".to_string(), "cluster_2".to_string());

    // Technical cluster
    ground_truth.insert("api-spec.md".to_string(), "cluster_3".to_string());
    ground_truth.insert("implementation-guide.md".to_string(), "cluster_3".to_string());

    (documents, ground_truth)
}

/// Create a realistic document with embedded content
fn create_realistic_document(
    path: &str,
    title: Option<String>,
    tags: Vec<String>,
    outbound_links: Vec<String>,
    content_length: usize,
) -> DocumentInfo {
    DocumentInfo {
        file_path: path.to_string(),
        title,
        tags,
        outbound_links,
        inbound_links: vec![],
        embedding: None,
        content_length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_kiln() {
        let temp_dir = create_test_kiln();
        let path = temp_dir.path();

        // Check that files were created
        assert!(path.join("Knowledge Hub.md").exists());
        assert!(path.join("Project Alpha.md").exists());
        assert!(path.join("AI Research.md").exists());
        assert!(path.join("Daily Note 2024-01-01.md").exists());

        // Check content
        let hub_content = std::fs::read_to_string(path.join("Knowledge Hub.md")).unwrap();
        assert!(hub_content.contains("# Knowledge Management Hub"));
        assert!(hub_content.contains("[[Project Alpha]]"));
    }

    #[test]
    fn test_create_test_kiln_with_structure() {
        let temp_dir = create_test_kiln_with_structure(3, 10, 4);
        let path = temp_dir.path();

        // Check MoCs
        for i in 0..3 {
            assert!(path.join(format!("moc_{}.md", i)).exists());
        }

        // Check content docs
        for i in 0..10 {
            assert!(path.join(format!("content_{}.md", i)).exists());
        }

        // Check MoC content
        let moc_0 = std::fs::read_to_string(path.join("moc_0.md")).unwrap();
        assert!(moc_0.contains("[[content_0.md]]"));
        assert!(moc_0.contains("[[content_1.md]]"));
    }

    #[test]
    fn test_create_complex_test_kiln() {
        let temp_dir = create_complex_test_kiln();
        let path = temp_dir.path();

        // Check main files
        assert!(path.join("README.md").exists());
        assert!(path.join("Projects.md").exists());
        assert!(path.join("Research.md").exists());
        assert!(path.join("Learning.md").exists());

        // Check content documents
        assert!(path.join("Project Alpha.md").exists());
        assert!(path.join("ML Basics.md").exists());

        // Verify MoC tags
        let projects_content = std::fs::read_to_string(path.join("Projects.md")).unwrap();
        assert!(projects_content.contains("tags: [moc, projects]"));
    }

    #[test]
    fn test_generate_test_kiln() {
        let documents = generate_realistic_test_kiln(10);
        assert_eq!(documents.len(), 10);

        // Check hub document
        let hub = &documents[0];
        assert_eq!(hub.title, Some("Knowledge Management Hub".to_string()));
        assert!(hub.tags.contains(&"hub".to_string()));
        assert!(!hub.outbound_links.is_empty());
    }

    #[test]
    fn test_moc_structure() {
        let documents = create_moc_structure();
        assert_eq!(documents.len(), 8);

        // Main index should be MoC
        let main_index = documents.iter().find(|d| d.file_path == "index.md").unwrap();
        assert!(main_index.tags.contains(&"moc".to_string()));
        assert_eq!(main_index.outbound_links.len(), 3);
    }

    #[test]
    fn test_clustering_scenario() {
        let (documents, ground_truth) = create_clustering_scenario();
        assert_eq!(documents.len(), 7);
        assert_eq!(ground_truth.len(), 7);

        // Verify ground truth clusters
        let mut clusters = HashSet::new();
        for cluster_id in ground_truth.values() {
            clusters.insert(cluster_id);
        }
        assert_eq!(clusters.len(), 3);
    }

    #[test]
    fn test_quality_metrics() {
        let ground_truth = HashMap::from([
            ("doc1.md".to_string(), "cluster_1".to_string()),
            ("doc2.md".to_string(), "cluster_1".to_string()),
            ("doc3.md".to_string(), "cluster_2".to_string()),
        ]);

        let result = ClusteringResult {
            clusters: vec![
                DocumentCluster {
                    id: "cluster_1".to_string(),
                    documents: vec!["doc1.md".to_string(), "doc2.md".to_string()],
                    centroid: None,
                    confidence: 0.9,
                },
                DocumentCluster {
                    id: "cluster_2".to_string(),
                    documents: vec!["doc3.md".to_string()],
                    centroid: None,
                    confidence: 0.8,
                },
            ],
            algorithm_metadata: AlgorithmMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                algorithm_type: AlgorithmType::Heuristic,
                description: "".to_string(),
                requires_embeddings: false,
                supports_async: true,
                embedding_dimensions: None,
                default_parameters: HashMap::new(),
                parameter_schema: None,
            },
            metrics: ClusteringMetrics {
                execution_time_ms: 100,
                documents_processed: 3,
                clusters_generated: 2,
                avg_cluster_size: 1.5,
                silhouette_score: None,
                custom_metrics: HashMap::new(),
            },
            warnings: vec![],
        };

        let metrics = measure_clustering_quality(&result, &ground_truth);
        assert_eq!(metrics.precision, 1.0);
        assert_eq!(metrics.recall, 1.0);
        assert_eq!(metrics.f1_score, 1.0);
    }

    #[test]
    fn test_generate_embeddings() {
        let mut documents = create_mock_documents();
        generate_embeddings(&mut documents, 384);

        for doc in &documents {
            assert!(doc.embedding.is_some());
            let embedding = doc.embedding.as_ref().unwrap();
            assert_eq!(embedding.len(), 384);

            // Check embedding values are in valid range
            for &val in embedding {
                assert!(val >= -1.0 && val <= 1.0);
            }
        }

        // Check embeddings are deterministic
        let mut docs2 = create_mock_documents();
        generate_embeddings(&mut docs2, 384);

        for (doc1, doc2) in documents.iter().zip(docs2.iter()) {
            assert_eq!(doc1.embedding, doc2.embedding);
        }
    }

    #[test]
    fn test_create_mock_documents() {
        let documents = create_mock_documents();
        assert_eq!(documents.len(), 4);

        // Check hub document
        let hub = &documents[0];
        assert_eq!(hub.file_path, "Knowledge Hub.md");
        assert!(hub.tags.contains(&"hub".to_string()));
        assert_eq!(hub.outbound_links.len(), 6);

        // Check inbound links are calculated
        let ai_research = documents.iter().find(|d| d.file_path == "AI Research.md").unwrap();
        assert_eq!(ai_research.inbound_links.len(), 2);
    }

    #[test]
    fn test_generate_test_document_set() {
        let documents = generate_test_document_set(5, 2);
        assert_eq!(documents.len(), 5);

        // Check each document has outbound links
        for doc in &documents {
            assert_eq!(doc.outbound_links.len(), 2);
        }

        // Check inbound links are calculated
        let total_inbound: usize = documents.iter().map(|d| d.inbound_links.len()).sum();
        assert_eq!(total_inbound, 10); // Each doc has 2 links, so total 10 inbound links
    }
}