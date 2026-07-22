//! Real ACP agent smoke tests for manual pre-demo verification.
//!
//! All tests are `#[ignore]`'d — they require real ACP agent binaries installed
//! and (for most agents) valid API credentials in the environment.
//!
//! # Running
//! ```bash
//! cargo nextest run -p crucible-daemon --test acp_real -- --run-ignored
//! ```

use crucible_core::config::{AcpConfig, AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_core::traits::chat::AgentHandle;
use crucible_core::turn::{Agent, TurnContext};
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crucible_daemon::agent_manager::AgentFactoryOverride;
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};

/// Binary names to look for in PATH for each known ACP agent.
/// Order matters — `find_available_agent` returns the first match.
const KNOWN_AGENTS: &[(&str, &str)] = &[
    ("opencode", "opencode"),
    ("claude", "npx"),
    ("gemini", "gemini"),
    ("codex", "npx"),
    ("cursor", "cursor-acp"),
];

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(binary))
        .find(|p| p.is_file())
}

/// Discover the first available ACP agent binary in PATH.
///
/// Checks agents in order: opencode, claude, gemini, codex, cursor.
/// Returns `Some((agent_name, binary_path))` for the first found, or `None`.
pub fn find_available_agent() -> Option<(String, PathBuf)> {
    for &(name, binary) in KNOWN_AGENTS {
        if let Some(path) = find_in_path(binary) {
            return Some((name.to_string(), path));
        }
    }
    None
}

/// Find two different available ACP agents for cross-agent delegation testing.
///
/// Returns `None` if fewer than 2 agents are discoverable.
pub fn find_delegation_pair() -> Option<((String, PathBuf), (String, PathBuf))> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for &(name, binary) in KNOWN_AGENTS {
        if let Some(path) = find_in_path(binary) {
            found.push((name.to_string(), path));
            if found.len() == 2 {
                let second = found.pop().unwrap();
                let first = found.pop().unwrap();
                return Some((first, second));
            }
        }
    }
    None
}

/// Create a [`SessionAgent`] configured for a real ACP agent.
///
/// Uses the standard agent name (e.g. `"opencode"`) so that
/// `resolve_agent_command()` maps it to the correct binary + args.
fn real_session_agent(agent_name: &str) -> SessionAgent {
    SessionAgent {
        mode: None,
        agent_type: "acp".to_string(),
        agent_name: Some(agent_name.to_string()),
        provider_key: None,
        provider: BackendType::Mock, // irrelevant for ACP agents
        model: "default".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
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
        tool_policy: None,
    }
}

async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    loop {
        if let Ok(event) = rx.recv().await {
            if event.event == event_name {
                return event;
            }
        }
    }
}

/// Agent-factory override that builds a delegated child session's agent as an
/// `AcpAgentHandle`. The child's `SessionAgent` (resolved from the named target
/// profile) supplies the agent name; the built-in command mapping resolves it
/// to a real binary, so `acp_config` here can stay `None`.
fn make_acp_agent_factory() -> AgentFactoryOverride {
    Box::new(move |agent_config: &SessionAgent, workspace: &Path| {
        let agent_config = agent_config.clone();
        let workspace = workspace.to_path_buf();
        Box::pin(async move {
            AcpAgentHandle::new(AcpAgentHandleParams {
                agent_config: &agent_config,
                workspace: &workspace,
                kiln_path: None,
                knowledge_repo: None,
                embedding_provider: None,
                background_spawner: None,
                delegation_spawner: None,
                parent_session_id: None,
                delegation_config: None,
                acp_config: None,
                permission_handler: None,
            })
            .await
            .map(|handle| Box::new(handle) as Box<dyn AgentHandle + Send + Sync>)
            .map_err(|e| e.to_string())
        })
            as Pin<
                Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
            >
    })
}

