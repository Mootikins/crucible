//! The `[permissions]` rules that apply to one session.
//!
//! Its own module because more than the agent dispatch path needs the answer.
//! A gate that runs with no agent and no prompt — a workflow's `## Validation`
//! command, when a run completes — still acts *for* a session, and reading the
//! daemon-global config there hands a session the operator locked down the
//! permissive global rules instead.

use super::{default_agent_profiles, messaging, resolve_agent_profile, AgentManager};
use crucible_core::config::components::permissions::PermissionConfig;

impl AgentManager {
    /// The `[permissions]` rules that apply to `session_id`.
    ///
    /// Resolved the same way and in the same order as the agent dispatch path:
    /// the session's agent profile permissions override the daemon-global
    /// config wholesale, exactly as `AgentProfile::permissions` documents. A
    /// session with no agent, no profile, or a profile carrying no
    /// `[permissions]` block falls back to the global config.
    ///
    /// LIMITATION: the per-message `permission_mode` override is not visible
    /// here. It is a parameter of one `session.send_message` request, never
    /// stored on the session, so a gate that runs outside a turn — a workflow
    /// assessment after the run completed — has nothing to read it from. Such
    /// a gate is as strict as the config says, and no stricter; the limitation
    /// is documented for operators in `docs/Help/Workflows/Index.md`.
    pub(crate) fn session_permission_config(&self, session_id: &str) -> Option<PermissionConfig> {
        let agent_permissions = self
            .session_manager
            .get_session(session_id)
            .and_then(|session| session.agent)
            .and_then(|agent| agent.agent_name)
            .and_then(|name| {
                let acp = self.acp_config.as_ref()?;
                let available = default_agent_profiles();
                resolve_agent_profile(&name, &acp.agents, &available)?.permissions
            });
        messaging::permission::resolve_effective_permission_config(
            None,
            agent_permissions,
            self.permission_config.clone(),
        )
    }
}
