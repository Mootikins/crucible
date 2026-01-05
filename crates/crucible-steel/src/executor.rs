//! Steel script executor
//!
//! Uses a persistent worker thread to maintain the Steel engine state,
//! avoiding the overhead of recreating the engine on every call.

use crate::error::SteelError;
use crate::worker::{CompiledId, SteelWorker};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Steel script executor
///
/// Wraps a persistent Steel worker thread for efficient repeated execution.
/// The engine state is maintained across calls, so functions defined in
/// earlier calls remain available.
pub struct SteelExecutor {
    worker: Arc<SteelWorker>,
}

impl SteelExecutor {
    /// Create a new Steel executor with a persistent worker thread
    pub fn new() -> Result<Self, SteelError> {
        let worker = SteelWorker::spawn()?;
        Ok(Self {
            worker: Arc::new(worker),
        })
    }

    /// Execute Steel source code and return the result
    ///
    /// Definitions from this source are retained for subsequent calls.
    pub async fn execute_source(&self, source: &str) -> Result<JsonValue, SteelError> {
        self.worker.execute_source(source).await
    }

    /// Compile source code and return an ID for later execution
    ///
    /// This also runs the source to define any functions/values.
    /// The compiled ID can be used with `run_compiled()` to re-execute.
    pub async fn compile(&self, source: &str) -> Result<CompiledId, SteelError> {
        self.worker.compile(source).await
    }

    /// Run a previously compiled program
    pub async fn run_compiled(&self, id: CompiledId) -> Result<JsonValue, SteelError> {
        self.worker.run_compiled(id).await
    }

    /// Call a previously defined function with JSON arguments
    ///
    /// Arguments are converted directly to SteelVal on the worker thread.
    pub async fn call_function(
        &self,
        name: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, SteelError> {
        self.worker.call_function(name, args).await
    }
}

impl Clone for SteelExecutor {
    fn clone(&self) -> Self {
        Self {
            worker: self.worker.clone(),
        }
    }
}
