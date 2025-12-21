pub mod algorithms;
/// Clustering algorithms and MoC (Map of Content) detection for Crucible knowledge bases
///
/// This module provides:
/// - Heuristic-based MoC detection
/// - Extensible clustering algorithms
/// - Plugin interface for custom algorithms via Rune
/// - Integration with SurrealDB storage layer
pub mod heuristics;
pub mod plugin_api;
pub mod registry;

// Re-export registry items for convenience
pub use registry::{
    AlgorithmFactory, ClusteringRegistry, ClusteringRequirements, QualityPreference,
};

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Represents a detected Map of Content candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MocCandidate {
    /// The document path/file
    pub file_path: String,
    /// MoC score (0.0 - 1.0)
    pub score: f64,
    /// Reasons why this was detected as MoC
    pub reasons: Vec<String>,
    /// Number of outbound links
    pub outbound_links: usize,
    /// Number of inbound links
    pub inbound_links: usize,
}

/// A cluster of related documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCluster {
    /// Unique cluster identifier
    pub id: String,
    /// Documents in this cluster
    pub documents: Vec<String>,
    /// Cluster centroid (for semantic clusters)
    pub centroid: Option<Vec<f32>>,
    /// Cluster confidence score
    pub confidence: f64,
}

/// Algorithm types supported by the clustering system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AlgorithmType {
    /// Heuristic-based algorithms using link structure and metadata
    Heuristic,
    /// Semantic clustering using embeddings and similarity measures
    Semantic,
    /// Graph-based clustering algorithms
    Graph,
    /// Hybrid approaches combining multiple methods
    Hybrid,
    /// Custom algorithms implemented in Rune scripts
    Custom(String),
}

/// Metadata about a clustering algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmMetadata {
    /// Unique identifier for the algorithm
    pub id: String,
    /// Display name
    pub name: String,
    /// Algorithm type
    pub algorithm_type: AlgorithmType,
    /// Description of what the algorithm does
    pub description: String,
    /// Whether the algorithm requires embeddings
    pub requires_embeddings: bool,
    /// Whether the algorithm supports async execution
    pub supports_async: bool,
    /// Expected embedding dimensions (if required)
    pub embedding_dimensions: Option<usize>,
    /// Default parameters
    pub default_parameters: HashMap<String, serde_json::Value>,
    /// Parameter schema for validation
    pub parameter_schema: Option<serde_json::Value>,
}

/// Results from a clustering operation with additional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringResult {
    /// The generated clusters
    pub clusters: Vec<DocumentCluster>,
    /// Algorithm metadata used
    pub algorithm_metadata: AlgorithmMetadata,
    /// Execution metrics
    pub metrics: ClusteringMetrics,
    /// Any warnings or diagnostic information
    pub warnings: Vec<String>,
}

/// Performance and quality metrics for clustering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringMetrics {
    /// Time taken to execute (milliseconds)
    pub execution_time_ms: u64,
    /// Number of documents processed
    pub documents_processed: usize,
    /// Number of clusters generated
    pub clusters_generated: usize,
    /// Average cluster size
    pub avg_cluster_size: f64,
    /// Silhouette score if available (semantic clustering)
    pub silhouette_score: Option<f64>,
    /// Additional algorithm-specific metrics
    pub custom_metrics: HashMap<String, f64>,
}

/// Algorithm-specific parameters with validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmParameters {
    /// Raw parameter values
    pub values: HashMap<String, serde_json::Value>,
    /// Parameter validation schema (optional)
    pub schema: Option<serde_json::Value>,
}

impl AlgorithmParameters {
    /// Create new parameters
    pub fn new(values: HashMap<String, serde_json::Value>) -> Self {
        Self {
            values,
            schema: None,
        }
    }

