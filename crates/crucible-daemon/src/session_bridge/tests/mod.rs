use super::*;
use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::kiln_manager::KilnManager;
use crate::session_manager::SessionManager;
use crate::test_support::temp_session_manager;
mod create;
mod lifecycle;

use crucible_core::config::{BackendType, LlmConfig};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

/// The context a bridge runs against.
///
/// `data_home` is the test's own tempdir, never `crucible_home()`: a kiln-less
/// `create` falls back to it, and reading the developer's real `~/.crucible`
/// is the hermeticity failure that passes on CI and fails locally.
fn bridge_ctx(
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    data_home: &std::path::Path,
) -> Arc<RpcContext> {
    bridge_ctx_with_llm_config(session_manager, agent_manager, event_tx, data_home, None)
}

/// As [`bridge_ctx`], with the LLM config the create path reads.
///
/// Separate rather than a parameter on every call site because only the create
/// tests need one: `bridge_configure_agent_refuses_a_provider_the_attached_kiln_does_not_clear`
/// depends on the absence of a config resolving its provider to Cloud.
fn bridge_ctx_with_llm_config(
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    data_home: &std::path::Path,
    llm_config: Option<LlmConfig>,
) -> Arc<RpcContext> {
    Arc::new(RpcContext::for_test(
        Arc::new(KilnManager::new()),
        session_manager,
        agent_manager,
        Arc::new(crate::project_manager::ProjectManager::new(
            data_home.join("projects.json"),
        )),
        event_tx,
        llm_config,
        data_home.to_path_buf(),
    ))
}

fn build_test_agent_manager(session_manager: Arc<SessionManager>) -> Arc<AgentManager> {
    build_test_agent_manager_with_llm_config(session_manager, None)
}

fn build_test_agent_manager_with_llm_config(
    session_manager: Arc<SessionManager>,
    llm_config: Option<LlmConfig>,
) -> Arc<AgentManager> {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    }))
}

/// The LLM config the create-path fixtures resolve cards against.
///
/// Not `None`: `SessionAgent::from_card` maps a card's `specialty:` through
/// `llm_config.models`, so with no config every specialty card silently
/// inherits the base model and the mapping is untested.
fn bridge_llm_config() -> LlmConfig {
    let mut config = crate::test_fixtures::build_llm_config("ollama", BackendType::Ollama);
    config
        .models
        .insert("research".to_string(), "ollama/llama3.2-deep".to_string());
    config
}

fn make_test_agent(context_budget: Option<usize>) -> SessionAgent {
    SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: Some(0.7),
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    }
}

/// Agent that calls `bash` once and waits for the tool result.
///
/// `bash` is not believed read-only, so it reaches the permission gate —
/// which is exactly the decision a plugin-created turn has nobody to make.
struct BashCallingAgent;

#[async_trait::async_trait]
impl crucible_core::turn::Agent for BashCallingAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        use crucible_core::turn::{StopReason, TurnEvent};
        let mut inbound = ctx.inbound;
        let body = async_stream::stream! {
            yield TurnEvent::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                args: serde_json::json!({"command": "rm -rf /"}),
                diffs: Vec::new(),
            };
            yield TurnEvent::ToolBatchEnd;
            if let Some(rx) = inbound.as_mut() {
                while let Some(event) = rx.recv().await {
                    if matches!(event, TurnEvent::ToolResult { .. }) {
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
impl crucible_core::traits::chat::AgentHandle for BashCallingAgent {
    async fn send_message_fire_and_forget(
        &mut self,
        _: String,
    ) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "normal"
    }
}

/// A session whose single turn calls `bash`, driven through the real
/// scheduler and tool dispatch behind a [`DaemonSessionBridge`]. No
/// `[permissions]` config, so the gate has no rule to fall back on.
async fn bash_calling_rig(
    event_tx: broadcast::Sender<SessionEventMessage>,
) -> (TempDir, DaemonSessionBridge, String) {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().to_path_buf();
    let session_manager = temp_session_manager();
    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    }));
    agent_manager.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async {
            Ok(Box::new(BashCallingAgent)
                as Box<
                    dyn crucible_core::traits::chat::AgentHandle + Send + Sync,
                >)
        })
    }));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![workspace.clone()],
            Some(workspace.clone()),
            None,
        )
        .await
        .unwrap();
    agent_manager
        .configure_agent(&session.id, make_test_agent(None))
        .await
        .unwrap();
    let ctx = bridge_ctx(session_manager, agent_manager, event_tx, &workspace);
    let bridge = DaemonSessionBridge::new(ctx);
    (tmp, bridge, session.id.to_string())
}

