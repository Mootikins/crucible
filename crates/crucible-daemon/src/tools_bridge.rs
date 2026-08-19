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

/// What a plugin is told when this daemon has no active-tool registry bound.
/// An error rather than a silent success: writing a set nothing reads would
/// leave the plugin believing it narrowed a session it did not.
const NO_ACTIVE_TOOL_SETS: &str =
    "this daemon has no agent manager, so it has no per-session tool sets to change";

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
    /// The registry behind `cru.tools.set_active` / `get_active`, with the
    /// sessions it is checked against.
    ///
    /// `None` in tests and in any embedding with no agent manager: a plugin
    /// calling `set_active` there is told so rather than writing into a
    /// registry nothing reads.
    active_tools: Option<ActiveToolBinding>,
}

/// The active-tool registry and the session table [`active_set_refusal`]
/// consults, bound together.
///
/// One field rather than two: a registry bound without a way to look a
/// session up would leave `set_active` unable to tell a live session from a
/// typo, which is the silent success this check exists to close.
struct ActiveToolBinding {
    sets: crate::tools::active_tools::ActiveToolSets,
    sessions: Arc<crate::session_manager::SessionManager>,
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
            active_tools: None,
        }
    }

    /// Bind the isolation registry so sandboxed sessions are honoured here too.
    pub fn with_isolation(mut self, isolation: crucible_lua::IsolationRegistry) -> Self {
        self.isolation = Some(isolation);
        self
    }

    /// Bind the manager's per-session active tool sets, so `set_active`
    /// writes the registry the agent handle and the dispatcher read.
    ///
    /// `sessions` is the same table the manager builds agents from; it is
    /// what tells `set_active` whether the session it names exists and
    /// whether the daemon assembles that session's tool list at all.
    pub fn with_active_tools(
        mut self,
        active_tools: crate::tools::active_tools::ActiveToolSets,
        sessions: Arc<crate::session_manager::SessionManager>,
    ) -> Self {
        self.active_tools = Some(ActiveToolBinding {
            sets: active_tools,
            sessions,
        });
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
                // Asked of the executor that will actually run the call rather
                // than asserted `Host` here. Same answer for all six workspace
                // tools, but a name this bridge cannot place answers `Unknown`
                // and is refused, instead of a second hand-maintained belief
                // about what sits behind the bridge. `WorkspaceTools::surface`
                // answers only for the tools it serves — it used to hand back
                // the whole built-in table, so a kiln tool's `Daemon` passed
                // this gate on the word of an executor that cannot run it.
                let surface = self.workspace_tools.surface(name);
                if isolation.host_execution_allowed(id, name, surface) {
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

    /// `Some(reason)` if this call must not run. See [`unattended_refusal`].
    fn refusal(&self, name: &str, args: &serde_json::Value) -> Option<String> {
        unattended_refusal(&self.permissions, name, args, "a Lua tool call")
    }
}

/// `Some(reason)` if a caller with nobody to prompt must not run `name`.
///
/// Same shape as the agent dispatch path, minus the prompt it cannot offer:
/// an operator `deny` is absolute; an `allow` runs; anything the rules leave
/// at `ask` falls back to the read-only exemption. Read-only tools go through,
/// as they do for an agent, and a tool that can mutate needs an explicit
/// `allow` — with nobody to ask, silently proceeding would hand the caller
/// exactly what a user would have been prompted about.
///
/// `caller` names the path in the refusal text, because there is more than one
/// such path: `cru.tools.call` (a plugin, through [`DaemonToolsBridge`]) and a
/// workflow note's `## Validation` command (`rpc/workflow_handlers.rs`). Both
/// take attacker-supplied text with no user attached, so both get one gate
/// rather than one each.
pub(crate) fn unattended_refusal(
    permissions: &PermissionEngine,
    name: &str,
    args: &serde_json::Value,
    caller: &str,
) -> Option<String> {
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
    match permissions.evaluate(name, &input, true) {
        PermissionDecision::Deny { reason } => return Some(reason),
        PermissionDecision::Allow => return None,
        // An `ask` rule names this tool on purpose. There is nobody to
        // prompt on this path, so being asked about it means refuse —
        // the read-only exemption below is for tools no rule decided.
        PermissionDecision::Ask { rule_matched: true } => {
            return Some(format!(
                "{name} is covered by an `ask` rule and {caller} has no prompt to answer it"
            ))
        }
        PermissionDecision::Ask { .. } => {}
    }
    if crate::agent_manager::is_safe(name) {
        return None;
    }
    Some(format!(
        "{name} can modify state and no allow rule covers it; \
         {caller} has no prompt to fall back on"
    ))
}

/// `Some(reason)` when `session` cannot carry an active tool set.
///
/// Both answers used to be a silent `Ok(())`, which is the worst outcome
/// available: the plugin reads success and believes it narrowed a session.
///
/// * **No such session.** Nothing will ever read the entry, and
///   `AgentManager::cleanup_session` — the only thing that clears one — never
///   runs for an id no session has, so the write also leaks a map entry for
///   the life of the daemon.
/// * **An ACP session.** Crucible does not assemble the tool list an external
///   agent offers its model. The agent brings its own file and shell tools;
///   the daemon serves it the Crucible surface over MCP *beside* them. An
///   active set would therefore narrow one half and leave the other whole,
///   which is not a control — a plugin that asked for `{"read_*"}` would get
///   a session whose agent still has its own `bash`. Say so instead.
///
/// Looked up in the live session table, not on disk: the set is read per
/// request by a running agent and dropped when the session is cleaned up, so
/// a session the daemon has not loaded has nothing to narrow.
pub(crate) fn active_set_refusal(
    sessions: &crate::session_manager::SessionManager,
    session: &str,
) -> Option<String> {
    let Some(found) = sessions.get_session(session) else {
        return Some(format!(
            "no active session '{session}' — cru.tools.set_active names the session to \
             narrow, which inside a hook is ctx.session_id"
        ));
    };
    let agent_type = found.agent.as_ref().map(|a| a.agent_type.as_str());
    if agent_type == Some("acp") {
        return Some(format!(
            "session {session} is delegated to an external ACP agent, which brings its \
             own tools — Crucible does not assemble that list, so cru.tools.set_active \
             cannot narrow it"
        ));
    }
    None
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

    fn set_active_tools(
        &self,
        session: String,
        patterns: Option<Vec<String>>,
    ) -> Result<(), String> {
        let bound = self.active_tools.as_ref().ok_or(NO_ACTIVE_TOOL_SETS)?;
        if let Some(reason) = active_set_refusal(&bound.sessions, &session) {
            return Err(reason);
        }
        match patterns {
            Some(patterns) => bound.sets.set(&session, patterns),
            None => bound.sets.clear(&session),
        }
        Ok(())
    }

    fn get_active_tools(&self, session: String) -> Result<Option<Vec<String>>, String> {
        let bound = self.active_tools.as_ref().ok_or(NO_ACTIVE_TOOL_SETS)?;
        Ok(bound.sets.get(&session))
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

    /// A session table holding one live session of `agent_type`, under the id
    /// the tests below name.
    fn sessions_with(
        id_out: &mut String,
        agent_type: &str,
    ) -> Arc<crate::session_manager::SessionManager> {
        let sessions = crate::test_support::temp_session_manager();
        let mut session =
            crucible_core::session::Session::new(crucible_core::session::SessionType::Chat, vec![]);
        // Through serde rather than a struct literal: `SessionAgent` has no
        // `Default` and this only needs the one field the check reads.
        session.agent = Some(
            serde_json::from_value(json!({
                "agent_type": agent_type,
                "provider": "ollama",
                "model": "llama3.2",
                "system_prompt": "",
                "env_overrides": {},
                "mcp_servers": [],
            }))
            .expect("session agent fixture"),
        );
        *id_out = session.id.to_string();
        sessions.register_transient(session);
        sessions
    }

    fn bridge_with_active_tools(
        sets: crate::tools::active_tools::ActiveToolSets,
        sessions: Arc<crate::session_manager::SessionManager>,
    ) -> DaemonToolsBridge {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            std::path::PathBuf::from("/tmp"),
        ));
        DaemonToolsBridge::new(workspace_tools, None).with_active_tools(sets, sessions)
    }

    /// `cru.tools.set_active` must write the registry the agent handle and the
    /// dispatcher read, not a private copy — the two halves were the whole
    /// point of binding it in `plugin_boot`.
    #[test]
    fn set_active_writes_the_bound_registry() {
        let mut id = String::new();
        let sessions = sessions_with(&mut id, "internal");
        let sets = crate::tools::active_tools::ActiveToolSets::new();
        let bridge = bridge_with_active_tools(sets.clone(), sessions);

        bridge
            .set_active_tools(id.clone(), Some(vec!["read_*".into()]))
            .unwrap();

        assert_eq!(sets.get(&id), Some(vec!["read_*".to_string()]));
        assert_eq!(
            bridge.get_active_tools(id).unwrap(),
            Some(vec!["read_*".to_string()])
        );
    }

    #[test]
    fn set_active_with_no_patterns_clears_the_registry() {
        let mut id = String::new();
        let sessions = sessions_with(&mut id, "internal");
        let sets = crate::tools::active_tools::ActiveToolSets::new();
        let bridge = bridge_with_active_tools(sets.clone(), sessions);
        bridge
            .set_active_tools(id.clone(), Some(vec!["read_*".into()]))
            .unwrap();

        bridge.set_active_tools(id.clone(), None).unwrap();

        assert_eq!(sets.get(&id), None);
        assert_eq!(bridge.get_active_tools(id).unwrap(), None);
    }

    /// A set written for a session that does not exist is read by nobody and
    /// cleared by nobody: `cleanup_session` only ever runs for a real session,
    /// so the entry outlives the daemon's interest in it. Answering `Ok(())`
    /// also told the plugin it had narrowed something.
    #[test]
    fn set_active_on_an_unknown_session_is_refused_and_writes_nothing() {
        let mut id = String::new();
        let sessions = sessions_with(&mut id, "internal");
        let sets = crate::tools::active_tools::ActiveToolSets::new();
        let bridge = bridge_with_active_tools(sets.clone(), sessions);

        let err = bridge
            .set_active_tools("no-such-session".into(), Some(vec!["read_*".into()]))
            .expect_err("a session that does not exist cannot be narrowed");

        assert!(err.contains("no active session"), "{err}");
        assert!(
            !sets.has_session("no-such-session"),
            "the refusal must not leave an entry behind"
        );
    }

    /// Crucible does not assemble the tool list an external agent offers its
    /// model, so an active set would narrow the MCP half and leave the
    /// agent's own tools whole. Reporting success for that is worse than
    /// refusing: the plugin reads a control it does not have.
    #[test]
    fn set_active_on_an_acp_session_is_refused_rather_than_silently_ignored() {
        let mut id = String::new();
        let sessions = sessions_with(&mut id, "acp");
        let sets = crate::tools::active_tools::ActiveToolSets::new();
        let bridge = bridge_with_active_tools(sets.clone(), sessions);

        let err = bridge
            .set_active_tools(id.clone(), Some(vec!["read_*".into()]))
            .expect_err("an ACP session's tool list is not Crucible's to narrow");

        assert!(err.contains("external ACP agent"), "{err}");
        assert!(
            !sets.has_session(&id),
            "the refusal must not leave an entry behind"
        );
    }

    /// An unbound registry answers with an error rather than pretending: a
    /// plugin that believes it narrowed a session it did not is worse off
    /// than one told the daemon cannot do it.
    #[test]
    fn an_unbound_registry_refuses_rather_than_pretending() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            std::path::PathBuf::from("/tmp"),
        ));
        let bridge = DaemonToolsBridge::new(workspace_tools, None);

        assert!(bridge
            .set_active_tools("s1".into(), Some(vec!["read_*".into()]))
            .is_err());
        assert!(bridge.get_active_tools("s1".into()).is_err());
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
                    exec: Default::default(),
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

    /// The gate must take its answer from the executor that would run the
    /// call, and `WorkspaceTools` runs six tools.
    ///
    /// It answered out of the whole built-in table instead, so asking it about
    /// `create_note` returned `Daemon` — the classification of a tool behind a
    /// completely different executor — and the isolation claim passed. Nothing
    /// escapes today only because execution then fails `NotFound` further
    /// down; the gate's verdict came from the wrong authority, which is one
    /// provider-list edit away from mattering.
    #[tokio::test]
    async fn a_tool_this_bridge_cannot_run_is_not_cleared_by_another_executors_surface() {
        let (bridge, _iso) = isolated_bridge(Some("s-sandboxed"));

        let err = bridge
            .call_tool(
                "create_note".to_string(),
                json!({"path": "x.md", "content": "hi"}),
                Some("s-sandboxed".to_string()),
            )
            .await
            .expect_err("a name this executor does not serve must not pass the gate");

        assert!(
            err.contains("Permission denied"),
            "the isolation gate must be what refuses this, not a downstream \
             NotFound: {err}"
        );
    }

    /// ...and the gate stays open for the tools it really does serve, on a
    /// session nobody claimed.
    #[tokio::test]
    async fn a_workspace_tool_still_clears_the_gate_on_an_unclaimed_session() {
        let (bridge, _iso) = isolated_bridge(Some("s-sandboxed"));
        let result = bridge
            .call_tool(
                "read_file".to_string(),
                json!({"path": "nope.txt"}),
                Some("s-free".to_string()),
            )
            .await;
        if let Err(e) = result {
            assert!(!e.contains("Permission denied"), "{e}");
        }
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