    /// Get a parameter value
    pub fn get<T>(&self, key: &str) -> Result<T, ClusteringError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let value = self
            .values
            .get(key)
            .ok_or_else(|| ClusteringError::Config(format!("Missing parameter: {}", key)))?;
        T::deserialize(value)
            .map_err(|_| ClusteringError::Config(format!("Invalid parameter type for: {}", key)))
    }

    /// Get a parameter with default
    pub fn get_or<T>(&self, key: &str, default: T) -> T
    where
        T: for<'de> Deserialize<'de> + Clone,
    {
        self.values
            .get(key)
            .and_then(|v| T::deserialize(v).ok())
            .unwrap_or(default)
    }

    /// Validate parameters against schema
    pub fn validate(&self) -> Result<(), ClusteringError> {
        if let Some(_schema) = &self.schema {
            // Use jsonschema for validation if available
            // For now, just check that all required parameters are present
            // Implementation would go here
        }
        Ok(())
    }
}

/// Configuration for clustering algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringConfig {
    /// Algorithm to use (can be auto-selected with "auto")
    pub algorithm: String,
    /// Algorithm-specific parameters
    pub parameters: AlgorithmParameters,
    /// Minimum cluster size
    pub min_cluster_size: usize,
    /// Maximum number of clusters
    pub max_clusters: Option<usize>,
    /// Whether to detect MoCs separately
    pub detect_mocs: bool,
    /// MoC detection configuration
    pub moc_config: Option<MocDetectionConfig>,
    /// Embedding provider configuration
    pub embedding_config: Option<EmbeddingConfig>,
    /// Performance settings
    pub performance: PerformanceConfig,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            algorithm: "auto".to_string(),
            parameters: AlgorithmParameters::new(HashMap::new()),
            min_cluster_size: 2,
            max_clusters: None,
            detect_mocs: true,
            moc_config: None,
            embedding_config: None,
            performance: PerformanceConfig {
                parallel: true,
                max_threads: None,
                timeout_seconds: Some(30),
            },
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            parallel: true,
            max_threads: None,
            timeout_seconds: Some(30),
        }
    }
}

/// Configuration for MoC detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MocDetectionConfig {
    /// Minimum outbound links to consider as MoC
    pub min_outbound_links: usize,
    /// MoC score threshold
    pub score_threshold: f64,
    /// Tags that indicate MoC
    pub moc_tags: Vec<String>,
    /// Title patterns for MoC detection
    pub title_patterns: Vec<String>,
}

impl Default for MocDetectionConfig {
    fn default() -> Self {
        Self {
            min_outbound_links: 5,
            score_threshold: 0.5,
            moc_tags: vec![
                "moc".to_string(),
                "map-of-content".to_string(),
                "index".to_string(),
            ],
            title_patterns: vec![
                "Index".to_string(),
                "Table of Contents".to_string(),
                "Contents".to_string(),
            ],
        }
    }
}

/// Configuration for embedding generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider to use
    pub provider: String,
    /// Model name
    pub model: String,
    /// Batch size for embedding generation
    pub batch_size: usize,
    /// Whether to cache embeddings
    pub cache_embeddings: bool,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Whether to run in parallel when possible
    pub parallel: bool,
    /// Maximum number of threads to use
    pub max_threads: Option<usize>,
    /// Timeout for clustering (seconds)
    pub timeout_seconds: Option<u64>,
}

/// Extended trait for clustering algorithms
#[async_trait]
pub trait ClusteringAlgorithm: Send + Sync + std::fmt::Debug {
    /// Execute the clustering algorithm
    async fn cluster(
        &self,
        documents: &[DocumentInfo],
        config: &ClusteringConfig,
    ) -> Result<ClusteringResult, ClusteringError>;

    /// Get metadata about this algorithm
    fn metadata(&self) -> &AlgorithmMetadata;

