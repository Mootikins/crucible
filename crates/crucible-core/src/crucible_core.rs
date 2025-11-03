//! # CrucibleCore - Dependency-Inverted Central Coordinator
//!
//! This module provides a central coordinator that orchestrates operations through trait abstractions.
//! Core depends ONLY on traits (Storage, MarkdownParser, ToolExecutor), never on concrete implementations.
//!
//! ## Architecture (Dependency Inversion)
//! - Core defines traits (abstractions) in `traits/` module
//! - Implementations (SurrealDB, Pulldown, etc.) implement these traits
//! - Core receives trait objects via Builder pattern
//! - CLI/REPL/Desktop construct implementations and pass to Core via builder
//!
//! ## Usage
//! ```ignore
//! let storage = SurrealClient::new(config).await?;
//! let parser = PulldownParser::new();
//! let tools = RuneToolExecutor::new();
//!
//! let core = CrucibleCore::builder()
//!     .with_storage(storage)
//!     .with_parser(parser)
//!     .with_tools(tools)
//!     .build()?;
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::traits::{MarkdownParser, Storage, ToolExecutor};

/// Central coordinator for Crucible - Orchestrates operations through trait abstractions
///
/// Core is the single dependency for all frontends (CLI, REPL, Desktop).
/// It coordinates operations by delegating to injected trait implementations.
///
/// ## Dependency Inversion
/// Core depends on abstractions (traits), not concrete implementations:
/// - `Storage` - Database/persistence operations
/// - `MarkdownParser` - Document parsing
/// - `ToolExecutor` - Tool/plugin execution
///
/// Use `CrucibleCore::builder()` to construct instances.
pub struct CrucibleCore {
    /// Storage abstraction (database operations)
    storage: Arc<dyn Storage>,

    /// Markdown parser abstraction (optional - for parse_and_store operations)
    // TODO: Will be used once parser implementation is injected via builder
    #[allow(dead_code)]
    parser: Option<Arc<dyn MarkdownParser>>,

    /// Tool executor abstraction (optional - for agent/tool operations)
    // TODO: Will be used once tool executor implementation is injected via builder
    #[allow(dead_code)]
    tools: Option<Arc<dyn ToolExecutor>>,
}

impl CrucibleCore {
    /// Create a new builder for CrucibleCore
    ///
    /// Use this to construct CrucibleCore instances with injected dependencies.
    ///
    /// # Example
    /// ```ignore
    /// let storage = SurrealClient::new(config).await?;
    /// let core = CrucibleCore::builder()
    ///     .with_storage(storage)
    ///     .build()?;
    /// ```
    pub fn builder() -> CrucibleCoreBuilder {
        CrucibleCoreBuilder::new()
    }

    /// Execute a raw query
    ///
    /// Delegates to the Storage trait implementation.
    /// CLI/REPL call this method, Core delegates to storage abstraction.
    pub async fn query(
        &self,
        query: &str,
    ) -> Result<Vec<BTreeMap<String, serde_json::Value>>, String> {
        // Delegate to storage trait
        let result = self
            .storage
            .query(query, &[])
            .await
            .map_err(|e| format!("Query failed: {}", e))?;

        // Convert Record to BTreeMap (maintains backward compatibility with existing CLI code)
        let rows = result
            .records
            .into_iter()
            .map(|record| {
                let mut map = BTreeMap::new();

                // Add ID if present
                if let Some(id) = record.id {
                    map.insert("id".to_string(), serde_json::Value::String(id.0));
                }

                // Add all data fields
                for (key, value) in record.data {
                    map.insert(key, value);
                }

                map
            })
            .collect();

        Ok(rows)
    }

    /// Get database statistics
    ///
    /// Delegates to the Storage trait implementation.
    pub async fn get_stats(&self) -> Result<BTreeMap<String, serde_json::Value>, String> {
        // Delegate to storage trait - it handles stats calculation
        let stats = self
            .storage
            .get_stats()
            .await
            .map_err(|e| format!("Failed to get stats: {}", e))?;

        // Convert HashMap to BTreeMap for backward compatibility
        Ok(stats.into_iter().collect())
    }

    /// List database tables (for autocomplete)
    ///
    /// Delegates to the Storage trait implementation.
    pub async fn list_tables(&self) -> Result<Vec<String>, String> {
        self.storage
            .list_tables()
            .await
            .map_err(|e| format!("Failed to list tables: {}", e))
    }

    /// Initialize database schema
    ///
    /// Delegates to the Storage trait implementation.
    pub async fn initialize_database(&self) -> Result<(), String> {
        self.storage
            .initialize_schema()
            .await
            .map_err(|e| format!("Failed to initialize schema: {}", e))
    }
}

