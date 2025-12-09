//! Heuristic clustering algorithm implementation

use crate::clustering::*;
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

/// Heuristic clustering based on link structure and metadata
#[derive(Debug)]
pub struct HeuristicClusteringAlgorithm {
    metadata: AlgorithmMetadata,
}

impl HeuristicClusteringAlgorithm {
    pub fn new() -> Self {
        let mut default_params = HashMap::new();
        default_params.insert("link_weight".to_string(), serde_json::json!(0.6));
        default_params.insert("tag_weight".to_string(), serde_json::json!(0.3));
        default_params.insert("title_weight".to_string(), serde_json::json!(0.1));
        default_params.insert("min_similarity".to_string(), serde_json::json!(0.2));

        Self {
            metadata: AlgorithmMetadata {
                id: "heuristic".to_string(),
                name: "Heuristic Clustering".to_string(),
                algorithm_type: AlgorithmType::Heuristic,
                description: "Clusters documents based on link structure, tags, and title similarities".to_string(),
                requires_embeddings: false,
                supports_async: true,
                embedding_dimensions: None,
                default_parameters: default_params,
                parameter_schema: None,
            },
        }
    }

    fn calculate_similarity(&self, doc1: &DocumentInfo, doc2: &DocumentInfo, params: &AlgorithmParameters) -> f64 {
        let link_weight: f64 = params.get_or("link_weight", 0.6);
        let tag_weight: f64 = params.get_or("tag_weight", 0.3);
        let title_weight: f64 = params.get_or("title_weight", 0.1);

        // Link similarity
        let link_similarity = self.calculate_link_similarity(doc1, doc2);

        // Tag similarity
        let tag_similarity = self.calculate_tag_similarity(doc1, doc2);

        // Title similarity
        let title_similarity = self.calculate_title_similarity(doc1, doc2);

        link_similarity * link_weight + tag_similarity * tag_weight + title_similarity * title_weight
    }

    fn calculate_link_similarity(&self, doc1: &DocumentInfo, doc2: &DocumentInfo) -> f64 {
        let set1: std::collections::HashSet<_> = doc1.outbound_links.iter().collect();
        let set2: std::collections::HashSet<_> = doc2.outbound_links.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }

    fn calculate_tag_similarity(&self, doc1: &DocumentInfo, doc2: &DocumentInfo) -> f64 {
        let set1: std::collections::HashSet<_> = doc1.tags.iter().collect();
        let set2: std::collections::HashSet<_> = doc2.tags.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }

    fn calculate_title_similarity(&self, doc1: &DocumentInfo, doc2: &DocumentInfo) -> f64 {
        let title1 = doc1.title.as_deref().unwrap_or("");
        let title2 = doc2.title.as_deref().unwrap_or("");

        // Simple word overlap similarity
        let title1_lower = title1.to_lowercase();
        let title2_lower = title2.to_lowercase();

        let words1: std::collections::HashSet<_> = title1_lower.split_whitespace().collect();
        let words2: std::collections::HashSet<_> = title2_lower.split_whitespace().collect();

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }
}

#[async_trait]
impl ClusteringAlgorithm for HeuristicClusteringAlgorithm {
    async fn cluster(
        &self,
        documents: &[DocumentInfo],
        config: &ClusteringConfig,
    ) -> Result<ClusteringResult, ClusteringError> {
        let start_time = std::time::Instant::now();

        // Validate parameters
        self.validate_parameters(&config.parameters.values)?;

        // Calculate similarity matrix
        let n = documents.len();
        let mut similarities = vec![vec![0.0; n]; n];

        for i in 0..n {
            for j in (i + 1)..n {
                let similarity = self.calculate_similarity(&documents[i], &documents[j], &config.parameters);
                similarities[i][j] = similarity;
                similarities[j][i] = similarity;
            }
        }

        // Simple threshold-based clustering
        let min_similarity: f64 = config.parameters.get_or("min_similarity", 0.2);
        let mut clusters = Vec::new();
        let mut assigned = vec![false; n];

        for i in 0..n {
            if assigned[i] {
                continue;
            }

            let mut cluster = vec![i];
            assigned[i] = true;

            // Find all similar documents
            for j in (i + 1)..n {
                if !assigned[j] && similarities[i][j] >= min_similarity {
                    cluster.push(j);
                    assigned[j] = true;
                }
            }

            if cluster.len() >= config.min_cluster_size {
                let document_paths: Vec<String> = cluster
                    .iter()
                    .map(|&idx| documents[idx].file_path.clone())
                    .collect();

                clusters.push(DocumentCluster {
                    id: format!("cluster_{}", clusters.len()),
                    documents: document_paths,
                    centroid: None,
                    confidence: 0.8,
                });
            }
        }

        let execution_time = start_time.elapsed().as_millis() as u64;
        let clusters_count = clusters.len();
        let avg_cluster_size = if clusters.is_empty() {
            0.0
        } else {
            documents.len() as f64 / clusters_count as f64
        };

        Ok(ClusteringResult {
            clusters,
            algorithm_metadata: self.metadata.clone(),
            metrics: ClusteringMetrics {
                execution_time_ms: execution_time,
                documents_processed: documents.len(),
                clusters_generated: clusters_count,
                avg_cluster_size,
                silhouette_score: None,
                custom_metrics: HashMap::new(),
            },
            warnings: vec![],
        })
    }

    fn metadata(&self) -> &AlgorithmMetadata {
        &self.metadata
    }

