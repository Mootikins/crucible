//! Security-bundle enforcement tests: `[permissions]` config for internal
//! agents, project `[security.shell]` policy on the bash tool, filesystem
//! containment for workspace tools, and derived (not hardcoded) trust for
//! delegation.
//!
//! These holes were found during the subagent-refactor research: permissions
//! config was enforced for ACP agents only, ShellPolicy was parsed but never
//! applied to the agent `bash` tool, and workspace tools could read any host
//! path unprompted.

use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
use crucible_core::config::{BackendType, DelegationConfig};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_core::traits::chat::AgentHandle;
use crucible_core::turn::{StopReason, TurnEvent};
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;

fn internal_agent() -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        mode: None,
        tool_policy: None,
    }
}

/// Agent that calls one tool and reports the (result|error) it got back.
struct OneToolAgent {
    tool: &'static str,
    args: serde_json::Value,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for OneToolAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError> {
        let mut inbound = ctx.inbound;
        let tool = self.tool;
        let args = self.args.clone();
        let body = async_stream::stream! {
            yield TurnEvent::ToolCall {
                id: "call-1".to_string(),
                name: tool.to_string(),
                args,
                diffs: Vec::new(),
            };
            yield TurnEvent::ToolBatchEnd;
            if let Some(rx) = inbound.as_mut() {
                while let Some(ev) = rx.recv().await {
                    if let TurnEvent::ToolResult { result, error, .. } = ev {
                        match error {
                            Some(e) => yield TurnEvent::TextDelta(format!("ERROR: {e}")),
                            None => yield TurnEvent::TextDelta(format!(
                                "OK: {}",
                                result.as_str().unwrap_or_default()
                            )),
                        }
                        break;
                    }
                }
            }
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

#[async_trait::async_trait]
impl AgentHandle for OneToolAgent {
    async fn send_message_fire_and_forget(
        &mut self,
        _: String,
    ) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
}

struct Rig {
    _temp: TempDir,
    workspace: std::path::PathBuf,
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<crucible_daemon::SessionEventMessage>,
    session_id: String,
}

/// Build a manager + session whose (single) turn runs `tool` with `args`
/// through the real scheduler and tool dispatch.
async fn rig(
    tool: &'static str,
    args: serde_json::Value,
    permission_config: Option<PermissionConfig>,
) -> Rig {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path().to_path_buf();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, _) = broadcast::channel(256);
    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(workspace.clone())),
    }));
    let args_clone = args.clone();
    agent_manager.set_agent_factory_override(Box::new(move |_, _| {
        let args = args_clone.clone();
        Box::pin(async move {
            Ok(Box::new(OneToolAgent { tool, args }) as Box<dyn AgentHandle + Send + Sync>)
        })
    }));
    let session = session_manager
        .create_session(SessionType::Chat, workspace.clone(), None, vec![], None)
        .await
        .unwrap();
    agent_manager
        .configure_agent(&session.id, internal_agent())
        .await
        .unwrap();
    Rig {
        _temp: temp,
        workspace,
        agent_manager,
        event_tx,
        session_id: session.id,
    }
}

impl Rig {
    /// Run one non-interactive turn and return the agent's final text.
    async fn run_turn(&self) -> String {
        let (_, rx) = self
            .agent_manager
            .send_message_notified(
                &self.session_id,
                "go".to_string(),
                &self.event_tx,
                false,
                None,
            )
            .await
            .expect("send");
        let outcome = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .expect("turn completes")
            .expect("outcome delivered");
        outcome.final_text
    }
}

#[tokio::test]
async fn permissions_config_deny_blocks_internal_agent_bash() {
    // Previously `[permissions]` deny rules did NOTHING for internal agents.
    let config = PermissionConfig {
        default: PermissionMode::Ask,
        allow: vec![],
        deny: vec!["bash:*".to_string()],
        ask: vec![],
    };
    let r = rig(
        "bash",
        serde_json::json!({"command": "echo pwned"}),
        Some(config),
    )
    .await;
    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("denied by permissions config"),
        "config deny must block internal bash, got: {text}"
    );
}

