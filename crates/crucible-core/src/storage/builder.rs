//! Content-Addressed Storage Builder
//!
//! This module provides a fluent builder pattern for constructing content-addressed storage
//! instances following dependency inversion principles. The builder enables easy configuration
//! of storage backends, hashing algorithms, and processing options while maintaining type safety
//! and comprehensive validation.
//!
//! ## Architecture
//!
//! The builder follows SOLID principles:
//! - **Single Responsibility**: Each configuration option has a single purpose
//! - **Open/Closed**: Easy to extend with new backends and hashers
//! - **Liskov Substitution**: All implementations can be substituted via traits
//! - **Interface Segregation**: Small, focused trait interfaces
//! - **Dependency Inversion**: Depends on abstractions, not concretions
//!
//! ## Usage
//!
//! // TODO: Add example once API stabilizes

use crate::hashing::blake3::Blake3Hasher;
use crate::storage::change_application::ApplicationConfig;
use crate::storage::diff::DiffConfig;
use crate::storage::memory::MemoryStorage;
use crate::storage::{
    BlockSize, ContentAddressedStorage, ContentHasher, StorageError, StorageResult,
};
use std::sync::Arc;

/// Configuration options for storage backends
#[derive(Clone)]
pub enum StorageBackendType {
    /// In-memory storage for testing and temporary data
    InMemory,
    /// File-based storage with configurable directory
    FileBased {
        directory: String,
        create_if_missing: bool,
    },
    /// SurrealDB database storage
    SurrealDB {
        connection_string: String,
        namespace: String,
        database: String,
    },
    /// Custom backend implementation
    Custom(Arc<dyn ContentAddressedStorage>),
}

impl std::fmt::Debug for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InMemory => write!(f, "InMemory"),
            Self::FileBased {
                directory,
                create_if_missing,
            } => {
                write!(
                    f,
                    "FileBased {{ directory: {}, create_if_missing: {} }}",
                    directory, create_if_missing
                )
            }
            Self::SurrealDB {
                connection_string,
                namespace,
                database,
            } => {
                write!(
                    f,
                    "SurrealDB {{ connection_string: {}, namespace: {}, database: {} }}",
                    connection_string, namespace, database
                )
            }
            Self::Custom(_) => write!(f, "Custom(<dyn ContentAddressedStorage>)"),
        }
    }
}

/// Configuration for hasher algorithms
#[derive(Clone)]
pub enum HasherConfig {
    /// Use BLAKE3 hasher (recommended)
    Blake3(Blake3Hasher),
    /// Custom hasher implementation
    Custom(Arc<dyn ContentHasher>),
}

impl std::fmt::Debug for HasherConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blake3(_) => write!(f, "Blake3"),
            Self::Custom(_) => write!(f, "Custom(<dyn ContentHasher>)"),
        }
    }
}

/// Configuration for storage processing options
#[derive(Debug, Clone)]
pub struct ProcessingConfig {
    /// Block size for content processing
    pub block_size: BlockSize,
    /// Enable automatic deduplication
    pub enable_deduplication: bool,
    /// Enable compression (if supported by backend)
    pub enable_compression: bool,
    /// Maximum cache size for frequently accessed blocks
    pub cache_size: Option<usize>,
    /// Enable background maintenance tasks
    pub enable_maintenance: bool,
    /// Enhanced change detection configuration
    pub change_detection: Option<DiffConfig>,
    /// Change application configuration
    pub change_application: Option<ApplicationConfig>,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            block_size: BlockSize::Medium,
            enable_deduplication: true,
            enable_compression: false,
            cache_size: Some(1000),
            enable_maintenance: true,
            change_detection: Some(DiffConfig::default()),
            change_application: Some(ApplicationConfig::default()),
        }
    }
}

/// Builder for creating configured content-addressed storage instances
///
/// This builder follows the dependency inversion principle by depending on trait
/// abstractions rather than concrete implementations. It provides a fluent API
/// for configuring all aspects of the storage system.
#[derive(Debug, Clone)]
pub struct ContentAddressedStorageBuilder {
    /// Storage backend configuration
    backend_config: Option<StorageBackendType>,
    /// Hasher configuration
    hasher_config: Option<HasherConfig>,
    /// Processing configuration
    processing_config: ProcessingConfig,
    /// Whether to validate configuration before building
    validate_config: bool,
}

