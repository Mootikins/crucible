//! Team patterns: high-level orchestration of multiple subagents.
//!
//! Wraps the existing [`BackgroundJobManager`](crate::BackgroundJobManager)
//! subagent infrastructure (`spawn_subagent_blocking`) to provide three
//! coordination shapes exposed to Lua via `cru.team.*`:
//!
//! - [`Supervisor`] — a "manager" LLM decides which worker runs next,
//!   sequentially, until it says the task is done.
//! - [`Router`] — a Lua classifier picks one worker; that worker runs
//!   once; its output is returned.
//! - [`Broadcast`] — fan-out to N workers in parallel; collect results
//!   preserving input order.
//!
//! All three share the same machinery (`spawn_subagent_blocking` with a
//! `Target agent: <name>` context preamble), differing only in
//! orchestration. None of them implement the agent execution loop —
//! that lives in [`BackgroundJobManager`](crate::BackgroundJobManager).

mod broadcast;
mod router;
mod supervisor;

#[cfg(test)]
mod tests;

pub use broadcast::Broadcast;
pub use router::{Router, RouterClassifier, RouterError};
pub use supervisor::{Supervisor, SupervisorDecider, SupervisorDecision, SupervisorError};

use crate::background_manager::{BackgroundError, SubagentContext};
use crate::BackgroundJobManager;
use crucible_core::background::{JobResult, SubagentBlockingConfig};
use crucible_core::config::{AgentProfile, DelegationConfig};
use crucible_core::session::SessionAgent;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Setup data shared by the three team patterns.
///
/// Each pattern needs:
/// - the [`BackgroundJobManager`] (with its `SubagentFactory` already installed),
/// - the parent [`SessionAgent`] config (provider, model, system prompt template),
/// - the map of available agent profiles keyed by team-member name,
/// - the workspace path,
/// - a unique session id used for `register_subagent_context`.
///
/// We hold this in a small struct so `Supervisor`, `Router`, and
/// `Broadcast` don't each duplicate four fields.
#[derive(Clone)]
pub struct TeamCtx {
    pub manager: Arc<BackgroundJobManager>,
    pub session_id: String,
    pub parent_agent: SessionAgent,
    pub available_agents: HashMap<String, AgentProfile>,
    pub workspace: PathBuf,
}

impl TeamCtx {
    /// Register / refresh the subagent context for this team's session id.
    ///
    /// `max_concurrent` must be >= the number of agents the pattern may
    /// run in parallel; Broadcast sets it to `agents.len()`, Supervisor
    /// and Router run one at a time so 1 is fine but we set it generously
    /// to leave room for future parallel branches.
    fn register_context(&self, max_concurrent: u32) -> Result<(), BackgroundError> {
        // Build a delegation config sized to the team. Self-delegation
        // guard fires when delegator_name == target_name, so we use a
        // sentinel `_team` name that no real agent will collide with.
        let mut parent_agent = self.parent_agent.clone();
        parent_agent.delegation_config = Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: max_concurrent,
        });

        self.manager.register_subagent_context(
            &self.session_id,
            SubagentContext {
                agent: parent_agent,
                available_agents: self.available_agents.clone(),
                workspace: self.workspace.clone(),
                // No parent_session_id — team orchestration is its own
                // top-level activity, not a sub-delegation of another
                // session. This also keeps the delegation_depth at 0
                // and avoids creating subagent session files.
                parent_session_id: None,
                parent_session_dir: None,
                delegator_agent_name: Some("_team".to_string()),
                target_agent_name: None,
                delegation_depth: 0,
            },
        );
        Ok(())
    }

    /// Run a single team member to completion and return its output.
    ///
    /// `agent_name` selects from `available_agents`. The user prompt is
    /// what the agent sees; `user_context` is optional extra context the
    /// caller wants prepended.
    ///
    /// The `Target agent: <name>` preamble drives
    /// [`parse_target_agent_name`](crate::background_manager) so the
    /// underlying subagent execution picks up the right profile.
    pub async fn run_member(
        &self,
        agent_name: &str,
        prompt: String,
        user_context: Option<String>,
    ) -> Result<String, BackgroundError> {
        let target_line = format!("Target agent: {agent_name}");
        let context = match user_context {
            Some(ctx) => format!("{target_line}\n\n{ctx}"),
            None => target_line,
        };

        let job_result: JobResult = self
            .manager
            .spawn_subagent_blocking(
                &self.session_id,
                prompt,
                Some(context),
                SubagentBlockingConfig::default(),
                None,
            )
            .await?;

        if job_result.is_success() {
            Ok(job_result.output.unwrap_or_default())
        } else {
            Err(BackgroundError::SpawnFailed(
                job_result
                    .error
                    .unwrap_or_else(|| "subagent failed without error message".to_string()),
            ))
        }
    }
}