/// A Discord (or any plugin) turn has no Crucible principal on the other
/// end, so a tool that would need approval must come back as a tool error
/// rather than as a prompt anyone in the room could answer.
#[tokio::test]
async fn a_collected_plugin_turn_errors_on_a_gated_tool_instead_of_prompting() {
    let (event_tx, _keep_open) = broadcast::channel(256);
    let (_tmp, bridge, session_id) = bash_calling_rig(event_tx).await;

    let mut rx = bridge
        .send_and_collect(session_id, "go".to_string(), Some(5.0), None, false)
        .await
        .unwrap();

    let mut parts = Vec::new();
    while let Ok(Some(part)) = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
        parts.push(part);
    }

    assert!(
        !parts
            .iter()
            .any(|p| matches!(p, ResponsePart::PermissionRequest { .. })),
        "a plugin turn must never surface a permission prompt, got: {parts:?}"
    );
    assert!(
        parts.iter().any(|p| matches!(
            p,
            ResponsePart::ToolResult {
                tool,
                is_error: true,
                ..
            } if tool == "bash"
        )),
        "the gated bash call must come back as a tool error, got: {parts:?}"
    );
}

/// Same guarantee on the fire-and-forget path, which has no part stream to
/// inspect: nothing may reach the broadcast that a subscriber could answer.
#[tokio::test]
async fn a_fire_and_forget_plugin_turn_never_broadcasts_a_permission_request() {
    let (event_tx, mut events) = broadcast::channel(256);
    let (_tmp, bridge, session_id) = bash_calling_rig(event_tx).await;

    bridge
        .send_message(session_id, "go".to_string())
        .await
        .unwrap();

    let mut errored_tool = None;
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(30), events.recv()).await {
        assert_ne!(
            event.event, "interaction_requested",
            "a plugin turn must never ask the broadcast for permission"
        );
        if event.event == "tool_result"
            && event
                .data
                .get("result")
                .and_then(|r| r.get("error"))
                .is_some()
        {
            errored_tool = event
                .data
                .get("tool")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        if event.event == "message_complete" || event.event == "ended" {
            break;
        }
    }

    assert_eq!(
        errored_tool.as_deref(),
        Some("bash"),
        "the gated bash call must come back as a tool error"
    );
}

/// The bridge's manager handles must be the context's, not merely managers of
/// the same type: `create` delegates to the context while every other method
/// uses the fields, and two `SessionManager`s would mean a plugin created
/// sessions one place and read them from another.
#[test]
fn bridge_managers_are_the_contexts_own() {
    let tmp = TempDir::new().unwrap();
    let (event_tx, _) = broadcast::channel(100);
    let session_manager = temp_session_manager();
    let agent_manager = build_test_agent_manager(session_manager.clone());
    let ctx = bridge_ctx(
        session_manager.clone(),
        agent_manager.clone(),
        event_tx,
        tmp.path(),
    );

    let bridge = DaemonSessionBridge::new(ctx.clone());

    assert!(Arc::ptr_eq(&bridge.session_manager, &ctx.sessions));
    assert!(Arc::ptr_eq(&bridge.agent_manager, &ctx.agents));
    assert!(Arc::ptr_eq(&bridge.ctx, &ctx));
}

#[test]
fn parse_range_accepts_known_types() {
    use crucible_core::traits::context_ops::Range;
    assert!(matches!(
        parse_range(&serde_json::json!({"type": "all"})).unwrap(),
        Range::All
    ));
    assert!(matches!(
        parse_range(&serde_json::json!({"type": "last", "n": 3})).unwrap(),
        Range::Last(3)
    ));
    assert!(matches!(
        parse_range(&serde_json::json!({"type": "first", "n": 2})).unwrap(),
        Range::First(2)
    ));
    match parse_range(&serde_json::json!({"type": "indices", "start": 1, "end": 4})).unwrap() {
        Range::Indices(r) => assert_eq!(r, 1..4),
        _ => panic!("expected Indices"),
    }
}