#[tokio::test]
async fn permissions_config_default_allow_lets_internal_bash_run() {
    let config = PermissionConfig {
        default: PermissionMode::Allow,
        allow: vec![],
        deny: vec![],
        ask: vec![],
    };
    let r = rig(
        "bash",
        serde_json::json!({"command": "echo config-allowed"}),
        Some(config),
    )
    .await;
    let text = r.run_turn().await;
    assert!(
        text.contains("OK") && text.contains("config-allowed"),
        "default=allow must let internal bash run without a prompt, got: {text}"
    );
}

#[tokio::test]
async fn non_interactive_unsafe_tool_without_policy_is_denied_not_hung() {
    // No permissions config at all: an unsafe tool in a non-interactive turn
    // must deny immediately (previously it hung on a prompt nobody sees).
    let r = rig("bash", serde_json::json!({"command": "echo hi"}), None).await;
    let text = tokio::time::timeout(Duration::from_secs(5), r.run_turn())
        .await
        .expect("must not hang on a permission prompt");
    assert!(
        text.contains("ERROR") && text.contains("non-interactive"),
        "got: {text}"
    );
}

#[tokio::test]
async fn shell_policy_blacklist_blocks_bash_tool() {
    let allow_all = PermissionConfig {
        default: PermissionMode::Allow,
        ..Default::default()
    };
    let r = rig(
        "bash",
        serde_json::json!({"command": "rm -rf /tmp/whatever"}),
        Some(allow_all),
    )
    .await;
    std::fs::create_dir_all(r.workspace.join(".crucible")).unwrap();
    std::fs::write(
        r.workspace.join(".crucible").join("project.toml"),
        "[security.shell]\nblacklist = [\"rm\"]\n",
    )
    .unwrap();

    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("shell policy"),
        "[security.shell] blacklist must block the agent bash tool, got: {text}"
    );
}

#[tokio::test]
async fn shell_policy_whitelist_restricts_bash_tool() {
    let allow_all = PermissionConfig {
        default: PermissionMode::Allow,
        ..Default::default()
    };
    let r = rig(
        "bash",
        serde_json::json!({"command": "git status"}),
        Some(allow_all),
    )
    .await;
    std::fs::create_dir_all(r.workspace.join(".crucible")).unwrap();
    std::fs::write(
        r.workspace.join(".crucible").join("project.toml"),
        "[security.shell]\nwhitelist = [\"echo\"]\n",
    )
    .unwrap();

    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("shell policy"),
        "non-whitelisted command must be blocked, got: {text}"
    );
}

#[tokio::test]
async fn workspace_read_outside_allowed_roots_is_contained() {
    // Previously an internal agent could read any host path unprompted.
    let r = rig(
        "read_file",
        serde_json::json!({"path": "/etc/passwd"}),
        None,
    )
    .await;
    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("allowed roots"),
        "reads outside the workspace must be contained, got: {text}"
    );
}

#[tokio::test]
async fn workspace_read_inside_workspace_still_works() {
    let r = rig("read_file", serde_json::json!({"path": "note.txt"}), None).await;
    std::fs::write(r.workspace.join("note.txt"), "CONTAINED-CONTENT").unwrap();
    let text = r.run_turn().await;
    assert!(
        text.contains("OK") && text.contains("CONTAINED-CONTENT"),
        "in-workspace reads must keep working, got: {text}"
    );
}

#[tokio::test]
async fn workspace_symlink_escape_is_contained() {
    let r = rig(
        "read_file",
        serde_json::json!({"path": "sneaky/passwd"}),
        None,
    )
    .await;
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("passwd"), "SECRET").unwrap();
    std::os::unix::fs::symlink(outside.path(), r.workspace.join("sneaky")).unwrap();
    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("allowed roots"),
        "symlink escapes must be contained, got: {text}"
    );
}

