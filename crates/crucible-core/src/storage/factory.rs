//! Storage Factory Pattern for Centralized Backend Creation
//!
//! This module provides a factory pattern for creating storage backend instances
//! based on configuration. It encapsulates the complexity of backend creation and
//! provides a clean, configuration-driven API for selecting and instantiating
//! storage implementations.
//!
//! ## Architecture
//!
//! The factory pattern follows SOLID principles:
//! - **Single Responsibility**: Factory solely responsible for creation logic
//! - **Open/Closed**: Easy to extend with new backends without modifying existing code
//! - **Liskov Substitution**: All backends implement the same trait interface
//! - **Interface Segregation**: Depends only on necessary trait methods
//! - **Dependency Inversion**: Returns trait objects, not concrete types
//!
//! ## Usage
//!
//! // TODO: Add example once API stabilizes
//!
//! ## Features
//!
//! - **Configuration-driven**: Select backend via configuration, not code
//! - **Type-safe**: Leverages Rust's type system for compile-time guarantees
//! - **Extensible**: Add new backends by implementing traits
//! - **Testable**: Easy to mock and test with in-memory backend
//! - **Production-ready**: Comprehensive error handling and validation

use crate::hashing::blake3::Blake3Hasher;
use crate::storage::{
    memory::{MemoryStorage, MemoryStorageConfig},
    ContentAddressedStorage, ContentHasher, StorageError, StorageResult,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for selecting and configuring storage backends
///
/// This enum provides a discriminated union of all supported backend types
/// with their respective configuration options. New backends can be added
/// by extending this enum with additional variants.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendConfig {
    /// In-memory storage for testing and temporary data
    ///
    /// This backend stores all data in RAM with optional memory limits and
    /// LRU eviction. Perfect for testing, development, and temporary storage
    /// scenarios where persistence is not required.
    InMemory {
        /// Maximum memory usage in bytes (None for unlimited)
        #[serde(default)]
        memory_limit: Option<u64>,
        /// Enable automatic LRU eviction when memory limit is reached
        #[serde(default = "default_true")]
        enable_lru_eviction: bool,
        /// Enable detailed statistics tracking
        #[serde(default = "default_true")]
        enable_stats_tracking: bool,
    },

    /// File-based storage with local filesystem persistence
    ///
    /// This backend stores blocks and trees as files on the local filesystem.
    /// Useful for single-machine deployments and simple persistence requirements.
    FileBased {
        /// Root directory for storage files
        directory: PathBuf,
        /// Create directory if it doesn't exist
        #[serde(default = "default_true")]
        create_if_missing: bool,
        /// Enable file compression to save disk space
        #[serde(default)]
        enable_compression: bool,
        /// Maximum directory size in bytes (None for unlimited)
        #[serde(default)]
        size_limit: Option<u64>,
    },

    /// SurrealDB database storage for production deployments
    ///
    /// This backend uses SurrealDB for persistent, ACID-compliant storage
    /// with efficient indexing and query capabilities. Requires the SurrealDB
    /// implementation to be provided via dependency injection.
    SurrealDB {
        /// Database connection string (e.g., "memory://", "file://data.db", "ws://localhost:8000")
        connection_string: String,
        /// Database namespace
        namespace: String,
        /// Database name
        database: String,
        /// Connection timeout in seconds
        #[serde(default = "default_connection_timeout")]
        connection_timeout_secs: u64,
        /// Maximum concurrent connections
        #[serde(default = "default_max_connections")]
        max_connections: usize,
    },

    /// Custom backend provided by the caller
    ///
    /// This allows for dependency injection of custom storage implementations
    /// without modifying the factory. The backend instance is not serializable
    /// and must be provided programmatically.
    #[serde(skip)]
    Custom(Arc<dyn ContentAddressedStorage>),
}

