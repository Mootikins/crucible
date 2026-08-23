//! Storage client abstraction for daemon-based queries.
//!
//! This trait gives the CLI and the tools one interface for raw storage
//! queries. All storage is daemon-side, so the daemon answers each query.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Client abstraction for storage queries
///
/// `query_raw()` returns raw JSON, so a caller does not depend on a typed
/// result shape.
///
/// ## Implementations
///
/// - `DaemonStorageClient` (in `crucible-daemon`, module `rpc_client`) sends
///   each query to the daemon over RPC.
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
}
