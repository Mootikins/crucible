//! Algorithm registry and factory pattern for extensible clustering

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::*;

/// Factory for creating clustering algorithm instances
pub trait AlgorithmFactory: Send + Sync {
    /// Create a new instance of the algorithm
    fn create(&self, config: &ClusteringConfig) -> Result<Arc<dyn ClusteringAlgorithm>, ClusteringError>;

    /// Get metadata without creating an instance
    fn metadata(&self) -> &AlgorithmMetadata;
}

/// Registry for available clustering algorithms
#[derive(Default)]
pub struct ClusteringRegistry {
    factories: RwLock<HashMap<String, Box<dyn AlgorithmFactory>>>,
    default_algorithms: RwLock<HashMap<AlgorithmType, String>>,
}

impl std::fmt::Debug for ClusteringRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusteringRegistry")
            .field("factories", &"<algorithms>")
            .field("default_algorithms", &self.default_algorithms)
            .finish()
    }
}

impl ClusteringRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new algorithm
    pub fn register<F>(&self, factory: F) -> Result<(), ClusteringError>
    where
        F: AlgorithmFactory + 'static,
    {
        let metadata = factory.metadata();
        let mut factories = self.factories.write().unwrap();
        factories.insert(metadata.id.clone(), Box::new(factory));
        Ok(())
    }

    /// Get an algorithm by ID
    pub fn get_algorithm(
        &self,
        algorithm_id: &str,
        config: &ClusteringConfig,
    ) -> Result<Arc<dyn ClusteringAlgorithm>, ClusteringError> {
        let factories = self.factories.read().unwrap();
        let factory = factories
            .get(algorithm_id)
            .ok_or_else(|| ClusteringError::Algorithm(format!("Algorithm '{}' not found", algorithm_id)))?;
        factory.create(config)
    }

    /// List all available algorithms
    pub fn list_algorithms(&self) -> Vec<AlgorithmMetadata> {
        let factories = self.factories.read().unwrap();
        factories
            .values()
            .map(|f| f.metadata().clone())
            .collect()
    }

    /// Get algorithms by type
    pub fn get_algorithms_by_type(&self, algorithm_type: AlgorithmType) -> Vec<AlgorithmMetadata> {
        self.list_algorithms()
            .into_iter()
            .filter(|m| m.algorithm_type == algorithm_type)
            .collect()
    }

    /// Set default algorithm for a type
    pub fn set_default(&self, algorithm_type: AlgorithmType, algorithm_id: String) {
        let mut defaults = self.default_algorithms.write().unwrap();
        defaults.insert(algorithm_type, algorithm_id);
    }

    /// Get default algorithm for a type
    pub fn get_default(&self, algorithm_type: AlgorithmType) -> Option<String> {
        let defaults = self.default_algorithms.read().unwrap();
        defaults.get(&algorithm_type).cloned()
    }

    /// Auto-select best algorithm based on documents and requirements
    pub fn auto_select(
        &self,
        documents: &[DocumentInfo],
        requirements: &ClusteringRequirements,
    ) -> Result<String, ClusteringError> {
        let algorithms = self.list_algorithms();

        // Filter by requirements
        let mut candidates: Vec<_> = algorithms
            .into_iter()
            .filter(|alg| {
                // Check if algorithm can process the documents
                let has_embeddings = documents.iter().any(|d| d.embedding.is_some());

                if alg.requires_embeddings && !has_embeddings {
                    return false;
                }

                // Check type preference
                if let Some(pref_type) = &requirements.preferred_type {
                    if alg.algorithm_type != *pref_type {
                        return false;
                    }
                }

                // Check size constraints
                if let Some(max_size) = requirements.max_dataset_size {
                    if documents.len() > max_size {
                        // Only allow algorithms that can handle large datasets
                        matches!(alg.algorithm_type, AlgorithmType::Semantic | AlgorithmType::Graph)
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        // Sort by suitability score
        candidates.sort_by(|a, b| {
            let score_a = self.calculate_suitability_score(a, documents, requirements);
            let score_b = self.calculate_suitability_score(b, documents, requirements);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
            .into_iter()
            .next()
            .map(|alg| alg.id)
            .ok_or_else(|| ClusteringError::Algorithm("No suitable algorithm found".to_string()))
    }

    fn calculate_suitability_score(
        &self,
        metadata: &AlgorithmMetadata,
        documents: &[DocumentInfo],
        requirements: &ClusteringRequirements,
    ) -> f64 {
        let mut score = 0.0;

        // Base score for algorithm type
        match metadata.algorithm_type {
            AlgorithmType::Semantic if documents.iter().any(|d| d.embedding.is_some()) => score += 3.0,
            AlgorithmType::Heuristic if documents.len() < 200 => score += 2.0,
            AlgorithmType::Graph if documents.len() > 50 => score += 2.5,
            AlgorithmType::Hybrid => score += 2.0,
            _ => score += 1.0,
        }

        // Quality preference
        match requirements.quality_preference {
            QualityPreference::Speed => {
                if metadata.algorithm_type == AlgorithmType::Heuristic {
                    score += 1.5;
                }
            }
            QualityPreference::Accuracy => {
                if matches!(metadata.algorithm_type, AlgorithmType::Semantic | AlgorithmType::Hybrid) {
                    score += 1.5;
                }
            }
            QualityPreference::Balanced => score += 1.0,
        }

        score
    }
}

/// Requirements for algorithm selection
#[derive(Debug, Clone)]
pub struct ClusteringRequirements {
    /// Preferred algorithm type
    pub preferred_type: Option<AlgorithmType>,
    /// Maximum dataset size the algorithm should handle
    pub max_dataset_size: Option<usize>,
    /// Quality vs speed preference
    pub quality_preference: QualityPreference,
    /// Whether to prioritize explainability
    pub prioritize_explainability: bool,
}

#[derive(Debug, Clone)]
pub enum QualityPreference {
    Speed,
    Accuracy,
    Balanced,
}

impl Default for ClusteringRequirements {
    fn default() -> Self {
        Self {
            preferred_type: None,
            max_dataset_size: Some(500),
            quality_preference: QualityPreference::Balanced,
            prioritize_explainability: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ClusteringRegistry::new();
        assert_eq!(registry.list_algorithms().len(), 0);
    }

    #[test]
    fn test_empty_registry_lookup() {
        let registry = ClusteringRegistry::new();
        let result = registry.get_algorithm("nonexistent", &ClusteringConfig::default());
        assert!(matches!(result, Err(ClusteringError::Algorithm(_))));
    }

    #[test]
    fn test_requirements_default() {
        let req = ClusteringRequirements::default();
        assert_eq!(req.max_dataset_size, Some(500));
        assert!(matches!(req.quality_preference, QualityPreference::Balanced));
    }
}