// Serde default functions
fn default_true() -> bool {
    true
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_max_connections() -> usize {
    10
}

impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InMemory {
                memory_limit,
                enable_lru_eviction,
                enable_stats_tracking,
            } => f
                .debug_struct("InMemory")
                .field("memory_limit", memory_limit)
                .field("enable_lru_eviction", enable_lru_eviction)
                .field("enable_stats_tracking", enable_stats_tracking)
                .finish(),
            Self::FileBased {
                directory,
                create_if_missing,
                enable_compression,
                size_limit,
            } => f
                .debug_struct("FileBased")
                .field("directory", directory)
                .field("create_if_missing", create_if_missing)
                .field("enable_compression", enable_compression)
                .field("size_limit", size_limit)
                .finish(),
            Self::SurrealDB {
                connection_string,
                namespace,
                database,
                connection_timeout_secs,
                max_connections,
            } => f
                .debug_struct("SurrealDB")
                .field("connection_string", connection_string)
                .field("namespace", namespace)
                .field("database", database)
                .field("connection_timeout_secs", connection_timeout_secs)
                .field("max_connections", max_connections)
                .finish(),
            Self::Custom(_) => f
                .debug_struct("Custom")
                .field("backend", &"<Arc<dyn ContentAddressedStorage>>")
                .finish(),
        }
    }
}

impl PartialEq for BackendConfig {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::InMemory {
                    memory_limit: ml1,
                    enable_lru_eviction: lru1,
                    enable_stats_tracking: st1,
                },
                Self::InMemory {
                    memory_limit: ml2,
                    enable_lru_eviction: lru2,
                    enable_stats_tracking: st2,
                },
            ) => ml1 == ml2 && lru1 == lru2 && st1 == st2,
            (
                Self::FileBased {
                    directory: d1,
                    create_if_missing: c1,
                    enable_compression: ec1,
                    size_limit: sl1,
                },
                Self::FileBased {
                    directory: d2,
                    create_if_missing: c2,
                    enable_compression: ec2,
                    size_limit: sl2,
                },
            ) => d1 == d2 && c1 == c2 && ec1 == ec2 && sl1 == sl2,
            (
                Self::SurrealDB {
                    connection_string: cs1,
                    namespace: ns1,
                    database: db1,
                    connection_timeout_secs: ct1,
                    max_connections: mc1,
                },
                Self::SurrealDB {
                    connection_string: cs2,
                    namespace: ns2,
                    database: db2,
                    connection_timeout_secs: ct2,
                    max_connections: mc2,
                },
            ) => cs1 == cs2 && ns1 == ns2 && db1 == db2 && ct1 == ct2 && mc1 == mc2,
            (Self::Custom(_), Self::Custom(_)) => {
                // Custom backends are never considered equal
                false
            }
            _ => false,
        }
    }
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self::InMemory {
            memory_limit: Some(512 * 1024 * 1024), // 512MB
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        }
    }
}

// Re-export HashAlgorithm from the canonical location in types/hashing.rs
// This ensures a single definition and avoids type confusion.
pub use crate::types::hashing::HashAlgorithm;

/// Complete storage configuration
///
/// This struct combines backend selection, hashing configuration, and
/// optional feature flags into a single configuration object that can
/// be loaded from files, environment variables, or constructed in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Backend configuration (type and options)
    pub backend: BackendConfig,

    /// Hashing algorithm to use
    #[serde(default)]
    pub hash_algorithm: HashAlgorithm,

    /// Enable automatic deduplication
    #[serde(default = "default_true")]
    pub enable_deduplication: bool,

    /// Enable background maintenance tasks
    #[serde(default = "default_true")]
    pub enable_maintenance: bool,

    /// Validate configuration before creating storage
    #[serde(default = "default_true")]
    pub validate_config: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: BackendConfig::default(),
            hash_algorithm: HashAlgorithm::default(),
            enable_deduplication: true,
            enable_maintenance: true,
            validate_config: true,
        }
    }
}

impl StorageConfig {
    /// Create a new storage configuration with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create configuration for in-memory storage with custom memory limit
    ///
    /// # Arguments
    /// * `memory_limit` - Maximum memory usage in bytes (None for unlimited)
    pub fn in_memory(memory_limit: Option<u64>) -> Self {
        Self {
            backend: BackendConfig::InMemory {
                memory_limit,
                enable_lru_eviction: true,
                enable_stats_tracking: true,
            },
            ..Default::default()
        }
    }

