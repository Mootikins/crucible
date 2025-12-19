//! Clustering tools for Crucible knowledge bases
//!
//! This module provides MCP tools for detecting Maps of Content (`MoCs`) and
//! clustering documents using various algorithms.

use anyhow::{Context, Result};
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use walkdir::WalkDir;

/// A simplified document representation for tool inputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// File path relative to the vault root
    pub path: String,
    /// Document title (optional)
    pub title: Option<String>,
    /// Tags associated with the document
    pub tags: Vec<String>,
    /// Outbound wikilinks
    pub links: Vec<String>,
    /// Content length for weighting
    pub content_length: usize,
}

/// A detected Map of Content candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MocCandidate {
    /// The document path
    pub path: String,
    /// `MoC` score (0.0 - 1.0)
    pub score: f64,
    /// Reasons for detection
    pub reasons: Vec<String>,
    /// Number of outbound links
    pub outbound_links: usize,
    /// Number of inbound links
    pub inbound_links: usize,
}

/// A cluster of related documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCluster {
    /// Cluster identifier
    pub id: String,
    /// Documents in this cluster
    pub documents: Vec<String>,
    /// Confidence score
    pub confidence: f64,
}

/// Clustering tools for the MCP server
pub struct ClusteringTools {
    /// Path to the knowledge base/vault
    kiln_path: PathBuf,
}

impl ClusteringTools {
    /// Create new clustering tools instance
    #[must_use] 
    pub fn new(kiln_path: PathBuf) -> Self {
        Self { kiln_path }
    }

    /// Load documents from the vault
    async fn load_documents(&self) -> Result<Vec<Document>> {
        let mut documents = Vec::new();

        // Walk through all markdown files
        for entry in WalkDir::new(&self.kiln_path)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                e.file_type().is_file() && e.path().extension().is_some_and(|s| s == "md")
            })
        {
            let path = entry.path();
            let relative_path = path
                .strip_prefix(&self.kiln_path)
                .with_context(|| "Failed to get relative path")?;

            // Read file content
            let content = fs::read_to_string(path)
                .await
                .with_context(|| format!("Failed to read file: {path:?}"))?;

            // Parse frontmatter and extract basic info
            let (title, tags, links) = self.parse_document(&content);

            documents.push(Document {
                path: relative_path.to_string_lossy().to_string(),
                title,
                tags,
                links,
                content_length: content.len(),
            });
        }

        Ok(documents)
    }

    /// Parse document for title, tags, and links
    fn parse_document(&self, content: &str) -> (Option<String>, Vec<String>, Vec<String>) {
        let mut title = None;
        let mut tags = Vec::new();
        let mut links = Vec::new();

        // Extract title from first H1 or filename
        for line in content.lines() {
            if line.starts_with("# ") {
                title = Some(line.strip_prefix("# ").unwrap().trim().to_string());
                break;
            }
        }

        // Simple link extraction (wikilinks)
        for line in content.lines() {
            // Extract [[wikilinks]]
            let mut start = 0;
            while let Some(open) = line[start..].find("[[") {
                let open_pos = start + open;
                if let Some(close) = line[open_pos + 2..].find("]]") {
                    let close_pos = open_pos + 2 + close;
                    let link = line[open_pos + 2..close_pos].trim();
                    if !link.is_empty() {
                        // Handle aliases [[page|alias]]
                        let actual_link = link.split('|').next().unwrap_or(link);
                        links.push(actual_link.to_string());
                    }
                    start = close_pos + 2;
                } else {
                    break;
                }
            }
        }

        // Simple tag extraction from frontmatter or content
        if content.contains("---") {
            // Has frontmatter
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > 1 && lines[0] == "---" {
                let in_frontmatter = true;
                for line in lines.iter().skip(1) {
                    if *line == "---" {
                        break;
                    }
                    if line.starts_with("tags:") {
                        let tags_str = line.strip_prefix("tags:").unwrap_or("");
                        // Simple parsing for [tag1, tag2] format
                        if tags_str.contains('[') && tags_str.contains(']') {
                            let tags_content = tags_str
                                .trim()
                                .trim_start_matches('[')
                                .trim_end_matches(']');
                            for tag in tags_content.split(',') {
                                tags.push(tag.trim().trim_matches('"').to_string());
                            }
                        }
                    }
                }
            }
        }

        (title, tags, links)
    }

    /// Convert to crucible-surrealdb `DocumentInfo` format
    fn to_document_infos(&self, documents: &[Document]) -> Vec<crucible_surrealdb::DocumentInfo> {
        // Build a link mapping for inbound links
        let mut link_map: HashMap<String, Vec<String>> = HashMap::new();
        for doc in documents {
            for link in &doc.links {
                link_map
                    .entry(link.clone())
                    .or_default()
                    .push(doc.path.clone());
            }
        }

        documents
            .iter()
            .map(|doc| {
                let inbound_links = link_map.get(&doc.path).cloned().unwrap_or_default();
                crucible_surrealdb::DocumentInfo {
                    file_path: doc.path.clone(),
                    title: doc.title.clone(),
                    tags: doc.tags.clone(),
                    outbound_links: doc.links.clone(),
                    inbound_links,
                    embedding: None,
                    content_length: doc.content_length,
                }
            })
            .collect()
    }
}

