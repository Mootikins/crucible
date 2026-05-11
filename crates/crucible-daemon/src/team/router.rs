//! Single-shot router pattern.
//!
//! A classifier picks one route name from the input, the route maps to a
//! team-member agent name, and that agent runs once. Pure dispatch — no
//! loop, no history.

use super::TeamCtx;
use crate::background_manager::BackgroundError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Strategy for picking a route from the input.
///
/// In production this is typically a Lua function that string-matches the
/// input or asks a small LLM. In tests it's a fixed mapping.
#[async_trait]
pub trait RouterClassifier: Send + Sync {
    async fn classify(&self, input: &str) -> Result<String, String>;
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("classifier error: {0}")]
    Classifier(String),
    #[error("unknown route: {0}")]
    UnknownRoute(String),
    #[error(transparent)]
    Background(#[from] BackgroundError),
}

pub struct Router {
    ctx: TeamCtx,
    /// Map of route key (returned by classifier) -> team-member agent name.
    routes: HashMap<String, String>,
    classifier: Arc<dyn RouterClassifier>,
}

impl Router {
    pub fn new(
        ctx: TeamCtx,
        routes: HashMap<String, String>,
        classifier: Arc<dyn RouterClassifier>,
    ) -> Self {
        Self {
            ctx,
            routes,
            classifier,
        }
    }

    pub async fn run(&self, input: &str) -> Result<String, RouterError> {
        // We only ever run one agent at a time, but generous concurrency
        // keeps the underlying delegation guard happy if someone calls
        // `run` concurrently for the same Router (each call still spawns
        // sequentially relative to its own future, but a caller could
        // share the Router across tasks).
        self.ctx
            .register_context(self.routes.len().max(1) as u32)?;

        let route_key = self
            .classifier
            .classify(input)
            .await
            .map_err(RouterError::Classifier)?;

        let agent_name = self
            .routes
            .get(&route_key)
            .cloned()
            .ok_or(RouterError::UnknownRoute(route_key))?;

        let output = self
            .ctx
            .run_member(&agent_name, input.to_string(), None)
            .await?;
        Ok(output)
    }
}