impl ContentAddressedStorageBuilder {
    /// Create a new storage builder with default configuration
    ///
    /// # Returns
    /// A new builder instance
    pub fn new() -> Self {
        Self {
            backend_config: None,
            hasher_config: None,
            processing_config: ProcessingConfig::default(),
            validate_config: true,
        }
    }

    /// Set the storage backend type
    ///
    /// # Arguments
    /// * `backend` - The storage backend configuration
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_backend(mut self, backend: StorageBackendType) -> Self {
        self.backend_config = Some(backend);
        self
    }

    /// Set the hasher configuration
    ///
    /// # Arguments
    /// * `hasher` - The hasher configuration
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_hasher(mut self, hasher: HasherConfig) -> Self {
        self.hasher_config = Some(hasher);
        self
    }

    /// Set the block size for content processing
    ///
    /// # Arguments
    /// * `block_size` - The block size configuration
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_block_size(mut self, block_size: BlockSize) -> Self {
        self.processing_config.block_size = block_size;
        self
    }

    /// Enable or disable automatic deduplication
    ///
    /// # Arguments
    /// * `enable` - Whether to enable deduplication
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_deduplication(mut self, enable: bool) -> Self {
        self.processing_config.enable_deduplication = enable;
        self
    }

    /// Enable or disable compression (if supported by backend)
    ///
    /// # Arguments
    /// * `enable` - Whether to enable compression
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_compression(mut self, enable: bool) -> Self {
        self.processing_config.enable_compression = enable;
        self
    }

    /// Set the maximum cache size for frequently accessed blocks
    ///
    /// # Arguments
    /// * `size` - Maximum number of blocks to cache, None for no limit
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_cache_size(mut self, size: Option<usize>) -> Self {
        self.processing_config.cache_size = size;
        self
    }

    /// Enable or disable background maintenance tasks
    ///
    /// # Arguments
    /// * `enable` - Whether to enable maintenance tasks
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_maintenance(mut self, enable: bool) -> Self {
        self.processing_config.enable_maintenance = enable;
        self
    }

    /// Disable configuration validation (useful for testing)
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn without_validation(mut self) -> Self {
        self.validate_config = false;
        self
    }

    /// Set the enhanced change detection configuration
    ///
    /// # Arguments
    /// * `config` - Configuration for enhanced change detection
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_change_detection(mut self, config: DiffConfig) -> Self {
        self.processing_config.change_detection = Some(config);
        self
    }

    /// Disable enhanced change detection
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn without_change_detection(mut self) -> Self {
        self.processing_config.change_detection = None;
        self
    }

    /// Set the change application configuration
    ///
    /// # Arguments
    /// * `config` - Configuration for change application system
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_change_application(mut self, config: ApplicationConfig) -> Self {
        self.processing_config.change_application = Some(config);
        self
    }

    /// Disable change application system
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn without_change_application(mut self) -> Self {
        self.processing_config.change_application = None;
        self
    }

    /// Enable similarity detection for change detection
    ///
    /// # Arguments
    /// * `threshold` - Similarity threshold (0.0 to 1.0)
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_similarity_detection(mut self, threshold: f32) -> Self {
        let config = self
            .processing_config
            .change_detection
            .unwrap_or_default()
            .with_similarity_threshold(threshold);
        self.processing_config.change_detection = Some(config);
        self
    }

    /// Enable parallel processing for large trees
    ///
    /// # Arguments
    /// * `threshold` - Minimum block count for parallel processing
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_parallel_processing(mut self, threshold: usize) -> Self {
        let config = self
            .processing_config
            .change_detection
            .unwrap_or_default()
            .with_parallel_processing(threshold);
        self.processing_config.change_detection = Some(config);
        self
    }

    /// Enable rollback support for change application
    ///
    /// # Arguments
    /// * `enable` - Whether to enable rollback support
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_rollback_support(mut self, enable: bool) -> Self {
        let config = self
            .processing_config
            .change_application
            .unwrap_or_default()
            .with_rollback_support(enable);
        self.processing_config.change_application = Some(config);
        self
    }

    /// Enable strict validation for change application
    ///
    /// # Arguments
    /// * `enable` - Whether to enable strict validation
    ///
    /// # Returns
    /// The builder for method chaining
    pub fn with_strict_validation(mut self, enable: bool) -> Self {
        let config = self
            .processing_config
            .change_application
            .unwrap_or_default()
            .with_strict_validation(enable);
        self.processing_config.change_application = Some(config);
        self
    }

    /// Build the configured storage instance (non-async)
    ///
    /// # Returns
    /// A configured storage instance or error if configuration/build fails
    ///
    /// Note: For async backends like SurrealDB, use `build_async()` instead
    pub fn build(mut self) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        // Validate configuration if enabled
        if self.validate_config {
            self.validate()?;
        }