impl ClusteringTools {
    /// Detect Maps of Content in the knowledge base
    pub async fn detect_mocs(&self, min_score: Option<f64>) -> Result<Vec<MocCandidate>> {
        let documents = self
            .load_documents()
            .await
            .context("Failed to load documents")?;

        if documents.is_empty() {
            return Ok(Vec::new());
        }

        // Convert to DocumentInfo format
        let doc_infos = self.to_document_infos(&documents);

        // Use the clustering module's MoC detection
        let mocs = crucible_surrealdb::clustering::detect_mocs(&doc_infos)
            .await
            .context("Failed to detect MoCs")?;

        // Convert to tool output format
        let min_score = min_score.unwrap_or(0.5);
        let candidates: Vec<MocCandidate> = mocs
            .into_iter()
            .filter(|m| m.score >= min_score)
            .map(|m| MocCandidate {
                path: m.file_path,
                score: m.score,
                reasons: m.reasons,
                outbound_links: m.outbound_links,
                inbound_links: m.inbound_links,
            })
            .collect();

        Ok(candidates)
    }

    /// Cluster documents using heuristic algorithm
    pub async fn cluster_documents(
        &self,
        min_similarity: Option<f64>,
        min_cluster_size: Option<usize>,
        link_weight: Option<f64>,
        tag_weight: Option<f64>,
        title_weight: Option<f64>,
    ) -> Result<Vec<DocumentCluster>> {
        let documents = self
            .load_documents()
            .await
            .context("Failed to load documents")?;

        if documents.is_empty() {
            return Ok(Vec::new());
        }

        // Convert to DocumentInfo format
        let doc_infos = self.to_document_infos(&documents);

        // Create clustering configuration
        let mut params = HashMap::new();
        params.insert(
            "min_similarity".to_string(),
            serde_json::json!(min_similarity.unwrap_or(0.2)),
        );
        params.insert(
            "link_weight".to_string(),
            serde_json::json!(link_weight.unwrap_or(0.6)),
        );
        params.insert(
            "tag_weight".to_string(),
            serde_json::json!(tag_weight.unwrap_or(0.3)),
        );
        params.insert(
            "title_weight".to_string(),
            serde_json::json!(title_weight.unwrap_or(0.1)),
        );

        let config = crucible_surrealdb::clustering::ClusteringConfig {
            algorithm: "heuristic".to_string(),
            parameters: crucible_surrealdb::clustering::AlgorithmParameters::new(params),
            min_cluster_size: min_cluster_size.unwrap_or(2),
            max_clusters: None,
            detect_mocs: false,
            moc_config: None,
            embedding_config: None,
            performance: crucible_surrealdb::clustering::PerformanceConfig::default(),
        };

        // Run clustering
        let service = crucible_surrealdb::clustering::SimpleClusteringService::new();
        let result = service
            .cluster_documents(doc_infos, config)
            .await
            .context("Failed to cluster documents")?;

        // Convert to tool output format
        let clusters: Vec<DocumentCluster> = result
            .clusters
            .into_iter()
            .map(|c| DocumentCluster {
                id: c.id,
                documents: c.documents,
                confidence: c.confidence,
            })
            .collect();

        Ok(clusters)
    }