#[test]
fn parse_range_rejects_unknown_type() {
    let err = parse_range(&serde_json::json!({"type": "bogus"})).unwrap_err();
    assert!(err.contains("unknown range type"), "got: {err}");
}

#[test]
fn parse_range_requires_n_for_last_and_first() {
    let err = parse_range(&serde_json::json!({"type": "last"})).unwrap_err();
    assert!(err.contains("range.n required"), "got: {err}");
    let err = parse_range(&serde_json::json!({"type": "first"})).unwrap_err();
    assert!(err.contains("range.n required"), "got: {err}");
}

#[test]
fn parse_range_requires_start_end_for_indices() {
    let err = parse_range(&serde_json::json!({"type": "indices", "start": 0})).unwrap_err();
    assert!(err.contains("range.end required"), "got: {err}");
}

#[tokio::test]
async fn context_usage_returns_expected_shape() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, make_test_agent(Some(10_000)))
        .await
        .unwrap();

    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(
        session_manager.clone(),
        agent_manager.clone(),
        event_tx,
        tmp.path(),
    );
    let bridge = DaemonSessionBridge::new(ctx);

    let usage = bridge.context_usage(session.id.to_string()).await.unwrap();
    let obj = usage.as_object().expect("expected object");
    assert!(obj.contains_key("messages"));
    assert!(obj.contains_key("prompt_tokens"));
    assert!(obj.contains_key("budget"));
    assert!(obj.contains_key("percent"));
    assert_eq!(obj.get("budget").and_then(|v| v.as_u64()), Some(10_000));
    // No turn has run, so no tree, no tokens.
    assert_eq!(obj.get("messages").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(obj.get("prompt_tokens").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(obj.get("percent").and_then(|v| v.as_f64()), Some(0.0));
}

#[tokio::test]
async fn compact_transitions_session_to_compacting() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(
        session_manager.clone(),
        agent_manager.clone(),
        event_tx,
        tmp.path(),
    );
    let bridge = DaemonSessionBridge::new(ctx);

    bridge.compact(session.id.to_string()).await.unwrap();
    let after = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        format!("{}", after.state),
        "compacting",
        "session should be in compacting state after compact()"
    );
}

#[tokio::test]
async fn remove_messages_last_n_rewinds_tree() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, make_test_agent(None))
        .await
        .unwrap();

    // Seed the tree with three non-root nodes: User → Agent → User.
    let tree = agent_manager
        .get_or_rebuild_session_tree(&session.id, std::path::Path::new("/nonexistent"))
        .await;
    {
        let mut t = tree.lock().await;
        let root = t.root();
        let u1 = t.add_child_and_advance(
            root,
            crucible_core::turn::NodeContent::User { text: "u1".into() },
        );
        let a1 = t.add_child_and_advance(
            u1,
            crucible_core::turn::NodeContent::Agent { text: "a1".into() },
        );
        t.add_child_and_advance(
            a1,
            crucible_core::turn::NodeContent::User { text: "u2".into() },
        );
        // sanity: path has root + 3 nodes
        let cur = t.current();
        assert_eq!(t.path_to_here(cur).len(), 4);
    }

    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(session_manager.clone(), agent_manager, event_tx, tmp.path());
    let bridge = DaemonSessionBridge::new(ctx);

    let removed = bridge
        .remove_messages(
            session.id.to_string(),
            serde_json::json!({"type": "last", "n": 2}),
        )
        .await
        .unwrap();
    assert_eq!(removed, 2);

    let tree = bridge
        .agent_manager
        .get_session_tree(&session.id)
        .expect("tree should exist");
    let t = tree.lock().await;
    assert_eq!(
        t.path_to_here(t.current()).len(),
        2,
        "expected root + 1 surviving node"
    );
}