/// Builder for constructing CrucibleCore instances with dependency injection
///
/// Use this to inject trait implementations into CrucibleCore.
///
/// # Example
/// ```ignore
/// let storage = SurrealClient::new(config).await?;
/// let parser = PulldownParser::new();
/// let tools = RuneToolExecutor::new();
///
/// let core = CrucibleCore::builder()
///     .with_storage(storage)
///     .with_parser(parser)
///     .with_tools(tools)
///     .build()?;
/// ```
pub struct CrucibleCoreBuilder {
    storage: Option<Arc<dyn Storage>>,
    parser: Option<Arc<dyn MarkdownParser>>,
    tools: Option<Arc<dyn ToolExecutor>>,
}

impl CrucibleCoreBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            storage: None,
            parser: None,
            tools: None,
        }
    }

    /// Set the storage implementation (required)
    ///
    /// # Example
    /// ```ignore
    /// let storage = SurrealClient::new(config).await?;
    /// builder.with_storage(storage)
    /// ```
    pub fn with_storage<S: Storage + 'static>(mut self, storage: S) -> Self {
        self.storage = Some(Arc::new(storage));
        self
    }

    /// Set the markdown parser implementation (optional)
    ///
    /// # Example
    /// ```ignore
    /// let parser = PulldownParser::new();
    /// builder.with_parser(parser)
    /// ```
    pub fn with_parser<P: MarkdownParser + 'static>(mut self, parser: P) -> Self {
        self.parser = Some(Arc::new(parser));
        self
    }

    /// Set the tool executor implementation (optional)
    ///
    /// # Example
    /// ```ignore
    /// let tools = RuneToolExecutor::new();
    /// builder.with_tools(tools)
    /// ```
    pub fn with_tools<T: ToolExecutor + 'static>(mut self, tools: T) -> Self {
        self.tools = Some(Arc::new(tools));
        self
    }

    /// Build the CrucibleCore instance
    ///
    /// # Errors
    /// Returns an error if required dependencies (storage) are not provided.
    pub fn build(self) -> Result<CrucibleCore, String> {
        let storage = self
            .storage
            .ok_or_else(|| "Storage implementation is required".to_string())?;

        Ok(CrucibleCore {
            storage,
            parser: self.parser,
            tools: self.tools,
        })
    }
}

impl Default for CrucibleCoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    use crate::traits::storage::{QueryResult, StorageResult};

    // Mock storage implementation for testing
    struct MockStorage;

    #[async_trait]
    impl Storage for MockStorage {
        async fn query(
            &self,
            _query: &str,
            _params: &[(&str, serde_json::Value)],
        ) -> StorageResult<QueryResult> {
            Ok(QueryResult::empty())
        }

        async fn get_stats(&self) -> StorageResult<HashMap<String, serde_json::Value>> {
            let mut stats = HashMap::new();
            stats.insert(
                "database_type".to_string(),
                serde_json::Value::String("Mock".to_string()),
            );
            Ok(stats)
        }

        async fn list_tables(&self) -> StorageResult<Vec<String>> {
            Ok(vec!["notes".to_string(), "tags".to_string()])
        }

        async fn initialize_schema(&self) -> StorageResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_builder_creation() {
        let mock_storage = MockStorage;
        let core = CrucibleCore::builder()
            .with_storage(mock_storage)
            .build()
            .expect("Should build successfully");

        // Verify storage was set correctly (use Arc::strong_count to verify it's not null)
        assert!(std::sync::Arc::strong_count(&core.storage) > 0);
        assert!(core.parser.is_none());
        assert!(core.tools.is_none());
    }

    #[test]
    fn test_builder_requires_storage() {
        let result = CrucibleCore::builder().build();
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            "Storage implementation is required"
        );
    }

    #[tokio::test]
    async fn test_list_tables() {
        let mock_storage = MockStorage;
        let core = CrucibleCore::builder()
            .with_storage(mock_storage)
            .build()
            .unwrap();

        let tables = core.list_tables().await.unwrap();
        assert_eq!(tables, vec!["notes".to_string(), "tags".to_string()]);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let mock_storage = MockStorage;
        let core = CrucibleCore::builder()
            .with_storage(mock_storage)
            .build()
            .unwrap();

        let stats = core.get_stats().await.unwrap();
        assert_eq!(
            stats.get("database_type"),
            Some(&serde_json::Value::String("Mock".to_string()))
        );
    }

    #[tokio::test]
    async fn test_query() {
        let mock_storage = MockStorage;
        let core = CrucibleCore::builder()
            .with_storage(mock_storage)
            .build()
            .unwrap();

        let results = core.query("SELECT * FROM notes").await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_initialize_database() {
        let mock_storage = MockStorage;
        let core = CrucibleCore::builder()
            .with_storage(mock_storage)
            .build()
            .unwrap();

        let result = core.initialize_database().await;
        assert!(result.is_ok());
    }
}
