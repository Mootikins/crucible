//! The `[permissions]` rules that apply to one session.
//!
//! Its own module because more than the agent dispatch path needs the answer.
//! A gate that runs with no agent and no prompt — a workflow's `## Validation`
//! command, when a run completes — still acts *for* a session, and reading the
//! daemon-global config there hands a session the operator locked down the
//! permissive global rules instead.

use super::{messaging, AgentManager};
use crucible_core::config::components::permissions::PermissionConfig;
use tracing::warn;

impl AgentManager {
    /// The `[permissions]` block of the agent profile `name`, if it has one.
    ///
    /// One lookup for both callers: this module and the agent dispatch path in
    /// `messaging::send`. A profile that does not resolve contributes no
    /// permissions, which is safe because the same profile fails the launch —
    /// no turn ever runs under it.
    pub(crate) fn agent_profile_permissions(&self, name: &str) -> Option<PermissionConfig> {
        let config = self.acp_config.as_ref()?;
        match crate::acp::discovery::profile(name, config) {
            Ok(resolved) => resolved?.permissions,
            Err(error) => {
                warn!(
                    agent = %name,
                    %error,
                    "agent profile does not resolve; applying the global permission rules"
                );
                None
            }
        }
    }

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
            .and_then(|name| self.agent_profile_permissions(&name));
        messaging::permission::resolve_effective_permission_config(
            None,
            agent_permissions,
            self.permission_config.clone(),
        )
    }
}