#[tokio::test]
async fn remove_messages_indices_truncates_from_start() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, make_test_agent(None))
        .await
        .unwrap();

    // Seed the tree with three non-root nodes.
    let tree = agent_manager
        .get_or_rebuild_session_tree(&session.id, std::path::Path::new("/nonexistent"))
        .await;
    {
        let mut t = tree.lock().await;
        let root = t.root();
        let u1 = t.add_child_and_advance(
            root,
            crucible_core::turn::NodeContent::User { text: "u1".into() },
        );
        let a1 = t.add_child_and_advance(
            u1,
            crucible_core::turn::NodeContent::Agent { text: "a1".into() },
        );
        t.add_child_and_advance(
            a1,
            crucible_core::turn::NodeContent::User { text: "u2".into() },
        );
    }

    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(session_manager.clone(), agent_manager, event_tx, tmp.path());
    let bridge = DaemonSessionBridge::new(ctx);

    let removed = bridge
        .remove_messages(
            session.id.to_string(),
            serde_json::json!({"type": "indices", "start": 1, "end": 3}),
        )
        .await
        .unwrap();
    assert_eq!(removed, 2);
}

/// Seeds a tree with two complete turns and verifies that
/// `bridge.undo` rewinds by one turn, that `can_undo`/`undo_depth`
/// reflect the new state, and that an `undo_history` lists what's
/// still undoable.
#[tokio::test]
async fn undo_rewinds_tree_and_emits_event() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, make_test_agent(None))
        .await
        .unwrap();

    // Seed two complete turns: User → Agent → User → Agent.
    let tree = agent_manager
        .get_or_rebuild_session_tree(&session.id, std::path::Path::new("/nonexistent"))
        .await;
    {
        let mut t = tree.lock().await;
        let root = t.root();
        let u1 = t.add_child_and_advance(
            root,
            crucible_core::turn::NodeContent::User { text: "u1".into() },
        );
        let a1 = t.add_child_and_advance(
            u1,
            crucible_core::turn::NodeContent::Agent { text: "a1".into() },
        );
        let u2 = t.add_child_and_advance(
            a1,
            crucible_core::turn::NodeContent::User { text: "u2".into() },
        );
        t.add_child_and_advance(
            u2,
            crucible_core::turn::NodeContent::Agent { text: "a2".into() },
        );
    }

    let (event_tx, mut event_rx) = broadcast::channel(16);
    let ctx = bridge_ctx(
        session_manager.clone(),
        agent_manager.clone(),
        event_tx,
        tmp.path(),
    );
    let bridge = DaemonSessionBridge::new(ctx);

    // Pre-undo state.
    assert!(bridge.can_undo(session.id.to_string()).await.unwrap());
    assert_eq!(bridge.undo_depth(session.id.to_string()).await.unwrap(), 2);
    let history = bridge.undo_history(session.id.to_string()).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["turn_index"].as_u64(), Some(0));
    assert_eq!(history[0]["messages_removed"].as_u64(), Some(2));
    assert_eq!(history[1]["turn_index"].as_u64(), Some(1));
    assert_eq!(history[1]["messages_removed"].as_u64(), Some(2));

    // Undo one turn.
    let turns_undone = bridge.undo(session.id.to_string(), 1).await.unwrap();
    assert_eq!(turns_undone, 1);

    // session_undo event should have been broadcast.
    let mut saw_event = false;
    while let Ok(msg) = event_rx.try_recv() {
        if msg.event == "session_undo" {
            assert_eq!(msg.data["turns_undone"].as_u64(), Some(1));
            assert_eq!(msg.data["messages_removed"].as_u64(), Some(2));
            saw_event = true;
            break;
        }
    }
    assert!(saw_event, "expected a session_undo broadcast event");

    // Post-undo state: depth dropped, one turn still undoable.
    assert_eq!(bridge.undo_depth(session.id.to_string()).await.unwrap(), 1);
    let history2 = bridge.undo_history(session.id.to_string()).await.unwrap();
    assert_eq!(history2.len(), 1);
    assert_eq!(history2[0]["turn_index"].as_u64(), Some(0));
}

#[tokio::test]
async fn remove_messages_invalid_range_type_errors() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = build_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, make_test_agent(None))
        .await
        .unwrap();

    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(session_manager, agent_manager, event_tx, tmp.path());
    let bridge = DaemonSessionBridge::new(ctx);

    let err = bridge
        .remove_messages(session.id.to_string(), serde_json::json!({"type": "bogus"}))
        .await
        .unwrap_err();
    assert!(err.contains("unknown range type"), "got: {err}");
}