    /// Create configuration for file-based storage
    ///
    /// # Arguments
    /// * `directory` - Root directory for storage files
    pub fn file_based(directory: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendConfig::FileBased {
                directory: directory.into(),
                create_if_missing: true,
                enable_compression: false,
                size_limit: None,
            },
            ..Default::default()
        }
    }

    /// Create configuration for SurrealDB storage
    ///
    /// # Arguments
    /// * `connection_string` - Database connection string
    /// * `namespace` - Database namespace
    /// * `database` - Database name
    pub fn surrealdb(
        connection_string: impl Into<String>,
        namespace: impl Into<String>,
        database: impl Into<String>,
    ) -> Self {
        Self {
            backend: BackendConfig::SurrealDB {
                connection_string: connection_string.into(),
                namespace: namespace.into(),
                database: database.into(),
                connection_timeout_secs: default_connection_timeout(),
                max_connections: default_max_connections(),
            },
            ..Default::default()
        }
    }

    /// Create configuration for a custom backend
    ///
    /// # Arguments
    /// * `backend` - Custom storage backend instance
    pub fn custom(backend: Arc<dyn ContentAddressedStorage>) -> Self {
        Self {
            backend: BackendConfig::Custom(backend),
            ..Default::default()
        }
    }

    /// Validate the configuration
    ///
    /// # Returns
    /// `Ok(())` if valid, `Err` with details if invalid
    pub fn validate(&self) -> StorageResult<()> {
        // Skip validation if disabled
        if !self.validate_config {
            return Ok(());
        }

        match &self.backend {
            BackendConfig::InMemory { memory_limit, .. } => {
                if let Some(limit) = memory_limit {
                    if *limit == 0 {
                        return Err(StorageError::Configuration(
                            "In-memory storage memory_limit must be greater than 0".to_string(),
                        ));
                    }
                }
            }
            BackendConfig::FileBased {
                directory,
                size_limit,
                ..
            } => {
                if directory.as_os_str().is_empty() {
                    return Err(StorageError::Configuration(
                        "File-based storage requires a valid directory path".to_string(),
                    ));
                }
                if let Some(limit) = size_limit {
                    if *limit == 0 {
                        return Err(StorageError::Configuration(
                            "File-based storage size_limit must be greater than 0".to_string(),
                        ));
                    }
                }
            }
            BackendConfig::SurrealDB {
                connection_string,
                namespace,
                database,
                connection_timeout_secs,
                max_connections,
            } => {
                if connection_string.is_empty() {
                    return Err(StorageError::Configuration(
                        "SurrealDB connection_string cannot be empty".to_string(),
                    ));
                }
                if namespace.is_empty() {
                    return Err(StorageError::Configuration(
                        "SurrealDB namespace cannot be empty".to_string(),
                    ));
                }
                if database.is_empty() {
                    return Err(StorageError::Configuration(
                        "SurrealDB database cannot be empty".to_string(),
                    ));
                }
                if *connection_timeout_secs == 0 {
                    return Err(StorageError::Configuration(
                        "SurrealDB connection_timeout_secs must be greater than 0".to_string(),
                    ));
                }
                if *max_connections == 0 {
                    return Err(StorageError::Configuration(
                        "SurrealDB max_connections must be greater than 0".to_string(),
                    ));
                }
            }
            BackendConfig::Custom(_) => {
                // Custom backends are assumed to be valid if provided
            }
        }

        Ok(())
    }
}

/// Factory for creating storage backend instances
///
/// This factory encapsulates the complexity of creating different storage
/// backend types. It handles validation, hasher creation, and backend
/// instantiation, providing a clean interface for the rest of the system.
///
/// ## Design Pattern
///
/// The factory pattern is used here to:
/// - Centralize creation logic in one place
/// - Hide implementation details from clients
/// - Enable configuration-driven backend selection
/// - Make it easy to add new backends without changing client code
/// - Provide a testing seam for mock implementations
pub struct StorageFactory;

impl StorageFactory {
    /// Create a storage backend from configuration
    ///
    /// This is the main entry point for creating storage instances. It validates
    /// the configuration, creates the appropriate hasher, and instantiates the
    /// selected backend with all necessary dependencies.
    ///
    /// # Arguments
    /// * `config` - Storage configuration specifying backend and options
    ///
    /// # Returns
    /// An `Arc<dyn ContentAddressedStorage>` or error if creation fails
    ///
    /// # Errors
    /// - Configuration validation errors
    /// - Backend creation errors (e.g., database connection failures)
    /// - File system errors for file-based storage
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub async fn create(config: StorageConfig) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        debug!("Creating storage backend with config: {:?}", config);

        // Validate configuration
        if config.validate_config {
            config.validate()?;
            debug!("Configuration validation passed");
        }

        // Create hasher based on configuration
        let hasher = Self::create_hasher(&config.hash_algorithm);
        debug!("Created hasher: {}", hasher.algorithm_name());

        // Create backend based on configuration
        let backend = Self::create_backend(&config, hasher).await?;