        // Extract backend and hasher configs
        let backend_config = self.backend_config.take().ok_or_else(|| {
            StorageError::Configuration("Backend configuration not set".to_string())
        })?;
        let hasher_config = self.hasher_config.take().ok_or_else(|| {
            StorageError::Configuration("Hasher configuration not set".to_string())
        })?;
        let processing_config = std::mem::take(&mut self.processing_config);

        // Create storage instance
        self.create_storage_instance_from_configs(backend_config, hasher_config, processing_config)
    }

    /// Validate the current configuration
    ///
    /// # Returns
    /// Ok(()) if valid, Err with details if invalid
    fn validate(&self) -> StorageResult<()> {
        // If validation is disabled, always succeed
        if !self.validate_config {
            return Ok(());
        }

        // Check backend configuration
        if self.backend_config.is_none() {
            return Err(StorageError::Configuration(
                "Storage backend must be configured".to_string(),
            ));
        }

        // Check hasher configuration
        if self.hasher_config.is_none() {
            return Err(StorageError::Configuration(
                "Hasher must be configured".to_string(),
            ));
        }

        // Validate backend-specific configuration
        if let Some(backend_config) = &self.backend_config {
            match backend_config {
                StorageBackendType::FileBased { directory, .. } => {
                    if directory.is_empty() {
                        return Err(StorageError::Configuration(
                            "File-based storage requires a valid directory".to_string(),
                        ));
                    }
                }
                StorageBackendType::SurrealDB {
                    connection_string,
                    namespace,
                    database,
                } => {
                    if connection_string.is_empty() || namespace.is_empty() || database.is_empty() {
                        return Err(StorageError::Configuration(
                            "SurrealDB storage requires connection_string, namespace, and database"
                                .to_string(),
                        ));
                    }
                }
                _ => {} // Other types don't need additional validation
            }
        }

        // Validate processing configuration
        if let Some(cache_size) = self.processing_config.cache_size {
            if cache_size == 0 {
                return Err(StorageError::Configuration(
                    "Cache size must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Create the complete storage instance from configurations
    ///
    /// # Arguments
    /// * `backend_config` - Backend configuration
    /// * `hasher_config` - Hasher configuration
    /// * `processing_config` - Processing configuration
    ///
    /// # Returns
    /// The configured storage instance
    fn create_storage_instance_from_configs(
        &mut self,
        backend_config: StorageBackendType,
        hasher_config: HasherConfig,
        _processing_config: ProcessingConfig,
    ) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        // Create backend
        let backend = match backend_config {
            StorageBackendType::InMemory => {
                // Use MemoryStorage for in-memory backend
                MemoryStorage::new() as Arc<dyn ContentAddressedStorage>
            }
            StorageBackendType::FileBased {
                directory,
                create_if_missing: _,
            } => {
                return Err(StorageError::Configuration(format!(
                    "FileBased backend not yet implemented (directory: {})",
                    directory
                )));
            }
            StorageBackendType::SurrealDB {
                connection_string,
                namespace,
                database,
            } => {
                return Err(StorageError::Configuration(format!(
                    "SurrealDB backend requires async builder. Create ContentAddressedStorageSurrealDB manually and use StorageBackendType::Custom() ({}:{}/{})",
                    connection_string, namespace, database
                )));
            }
            StorageBackendType::Custom(backend) => backend,
        };

        // Create hasher (unused for now, but available for processing)
        let _hasher = match hasher_config {
            HasherConfig::Blake3(hasher) => Arc::new(hasher),
            HasherConfig::Custom(hasher) => hasher,
        };

        // For now, return the backend directly
        // In a full implementation, this would create a composite storage instance
        // that combines the backend with processing capabilities using the hasher and processing_config
        Ok(backend)
    }
}

impl Default for ContentAddressedStorageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::ContentHasher;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Mock hasher implementation for testing
    #[derive(Debug, Clone)]
    struct MockHasher {
        name: String,
    }

    impl MockHasher {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl ContentHasher for MockHasher {
        fn hash_block(&self, data: &[u8]) -> String {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            format!("{}:{:x}", self.name, hasher.finish())
        }

        fn hash_nodes(&self, left: &str, right: &str) -> String {
            let combined = format!("{}:{}", left, right);
            let mut hasher = DefaultHasher::new();
            combined.hash(&mut hasher);
            format!("{}:{:x}", self.name, hasher.finish())
        }

        fn algorithm_name(&self) -> &'static str {
            "mock"
        }

        fn hash_length(&self) -> usize {
            16
        }
    }

    /// Mock storage backend for testing
    #[derive(Debug)]
    struct MockStorageBackend {
        #[allow(dead_code)]
        name: String,
    }

    impl MockStorageBackend {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::storage::traits::BlockOperations for MockStorageBackend {
        async fn store_block(&self, _hash: &str, _data: &[u8]) -> StorageResult<()> {
            Ok(())
        }

        async fn get_block(&self, _hash: &str) -> StorageResult<Option<Vec<u8>>> {
            Ok(None)
        }

        async fn block_exists(&self, _hash: &str) -> StorageResult<bool> {
            Ok(false)
        }

        async fn delete_block(&self, _hash: &str) -> StorageResult<bool> {
            Ok(false)
        }
    }

    #[async_trait::async_trait]
    impl crate::storage::traits::TreeOperations for MockStorageBackend {
        async fn store_tree(
            &self,
            _root_hash: &str,
            _tree: &crate::storage::MerkleTree,
        ) -> StorageResult<()> {
            Ok(())
        }

        async fn get_tree(
            &self,
            _root_hash: &str,
        ) -> StorageResult<Option<crate::storage::MerkleTree>> {
            Ok(None)
        }

        async fn tree_exists(&self, _root_hash: &str) -> StorageResult<bool> {
            Ok(false)
        }

        async fn delete_tree(&self, _root_hash: &str) -> StorageResult<bool> {
            Ok(false)
        }
    }

    #[async_trait::async_trait]
    impl crate::storage::traits::StorageManagement for MockStorageBackend {
        async fn get_stats(&self) -> StorageResult<crate::storage::traits::StorageStats> {
            Ok(crate::storage::traits::StorageStats {
                backend: crate::storage::traits::StorageBackend::InMemory,
                block_count: 0,
                block_size_bytes: 0,
                tree_count: 0,
                section_count: 0,
                deduplication_savings: 0,
                average_block_size: 0.0,
                largest_block_size: 0,
                evicted_blocks: 0,
                quota_usage: None,
            })
        }

        async fn maintenance(&self) -> StorageResult<()> {
            Ok(())
        }
    }

    impl ContentAddressedStorage for MockStorageBackend {}

    #[test]
    fn test_builder_new() {
        let builder = ContentAddressedStorageBuilder::new();

        assert!(builder.backend_config.is_none());
        assert!(builder.hasher_config.is_none());
        assert!(builder.validate_config);
        assert_eq!(builder.processing_config.block_size, BlockSize::Medium);
        assert!(builder.processing_config.enable_deduplication);
        assert!(!builder.processing_config.enable_compression);
        assert_eq!(builder.processing_config.cache_size, Some(1000));
        assert!(builder.processing_config.enable_maintenance);
    }

    #[test]
    fn test_builder_default() {
        let builder = ContentAddressedStorageBuilder::default();

        // Should be same as new()
        assert!(builder.backend_config.is_none());
        assert!(builder.hasher_config.is_none());
        assert!(builder.validate_config);
    }

    #[test]
    fn test_builder_with_backend() {
        let backend = StorageBackendType::InMemory;
        let builder = ContentAddressedStorageBuilder::new().with_backend(backend);

        assert!(builder.backend_config.is_some());
        matches!(builder.backend_config, Some(StorageBackendType::InMemory));
    }

    #[test]
    fn test_builder_with_hasher() {
        let hasher = HasherConfig::Blake3(Blake3Hasher::new());
        let builder = ContentAddressedStorageBuilder::new().with_hasher(hasher);

        // Check that hasher was set
        assert!(builder.hasher_config.is_some());
    }

    #[test]
    fn test_builder_with_block_size() {
        let builder = ContentAddressedStorageBuilder::new().with_block_size(BlockSize::Large);

        assert_eq!(builder.processing_config.block_size, BlockSize::Large);
    }

    #[test]
    fn test_builder_with_deduplication() {
        let builder = ContentAddressedStorageBuilder::new().with_deduplication(false);

        assert!(!builder.processing_config.enable_deduplication);
    }

    #[test]
    fn test_builder_with_compression() {
        let builder = ContentAddressedStorageBuilder::new().with_compression(true);

        assert!(builder.processing_config.enable_compression);
    }

    #[test]
    fn test_builder_with_cache_size() {
        let builder = ContentAddressedStorageBuilder::new().with_cache_size(Some(500));

        assert_eq!(builder.processing_config.cache_size, Some(500));
    }

    #[test]
    fn test_builder_with_maintenance() {
        let builder = ContentAddressedStorageBuilder::new().with_maintenance(false);

        assert!(!builder.processing_config.enable_maintenance);
    }

    #[test]
    fn test_builder_without_validation() {
        let builder = ContentAddressedStorageBuilder::new().without_validation();

        assert!(!builder.validate_config);
    }

    #[test]
    fn test_builder_fluent_api() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::InMemory)
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
            .with_block_size(BlockSize::Small)
            .with_deduplication(false)
            .with_compression(true)
            .with_cache_size(Some(2000))
            .with_maintenance(false)
            .without_validation();

        assert!(builder.backend_config.is_some());
        assert!(builder.hasher_config.is_some());
        assert_eq!(builder.processing_config.block_size, BlockSize::Small);
        assert!(!builder.processing_config.enable_deduplication);
        assert!(builder.processing_config.enable_compression);
        assert_eq!(builder.processing_config.cache_size, Some(2000));
        assert!(!builder.processing_config.enable_maintenance);
        assert!(!builder.validate_config);
    }

    #[test]
    fn test_validation_missing_backend() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

        let result = builder.validate();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), StorageError::Configuration(msg)
            if msg.contains("backend must be configured"))
        );
    }

    #[test]
    fn test_validation_missing_hasher() {
        let builder =
            ContentAddressedStorageBuilder::new().with_backend(StorageBackendType::InMemory);

        let result = builder.validate();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), StorageError::Configuration(msg)
            if msg.contains("Hasher must be configured"))
        );
    }

    #[test]
    fn test_validation_invalid_file_directory() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::FileBased {
                directory: "".to_string(),
                create_if_missing: true,
            })
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

        let result = builder.validate();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), StorageError::Configuration(msg)
            if msg.contains("valid directory"))
        );
    }

    #[test]
    fn test_validation_invalid_surrealdb_config() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::SurrealDB {
                connection_string: "".to_string(),
                namespace: "test".to_string(),
                database: "test".to_string(),
            })
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

        let result = builder.validate();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), StorageError::Configuration(msg)
            if msg.contains("connection_string, namespace, and database"))
        );
    }

    #[test]
    fn test_validation_invalid_cache_size() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::InMemory)
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
            .with_cache_size(Some(0));

        let result = builder.validate();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), StorageError::Configuration(msg)
            if msg.contains("Cache size must be greater than 0"))
        );
    }

    #[test]
    fn test_validation_success() {
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::InMemory)
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
            .with_cache_size(Some(1000));

        let result = builder.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_skipped_when_disabled() {
        let builder = ContentAddressedStorageBuilder::new().without_validation();

        // Should succeed even without configuration
        let result = builder.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_storage_backend_type_clone() {
        let backend = StorageBackendType::InMemory;
        let cloned = backend.clone();
        // Both should be InMemory
        matches!(&backend, StorageBackendType::InMemory);
        matches!(&cloned, StorageBackendType::InMemory);

        let test_dir = std::env::temp_dir()
            .join("crucible_test_clone")
            .to_string_lossy()
            .into_owned();
        let file_backend = StorageBackendType::FileBased {
            directory: test_dir.clone(),
            create_if_missing: true,
        };
        let cloned_file = file_backend.clone();
        matches!(&file_backend, StorageBackendType::FileBased { directory, .. } if *directory == test_dir);
        matches!(&cloned_file, StorageBackendType::FileBased { directory, .. } if *directory == test_dir);
    }

    #[test]
    fn test_hasher_config_debug() {
        let hasher = HasherConfig::Blake3(Blake3Hasher::new());
        let debug_str = format!("{:?}", hasher);
        assert!(debug_str.contains("Blake3"));
    }

    #[test]
    fn test_processing_config_default() {
        let config = ProcessingConfig::default();

        assert_eq!(config.block_size, BlockSize::Medium);
        assert!(config.enable_deduplication);
        assert!(!config.enable_compression);
        assert_eq!(config.cache_size, Some(1000));
        assert!(config.enable_maintenance);
        assert!(config.change_detection.is_some());
        assert!(config.change_application.is_some());
    }

    #[test]
    fn test_processing_config_clone() {
        let config = ProcessingConfig {
            block_size: BlockSize::Large,
            enable_deduplication: false,
            enable_compression: true,
            cache_size: Some(2000),
            enable_maintenance: false,
            change_detection: None,
            change_application: None,
        };

        let cloned = config.clone();
        assert_eq!(config.block_size, cloned.block_size);
        assert_eq!(config.enable_deduplication, cloned.enable_deduplication);
        assert_eq!(config.enable_compression, cloned.enable_compression);
        assert_eq!(config.cache_size, cloned.cache_size);
        assert_eq!(config.enable_maintenance, cloned.enable_maintenance);
        assert_eq!(config.change_detection, cloned.change_detection);
        assert_eq!(config.change_application, cloned.change_application);
    }

    #[test]
    fn test_build_without_validation_fails() {
        // This should fail because we're providing incomplete configuration,
        // but validation should be skipped (it will fail at build time instead)
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::InMemory)
            // Missing hasher configuration intentionally
            .without_validation();

        let result = builder.build();
        // Should fail due to missing hasher configuration, not validation
        assert!(result.is_err());
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("Hasher configuration not set")
                    || error_msg.contains("must be configured")
            );
        }
    }

    #[test]
    fn test_build_with_validation_fails() {
        let builder = ContentAddressedStorageBuilder::new();

        let result = builder.build();
        // Should fail due to missing configuration
        assert!(result.is_err());
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("must be configured"));
        }
    }

    #[test]
    fn test_mock_hasher() {
        let hasher = MockHasher::new("test");

        assert_eq!(hasher.algorithm_name(), "mock");
        assert_eq!(hasher.hash_length(), 16);

        let hash1 = hasher.hash_block(b"test");
        let hash2 = hasher.hash_block(b"test");
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("test:"));

        let combined = hasher.hash_nodes("hash1", "hash2");
        assert!(combined.starts_with("test:"));
        assert_ne!(combined, hash1);
    }

    #[test]
    fn test_builder_chainable_methods() {
        // Test that all methods return Self for chaining
        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(StorageBackendType::InMemory)
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
            .with_block_size(BlockSize::Small)
            .with_deduplication(false)
            .with_compression(true)
            .with_cache_size(Some(500))
            .with_maintenance(false)
            .without_validation();

        // All configurations should be set
        assert!(builder.backend_config.is_some());
        assert!(builder.hasher_config.is_some());
        assert_eq!(builder.processing_config.block_size, BlockSize::Small);
        assert!(!builder.processing_config.enable_deduplication);
        assert!(builder.processing_config.enable_compression);
        assert_eq!(builder.processing_config.cache_size, Some(500));
        assert!(!builder.processing_config.enable_maintenance);
        assert!(!builder.validate_config);
    }

    // Integration test with actual storage backend (mock)
    #[tokio::test]
    async fn test_builder_with_custom_backend() {
        let mock_backend = Arc::new(MockStorageBackend::new("test"));
        let backend_type = StorageBackendType::Custom(
            Arc::clone(&mock_backend) as Arc<dyn ContentAddressedStorage>
        );

        let builder = ContentAddressedStorageBuilder::new()
            .with_backend(backend_type)
            .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()));

        let result = builder.build();
        // Should succeed with custom backend (though limited implementation)
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_messages_are_descriptive() {
        // Test that error messages provide useful information
        let tests = vec![
            (
                ContentAddressedStorageBuilder::new(),
                "backend must be configured",
            ),
            (
                ContentAddressedStorageBuilder::new().with_backend(StorageBackendType::InMemory),
                "Hasher must be configured",
            ),
            (
                ContentAddressedStorageBuilder::new()
                    .with_backend(StorageBackendType::FileBased {
                        directory: "".to_string(),
                        create_if_missing: true,
                    })
                    .with_hasher(HasherConfig::Blake3(Blake3Hasher::new())),
                "valid directory",
            ),
            (
                ContentAddressedStorageBuilder::new()
                    .with_backend(StorageBackendType::SurrealDB {
                        connection_string: "".to_string(),
                        namespace: "test".to_string(),
                        database: "test".to_string(),
                    })
                    .with_hasher(HasherConfig::Blake3(Blake3Hasher::new())),
                "connection_string, namespace, and database",
            ),
            (
                ContentAddressedStorageBuilder::new()
                    .with_backend(StorageBackendType::InMemory)
                    .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
                    .with_cache_size(Some(0)),
                "Cache size must be greater than 0",
            ),
        ];

        for (builder, expected_message) in tests {
            let result = builder.validate();
            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(
                error_msg.contains(expected_message),
                "Expected error message containing '{}', got: {}",
                expected_message,
                error_msg
            );
        }
    }
}
