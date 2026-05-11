//! Sequential supervisor pattern.
//!
//! A "manager" LLM (or any function implementing [`SupervisorDecider`])
//! decides which worker should run next given the task and the history of
//! prior (agent, output) pairs. Each chosen worker runs to completion, its
//! output is appended to history, and the supervisor is consulted again
//! until it returns [`SupervisorDecision::Done`].
//!
//! Returns the *last* worker's output as the team result. Future work:
//! optionally let the supervisor synthesise a final answer instead of
//! verbatim-returning the last worker's output.

use super::TeamCtx;
use crate::background_manager::BackgroundError;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

/// What the supervisor wants to do next.
#[derive(Debug, Clone)]
pub enum SupervisorDecision {
    /// Run `agent_name` with the given prompt.
    Run(String, String),
    /// Stop — task is complete.
    Done,
}

/// Strategy for picking the next worker.
///
/// In production this is backed by an LLM call that reads the task and
/// the running transcript. In tests it's a scripted decider.
#[async_trait]
pub trait SupervisorDecider: Send + Sync {
    async fn decide(
        &self,
        task: &str,
        history: &[(String, String)],
    ) -> Result<SupervisorDecision, String>;
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("decider error: {0}")]
    Decider(String),
    #[error("unknown agent: {0}")]
    UnknownAgent(String),
    #[error(transparent)]
    Background(#[from] BackgroundError),
}

/// Per-agent token budget is enforced by the underlying subagent
/// execution (`SubagentBlockingConfig.result_max_bytes`). A *shared*
/// budget across the whole team — capping total output bytes regardless
/// of which member produced them — is future work; the team currently
/// has no concept of a cross-member ledger.
pub struct Supervisor {
    ctx: TeamCtx,
    agents: Vec<String>,
    decider: Arc<dyn SupervisorDecider>,
}

impl Supervisor {
    pub fn new(ctx: TeamCtx, agents: Vec<String>, decider: Arc<dyn SupervisorDecider>) -> Self {
        Self {
            ctx,
            agents,
            decider,
        }
    }

    /// Run the supervisor loop until [`SupervisorDecision::Done`].
    ///
    /// Returns the last worker's output, or the empty string if the
    /// supervisor terminates without running any worker.
    pub async fn run(&self, task: &str) -> Result<String, SupervisorError> {
        // Sized for max_concurrent: supervisor is sequential so 1 is
        // technically enough, but the manager's enforce_delegation_capabilities
        // compares against the count of *currently running* delegations,
        // and we'd rather pass the cap generously than be a fence-post off.
        self.ctx.register_context(self.agents.len().max(1) as u32)?;

        let mut history: Vec<(String, String)> = Vec::new();
        let mut last_output = String::new();

        // Hard ceiling to avoid pathological infinite loops if a decider
        // never returns Done. 2x the team size lets each agent run twice.
        // Future work: make this configurable.
        let max_steps = self.agents.len().max(1) * 2 + 1;
        let mut steps = 0;

        loop {
            steps += 1;
            if steps > max_steps {
                break;
            }

            let decision = self
                .decider
                .decide(task, &history)
                .await
                .map_err(SupervisorError::Decider)?;

            match decision {
                SupervisorDecision::Done => break,
                SupervisorDecision::Run(agent_name, prompt) => {
                    if !self.agents.iter().any(|n| n == &agent_name) {
                        return Err(SupervisorError::UnknownAgent(agent_name));
                    }
                    let output = self.ctx.run_member(&agent_name, prompt, None).await?;
                    history.push((agent_name, output.clone()));
                    last_output = output;
                }
            }
        }

        Ok(last_output)
    }
}