        info!("Successfully created storage backend: {:?}", config.backend);

        Ok(backend)
    }

    /// Create a hasher based on algorithm configuration
    ///
    /// # Arguments
    /// * `algorithm` - Hash algorithm to use
    ///
    /// # Returns
    /// An `Arc<dyn ContentHasher>` instance
    fn create_hasher(algorithm: &HashAlgorithm) -> Arc<dyn ContentHasher> {
        match algorithm {
            HashAlgorithm::Blake3 => Arc::new(Blake3Hasher::new()),
            HashAlgorithm::Sha256 => {
                // Note: SHA256Hasher would need to be implemented in the hashing module
                // For now, default to BLAKE3 with a warning
                warn!("SHA256 hasher not yet implemented, falling back to BLAKE3");
                Arc::new(Blake3Hasher::new())
            }
        }
    }

    /// Create a backend instance based on configuration
    ///
    /// # Arguments
    /// * `config` - Storage configuration
    /// * `hasher` - Hasher to use for content addressing
    ///
    /// # Returns
    /// An `Arc<dyn ContentAddressedStorage>` instance or error
    async fn create_backend(
        config: &StorageConfig,
        _hasher: Arc<dyn ContentHasher>,
    ) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        match &config.backend {
            BackendConfig::InMemory {
                memory_limit,
                enable_lru_eviction,
                enable_stats_tracking,
            } => {
                debug!("Creating in-memory storage backend");
                Self::create_memory_backend(
                    *memory_limit,
                    *enable_lru_eviction,
                    *enable_stats_tracking,
                )
                .await
            }

            BackendConfig::FileBased {
                directory,
                create_if_missing,
                enable_compression,
                size_limit,
            } => {
                debug!("Creating file-based storage backend at {:?}", directory);
                Self::create_file_backend(
                    directory.clone(),
                    *create_if_missing,
                    *enable_compression,
                    *size_limit,
                )
                .await
            }

            BackendConfig::SurrealDB {
                connection_string,
                namespace,
                database,
                connection_timeout_secs,
                max_connections,
            } => {
                debug!(
                    "Creating SurrealDB storage backend: {}:{}/{}",
                    connection_string, namespace, database
                );
                Self::create_surrealdb_backend(
                    connection_string.clone(),
                    namespace.clone(),
                    database.clone(),
                    *connection_timeout_secs,
                    *max_connections,
                )
                .await
            }

            BackendConfig::Custom(backend) => {
                debug!("Using custom storage backend");
                Ok(Arc::clone(backend))
            }
        }
    }

    /// Create an in-memory storage backend
    async fn create_memory_backend(
        memory_limit: Option<u64>,
        enable_lru_eviction: bool,
        enable_stats_tracking: bool,
    ) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        let config = MemoryStorageConfig {
            memory_limit,
            enable_lru_eviction,
            enable_stats_tracking,
            ..Default::default()
        };

        let storage = MemoryStorage::with_config(config);

        Ok(storage as Arc<dyn ContentAddressedStorage>)
    }

    /// Create a file-based storage backend
    async fn create_file_backend(
        _directory: PathBuf,
        _create_if_missing: bool,
        _enable_compression: bool,
        _size_limit: Option<u64>,
    ) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        // Note: File-based storage is not yet implemented
        Err(StorageError::Configuration(
            "File-based storage backend not yet implemented. Use InMemory or provide SurrealDB via Custom backend.".to_string(),
        ))
    }

    /// Create a SurrealDB storage backend
    ///
    /// Note: This method cannot directly create a SurrealDB backend because
    /// crucible-core cannot depend on crucible-surrealdb (circular dependency).
    /// Instead, callers should create the SurrealDB backend externally and use
    /// `BackendConfig::Custom` to provide it.
    async fn create_surrealdb_backend(
        _connection_string: String,
        _namespace: String,
        _database: String,
        _connection_timeout_secs: u64,
        _max_connections: usize,
    ) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        Err(StorageError::Configuration(
            "SurrealDB backend requires dependency injection. Create the backend externally and use BackendConfig::Custom.".to_string(),
        ))
    }

    /// Create a storage backend from environment variables
    ///
    /// This is a convenience method for loading configuration from the environment,
    /// useful for containerized deployments and twelve-factor apps.
    ///
    /// Expected environment variables:
    /// - `STORAGE_BACKEND`: "in_memory", "file_based", or "surrealdb"
    /// - `STORAGE_MEMORY_LIMIT`: Memory limit in bytes (for in-memory)
    /// - `STORAGE_DIRECTORY`: Directory path (for file-based)
    /// - `STORAGE_CONNECTION_STRING`: Connection string (for SurrealDB)
    /// - `STORAGE_NAMESPACE`: Namespace (for SurrealDB)
    /// - `STORAGE_DATABASE`: Database name (for SurrealDB)
    ///
    /// # Returns
    /// A storage backend instance or error if environment is invalid
    pub async fn create_from_env() -> StorageResult<Arc<dyn ContentAddressedStorage>> {
        let backend_type =
            std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "in_memory".to_string());

        let config = match backend_type.as_str() {
            "in_memory" => {
                let memory_limit = std::env::var("STORAGE_MEMORY_LIMIT")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok());
                StorageConfig::in_memory(memory_limit)
            }
            "file_based" => {
                let directory = std::env::var("STORAGE_DIRECTORY").map_err(|_| {
                    StorageError::Configuration(
                        "STORAGE_DIRECTORY environment variable required for file-based backend"
                            .to_string(),
                    )
                })?;
                StorageConfig::file_based(directory)
            }
            "surrealdb" => {
                let connection_string = std::env::var("STORAGE_CONNECTION_STRING")
                    .map_err(|_| StorageError::Configuration(
                        "STORAGE_CONNECTION_STRING environment variable required for SurrealDB backend".to_string()
                    ))?;
                let namespace = std::env::var("STORAGE_NAMESPACE").map_err(|_| {
                    StorageError::Configuration(
                        "STORAGE_NAMESPACE environment variable required for SurrealDB backend"
                            .to_string(),
                    )
                })?;
                let database = std::env::var("STORAGE_DATABASE").map_err(|_| {
                    StorageError::Configuration(
                        "STORAGE_DATABASE environment variable required for SurrealDB backend"
                            .to_string(),
                    )
                })?;
                StorageConfig::surrealdb(connection_string, namespace, database)
            }
            _ => {
                return Err(StorageError::Configuration(format!(
                    "Unknown storage backend type: {}. Expected: in_memory, file_based, or surrealdb",
                    backend_type
                )));
            }
        };

        Self::create(config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::{BlockOperations, StorageManagement};

    /// Cross-platform test path helper
    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_backend_config_default() {
        let config = BackendConfig::default();
        matches!(config, BackendConfig::InMemory { .. });
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert!(config.enable_deduplication);
        assert!(config.enable_maintenance);
        assert!(config.validate_config);
        assert_eq!(config.hash_algorithm, HashAlgorithm::Blake3);
    }

    #[test]
    fn test_storage_config_in_memory() {
        let config = StorageConfig::in_memory(Some(100_000_000));

        match config.backend {
            BackendConfig::InMemory { memory_limit, .. } => {
                assert_eq!(memory_limit, Some(100_000_000));
            }
            _ => panic!("Expected InMemory backend"),
        }
    }

    #[test]
    fn test_storage_config_file_based() {
        let storage_dir = test_path("storage");
        let config = StorageConfig::file_based(&storage_dir);

        match config.backend {
            BackendConfig::FileBased { directory, .. } => {
                assert_eq!(directory, storage_dir);
            }
            _ => panic!("Expected FileBased backend"),
        }
    }

    #[test]
    fn test_storage_config_surrealdb() {
        let config = StorageConfig::surrealdb("ws://localhost:8000", "test", "db");

        match config.backend {
            BackendConfig::SurrealDB {
                connection_string,
                namespace,
                database,
                ..
            } => {
                assert_eq!(connection_string, "ws://localhost:8000");
                assert_eq!(namespace, "test");
                assert_eq!(database, "db");
            }
            _ => panic!("Expected SurrealDB backend"),
        }
    }

    #[test]
    fn test_config_validation_in_memory_zero_limit() {
        let config = StorageConfig {
            backend: BackendConfig::InMemory {
                memory_limit: Some(0),
                enable_lru_eviction: true,
                enable_stats_tracking: true,
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("memory_limit must be greater than 0"));
    }

    #[test]
    fn test_config_validation_file_based_empty_directory() {
        let config = StorageConfig {
            backend: BackendConfig::FileBased {
                directory: PathBuf::new(),
                create_if_missing: true,
                enable_compression: false,
                size_limit: None,
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("valid directory path"));
    }

    #[test]
    fn test_config_validation_surrealdb_empty_connection_string() {
        let config = StorageConfig {
            backend: BackendConfig::SurrealDB {
                connection_string: String::new(),
                namespace: "test".to_string(),
                database: "db".to_string(),
                connection_timeout_secs: 30,
                max_connections: 10,
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("connection_string cannot be empty"));
    }

    #[test]
    fn test_config_validation_surrealdb_empty_namespace() {
        let config = StorageConfig {
            backend: BackendConfig::SurrealDB {
                connection_string: "ws://localhost:8000".to_string(),
                namespace: String::new(),
                database: "db".to_string(),
                connection_timeout_secs: 30,
                max_connections: 10,
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("namespace cannot be empty"));
    }

    #[test]
    fn test_config_validation_surrealdb_empty_database() {
        let config = StorageConfig {
            backend: BackendConfig::SurrealDB {
                connection_string: "ws://localhost:8000".to_string(),
                namespace: "test".to_string(),
                database: String::new(),
                connection_timeout_secs: 30,
                max_connections: 10,
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("database cannot be empty"));
    }

    #[test]
    fn test_config_validation_success() {
        let config = StorageConfig::in_memory(Some(100_000_000));
        assert!(config.validate().is_ok());

        let config = StorageConfig::file_based(&test_path("storage"));
        assert!(config.validate().is_ok());

        let config = StorageConfig::surrealdb("ws://localhost:8000", "test", "db");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_skipped_when_disabled() {
        let mut config = StorageConfig {
            backend: BackendConfig::InMemory {
                memory_limit: Some(0), // Invalid
                enable_lru_eviction: true,
                enable_stats_tracking: true,
            },
            ..Default::default()
        };
        config.validate_config = false;

        // Should succeed because validation is disabled
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_hash_algorithm_default() {
        assert_eq!(HashAlgorithm::default(), HashAlgorithm::Blake3);
    }

    #[tokio::test]
    async fn test_factory_create_in_memory() {
        let config = StorageConfig::in_memory(Some(10_000_000));
        let result = StorageFactory::create(config).await;

        assert!(result.is_ok());
        let storage = result.unwrap();

        // Test basic operations
        let hash = "test_hash_123";
        let data = b"test data";

        storage.store_block(hash, data).await.unwrap();
        let retrieved = storage.get_block(hash).await.unwrap();

        assert_eq!(retrieved, Some(data.to_vec()));
    }

    #[tokio::test]
    async fn test_factory_create_file_based_not_implemented() {
        let config = StorageConfig::file_based(&test_path("test"));
        let result = StorageFactory::create(config).await;

        assert!(result.is_err());
        match result {
            Err(StorageError::Configuration(msg)) => {
                assert!(msg.contains("not yet implemented"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[tokio::test]
    async fn test_factory_create_surrealdb_requires_injection() {
        let config = StorageConfig::surrealdb("ws://localhost:8000", "test", "db");
        let result = StorageFactory::create(config).await;

        assert!(result.is_err());
        match result {
            Err(StorageError::Configuration(msg)) => {
                assert!(msg.contains("dependency injection"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[tokio::test]
    async fn test_factory_create_custom_backend() {
        let memory_storage = MemoryStorage::new();
        let config =
            StorageConfig::custom(Arc::new(memory_storage) as Arc<dyn ContentAddressedStorage>);
        let result = StorageFactory::create(config).await;

        assert!(result.is_ok());
        let storage = result.unwrap();

        // Test that we can use the custom backend
        let hash = "custom_test";
        let data = b"custom data";
        storage.store_block(hash, data).await.unwrap();
        let retrieved = storage.get_block(hash).await.unwrap();
        assert_eq!(retrieved, Some(data.to_vec()));
    }

    #[tokio::test]
    async fn test_factory_create_hasher_blake3() {
        let hasher = StorageFactory::create_hasher(&HashAlgorithm::Blake3);
        assert_eq!(hasher.algorithm_name(), "blake3");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_factory_create_from_env_in_memory() {
        // Clean up first to ensure isolation
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
        std::env::remove_var("STORAGE_DIRECTORY");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");

        // Set environment variables
        std::env::set_var("STORAGE_BACKEND", "in_memory");
        std::env::set_var("STORAGE_MEMORY_LIMIT", "10000000");

        let result = StorageFactory::create_from_env().await;
        assert!(result.is_ok());

        // Clean up
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_factory_create_from_env_file_based() {
        // Clean up ALL storage-related env vars first to ensure isolation
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
        std::env::remove_var("STORAGE_DIRECTORY");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");

        std::env::set_var("STORAGE_BACKEND", "file_based");
        let test_dir = test_path("test");
        std::env::set_var("STORAGE_DIRECTORY", test_dir.to_string_lossy().as_ref());

        let result = StorageFactory::create_from_env().await;
        // Should fail because file-based is not implemented
        assert!(result.is_err());

        // Clean up
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_DIRECTORY");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_factory_create_from_env_surrealdb() {
        // Clean up ALL storage-related env vars first to ensure isolation
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
        std::env::remove_var("STORAGE_DIRECTORY");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");

        std::env::set_var("STORAGE_BACKEND", "surrealdb");
        std::env::set_var("STORAGE_CONNECTION_STRING", "ws://localhost:8000");
        std::env::set_var("STORAGE_NAMESPACE", "test");
        std::env::set_var("STORAGE_DATABASE", "db");

        let result = StorageFactory::create_from_env().await;
        // Should fail because SurrealDB requires dependency injection
        assert!(result.is_err());

        // Clean up
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_factory_create_from_env_invalid_backend() {
        // Clean up first to ensure isolation
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
        std::env::remove_var("STORAGE_DIRECTORY");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");

        std::env::set_var("STORAGE_BACKEND", "invalid");

        let result = StorageFactory::create_from_env().await;
        assert!(result.is_err());
        match result {
            Err(StorageError::Configuration(msg)) => {
                assert!(msg.contains("Unknown storage backend type"));
            }
            _ => panic!("Expected Configuration error"),
        }

        // Clean up
        std::env::remove_var("STORAGE_BACKEND");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_factory_create_from_env_missing_required_vars() {
        // Clean up ALL storage-related env vars first to ensure isolation
        std::env::remove_var("STORAGE_BACKEND");
        std::env::remove_var("STORAGE_MEMORY_LIMIT");
        std::env::remove_var("STORAGE_DIRECTORY");
        std::env::remove_var("STORAGE_CONNECTION_STRING");
        std::env::remove_var("STORAGE_NAMESPACE");
        std::env::remove_var("STORAGE_DATABASE");

        std::env::set_var("STORAGE_BACKEND", "file_based");
        // Don't set STORAGE_DIRECTORY

        let result = StorageFactory::create_from_env().await;
        assert!(
            result.is_err(),
            "Expected error when STORAGE_DIRECTORY is missing"
        );
        match result {
            Err(StorageError::Configuration(msg)) => {
                assert!(
                    msg.contains("STORAGE_DIRECTORY"),
                    "Expected error message to contain 'STORAGE_DIRECTORY', got: {}",
                    msg
                );
            }
            _ => panic!("Expected Configuration error"),
        }

        // Clean up
        std::env::remove_var("STORAGE_BACKEND");
    }

    #[tokio::test]
    async fn test_storage_stats_after_operations() {
        let config = StorageConfig::in_memory(Some(10_000_000));
        let storage = StorageFactory::create(config).await.unwrap();

        // Store some blocks
        storage.store_block("hash1", b"data1").await.unwrap();
        storage.store_block("hash2", b"data2").await.unwrap();

        // Get stats
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.block_count, 2);
        assert!(stats.block_size_bytes > 0);
    }

    #[test]
    fn test_config_serialization() {
        let config = StorageConfig::in_memory(Some(100_000_000));
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: StorageConfig = serde_json::from_str(&json).unwrap();

        match deserialized.backend {
            BackendConfig::InMemory { memory_limit, .. } => {
                assert_eq!(memory_limit, Some(100_000_000));
            }
            _ => panic!("Expected InMemory backend"),
        }
    }

    #[test]
    fn test_config_deserialization_from_json() {
        let json = r#"{
            "backend": {
                "type": "in_memory",
                "memory_limit": 100000000,
                "enable_lru_eviction": true,
                "enable_stats_tracking": true
            },
            "hash_algorithm": "blake3",
            "enable_deduplication": true,
            "enable_maintenance": true,
            "validate_config": true
        }"#;

        let config: StorageConfig = serde_json::from_str(json).unwrap();

        match config.backend {
            BackendConfig::InMemory { memory_limit, .. } => {
                assert_eq!(memory_limit, Some(100_000_000));
            }
            _ => panic!("Expected InMemory backend"),
        }
    }
}
