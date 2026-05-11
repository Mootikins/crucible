//! Bridges `crucible_lua::DaemonTeamApi` to the daemon-side
//! [`Supervisor`](crate::team::Supervisor) / [`Router`](crate::team::Router) /
//! [`Broadcast`](crate::team::Broadcast) primitives.
//!
//! The Lua wrapper owns the Lua-side decider/classifier closures and
//! hands them to us via the [`DaemonTeamApi`] trait as
//! `Arc<dyn LuaSupervisorDecideFn>` / `Arc<dyn LuaClassifyFn>`. We adapt
//! those into the daemon's `SupervisorDecider` / `RouterClassifier`
//! traits and run the patterns.

use crate::team::{
    Broadcast, Router, RouterClassifier, RouterError, Supervisor, SupervisorDecider,
    SupervisorDecision, SupervisorError, TeamCtx,
};
use crate::BackgroundJobManager;
use async_trait::async_trait;
use crucible_core::config::AgentProfile;
use crucible_core::session::SessionAgent;
use crucible_lua::{
    DaemonTeamApi, LuaClassifyFn, LuaSupervisorDecideFn, LuaSupervisorDecision, TeamHistoryEntry,
};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// Daemon-side implementation of [`DaemonTeamApi`].
///
/// Holds the singletons needed to spin up a [`TeamCtx`] on demand: the
/// [`BackgroundJobManager`] (with `SubagentFactory` already installed),
/// the parent [`SessionAgent`] config (template for every member), the
/// `available_agents` map, the workspace path, and a session id used as
/// the [`BackgroundJobManager`]'s `register_subagent_context` key.
///
/// One bridge can be shared across many concurrent Lua calls; each call
/// gets its own `TeamCtx` clone so the underlying `register_subagent_context`
/// remains race-free per-session.
pub struct DaemonTeamBridge {
    pub manager: Arc<BackgroundJobManager>,
    pub session_id: String,
    pub parent_agent: SessionAgent,
    pub available_agents: HashMap<String, AgentProfile>,
    pub workspace: PathBuf,
}

impl DaemonTeamBridge {
    pub fn new(
        manager: Arc<BackgroundJobManager>,
        session_id: String,
        parent_agent: SessionAgent,
        available_agents: HashMap<String, AgentProfile>,
        workspace: PathBuf,
    ) -> Self {
        Self {
            manager,
            session_id,
            parent_agent,
            available_agents,
            workspace,
        }
    }

    fn ctx(&self) -> TeamCtx {
        TeamCtx {
            manager: Arc::clone(&self.manager),
            session_id: self.session_id.clone(),
            parent_agent: self.parent_agent.clone(),
            available_agents: self.available_agents.clone(),
            workspace: self.workspace.clone(),
        }
    }
}

impl DaemonTeamApi for DaemonTeamBridge {
    fn supervisor(
        &self,
        agents: Vec<String>,
        decide: Arc<dyn LuaSupervisorDecideFn>,
        task: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let ctx = self.ctx();
        Box::pin(async move {
            let adapter: Arc<dyn SupervisorDecider> = Arc::new(LuaToSupervisorDecider(decide));
            Supervisor::new(ctx, agents, adapter)
                .run(&task)
                .await
                .map_err(|e: SupervisorError| e.to_string())
        })
    }

    fn router(
        &self,
        routes: HashMap<String, String>,
        classify: Arc<dyn LuaClassifyFn>,
        input: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let ctx = self.ctx();
        Box::pin(async move {
            let adapter: Arc<dyn RouterClassifier> = Arc::new(LuaToRouterClassifier(classify));
            Router::new(ctx, routes, adapter)
                .run(&input)
                .await
                .map_err(|e: RouterError| e.to_string())
        })
    }

    fn broadcast(
        &self,
        agents: Vec<String>,
        prompt: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + Send>> {
        let ctx = self.ctx();
        Box::pin(async move {
            Broadcast::new(ctx, agents)
                .run(&prompt)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

// ---- adapters ----

struct LuaToSupervisorDecider(Arc<dyn LuaSupervisorDecideFn>);

#[async_trait]
impl SupervisorDecider for LuaToSupervisorDecider {
    async fn decide(
        &self,
        task: &str,
        history: &[(String, String)],
    ) -> Result<SupervisorDecision, String> {
        let entries: Vec<TeamHistoryEntry> = history
            .iter()
            .map(|(a, o)| TeamHistoryEntry {
                agent: a.clone(),
                output: o.clone(),
            })
            .collect();
        let lua_decision = self.0.call(task, &entries).await?;
        Ok(match lua_decision {
            LuaSupervisorDecision::Done => SupervisorDecision::Done,
            LuaSupervisorDecision::Run { agent, prompt } => SupervisorDecision::Run(agent, prompt),
        })
    }
}

struct LuaToRouterClassifier(Arc<dyn LuaClassifyFn>);

#[async_trait]
impl RouterClassifier for LuaToRouterClassifier {
    async fn classify(&self, input: &str) -> Result<String, String> {
        self.0.call(input).await
    }
}