    /// Get document statistics
    pub async fn get_document_stats(&self) -> Result<DocumentStats> {
        let documents = self
            .load_documents()
            .await
            .context("Failed to load documents")?;

        let total_docs = documents.len();
        let total_links: usize = documents.iter().map(|d| d.links.len()).sum();
        let total_tags: usize = documents.iter().map(|d| d.tags.len()).sum();
        let total_content: usize = documents.iter().map(|d| d.content_length).sum();

        // Calculate unique tags
        let mut unique_tags = std::collections::HashSet::new();
        for doc in &documents {
            for tag in &doc.tags {
                unique_tags.insert(tag.clone());
            }
        }

        Ok(DocumentStats {
            total_documents: total_docs,
            total_links,
            total_tags,
            unique_tags: unique_tags.len(),
            average_links_per_doc: if total_docs > 0 {
                total_links as f64 / total_docs as f64
            } else {
                0.0
            },
            average_tags_per_doc: if total_docs > 0 {
                total_tags as f64 / total_docs as f64
            } else {
                0.0
            },
            average_content_length: if total_docs > 0 {
                total_content as f64 / total_docs as f64
            } else {
                0.0
            },
        })
    }

    /// List all available clustering tools for MCP server
    pub async fn list_tools(&self) -> Vec<Tool> {
        let mut schema = Map::new();
        schema.insert("type".to_string(), "object".into());
        let mut props = Map::new();
        let mut min_score = Map::new();
        min_score.insert("type".to_string(), "number".into());
        min_score.insert(
            "description".to_string(),
            "Minimum MoC score threshold (0.0-1.0)".into(),
        );
        min_score.insert("minimum".to_string(), 0.0.into());
        min_score.insert("maximum".to_string(), 1.0.into());
        props.insert("min_score".to_string(), min_score.into());
        schema.insert("properties".to_string(), props.into());

        vec![
            Tool {
                name: Cow::Borrowed("detect_mocs"),
                title: None,
                description: Some(Cow::Borrowed(
                    "Detect Maps of Content (MoCs) in the knowledge base using heuristic analysis",
                )),
                input_schema: Arc::new(schema),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            },
            // Create schema for cluster_documents
            {
                let mut schema = Map::new();
                schema.insert("type".to_string(), "object".into());
                let mut props = Map::new();

                let fields = vec![
                    (
                        "min_similarity",
                        "number",
                        "Minimum similarity threshold (0.0-1.0)",
                        Some(0.0),
                        Some(1.0),
                    ),
                    (
                        "min_cluster_size",
                        "integer",
                        "Minimum documents per cluster",
                        Some(1.0),
                        None,
                    ),
                    (
                        "link_weight",
                        "number",
                        "Link weight in similarity calculation",
                        Some(0.0),
                        Some(1.0),
                    ),
                    (
                        "tag_weight",
                        "number",
                        "Tag weight in similarity calculation",
                        Some(0.0),
                        Some(1.0),
                    ),
                    (
                        "title_weight",
                        "number",
                        "Title weight in similarity calculation",
                        Some(0.0),
                        Some(1.0),
                    ),
                ];

                for (name, type_, desc, min, max) in fields {
                    let mut field = Map::new();
                    field.insert("type".to_string(), type_.into());
                    field.insert("description".to_string(), desc.into());
                    if let Some(val) = min {
                        field.insert("minimum".to_string(), val.into());
                    }
                    if let Some(val) = max {
                        field.insert("maximum".to_string(), val.into());
                    }
                    props.insert(name.to_string(), field.into());
                }

                schema.insert("properties".to_string(), props.into());

                Tool {
                    name: Cow::Borrowed("cluster_documents"),
                    title: None,
                    description: Some(Cow::Borrowed(
                        "Cluster documents in the knowledge base using heuristic similarity",
                    )),
                    input_schema: Arc::new(schema),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                }
            },
            // Create schema for get_document_stats
            {
                let mut schema = Map::new();
                schema.insert("type".to_string(), "object".into());
                let props = Map::new();
                schema.insert("properties".to_string(), props.into());

                Tool {
                    name: Cow::Borrowed("get_document_stats"),
                    title: None,
                    description: Some(Cow::Borrowed(
                        "Get statistics about documents in the knowledge base",
                    )),
                    input_schema: Arc::new(schema),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                }
            },
        ]
    }
}

