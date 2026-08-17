//! A3: what a delegated agent's `usage_update` becomes at the daemon boundary.
//!
//! `turn_event_parity.rs` pins the ACP half — the frame becomes a
//! `TurnEvent::ContextWindow`. This pins the other half, and it is the half
//! that decides whether the statusline lights up: the daemon must turn that
//! event into the `SessionEventMessage`s the TUI already knows how to render,
//! with **no client change at all**.
//!
//! The two events involved, and the client code that already consumes them:
//!
//! | event                    | `session_event_to_chat_msgs` arm  | effect                       |
//! |--------------------------|-----------------------------------|------------------------------|
//! | `context_limit_resolved` | → `ChatAppMsg::ContextLimitResolved` | sets `context_total`      |
//! | `message_complete`       | → `ChatAppMsg::ContextUsage`         | sets `context_used`       |
//!
//! Both arms predate this work (`chat_runner/commands.rs`,
//! `chat_app/message_handlers.rs`). Reusing them rather than inventing an
//! ACP-shaped event is the whole point: `context_label`
//! (`components/status_items.rs`) needs `total > 0` to draw a percentage, and
//! it gets it from the same event the internal agent's provider lookup emits.
//!
//! These drive a real `AgentManager` over a real spawned `mock-acp-agent`, so
//! the chain under test is the production one end to end: ACP wire frame →
//! `acp/client/streaming.rs` → `StreamingChunk` → `acp_handle.rs` →
//! `TurnEvent` → `agent_manager/messaging/stream.rs` → broadcast.
use crucible_daemon::test_support::temp_session_manager;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::protocol::session_events::{ContextLimitResolvedPayload, ContextLimitSource};
use crucible_core::session::{SessionAgent, SessionType};
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::support::{mock_agent_path, mock_session_agent};

/// Run one delegated turn and return every event the daemon broadcast.
///
/// `env` is layered onto the session agent's `env_overrides`, which is how a
/// spawned mock is scripted — the binary cannot be handed an in-process config,
/// and mutating this process's environment would race the rest of the suite.
async fn delegated_turn_events(env: &[(&str, &str)]) -> Vec<SessionEventMessage> {
    let temp = TempDir::new().expect("temp workspace");
    let session_manager = temp_session_manager();
    let (event_tx, mut event_rx) = broadcast::channel(256);

    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    });

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crucible_daemon::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .expect("create session");

    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let agent = SessionAgent {
        env_overrides: env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>(),
        ..mock_session_agent(&agent_path)
    };
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .expect("configure delegated agent");

    let (_message_id, completion) = agent_manager
        .send_message_notified(
            &session.id,
            "what is 2+2?".to_string(),
            &event_tx,
            true,
            None,
        )
        .await
        .expect("send_message_notified");

    timeout(Duration::from_secs(30), completion)
        .await
        .expect("delegated turn timed out")
        .expect("turn completion channel dropped");

    let mut events = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        events.push(event);
    }
    events
}

/// Decode the payload of the one `context_limit_resolved` event, if any.
///
/// Decoded with the same typed payload the CLI decodes it with, so a field
/// this test reads but `crucible-cli` cannot would fail here too.
fn context_limit(events: &[SessionEventMessage]) -> Option<ContextLimitResolvedPayload> {
    let mut found: Vec<ContextLimitResolvedPayload> = events
        .iter()
        .filter(|e| e.event == "context_limit_resolved")
        .map(|e| {
            serde_json::from_value(e.data.clone()).unwrap_or_else(|err| {
                panic!("context_limit_resolved payload the CLI cannot decode: {err}")
            })
        })
        .collect();
    assert!(
        found.len() <= 1,
        "one usage_update must not produce {} limit events",
        found.len()
    );
    found.pop()
}

/// `message_complete`'s `total_tokens`, which the CLI turns into
/// `ChatAppMsg::ContextUsage { used, .. }`.
fn context_used(events: &[SessionEventMessage]) -> Option<u64> {
    events
        .iter()
        .find(|e| e.event == "message_complete")
        .and_then(|e| e.data.get("total_tokens").and_then(|v| v.as_u64()))
}

/// The whole of A3, at the boundary that decides it.
///
/// An ACP session can never reach the internal path: `context_limit_resolved`
/// is emitted from the session setup task only when the agent has both an
/// `endpoint` and a non-empty `model` to query, and a delegated agent has
/// neither. So before this, `context_total` stayed 0 for the life of the
/// session and the indicator drew its no-data path (US-205).
#[tokio::test]
async fn a_delegated_usage_update_resolves_the_session_context_limit() {
    let events = delegated_turn_events(&[
        ("CRU_MOCK_STREAM_CHUNKS", "The answer is 4"),
        ("CRU_MOCK_USAGE_UPDATE", "22700/1000000"),
    ])
    .await;

    let payload = context_limit(&events).unwrap_or_else(|| {
        panic!(
            "no context_limit_resolved event; a delegated session still has no \
             context limit. Events: {:?}",
            events.iter().map(|e| &e.event).collect::<Vec<_>>()
        )
    });

    assert_eq!(payload.limit, 1_000_000, "context window size");
    assert_eq!(
        payload.source,
        ContextLimitSource::Agent,
        "the window came from the agent, not a provider API lookup"
    );
}

/// Both operands reach the client, or the indicator is worse than useless.
///
/// `context_label` divides `used` by `total`. A limit with no occupancy draws
/// a confident "0% ctx"; an occupancy with no limit draws a bare token count.
/// The pair is the contract, so it is asserted as a pair.
#[tokio::test]
async fn a_delegated_turn_reports_both_context_operands() {
    let events = delegated_turn_events(&[
        ("CRU_MOCK_STREAM_CHUNKS", "The answer is 4"),
        ("CRU_MOCK_USAGE_UPDATE", "22700/1000000"),
    ])
    .await;

    let limit = context_limit(&events).expect("context limit").limit as u64;
    let used = context_used(&events).unwrap_or_else(|| {
        panic!(
            "message_complete carried no total_tokens, so ContextUsage.used stays \
             0 and the statusline draws 0%. Events: {:?}",
            events.iter().map(|e| &e.event).collect::<Vec<_>>()
        )
    });

    assert!(used > 0 && limit > 0, "used={used} limit={limit}");
    assert!(
        used <= limit,
        "occupancy {used} exceeds the window {limit}, which would render >100%"
    );
}

/// An agent that reports no window gets no window.
///
/// The mock's `PromptResponse` carries no `usage` either, so this is the
/// cursor/gemini shape: a delegated session with genuinely no context data.
/// Fabricating a limit for it would make the statusline draw "0% ctx" — a
/// confident wrong answer where "— ctx" is the honest one.
#[tokio::test]
async fn a_delegated_turn_without_a_usage_update_resolves_no_limit() {
    let events = delegated_turn_events(&[("CRU_MOCK_STREAM_CHUNKS", "The answer is 4")]).await;

    assert!(
        context_limit(&events).is_none(),
        "a session whose agent never reported a window got one anyway; events: {:?}",
        events.iter().map(|e| &e.event).collect::<Vec<_>>()
    );
    assert_eq!(
        context_used(&events),
        None,
        "message_complete invented a token count for an agent that reported none"
    );
}
