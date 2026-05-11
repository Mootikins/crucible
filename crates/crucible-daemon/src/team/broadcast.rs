//! Parallel broadcast pattern.
//!
//! Sends the same prompt to every team member concurrently via
//! `tokio::join_all` and collects the results preserving input order. Per
//! the design doc we intentionally do *not* throttle concurrency — if N
//! is large enough to OOM the host, that's a knob to add later, not a
//! default to bake in.

use super::TeamCtx;
use crate::background_manager::BackgroundError;
use futures::future::join_all;

pub struct Broadcast {
    ctx: TeamCtx,
    agents: Vec<String>,
}

impl Broadcast {
    pub fn new(ctx: TeamCtx, agents: Vec<String>) -> Self {
        Self { ctx, agents }
    }

    /// Run all agents in parallel and return results in `agents` order.
    ///
    /// A per-agent failure short-circuits the whole call. If you need
    /// "best effort" semantics, run with N=1 and OR the calls together
    /// in Lua — at this layer we propagate the first error encountered.
    pub async fn run(&self, prompt: &str) -> Result<Vec<String>, BackgroundError> {
        if self.agents.is_empty() {
            return Ok(Vec::new());
        }

        // max_concurrent == agents.len() so the delegation guard in
        // BackgroundJobManager doesn't reject our N-th spawn.
        self.ctx.register_context(self.agents.len() as u32)?;

        let futures = self.agents.iter().map(|agent_name| {
            let prompt = prompt.to_string();
            let ctx = &self.ctx;
            let agent_name = agent_name.clone();
            async move { ctx.run_member(&agent_name, prompt, None).await }
        });

        let results = join_all(futures).await;
        results.into_iter().collect()
    }
}