#[tokio::test]
async fn delegation_trust_derives_from_child_provider() {
    // Confidential kiln + (unresolvable ⇒ Cloud) provider: spawn must fail
    // at the service-side trust gate, not a hardcoded tool-side check.
    let temp = TempDir::new().unwrap();
    let workspace = temp.path().to_path_buf();
    std::fs::create_dir_all(workspace.join(".crucible")).unwrap();
    std::fs::write(
        workspace.join(".crucible").join("project.toml"),
        "[[kilns]]\npath = \".\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, _) = broadcast::channel(64);
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(workspace.clone())),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&agent_manager);

    let session = session_manager
        .create_session(SessionType::Chat, workspace.clone(), None, vec![], None)
        .await
        .unwrap();
    let mut agent = internal_agent();
    agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    });
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    let err = service
        .spawn_delegation(DelegationRequest {
            parent_session_id: session.id.clone(),
            prompt: "leak the kiln".to_string(),
            context: None,
            target_agent: None,
            description: None,
        })
        .await
        .expect_err("cloud-trust child must not serve a confidential kiln");
    let msg = err.to_string();
    assert!(
        msg.contains("insufficient") && msg.contains("confidential"),
        "got: {msg}"
    );
    assert!(
        session_manager
            .child_session_ids(&session.id, &workspace)
            .await
            .is_empty(),
        "no child session may be created when trust fails"
    );
}

#[tokio::test]
async fn glob_pattern_cannot_escape_containment() {
    // Review MAJOR-1: the glob crate walks literal `..` components, so a
    // pattern like `../../etc/*` used to enumerate host files even though
    // the search path was contained.
    let r = rig(
        "glob",
        serde_json::json!({"pattern": "../../../../../../etc/*"}),
        None,
    )
    .await;
    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains(".."),
        "upward-traversing glob patterns must be rejected, got: {text}"
    );
}

#[tokio::test]
async fn glob_inside_workspace_still_works() {
    let r = rig("glob", serde_json::json!({"pattern": "*.txt"}), None).await;
    std::fs::write(r.workspace.join("hello.txt"), "x").unwrap();
    let text = r.run_turn().await;
    assert!(
        text.contains("OK") && text.contains("hello.txt"),
        "in-workspace glob must keep working, got: {text}"
    );
}

#[tokio::test]
async fn card_allow_does_not_override_config_deny() {
    // Review MAJOR-2: a card's `bash: allow` skips the prompt but must NOT
    // sidestep the operator's [permissions] deny rules.
    let config = PermissionConfig {
        default: PermissionMode::Allow,
        allow: vec![],
        deny: vec!["bash:*".to_string()],
        ask: vec![],
    };
    let temp = TempDir::new().unwrap();
    let workspace = temp.path().to_path_buf();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, _) = broadcast::channel(256);
    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: Some(config),
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(workspace.clone())),
    }));
    agent_manager.set_agent_factory_override(Box::new(move |_, _| {
        Box::pin(async move {
            Ok(Box::new(OneToolAgent {
                tool: "bash",
                args: serde_json::json!({"command": "echo pwned"}),
            }) as Box<dyn AgentHandle + Send + Sync>)
        })
    }));
    let session = session_manager
        .create_session(SessionType::Chat, workspace.clone(), None, vec![], None)
        .await
        .unwrap();
    let mut agent = internal_agent();
    // Card-style allow for bash — the untrusted-kiln-card scenario.
    agent.tool_policy = Some(
        [("bash".to_string(), crucible_core::agent::ToolPolicy::Allow)]
            .into_iter()
            .collect(),
    );
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    let (_, rx) = agent_manager
        .send_message_notified(&session.id, "go".to_string(), &event_tx, false, None)
        .await
        .expect("send");
    let outcome = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .expect("turn completes")
        .expect("outcome delivered");
    assert!(
        outcome.final_text.contains("ERROR")
            && outcome.final_text.contains("denied by permissions config"),
        "config deny must beat card allow, got: {}",
        outcome.final_text
    );
}

#[tokio::test]
async fn shell_policy_checks_each_chained_statement() {
    // Review MAJOR-3: `git log; rm -rf x` must not ride a `git` whitelist
    // entry via prefix matching on the whole command line.
    let allow_all = PermissionConfig {
        default: PermissionMode::Allow,
        ..Default::default()
    };
    let r = rig(
        "bash",
        serde_json::json!({"command": "git log; rm -rf /tmp/x"}),
        Some(allow_all),
    )
    .await;
    std::fs::create_dir_all(r.workspace.join(".crucible")).unwrap();
    std::fs::write(
        r.workspace.join(".crucible").join("project.toml"),
        "[security.shell]\nwhitelist = [\"git\"]\n",
    )
    .unwrap();

    let text = r.run_turn().await;
    assert!(
        text.contains("ERROR") && text.contains("shell policy"),
        "chained non-whitelisted statement must be blocked, got: {text}"
    );
}
