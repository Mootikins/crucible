//! Delegation service: spawns delegated child sessions through the main
//! scheduler loop.
//!
//! A delegated child is a real session — created via
//! `SessionManager::create_child_session` with `parent_session_id` set, driven
//! by `AgentManager::send_message_notified`, so it gets working tool dispatch,
//! Precognition, Lua hooks, per-turn events on its own session id, and
//! standard persistence. "Not first-class" means: hidden from default
//! `session.list`, lifecycle-subordinate to its parent, ended automatically
//! when its turn completes.
//!
//! The service holds a `Weak<AgentManager>` (bound after the manager is
//! Arc'd) so the manager can hold an `Arc<DelegationService>` without a
//! reference cycle.

use crate::agent_manager::{AgentManager, TurnOutcome, TurnStatus};
use crate::event_emitter::emit_event;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use async_trait::async_trait;
use crucible_core::background::{truncate, JobError, JobInfo, JobKind, JobResult};
use crucible_core::session::SessionAgent;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;
use tokio::sync::{broadcast, watch, Semaphore};
use tracing::{debug, info, warn};

/// Parent-facing lifecycle event names. `delegation_*` names are preserved
/// from the pre-refactor system for subscriber compatibility; payloads now
/// carry `child_session_id`.
pub mod events {
    pub const DELEGATION_SPAWNED: &str = "delegation_spawned";
    pub const DELEGATION_COMPLETED: &str = "delegation_completed";
    pub const DELEGATION_FAILED: &str = "delegation_failed";
}

/// A request to delegate work to a child session.
#[derive(Debug, Clone)]
pub struct DelegationRequest {
    pub parent_session_id: String,
    pub prompt: String,
    /// Free-text context block prepended to the child's first message.
    pub context: Option<String>,
    /// Named target agent (ACP profile today; agent cards later). `None`
    /// clones the parent's agent config.
    pub target_agent: Option<String>,
    /// Human-readable description; becomes the child session's title.
    pub description: Option<String>,
}

/// Result of spawning a delegation. `delegation_id == child_session_id`.
#[derive(Debug, Clone)]
pub struct DelegationSpawned {
    pub delegation_id: String,
    pub child_session_id: String,
    pub message_id: String,
}

/// Seam the `delegate_session` tool (and ACP in-process MCP host) calls into.
/// Trait rather than a concrete type so tool-layer tests can mock it and so
/// `tools/` doesn't depend on `AgentManager` construction.
#[async_trait]
pub trait DelegationSpawner: Send + Sync {
    async fn spawn_delegation(&self, req: DelegationRequest)
        -> Result<DelegationSpawned, JobError>;

    /// Await a spawned delegation's terminal result. On timeout the child is
    /// cancelled and a failed result is returned (no orphans).
    async fn await_delegation(
        &self,
        delegation_id: &str,
        timeout: Duration,
    ) -> Result<JobResult, JobError>;

    fn list_delegations(&self, parent_session_id: &str) -> Vec<JobInfo>;

    fn get_delegation_result(&self, delegation_id: &str) -> Option<JobResult>;

    async fn cancel_delegation(&self, delegation_id: &str) -> bool;
}

struct DelegationRecord {
    info: JobInfo,
    parent_session_id: String,
    /// `None` until terminal; watchers subscribe for completion.
    result_tx: watch::Sender<Option<JobResult>>,
}

/// Delegation state + spawn logic. Cheap to clone into watcher tasks: all
/// fields are `Arc`s.
pub struct DelegationService {
    agent_manager: OnceLock<Weak<AgentManager>>,
    session_manager: Arc<SessionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    records: Arc<DashMap<String, DelegationRecord>>,
    /// Per-parent concurrency permits. Sized from the parent's
    /// `max_concurrent_delegations` at first use; a config change for an
    /// existing parent takes effect after its entry is cleaned up.
    permits: Arc<DashMap<String, Arc<Semaphore>>>,
}

