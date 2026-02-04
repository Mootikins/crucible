//! Storage client abstraction for daemon-based queries
//!
//! This trait provides a high-level interface for querying storage through
//! the daemon, abstracting over direct Storage access vs daemon RPC.

use anyhow::Result;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Client abstraction for storage queries
///
/// This trait enables CLI and tools to query storage without knowing whether
/// they're talking directly to SurrealDB or through the daemon.
///
/// ## Design
///
/// - `query_raw()` returns raw JSON for maximum flexibility
/// - `query()` provides typed deserialization convenience
/// - Both methods abstract over daemon RPC vs direct storage access
///
/// ## Implementations
///
/// - `DaemonStorageClient` (in crucible-rpc) - queries through daemon
/// - `DirectStorageClient` (future) - direct SurrealDB access for testing
#[async_trait]
pub trait StorageClient: Send + Sync {
    /// Execute a raw query and return JSON
    ///
    /// # Arguments
    ///
    /// * `sql` - The query string to execute
    ///
    /// # Returns
    ///
    /// Returns raw JSON result from the storage backend
    async fn query_raw(&self, sql: &str) -> Result<Value>;

    /// Execute a query and deserialize results
    ///
    /// This is a convenience method that calls `query_raw()` and deserializes
    /// the result into the specified type.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type to deserialize results into
    ///
    /// # Arguments
    ///
    /// * `sql` - The query string to execute
    ///
    /// # Returns
    ///
    /// Returns a vector of deserialized records
    async fn query<T: DeserializeOwned + Send>(&self, sql: &str) -> Result<Vec<T>> {
        let result = self.query_raw(sql).await?;
        Ok(serde_json::from_value(result)?)
    }
}

#[cfg(feature = "test-utils")]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock storage client for testing
    ///
    /// Allows tests to pre-configure responses for specific queries.
    pub struct MockStorageClient {
        responses: Arc<Mutex<HashMap<String, Value>>>,
    }

    impl MockStorageClient {
        /// Create a new mock client
        pub fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        /// Set a response for a specific query
        pub async fn set_response(&self, query: &str, response: Value) {
            self.responses
                .lock()
                .await
                .insert(query.to_string(), response);
        }
    }

    impl Default for MockStorageClient {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl StorageClient for MockStorageClient {
        async fn query_raw(&self, sql: &str) -> Result<Value> {
            let responses = self.responses.lock().await;
            responses
                .get(sql)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("No mock response for query: {}", sql))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        #[tokio::test]
        async fn test_mock_client_returns_configured_response() {
            let client = MockStorageClient::new();
            let expected = json!({"result": "test"});
            client
                .set_response("SELECT * FROM test", expected.clone())
                .await;

            let result = client.query_raw("SELECT * FROM test").await.unwrap();
            assert_eq!(result, expected);
        }

        #[tokio::test]
        async fn test_mock_client_errors_on_unknown_query() {
            let client = MockStorageClient::new();
            let result = client.query_raw("SELECT * FROM unknown").await;
            assert!(result.is_err());
        }
    }
}