#[tokio::test]
#[ignore = "requires real ACP agent binary + credentials — run with --run-ignored"]
async fn real_agent_handshake_succeeds() {
    let (name, path) = match find_available_agent() {
        Some(pair) => pair,
        None => {
            eprintln!("SKIP: no real ACP agent binaries found in PATH");
            return;
        }
    };
    eprintln!("Using agent '{name}' (binary: {path:?})");

    let workspace = TempDir::new().expect("temp workspace");
    let agent_config = real_session_agent(&name);

    let handle = timeout(
        Duration::from_secs(60),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out (60s)")
    .expect("ACP handshake failed");

    // Handshake success implies a live connection; no further assertion needed.
    let _ = handle;
}

#[tokio::test]
#[ignore = "requires real ACP agent binary + credentials — run with --run-ignored"]
async fn real_agent_single_message() {
    let (name, path) = match find_available_agent() {
        Some(pair) => pair,
        None => {
            eprintln!("SKIP: no real ACP agent binaries found in PATH");
            return;
        }
    };
    eprintln!("Using agent '{name}' (binary: {path:?})");

    let workspace = TempDir::new().expect("temp workspace");
    let agent_config = real_session_agent(&name);

    let mut handle = timeout(
        Duration::from_secs(60),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out (60s)")
    .expect("ACP handshake failed");

    let events = timeout(Duration::from_secs(60), async {
        let stream = handle
            .turn(TurnContext::new("Reply with just the word 'hello'"))
            .await
            .expect("Agent::turn failed");
        stream.collect::<Vec<_>>().await
    })
    .await
    .expect("streaming response timed out (60s)");

    assert!(
        !events.is_empty(),
        "Expected at least one TurnEvent from real agent"
    );
}

#[tokio::test]
#[ignore = "requires 2+ real ACP agents + credentials — run with --run-ignored"]
async fn real_cross_agent_delegation() {
    let ((primary_name, primary_path), (secondary_name, secondary_path)) =
        match find_delegation_pair() {
            Some(pair) => pair,
            None => {
                eprintln!(
                    "SKIP: fewer than 2 real ACP agent binaries found in PATH — \
                     need 2 for cross-agent delegation"
                );
                return;
            }
        };
    eprintln!(
        "Primary: '{primary_name}' ({primary_path:?}), \
         Secondary: '{secondary_name}' ({secondary_path:?})"
    );

    let temp = TempDir::new().expect("temp dir");
    let (event_tx, mut rx) = broadcast::channel(256);

    // Register both agents as ACP profiles so the named delegation target
    // resolves through the AgentManager's acp_config.
    let mut acp_config = AcpConfig::default();
    acp_config.agents.insert(
        primary_name.clone(),
        AgentProfile {
            command: Some(primary_path.to_string_lossy().into_owned()),
            ..Default::default()
        },
    );
    acp_config.agents.insert(
        secondary_name.clone(),
        AgentProfile {
            command: Some(secondary_path.to_string_lossy().into_owned()),
            ..Default::default()
        },
    );

    let session_manager = Arc::new(SessionManager::with_storage(Arc::new(
        FileSessionStorage::new(),
    )));
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: Some(acp_config),
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&manager);
    manager.set_agent_factory_override(make_acp_agent_factory());

    // Parent = primary ACP agent, allowed to delegate to the secondary.
    let mut parent_agent = real_session_agent(&primary_name);
    parent_agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: Some(vec![secondary_name.clone()]),
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    });

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("parent session should be created");
    manager
        .configure_agent(&session.id, parent_agent)
        .await
        .expect("parent agent should be configured");
    let parent_id = session.id;

    let spawned = timeout(
        Duration::from_secs(120),
        service.spawn_delegation(DelegationRequest {
            parent_session_id: parent_id.clone(),
            prompt: "Say hello".to_string(),
            context: None,
            target_agent: Some(secondary_name.clone()),
            description: Some("cross-agent delegation smoke test".to_string()),
        }),
    )
    .await
    .expect("delegation spawn timed out (120s)")
    .expect("delegation spawn failed");
    let delegation_id = spawned.delegation_id.clone();

    let spawned_event = timeout(
        Duration::from_secs(120),
        next_event(&mut rx, "delegation_spawned"),
    )
    .await
    .expect("timed out waiting for delegation_spawned event");

    let completed_event = timeout(
        Duration::from_secs(120),
        next_event(&mut rx, "delegation_completed"),
    )
    .await
    .expect("timed out waiting for delegation_completed event");

    assert_eq!(
        spawned_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(
        completed_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
}