    fn validate_parameters(&self, parameters: &HashMap<String, serde_json::Value>) -> Result<(), ClusteringError> {
        // Validate parameter types and ranges
        if let Some(min_sim) = parameters.get("min_similarity") {
            let min_sim: f64 = serde_json::from_value(min_sim.clone())
                .map_err(|_| ClusteringError::Config("min_similarity must be a number".to_string()))?;

            if min_sim < 0.0 || min_sim > 1.0 {
                return Err(ClusteringError::Config("min_similarity must be between 0.0 and 1.0".to_string()));
            }
        }

        // Validate weights sum to 1.0
        let link_weight: f64 = parameters
            .get("link_weight")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(0.6);
        let tag_weight: f64 = parameters
            .get("tag_weight")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(0.3);
        let title_weight: f64 = parameters
            .get("title_weight")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(0.1);

        let total = link_weight + tag_weight + title_weight;
        if (total - 1.0).abs() > 0.01 {
            return Err(ClusteringError::Config("Weights must sum to 1.0".to_string()));
        }

        Ok(())
    }
}

/// Factory for creating heuristic clustering algorithm instances
#[derive(Debug)]
pub struct HeuristicAlgorithmFactory;

impl HeuristicAlgorithmFactory {
    pub fn new() -> Self {
        Self
    }
}

impl AlgorithmFactory for HeuristicAlgorithmFactory {
    fn create(&self, _config: &ClusteringConfig) -> Result<Arc<dyn ClusteringAlgorithm>, ClusteringError> {
        Ok(Arc::new(HeuristicClusteringAlgorithm::new()))
    }

    fn metadata(&self) -> &AlgorithmMetadata {
        static METADATA: once_cell::sync::Lazy<AlgorithmMetadata> = once_cell::sync::Lazy::new(|| {
            AlgorithmMetadata {
                id: "heuristic".to_string(),
                name: "Heuristic Clustering".to_string(),
                algorithm_type: AlgorithmType::Heuristic,
                description: "Clusters documents based on link structure, tags, and title similarities".to_string(),
                requires_embeddings: false,
                supports_async: true,
                embedding_dimensions: None,
                default_parameters: {
                    let mut params = HashMap::new();
                    params.insert("link_weight".to_string(), serde_json::json!(0.6));
                    params.insert("tag_weight".to_string(), serde_json::json!(0.3));
                    params.insert("title_weight".to_string(), serde_json::json!(0.1));
                    params.insert("min_similarity".to_string(), serde_json::json!(0.2));
                    params
                },
                parameter_schema: None,
            }
        });
        &METADATA
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_algorithm_creation() {
        let algorithm = HeuristicClusteringAlgorithm::new();
        assert_eq!(algorithm.metadata().id, "heuristic");
        assert_eq!(algorithm.metadata().algorithm_type, AlgorithmType::Heuristic);
        assert!(!algorithm.metadata().requires_embeddings);
    }

    #[test]
    fn test_heuristic_similarity_calculation() {
        let algorithm = HeuristicClusteringAlgorithm::new();

        let doc1 = DocumentInfo {
            file_path: "doc1.md".to_string(),
            title: Some("Introduction to Rust".to_string()),
            tags: vec!["rust".to_string(), "programming".to_string()],
            outbound_links: vec!["doc2.md".to_string(), "doc3.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 1000,
        };

        let doc2 = DocumentInfo {
            file_path: "doc2.md".to_string(),
            title: Some("Advanced Rust".to_string()),
            tags: vec!["rust".to_string(), "advanced".to_string()],
            outbound_links: vec!["doc3.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 1500,
        };

        let params = AlgorithmParameters::new(HashMap::new());
        let similarity = algorithm.calculate_similarity(&doc1, &doc2, &params);
        assert!(similarity > 0.0);
    }

    #[tokio::test]
    async fn test_heuristic_clustering() {
        let algorithm = HeuristicClusteringAlgorithm::new();

        let documents = vec![
            DocumentInfo {
                file_path: "doc1.md".to_string(),
                title: Some("Rust Basics".to_string()),
                tags: vec!["rust".to_string()],
                outbound_links: vec!["doc2.md".to_string()],
                inbound_links: vec![],
                embedding: None,
                content_length: 1000,
            },
            DocumentInfo {
                file_path: "doc2.md".to_string(),
                title: Some("Rust Advanced".to_string()),
                tags: vec!["rust".to_string()],
                outbound_links: vec!["doc1.md".to_string()],
                inbound_links: vec![],
                embedding: None,
                content_length: 1000,
            },
            DocumentInfo {
                file_path: "doc3.md".to_string(),
                title: Some("Python Basics".to_string()),
                tags: vec!["python".to_string()],
                outbound_links: vec![],
                inbound_links: vec![],
                embedding: None,
                content_length: 1000,
            },
        ];

        let mut params = HashMap::new();
        params.insert("min_similarity".to_string(), serde_json::json!(0.1));
        let config = ClusteringConfig {
            algorithm: "heuristic".to_string(),
            parameters: AlgorithmParameters::new(params),
            min_cluster_size: 2,
            max_clusters: None,
            detect_mocs: false,
            moc_config: None,
            embedding_config: None,
            performance: PerformanceConfig::default(),
        };

        let result = algorithm.cluster(&documents, &config).await.unwrap();
        assert_eq!(result.clusters.len(), 1);
        assert_eq!(result.clusters[0].documents.len(), 2);
    }

    #[test]
    fn test_heuristic_factory() {
        let factory = HeuristicAlgorithmFactory::new();
        let metadata = factory.metadata();
        assert_eq!(metadata.id, "heuristic");

        let config = ClusteringConfig::default();
        let algorithm = factory.create(&config).unwrap();
        assert_eq!(algorithm.metadata().id, "heuristic");
    }
}