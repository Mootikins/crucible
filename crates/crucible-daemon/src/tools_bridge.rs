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
    /// Sessions a plugin claimed isolation over.
    ///
    /// This path executes workspace tools — `bash` included — with no agent
    /// and no session of its own, so the dispatcher's default-deny gate never
    /// sees it. Without this, plugin Lua could run `bash` on the host inside a
    /// session the user believed was containerized: the sandbox held for the
    /// agent and not for the plugins beside it.
    isolation: Option<crucible_lua::IsolationRegistry>,
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
            isolation: None,
        }
    }

    /// Bind the isolation registry so sandboxed sessions are honoured here too.
    pub fn with_isolation(mut self, isolation: crucible_lua::IsolationRegistry) -> Self {
        self.isolation = Some(isolation);
        self
    }

    /// `Some(reason)` if isolation forbids running `name` on the host.
    ///
    /// `session` is what the caller stated. Stated and sandboxed refuses;
    /// stated and unsandboxed proceeds. **Not stated refuses whenever any
    /// session is currently sandboxed** — the bridge cannot prove which
    /// session it is acting for, and "unproven" has to mean "no" here for the
    /// same reason a failed container start refuses the session rather than
    /// falling back to the host.
    ///
    /// Kiln tools never reach this path; `WorkspaceTools` is the only executor
    /// behind it, and every tool it serves is host-touching by definition.
    fn isolation_refusal(&self, name: &str, session: Option<&str>) -> Option<String> {
        let isolation = self.isolation.as_ref()?;
        match session {
            Some(id) => {
                // Everything behind this bridge is `WorkspaceTools`, so the
                // surface is `Host` by construction — there is no kiln tool
                // here that a `Daemon` surface would wave through.
                if isolation.host_execution_allowed(
                    id,
                    name,
                    crucible_core::traits::tools::ToolSurface::Host,
                ) {
                    return None;
                }
                let plugin = isolation
                    .get(id)
                    .map(|c| c.plugin)
                    .unwrap_or_else(|| "a plugin".into());
                Some(format!(
                    "session {id} is isolated by '{plugin}', so '{name}' may not run on the \
                     host through cru.tools.call"
                ))
            }
            None if isolation.any_claim() => Some(format!(
                "'{name}' would run on the host, and this call named no session while at \
                 least one session is isolated. Pass the session it is for — \
                 `cru.tools.call(name, args, {{ session = ctx.session_id }})` — so the \
                 sandbox can be checked"
            )),
            None => None,
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
            // An `ask` rule names this tool on purpose. There is nobody to
            // prompt on a Lua call, so being asked about it means refuse —
            // the read-only exemption below is for tools no rule decided.
            PermissionDecision::Ask { rule_matched: true } => {
                return Some(format!(
                    "{name} is covered by an `ask` rule and a Lua tool call has no prompt to \
                     answer it"
                ))
            }
            PermissionDecision::Ask { .. } => {}
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
    fn call_tool(
        &self,
        name: String,
        args: serde_json::Value,
        session: Option<String>,
    ) -> BoxFut<serde_json::Value> {
        if let Some(reason) = self.isolation_refusal(&name, session.as_deref()) {
            tracing::warn!(tool = %name, %reason, "refused cru.tools.call: isolated session");
            return Box::pin(async move { Err(format!("Permission denied: {reason}")) });
        }
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

    fn isolated_bridge(
        session: Option<&str>,
    ) -> (DaemonToolsBridge, crucible_lua::IsolationRegistry) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let isolation = crucible_lua::IsolationRegistry::new();
        if let Some(id) = session {
            isolation.claim(
                id,
                crucible_lua::IsolationClaim {
                    plugin: "oci".to_string(),
                    exempt: Default::default(),
                    exec_prefix: Vec::new(),
                },
            );
        }
        // Permissions wide open, so any refusal below is the isolation one.
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            ..Default::default()
        };
        let bridge = DaemonToolsBridge::new(
            Arc::new(crate::tools::workspace::WorkspaceTools::new(
                tmp.path().to_path_buf(),
            )),
            Some(config),
        )
        .with_isolation(isolation.clone());
        (bridge, isolation)
    }

    /// `cru.tools.call` runs workspace tools with no agent and no session, so
    /// the dispatcher's default-deny gate never sees it. A sandboxed session's
    /// plugins could therefore run `bash` on the host beside an agent that
    /// could not — the sandbox held for the agent and not for the plugins.
    #[tokio::test]
    async fn a_sandboxed_session_cannot_reach_the_host_through_cru_tools_call() {
        let (bridge, _iso) = isolated_bridge(Some("s-sandboxed"));
        let err = bridge
            .call_tool(
                "bash".to_string(),
                json!({"command": "echo hi"}),
                Some("s-sandboxed".to_string()),
            )
            .await
            .expect_err("an isolated session's plugin must not run bash on the host");
        assert!(err.contains("isolated") && err.contains("oci"), "{err}");
    }

    /// Isolation is per session, so an unsandboxed one is unaffected.
    #[tokio::test]
    async fn an_unsandboxed_session_still_reaches_the_host() {
        let (bridge, _iso) = isolated_bridge(Some("s-other"));
        // Asserted on the gate, not on the run: whether `bash` succeeds in a
        // bare tempdir is not what this covers.
        let result = bridge
            .call_tool(
                "bash".to_string(),
                json!({"command": "echo hi"}),
                Some("s-free".to_string()),
            )
            .await;
        if let Err(e) = result {
            assert!(
                !e.contains("Permission denied"),
                "a session nobody claimed must not be refused: {e}"
            );
        }
    }

    /// The bridge cannot prove which session it acts for, so silence has to
    /// mean "no" while anything is sandboxed — the same fail-closed stance as
    /// refusing a session whose container would not start.
    #[tokio::test]
    async fn a_call_naming_no_session_is_refused_while_any_session_is_sandboxed() {
        let (bridge, _iso) = isolated_bridge(Some("s-sandboxed"));
        let err = bridge
            .call_tool("bash".to_string(), json!({"command": "echo hi"}), None)
            .await
            .expect_err("unproven must not mean safe while a sandbox is live");
        assert!(err.contains("named no session"), "{err}");
    }

    /// ...but with nothing sandboxed, an unstated session is the ordinary case
    /// and must not be penalised.
    #[tokio::test]
    async fn a_call_naming_no_session_runs_when_nothing_is_sandboxed() {
        let (bridge, _iso) = isolated_bridge(None);
        let result = bridge
            .call_tool("bash".to_string(), json!({"command": "echo hi"}), None)
            .await;
        if let Err(e) = result {
            assert!(
                !e.contains("Permission denied"),
                "with nothing sandboxed, an unstated session must not be refused: {e}"
            );
        }
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
            .call_tool("bash".to_string(), json!({"command": "echo hi"}), None)
            .await;

        let err = result.expect_err("a denied tool must not execute");
        assert!(
            err.contains("Permission denied"),
            "the refusal should say why, got: {err}"
        );
    }

    /// An `ask` rule the operator wrote is not the same as "no rule matched".
    ///
    /// The read-only exemption ran on any undecided tool, and the engine
    /// reports both "default is ask" and "an `ask` rule matched" as the same
    /// `Ask` — so `ask = ["read_file:*"]` was discarded exactly the way
    /// `deny` used to be. There is nobody to prompt on this path, so an
    /// operator asking to be asked means refuse.
    #[tokio::test]
    async fn an_ask_rule_is_not_discarded_by_the_read_only_exemption() {
        let bridge = bridge_with(PermissionConfig {
            default: PermissionMode::Allow,
            ask: vec!["read_file:*".to_string()],
            ..Default::default()
        });

        let err = bridge
            .call_tool("read_file".to_string(), json!({"path": "x"}), None)
            .await
            .expect_err("an explicit ask must not be silently allowed");

        assert!(err.contains("Permission denied"), "got: {err}");
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
                None,
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
            .call_tool("read_file".to_string(), json!({"path": "note.txt"}), None)
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
            .call_tool("read_file".to_string(), json!({"path": "note.txt"}), None)
            .await
            .expect_err("an explicit deny is absolute");

        assert!(err.contains("Permission denied"), "got: {err}");
    }
}
