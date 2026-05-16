use super::*;
use crucible_core::types::ToolSource;

const DEFAULT_MAX_TOOL_DEPTH: usize = 10;

mod permission;
mod send;
mod stream;
mod tool_call;

impl AgentManager {
    fn format_tool_source(source: &ToolSource) -> String {
        match source {
            ToolSource::Core => "Core".to_string(),
            ToolSource::Crucible => "Crucible".to_string(),
            ToolSource::Mcp { server } => format!("Mcp:{server}"),
            ToolSource::Plugin { name } => format!("Plugin:{name}"),
        }
    }

    pub async fn cancel(&self, session_id: &str) -> bool {
        // Drop any pending permission `oneshot::Sender`s for this session so
        // their receivers Err out immediately and any callers blocked inside
        // `PermissionSerializer::run` release the per-session lock. Without
        // this, partial cancel (user hits Esc) leaves prompts dangling for
        // the full 300 s timeout, blocking subsequent prompts behind them.
        let dropped_pending = self
            .pending_permissions
            .remove(session_id)
            .map(|(_, m)| m.len())
            .unwrap_or(0);
        if dropped_pending > 0 {
            debug!(
                session_id = %session_id,
                count = dropped_pending,
                "Dropped pending permission senders on cancel"
            );
        }

        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }

            if let Some(handle) = state.task_handle.take() {
                // Give task 500ms to respond to cancellation signal before force-aborting
                match tokio::time::timeout(std::time::Duration::from_millis(500), handle).await {
                    Ok(Ok(())) => debug!(session_id = %session_id, "Task completed gracefully"),
                    Ok(Err(e)) => warn!(session_id = %session_id, error = %e, "Task panicked"),
                    Err(_) => {
                        debug!(session_id = %session_id, "Task did not respond to cancellation, was aborted");
                    }
                }
            }

            info!(session_id = %session_id, "Request cancelled");
            true
        } else if dropped_pending > 0 {
            // No active request, but we did clear stale prompts.
            true
        } else {
            warn!(session_id = %session_id, "No active request to cancel");
            false
        }
    }
}

#[cfg(test)]
mod permission_override_tests {
    use super::permission::resolve_effective_permission_config;
    use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};

    fn base_config_with_ask_rule() -> PermissionConfig {
        PermissionConfig {
            default: PermissionMode::Ask,
            allow: Vec::new(),
            deny: Vec::new(),
            // Matches any input for the `Task` tool (glob `*` against JSON args).
            ask: vec!["Task:*".to_string()],
        }
    }

    #[test]
    fn allow_override_discards_base_ask_rules() {
        let base = base_config_with_ask_rule();
        let effective =
            resolve_effective_permission_config(Some(PermissionMode::Allow), None, Some(base))
                .expect("config produced");
        assert_eq!(effective.default, PermissionMode::Allow);
        assert!(effective.allow.is_empty());
        assert!(effective.deny.is_empty());
        assert!(
            effective.ask.is_empty(),
            "ask rules must be dropped so --permissions allow is unconditional"
        );
    }

    #[test]
    fn allow_override_discards_agent_specific_rules() {
        let mut agent = base_config_with_ask_rule();
        agent.deny = vec!["bash:rm *".to_string()];
        let effective =
            resolve_effective_permission_config(Some(PermissionMode::Allow), Some(agent), None)
                .expect("config produced");
        assert_eq!(effective.default, PermissionMode::Allow);
        assert!(effective.deny.is_empty());
        assert!(effective.ask.is_empty());
    }

    #[test]
    fn deny_override_discards_base_rules() {
        let mut base = base_config_with_ask_rule();
        base.allow = vec!["*:read_file".to_string()];
        let effective =
            resolve_effective_permission_config(Some(PermissionMode::Deny), None, Some(base))
                .expect("config produced");
        assert_eq!(effective.default, PermissionMode::Deny);
        assert!(effective.allow.is_empty());
        assert!(effective.deny.is_empty());
        assert!(effective.ask.is_empty());
    }

    #[test]
    fn ask_override_preserves_base_rules() {
        let mut base = base_config_with_ask_rule();
        base.allow = vec!["*:read_file".to_string()];
        let effective = resolve_effective_permission_config(
            Some(PermissionMode::Ask),
            None,
            Some(base.clone()),
        )
        .expect("config produced");
        assert_eq!(effective.default, PermissionMode::Ask);
        assert_eq!(effective.allow, base.allow);
        assert_eq!(effective.ask, base.ask);
    }

    #[test]
    fn no_override_falls_back_to_agent_then_global() {
        let agent = PermissionConfig {
            default: PermissionMode::Allow,
            ..Default::default()
        };
        let global = PermissionConfig {
            default: PermissionMode::Deny,
            ..Default::default()
        };

        let with_agent =
            resolve_effective_permission_config(None, Some(agent.clone()), Some(global.clone()))
                .expect("config produced");
        assert_eq!(with_agent.default, PermissionMode::Allow);

        let without_agent = resolve_effective_permission_config(None, None, Some(global.clone()))
            .expect("config produced");
        assert_eq!(without_agent.default, PermissionMode::Deny);

        let neither = resolve_effective_permission_config(None, None, None);
        assert!(neither.is_none());
    }

    #[tokio::test]
    async fn gate_with_allow_override_approves_tool_blocked_by_base_ask_rule() {
        use crate::permission_bridge::DaemonPermissionGate;
        use crucible_core::interaction::PermRequest;
        use crucible_core::traits::PermissionGate;

        let base = base_config_with_ask_rule();
        let effective =
            resolve_effective_permission_config(Some(PermissionMode::Allow), None, Some(base));
        let gate = DaemonPermissionGate::new(effective, false);

        let response = gate
            .request_permission(PermRequest::tool("Task", serde_json::json!({})))
            .await;
        assert!(
            response.allowed,
            "--permissions allow must approve tools even when base config has ask rules for them"
        );
    }

    #[tokio::test]
    async fn gate_with_deny_override_blocks_tool_allowed_by_base_allow_rule() {
        use crate::permission_bridge::DaemonPermissionGate;
        use crucible_core::interaction::PermRequest;
        use crucible_core::traits::PermissionGate;

        let base = PermissionConfig {
            default: PermissionMode::Allow,
            allow: vec!["Task:*".to_string()],
            ..Default::default()
        };

        let effective =
            resolve_effective_permission_config(Some(PermissionMode::Deny), None, Some(base));
        let gate = DaemonPermissionGate::new(effective, false);

        let response = gate
            .request_permission(PermRequest::tool("Task", serde_json::json!({})))
            .await;
        assert!(
            !response.allowed,
            "--permissions deny must block tools even when base config allows them"
        );
    }
}