impl DelegationService {
    pub fn new(
        session_manager: Arc<SessionManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
    ) -> Arc<Self> {
        Arc::new(Self {
            agent_manager: OnceLock::new(),
            session_manager,
            event_tx,
            records: Arc::new(DashMap::new()),
            permits: Arc::new(DashMap::new()),
        })
    }

    /// Bind the (Arc'd) agent manager. Must be called once at startup, after
    /// the manager is constructed with this service. Weak: the manager holds
    /// the strong `Arc<DelegationService>`.
    pub fn bind_agent_manager(&self, manager: &Arc<AgentManager>) {
        let _ = self.agent_manager.set(Arc::downgrade(manager));
    }

    fn manager(&self) -> Result<Arc<AgentManager>, JobError> {
        self.agent_manager
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                JobError::SpawnFailed("delegation service not bound to an agent manager".into())
            })
    }

    /// Delegation depth of a session: number of parent links above it.
    /// Children are real sessions, so the chain is walkable directly.
    fn depth_of(&self, session: &crucible_core::session::Session) -> u32 {
        let mut depth = 0;
        let mut current = session.parent_session_id.clone();
        while let Some(parent_id) = current {
            depth += 1;
            if depth >= 32 {
                break; // cycle guard; never expected
            }
            current = self
                .session_manager
                .get_session(&parent_id)
                .and_then(|s| s.parent_session_id);
        }
        depth
    }

    /// Cancel every non-terminal child of `parent_session_id`. Used by the
    /// parent-cancel and session-cleanup cascades.
    pub async fn cancel_children_of(&self, parent_session_id: &str) -> usize {
        let child_ids: Vec<String> = self
            .records
            .iter()
            .filter(|e| {
                e.value().parent_session_id == parent_session_id
                    && !e.value().info.status.is_terminal()
            })
            .map(|e| e.key().clone())
            .collect();
        let mut cancelled = 0;
        for id in child_ids {
            if self.cancel_delegation(&id).await {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Drop delegation records and the concurrency permit entry for a parent.
    /// Called on parent session cleanup so the maps don't grow unbounded.
    pub fn forget_parent(&self, parent_session_id: &str) {
        self.records
            .retain(|_, r| r.parent_session_id != parent_session_id);
        self.permits.remove(parent_session_id);
    }

    fn build_job_result(info: JobInfo, outcome: &TurnOutcome) -> JobResult {
        match outcome.status {
            TurnStatus::Completed => {
                let mut info = info;
                info.mark_completed();
                JobResult::success(info, outcome.final_text.clone())
            }
            TurnStatus::Cancelled => {
                let mut info = info;
                info.mark_cancelled();
                JobResult::failure(info, "Delegated session cancelled".into())
            }
            TurnStatus::TimedOut => {
                let mut info = info;
                info.mark_failed();
                JobResult::failure(info, "Delegated session timed out".into())
            }
            TurnStatus::Failed => {
                let mut info = info;
                info.mark_failed();
                JobResult::failure(
                    info,
                    outcome
                        .error
                        .clone()
                        .unwrap_or_else(|| "Delegated session failed".into()),
                )
            }
        }
    }

    fn emit_completion_events(
        event_tx: &broadcast::Sender<SessionEventMessage>,
        parent_id: &str,
        delegation_id: &str,
        result: &JobResult,
    ) {
        let (event_type, data) = if result.is_success() {
            (
                events::DELEGATION_COMPLETED,
                serde_json::json!({
                    "delegation_id": delegation_id,
                    "child_session_id": delegation_id,
                    "result_summary": truncate(result.output.as_deref().unwrap_or(""), 500),
                    "parent_session_id": parent_id,
                }),
            )
        } else {
            (
                events::DELEGATION_FAILED,
                serde_json::json!({
                    "delegation_id": delegation_id,
                    "child_session_id": delegation_id,
                    "error": result.error.as_deref().unwrap_or("Unknown error"),
                    "parent_session_id": parent_id,
                }),
            )
        };
        if !emit_event(
            event_tx,
            SessionEventMessage::new(parent_id, event_type, data),
        ) {
            debug!(
                delegation_id,
                "No subscribers for delegation completion event"
            );
        }
    }
}

#[async_trait]
impl DelegationSpawner for DelegationService {
    async fn spawn_delegation(
        &self,
        req: DelegationRequest,
    ) -> Result<DelegationSpawned, JobError> {
        let manager = self.manager()?;
        let parent = self
            .session_manager
            .get_session(&req.parent_session_id)
            .ok_or_else(|| JobError::SessionNotFound(req.parent_session_id.clone()))?;
        let parent_agent = parent.agent.clone().ok_or_else(|| {
            JobError::SpawnFailed("parent session has no agent configured".into())
        })?;

        let delegation_cfg = parent_agent
            .delegation_config
            .clone()
            .filter(|c| c.enabled)
            .ok_or_else(|| JobError::SpawnFailed("Delegation is disabled for this agent".into()))?;

        // Depth: the child sits one level below the parent.
        let child_depth = self.depth_of(&parent).saturating_add(1);
        if child_depth > delegation_cfg.max_depth {
            return Err(JobError::SpawnFailed(format!(
                "Delegation depth limit exceeded (max_depth = {})",
                delegation_cfg.max_depth
            )));
        }

        // Target policy. With an allowlist configured, an explicit,
        // allowlisted target is required (matches pre-refactor behavior).
        if let Some(allowed) = &delegation_cfg.allowed_targets {
            let target = req.target_agent.as_deref().ok_or_else(|| {
                JobError::SpawnFailed("Delegation target could not be determined".into())
            })?;
            if !allowed.iter().any(|a| a == target) {
                return Err(JobError::SpawnFailed(format!(
                    "Delegation target '{target}' is not allowed"
                )));
            }
        }
        if let Some(target) = req.target_agent.as_deref() {
            let is_self = parent_agent.agent_name.as_deref() == Some(target)
                || parent_agent.agent_card_name.as_deref() == Some(target);
            if is_self {
                return Err(JobError::SpawnFailed(
                    "Delegation rejected by self-delegation guard".into(),
                ));
            }
        }

        // Resolve the child agent config: a named target resolves to an
        // agent CARD (specialized internal agent) first, then an ACP
        // profile; no target clones the parent.
        let mut child_agent: SessionAgent = match req.target_agent.as_deref() {
            Some(name) => {
                let cards = crate::agent_cards::discover_agent_cards(
                    &parent.workspace,
                    Some(parent.kiln.as_path()),
                );
                if let Some(card) = cards.get(name) {
                    SessionAgent::from_card(card, &parent_agent)
                } else {
                    let available = manager.build_available_agents();
                    let profile = available.get(name).cloned().ok_or_else(|| {
                        let mut names: Vec<_> =
                            cards.keys().chain(available.keys()).cloned().collect();
                        names.sort();
                        names.dedup();
                        let list = if names.is_empty() {
                            "(none)".to_string()
                        } else {
                            names.join(", ")
                        };
                        JobError::SpawnFailed(format!(
                            "Delegation target '{name}' not found. Available agents: {list}"
                        ))
                    })?;
                    SessionAgent::from_profile(&profile, name)
                }
            }
            None => parent_agent.clone(),
        };
        // Nested delegation is depth-aware: the child keeps delegation only
        // while another level would still fit under max_depth. The default
        // (max_depth = 1) preserves no-nesting behavior; max_depth = 2 lets
        // a child delegate once more, and so on. Depth is derived from the
        // parent_session_id chain at every level, so a child cannot exceed
        // the cap by editing its own config.
        child_agent.delegation_config = if child_depth < delegation_cfg.max_depth {
            Some(delegation_cfg.clone())
        } else {
            None
        };

        // Trust gate: the CHILD's resolved provider trust (not a hardcoded
        // Cloud assumption) must satisfy the kiln's data classification. A
        // local-model card can therefore serve a confidential kiln that a
        // cloud target cannot. Unresolved trust still fails closed to Cloud
        // inside resolve_agent_trust.
        let child_trust = manager.resolve_agent_trust(&child_agent);
        let classification =
            crate::trust_resolution::resolve_kiln_classification(&parent.workspace, &parent.kiln)
                .unwrap_or(crucible_core::config::DataClassification::Public);
        if !child_trust.satisfies(classification) {
            return Err(JobError::SpawnFailed(format!(
                "Delegated agent's trust level '{child_trust}' is insufficient for kiln data                  classification '{classification}'. Requires '{}' trust.",
                classification.required_trust_level(),
            )));
        }

        // Concurrency permit — acquired before session creation, released by
        // the watcher when the child reaches a terminal state. try_acquire on
        // a semaphore is atomic: two racing spawns cannot both pass the cap.
        let semaphore = self
            .permits
            .entry(parent.id.clone())
            .or_insert_with(|| {
                Arc::new(Semaphore::new(
                    delegation_cfg.max_concurrent_delegations as usize,
                ))
            })
            .clone();
        let permit = semaphore.try_acquire_owned().map_err(|_| {
            JobError::SpawnFailed(format!(
                "Maximum concurrent delegations ({}) exceeded",
                delegation_cfg.max_concurrent_delegations
            ))
        })?;

        let title = req
            .description
            .clone()
            .unwrap_or_else(|| truncate(&req.prompt, 60));
        let child = self
            .session_manager
            .create_child_session(&parent, child_agent, Some(title))
            .await
            .map_err(|e| JobError::SpawnFailed(e.to_string()))?;

        let mut info = JobInfo::new(
            parent.id.clone(),
            JobKind::Subagent {
                prompt: req.prompt.clone(),
                context: req.context.clone(),
            },
        );
        info.id = child.id.clone();
        info.session_path = Some(child.storage_path());

        let full_prompt = match &req.context {
            Some(ctx) if !ctx.is_empty() => format!("{ctx}\n\n{}", req.prompt),
            _ => req.prompt.clone(),
        };

        if !emit_event(
            &self.event_tx,
            SessionEventMessage::new(
                &parent.id,
                events::DELEGATION_SPAWNED,
                serde_json::json!({
                    "delegation_id": child.id,
                    "child_session_id": child.id,
                    "prompt": truncate(&req.prompt, 100),
                    "target_agent": req.target_agent,
                    "parent_session_id": parent.id,
                }),
            ),
        ) {
            debug!("No subscribers for delegation_spawned event");
        }

        let (message_id, completion_rx) = match manager
            .send_message_notified(&child.id, full_prompt, &self.event_tx, false, None)
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                // Undo the spawn: end the child session, free the permit.
                drop(permit);
                let _ = self.session_manager.end_session(&child.id).await;
                let mut failed_info = info;
                failed_info.mark_failed();
                let result = JobResult::failure(failed_info, e.to_string());
                Self::emit_completion_events(&self.event_tx, &parent.id, &child.id, &result);
                return Err(JobError::SpawnFailed(e.to_string()));
            }
        };

        let (result_tx, _) = watch::channel(None);
        self.records.insert(
            child.id.clone(),
            DelegationRecord {
                info: info.clone(),
                parent_session_id: parent.id.clone(),
                result_tx: result_tx.clone(),
            },
        );

        info!(
            delegation_id = %child.id,
            parent_session_id = %parent.id,
            target = ?req.target_agent,
            "Spawned delegated child session"
        );

        // Watcher: single place that finalizes the delegation — builds the
        // JobResult, publishes it to awaiters, emits parent events, ends the
        // child session, and releases the concurrency permit. Timeout here is
        // the backstop for BOTH modes (blocking callers may time out earlier
        // and cancel explicitly).
        let records = self.records.clone();
        let session_manager = self.session_manager.clone();
        let event_tx = self.event_tx.clone();
        let manager_weak = self.agent_manager.get().cloned();
        let parent_id = parent.id.clone();
        let child_id = child.id.clone();
        let timeout = Duration::from_secs(delegation_cfg.timeout_secs);
        tokio::spawn(async move {
            let outcome = match tokio::time::timeout(timeout, completion_rx).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => TurnOutcome {
                    status: TurnStatus::Failed,
                    final_text: String::new(),
                    error: Some("child turn ended without reporting an outcome".into()),
                },
                Err(_) => {
                    // Timed out: cancel the child's running turn.
                    if let Some(m) = manager_weak.as_ref().and_then(Weak::upgrade) {
                        m.cancel(&child_id).await;
                    }
                    TurnOutcome {
                        status: TurnStatus::TimedOut,
                        final_text: String::new(),
                        error: Some(format!("delegation timed out after {}s", timeout.as_secs())),
                    }
                }
            };

            let result = DelegationService::build_job_result(info, &outcome);

            // One-shot delegation semantics: the child ends when its turn
            // does. Finalize the child's lifecycle BEFORE publishing the
            // result so an awaiter observes a fully-ended child session.
            if let Err(e) = session_manager.end_session(&child_id).await {
                debug!(child_id = %child_id, error = %e, "Child session already ended");
            }
            if let Some(m) = manager_weak.as_ref().and_then(Weak::upgrade) {
                m.cleanup_session(&child_id);
            }

            if let Some(mut record) = records.get_mut(&child_id) {
                record.info = result.info.clone();
            }
            // send_replace, not send: `send` fails (and DISCARDS the value)
            // when no receiver currently exists, which is exactly the case
            // when the delegation finishes before anyone awaits it.
            let _ = result_tx.send_replace(Some(result.clone()));
            DelegationService::emit_completion_events(&event_tx, &parent_id, &child_id, &result);
            drop(permit);
        });

        Ok(DelegationSpawned {
            delegation_id: child.id.clone(),
            child_session_id: child.id,
            message_id,
        })
    }

    async fn await_delegation(
        &self,
        delegation_id: &str,
        timeout: Duration,
    ) -> Result<JobResult, JobError> {
        let mut rx = self
            .records
            .get(delegation_id)
            .map(|r| r.result_tx.subscribe())
            .ok_or_else(|| JobError::NotFound(delegation_id.to_string()))?;

        let waited = tokio::time::timeout(timeout, async {
            loop {
                if let Some(result) = rx.borrow().clone() {
                    return Some(result);
                }
                if rx.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await;

        match waited {
            Ok(Some(result)) => Ok(result),
            Ok(None) => Err(JobError::SpawnFailed(
                "delegation watcher dropped without a result".into(),
            )),
            Err(_) => {
                // Caller-level timeout: cancel the child and surface failure.
                self.cancel_delegation(delegation_id).await;
                let info = self
                    .records
                    .get(delegation_id)
                    .map(|r| r.info.clone())
                    .ok_or_else(|| JobError::NotFound(delegation_id.to_string()))?;
                let mut info = info;
                if !info.status.is_terminal() {
                    info.mark_failed();
                }
                Ok(JobResult::failure(
                    info,
                    format!("delegation timed out after {}s", timeout.as_secs()),
                ))
            }
        }
    }

    fn list_delegations(&self, parent_session_id: &str) -> Vec<JobInfo> {
        let mut jobs: Vec<JobInfo> = self
            .records
            .iter()
            .filter(|e| e.value().parent_session_id == parent_session_id)
            .map(|e| e.value().info.clone())
            .collect();
        jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        jobs
    }

    fn get_delegation_result(&self, delegation_id: &str) -> Option<JobResult> {
        let record = self.records.get(delegation_id)?;
        if let Some(result) = record.result_tx.subscribe().borrow().clone() {
            return Some(result);
        }
        Some(JobResult {
            info: record.info.clone(),
            output: None,
            error: None,
            exit_code: None,
        })
    }

    async fn cancel_delegation(&self, delegation_id: &str) -> bool {
        let is_active = self
            .records
            .get(delegation_id)
            .map(|r| !r.info.status.is_terminal())
            .unwrap_or(false);
        if !is_active {
            warn!(delegation_id, "Delegation not found or already terminal");
            return false;
        }
        match self.manager() {
            // Cancelling the child's turn resolves its completion oneshot
            // (Cancelled); the watcher finalizes the record from there.
            Ok(m) => m.cancel(delegation_id).await,
            Err(_) => false,
        }
    }
}
