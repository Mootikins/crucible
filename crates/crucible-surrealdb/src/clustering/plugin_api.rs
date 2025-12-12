/// Plugin API for clustering algorithms via Rune
///
/// This module provides the interface between Rust clustering functions
/// and Rune scripts, allowing users to write custom algorithms in Rune.
use super::{ClusteringAlgorithm, DocumentCluster, DocumentInfo};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Registry for clustering algorithms
#[derive(Default)]
pub struct AlgorithmRegistry {
    algorithms: RwLock<HashMap<String, Arc<dyn ClusteringAlgorithm>>>,
}

impl std::fmt::Debug for AlgorithmRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let algorithms = self.algorithms.read().unwrap();
        let names: Vec<&String> = algorithms.keys().collect();
        f.debug_struct("AlgorithmRegistry")
            .field("algorithms", &names)
            .finish()
    }
}

impl AlgorithmRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Default::default()
    }

    /// Register a clustering algorithm
    pub fn register(&self, name: String, algorithm: Arc<dyn ClusteringAlgorithm>) {
        let mut algorithms = self.algorithms.write().unwrap();
        algorithms.insert(name, algorithm);
    }

    /// Get an algorithm by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn ClusteringAlgorithm>> {
        let algorithms = self.algorithms.read().unwrap();
        algorithms.get(name).cloned()
    }

    /// List all registered algorithms
    pub fn list(&self) -> Vec<String> {
        let algorithms = self.algorithms.read().unwrap();
        algorithms.keys().cloned().collect()
    }
}

/// Results from a Rune clustering algorithm
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuneClusteringResult {
    /// List of clusters with document indices
    pub clusters: Vec<Vec<usize>>,
    /// Optional metadata for each cluster
    pub cluster_metadata: Vec<HashMap<String, serde_json::Value>>,
}

/// Convert document information to a format suitable for Rune
pub fn documents_to_rune_format(docs: &[DocumentInfo]) -> serde_json::Value {
    let mut doc_list = Vec::new();

    for (i, doc) in docs.iter().enumerate() {
        let mut doc_map = serde_json::Map::new();
        doc_map.insert("index".to_string(), serde_json::Value::Number(i.into()));
        doc_map.insert(
            "path".to_string(),
            serde_json::Value::String(doc.file_path.clone()),
        );

        if let Some(title) = &doc.title {
            doc_map.insert(
                "title".to_string(),
                serde_json::Value::String(title.clone()),
            );
        }

        doc_map.insert(
            "tags".to_string(),
            serde_json::Value::Array(
                doc.tags
                    .iter()
                    .map(|t| serde_json::Value::String(t.clone()))
                    .collect(),
            ),
        );

        doc_map.insert(
            "outbound_links".to_string(),
            serde_json::Value::Number(doc.outbound_links.len().into()),
        );

        doc_map.insert(
            "inbound_links".to_string(),
            serde_json::Value::Number(doc.inbound_links.len().into()),
        );

        doc_map.insert(
            "content_length".to_string(),
            serde_json::Value::Number(doc.content_length.into()),
        );

        doc_list.push(serde_json::Value::Object(doc_map));
    }

    serde_json::Value::Array(doc_list)
}

/// Convert Rune clustering result back to DocumentCluster format
pub fn rune_result_to_clusters(
    result: RuneClusteringResult,
    docs: &[DocumentInfo],
) -> Vec<DocumentCluster> {
    result
        .clusters
        .into_iter()
        .enumerate()
        .map(|(i, cluster_indices)| {
            let documents = cluster_indices
                .into_iter()
                .map(|idx| docs[idx].file_path.clone())
                .collect();

            DocumentCluster {
                id: format!("cluster_{}", i),
                documents,
                centroid: None,
                confidence: 0.8, // Default confidence
            }
        })
        .collect()
}