    /// Validate algorithm-specific parameters
    fn validate_parameters(
        &self,
        parameters: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ClusteringError>;

    /// Check if the algorithm can process the given documents
    fn can_process(&self, documents: &[DocumentInfo]) -> Result<bool, ClusteringError> {
        // Default implementation checks embedding requirements
        if self.metadata().requires_embeddings {
            let has_embeddings = documents.iter().any(|d| d.embedding.is_some());
            Ok(has_embeddings)
        } else {
            Ok(true)
        }
    }

    /// Get algorithm-specific suggestions for improving clustering
    async fn suggest_improvements(
        &self,
        _documents: &[DocumentInfo],
        _result: &ClusteringResult,
    ) -> Result<Vec<String>, ClusteringError> {
        // Default implementation returns empty suggestions
        Ok(vec![])
    }

    /// Initialize the algorithm with optional resources
    async fn initialize(&mut self) -> Result<(), ClusteringError> {
        // Default implementation does nothing
        Ok(())
    }

    /// Cleanup resources
    async fn cleanup(&self) -> Result<(), ClusteringError> {
        // Default implementation does nothing
        Ok(())
    }
}

/// Simple clustering service for demonstration
pub struct SimpleClusteringService {
    registry: Arc<ClusteringRegistry>,
}

impl SimpleClusteringService {
    pub fn new() -> Self {
        let registry = Arc::new(ClusteringRegistry::new());

        // Register built-in algorithms
        registry.register(HeuristicAlgorithmFactory::new()).unwrap();
        registry.register(KMeansAlgorithmFactory::new()).unwrap();

        Self { registry }
    }

    /// Get the algorithm registry
    pub fn registry(&self) -> &Arc<ClusteringRegistry> {
        &self.registry
    }

    /// List available algorithms
    pub fn list_algorithms(&self) -> Vec<AlgorithmMetadata> {
        self.registry.list_algorithms()
    }

    /// Auto-select an algorithm for the given documents
    pub fn auto_select_algorithm(
        &self,
        documents: &[DocumentInfo],
    ) -> Result<String, ClusteringError> {
        let requirements = ClusteringRequirements::default();
        self.registry.auto_select(documents, &requirements)
    }

    /// Cluster documents using auto-selected algorithm
    pub async fn cluster_documents(
        &self,
        documents: Vec<DocumentInfo>,
        mut config: ClusteringConfig,
    ) -> Result<ClusteringResult, ClusteringError> {
        // Auto-select algorithm if needed
        if config.algorithm == "auto" {
            config.algorithm = self.auto_select_algorithm(&documents)?;
        }

        // Get algorithm instance
        let algorithm = self.registry.get_algorithm(&config.algorithm, &config)?;

        // Execute clustering
        algorithm.cluster(&documents, &config).await
    }
}

impl Default for SimpleClusteringService {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export algorithm factories for convenience
pub use algorithms::{
    HeuristicAlgorithmFactory, HeuristicClusteringAlgorithm, KMeansAlgorithmFactory,
    KMeansClusteringAlgorithm,
};

/// Information about a document for clustering
#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub file_path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub outbound_links: Vec<String>,
    pub inbound_links: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub content_length: usize,
}

/// Errors that can occur during clustering
#[derive(Debug, thiserror::Error)]
pub enum ClusteringError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Algorithm error: {0}")]
    Algorithm(String),

    #[error("Database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("Invalid embedding dimensions: expected {expected}, got {actual}")]
    InvalidEmbeddingDimensions { expected: usize, actual: usize },
}

/// Detect potential Maps of Content using heuristic rules
pub async fn detect_mocs(documents: &[DocumentInfo]) -> Result<Vec<MocCandidate>, ClusteringError> {
    heuristics::detect_mocs(documents).await
}

/// Cluster documents using the specified algorithm
pub async fn cluster_documents(
    _documents: &[DocumentInfo],
    _config: ClusteringConfig,
) -> Result<Vec<DocumentCluster>, ClusteringError> {
    // This will be implemented to dispatch to the appropriate algorithm
    // For now, return empty clusters
    Ok(vec![])
}