/// Document statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStats {
    pub total_documents: usize,
    pub total_links: usize,
    pub total_tags: usize,
    pub unique_tags: usize,
    pub average_links_per_doc: f64,
    pub average_tags_per_doc: f64,
    pub average_content_length: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_vault() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path().to_path_buf();

        // Create test documents
        fs::write(
            vault_path.join("index.md"),
            r#"---
tags: [moc, index]
---

# Knowledge Index

## Projects
- [[project-alpha]]
- [[project-beta]]

## Research
- [[research-topic]]

## Daily Notes
- [[2024-01-01]]
"#,
        )
        .await
        .unwrap();

        fs::write(
            vault_path.join("project-alpha.md"),
            r#"---
tags: [project, active]
---

# Project Alpha

A test project.

Related: [[project-beta]]
"#,
        )
        .await
        .unwrap();

        fs::write(
            vault_path.join("project-beta.md"),
            r#"---
tags: [project]
---

# Project Beta

Another project.

See also: [[index]]
"#,
        )
        .await
        .unwrap();

        fs::write(
            vault_path.join("research-topic.md"),
            r#"# Research Topic

Some research notes.
"#,
        )
        .await
        .unwrap();

        fs::write(
            vault_path.join("2024-01-01.md"),
            "# Daily Note

Worked on [[project-alpha]] today.
",
        )
        .await
        .unwrap();

        (temp_dir, vault_path)
    }

    #[tokio::test]
    async fn test_detect_mocs() {
        let (_temp, vault_path) = create_test_vault().await;
        let tools = ClusteringTools::new(vault_path);

        let mocs = tools.detect_mocs(Some(0.1)).await.unwrap();

        // Should detect index.md as an MoC
        assert!(!mocs.is_empty());
        let index_moc = mocs.iter().find(|m| m.path.contains("index.md"));
        assert!(index_moc.is_some());
    }

    #[tokio::test]
    async fn test_cluster_documents() {
        let (_temp, vault_path) = create_test_vault().await;
        let tools = ClusteringTools::new(vault_path);

        let clusters = tools
            .cluster_documents(Some(0.1), Some(2), Some(0.5), Some(0.3), Some(0.2))
            .await
            .unwrap();

        // Should create at least one cluster
        assert!(!clusters.is_empty());
    }

    #[tokio::test]
    async fn test_get_document_stats() {
        let (_temp, vault_path) = create_test_vault().await;
        let tools = ClusteringTools::new(vault_path);

        let stats = tools.get_document_stats().await.unwrap();

        // Debug output
        println!("Total documents: {}", stats.total_documents);
        println!("Total links: {}", stats.total_links);
        println!("Total tags: {}", stats.total_tags);
        println!("Unique tags: {}", stats.unique_tags);

        assert_eq!(stats.total_documents, 5);
        // Updated link count - the test data has more links than expected
        assert_eq!(stats.total_links, 7);
        assert_eq!(stats.total_tags, 5);
        assert_eq!(stats.unique_tags, 4);
        assert!(stats.average_links_per_doc > 0.0);
        assert!(stats.average_tags_per_doc > 0.0);
    }
}
