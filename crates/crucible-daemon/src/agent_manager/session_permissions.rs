//! The `[permissions]` rules that apply to one session.
//!
//! Its own module because more than the agent dispatch path needs the answer.
//! A gate that runs with no agent and no prompt — a workflow's `## Validation`
//! command, when a run completes — still acts *for* a session, and reading the
//! daemon-global config there hands a session the operator locked down the
//! permissive global rules instead.

use super::AgentManager;
use crucible_core::config::components::permissions::{PermissionConfig, PermissionEngine};
use tracing::warn;

/// The mode of `session`. A session whose card names no mode runs `normal`.
pub(crate) fn session_mode(session: &crucible_core::session::Session) -> &str {
    session
        .agent
        .as_ref()
        .and_then(|a| a.mode.as_deref())
        .unwrap_or("normal")
}

impl AgentManager {
    /// Where the note writes of `session` go now. A running turn fixed its
    /// write mode when it started; between turns the session's mode decides.
    pub(crate) fn write_mode_for(
        &self,
        session: &crucible_core::session::Session,
    ) -> crate::tools::notes::TurnWriteMode {
        let id = session.id.as_str();
        if self.turn_running(id) {
            return self.slot(id).write_mode().clone();
        }
        let mode = crate::tools::notes::TurnWriteMode::default();
        mode.set(self.mode_writes(session_mode(session)));
        mode
    }

    /// Decide a Bases write that a plugin makes for `session`.
    ///
    /// The card, the mode and the operator rules apply as they do to an agent
    /// write. Nobody is prompted: inside a tool call that the gate already
    /// allowed, that grant answers what only a person could decide, as it
    /// does for the other writes the tool makes. Outside such a call the
    /// write is unattended, so a write that needs a prompt is refused.
    pub(crate) async fn bases_write_permission(
        &self,
        session: &crucible_core::session::Session,
        path: &str,
        content: Option<&str>,
    ) -> Result<(), String> {
        use super::messaging::gate_decision::{
            decide_nested, decide_permission, Decision, PermissionContext,
        };
        let engine = self.session_permission_engine(session.id.as_str());
        let slot = self.slot(session.id.as_str());
        let hooks = self.plugin_handlers();
        let context = PermissionContext {
            session_id: session.id.as_str(),
            tool_policy: session.agent.as_ref().and_then(|a| a.tool_policy.as_ref()),
            engine: &engine,
            permission_override: None,
            plugin: session.plugin.as_deref(),
            plugin_approval: session
                .plugin
                .as_deref()
                .map(|p| session.plugin_approval(p))
                .unwrap_or_default(),
            patterns: None,
            slot: Some(&slot),
            hooks: hooks.as_ref(),
            mode: session_mode(session),
            modes: &self.modes,
            mcp_read_only: &Default::default(),
            prompt: None,
        };
        let args = serde_json::json!({"path": path, "content": content});
        let call = crucible_core::types::CanonicalToolCall::crucible_tool("write_file", &args);
        let decision = if super::messaging::review_capture::call_allowed_in(session.id.as_str()) {
            decide_nested(&context, &call, &args)
        } else {
            decide_permission(&context, &call, &args).await
        };
        match decision {
            Decision::Allow(_) | Decision::UserAllowed => Ok(()),
            Decision::Deny(reason) => Err(reason),
            Decision::NoAnswer => Err("Bases write needs approval".into()),
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
        session_config(
            &self.session_manager,
            self.acp_config.as_ref(),
            self.permission_config.as_ref(),
            session_id,
        )
    }

    /// The engine of [`Self::session_permission_config`]. The tool gate of
    /// every agent kind reads it, so a profile's rules bind an internal
    /// agent as they bind an ACP agent.
    /// With no config at all, the engine still holds the hardcoded denies.
    pub(crate) fn session_permission_engine(&self, session_id: &str) -> PermissionEngine {
        PermissionEngine::new(self.session_permission_config(session_id).as_ref())
    }

    /// The inputs of [`Self::session_permission_engine`], for a holder that
    /// outlives one turn: the ACP gate of a cached agent handle.
    pub(crate) fn session_rules(&self) -> SessionRules {
        SessionRules {
            session_manager: self.session_manager.clone(),
            acp_config: self.acp_config.clone(),
            permission_config: self.permission_config.clone(),
        }
    }
}

/// What resolves the `[permissions]` rules of a session.
///
/// The ACP gate lives as long as its agent handle. It reads the rules from
/// this at each call, not once when the handle is built, so it applies the
/// rules that apply to the session now.
#[derive(Clone)]
pub(crate) struct SessionRules {
    session_manager: std::sync::Arc<crate::session_manager::SessionManager>,
    acp_config: Option<crucible_core::config::components::acp::AcpConfig>,
    permission_config: Option<PermissionConfig>,
}

impl SessionRules {
    /// The rules `config` for each session, with no agent profiles.
    #[cfg(test)]
    pub(crate) fn global(config: Option<PermissionConfig>) -> Self {
        Self {
            session_manager: crate::test_support::temp_session_manager(),
            acp_config: None,
            permission_config: config,
        }
    }

    /// The engine of the rules that apply to `session_id` now.
    pub(crate) fn engine(&self, session_id: &str) -> PermissionEngine {
        PermissionEngine::new(
            session_config(
                &self.session_manager,
                self.acp_config.as_ref(),
                self.permission_config.as_ref(),
                session_id,
            )
            .as_ref(),
        )
    }
}

/// The `[permissions]` block of the agent profile `name`, if it has one.
///
/// One lookup for every caller. A profile that does not resolve contributes no
/// permissions, which is safe because the same profile fails the launch —
/// no turn ever runs under it.
fn profile_permissions(
    acp_config: Option<&crucible_core::config::components::acp::AcpConfig>,
    name: &str,
) -> Option<PermissionConfig> {
    match crate::acp::discovery::profile(name, acp_config?) {
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

fn session_config(
    session_manager: &crate::session_manager::SessionManager,
    acp_config: Option<&crucible_core::config::components::acp::AcpConfig>,
    global: Option<&PermissionConfig>,
    session_id: &str,
) -> Option<PermissionConfig> {
    session_manager
        .get_session(session_id)
        .and_then(|session| session.agent)
        .and_then(|agent| agent.agent_name)
        .and_then(|name| profile_permissions(acp_config, &name))
        .or_else(|| global.cloned())
}
