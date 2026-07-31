use crate::tools::workspace::WorkspaceTools;
use crucible_core::config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine,
};
use crucible_core::traits::tools::{ExecutionContext, ToolExecutor};
use crucible_lua::DaemonToolsApi;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type BoxFut<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

pub struct DaemonToolsBridge {
    workspace_tools: Arc<WorkspaceTools>,
    /// The operator's rules, applied to `cru.tools.call` before execution.
    ///
    /// This path has no agent and no prompt, so it cannot reuse the gate in
    /// `agent_manager/messaging/permission.rs` — but the rules are the same
    /// rules, and skipping them entirely is what made any loaded plugin able
    /// to run `bash` unprompted in any mode.
    permissions: PermissionEngine,
}

impl DaemonToolsBridge {
    pub fn new(
        workspace_tools: Arc<WorkspaceTools>,
        permission_config: Option<PermissionConfig>,
    ) -> Self {
        Self {
            workspace_tools,
            permissions: PermissionEngine::new(permission_config.as_ref()),
        }
    }

    /// `Some(reason)` if this call must not run.
    ///
    /// Same shape as the agent dispatch path, minus the prompt it cannot
    /// offer: an operator `deny` is absolute; an `allow` runs; anything the
    /// rules leave at `ask` falls back to the read-only exemption. Read-only
    /// tools go through, as they do for an agent, and a tool that can mutate
    /// needs an explicit `allow` — with nobody to ask, silently proceeding
    /// would hand a plugin exactly what a user would have been prompted about.
    fn refusal(&self, name: &str, args: &serde_json::Value) -> Option<String> {
        // `bash` is gated on the command itself — the hardcoded denies and
        // the rule patterns both match against it, not the JSON envelope.
        let input = if name == "bash" {
            args.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            args.to_string()
        };
        // `is_interactive: true` keeps `Ask` distinguishable from `Deny`;
        // this decides what an unmatched tool means, and it is not the same
        // answer as an operator writing `deny`.
        match self.permissions.evaluate(name, &input, true) {
            PermissionDecision::Deny { reason } => return Some(reason),
            PermissionDecision::Allow => return None,
            PermissionDecision::Ask => {}
        }
        if crate::agent_manager::is_safe(name) {
            return None;
        }
        Some(format!(
            "{name} can modify state and no allow rule covers it; \
             a Lua tool call has no prompt to fall back on"
        ))
    }
}

impl DaemonToolsApi for DaemonToolsBridge {
    fn call_tool(&self, name: String, args: serde_json::Value) -> BoxFut<serde_json::Value> {
        if let Some(reason) = self.refusal(&name, &args) {
            tracing::warn!(tool = %name, %reason, "refused cru.tools.call");
            return Box::pin(async move { Err(format!("Permission denied: {reason}")) });
        }
        let tools = Arc::clone(&self.workspace_tools);
        Box::pin(async move {
            let ctx = ExecutionContext::default();
            tools
                .execute_tool(&name, args, &ctx)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn list_tools(&self) -> BoxFut<Vec<serde_json::Value>> {
        let tools = Arc::clone(&self.workspace_tools);
        Box::pin(async move {
            let defs = tools.list_tools().await.map_err(|e| e.to_string())?;
            defs.into_iter()
                .map(|t| serde_json::to_value(&t).map_err(|e| e.to_string()))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::components::permissions::PermissionMode;
    use serde_json::json;

    #[test]
    fn test_daemon_tools_bridge_construction() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            std::path::PathBuf::from("/tmp"),
        ));

        let bridge = DaemonToolsBridge::new(workspace_tools.clone(), None);

        // Verify bridge was created (no panic)
        assert!(std::mem::size_of_val(&bridge) > 0);
    }

    #[test]
    fn test_daemon_tools_bridge_delegates_to_workspace_tools() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            std::path::PathBuf::from("/tmp"),
        ));

        let strong_count = Arc::strong_count(&workspace_tools);

        let _bridge = DaemonToolsBridge::new(workspace_tools.clone(), None);

        // Verify Arc reference is held (strong count increased)
        assert_eq!(Arc::strong_count(&workspace_tools), strong_count + 1);
    }

    fn bridge_with(config: PermissionConfig) -> DaemonToolsBridge {
        let tmp = tempfile::tempdir().expect("tempdir");
        DaemonToolsBridge::new(
            Arc::new(crate::tools::workspace::WorkspaceTools::new(
                tmp.path().to_path_buf(),
            )),
            Some(config),
        )
    }

    /// An operator deny rule must hold on this path too.
    ///
    /// `cru.tools.call` documents itself as respecting the session permission
    /// scope. It went straight to `WorkspaceTools::execute_tool` with an
    /// `ExecutionContext::default()`, so every check — config rules, hardcoded
    /// denies, plan mode — lived on the agent dispatch path this skips.
    #[tokio::test]
    async fn a_denied_tool_is_refused_through_the_lua_bridge() {
        let bridge = bridge_with(PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["bash:*".to_string()],
            ..Default::default()
        });

        let result = bridge
            .call_tool("bash".to_string(), json!({"command": "echo hi"}))
            .await;

        let err = result.expect_err("a denied tool must not execute");
        assert!(
            err.contains("Permission denied"),
            "the refusal should say why, got: {err}"
        );
    }

    /// Fails closed: a tool that can mutate needs an explicit allow.
    ///
    /// The default config is `default = ask`, so this is what an unconfigured
    /// daemon does. Before, any loaded plugin could write files or run `bash`
    /// unprompted, in any mode, including plan.
    #[tokio::test]
    async fn a_mutating_tool_needs_an_explicit_allow() {
        let bridge = bridge_with(PermissionConfig::default());

        let err = bridge
            .call_tool(
                "write_file".to_string(),
                json!({"path": "x.txt", "content": "hi"}),
            )
            .await
            .expect_err("there is no prompt path from a plugin call");

        assert!(err.contains("Permission denied"), "got: {err}");
    }

    /// Read-only tools still run under the default config — the same
    /// exemption the agent dispatch path gives them. The gate must not become
    /// a wall; `cru.tools.call` is useless if a plugin cannot read a file.
    #[tokio::test]
    async fn a_read_only_tool_still_executes_through_the_lua_bridge() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("note.txt"), "hello").expect("write");
        let bridge = DaemonToolsBridge::new(
            Arc::new(crate::tools::workspace::WorkspaceTools::new(
                tmp.path().to_path_buf(),
            )),
            Some(PermissionConfig::default()),
        );

        let result = bridge
            .call_tool("read_file".to_string(), json!({"path": "note.txt"}))
            .await;

        assert!(
            result.is_ok(),
            "a read-only tool must reach the executor, got: {result:?}"
        );
    }

    /// The read-only exemption does not outrank the operator.
    #[tokio::test]
    async fn an_operator_deny_beats_the_read_only_exemption() {
        let bridge = bridge_with(PermissionConfig {
            deny: vec!["read_file:*".to_string()],
            ..Default::default()
        });

        let err = bridge
            .call_tool("read_file".to_string(), json!({"path": "note.txt"}))
            .await
            .expect_err("an explicit deny is absolute");

        assert!(err.contains("Permission denied"), "got: {err}");
    }
}
