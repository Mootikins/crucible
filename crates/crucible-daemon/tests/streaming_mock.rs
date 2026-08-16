//! Reusable test infrastructure for streaming agent scenarios.
//!
//! Provides `TestHarness` (full daemon setup) and a `next_event` helper
//! for tests that drive real agents through the daemon.

use crucible_core::session::SessionType;
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::temp_session_manager;
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{AgentManager, AgentManagerParams, KilnManager, SessionManager};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};

/// Test harness bundling full daemon test setup
pub struct TestHarness {
    #[allow(dead_code)]
    pub temp_dir: TempDir,
    pub session_manager: Arc<SessionManager>,
    #[allow(dead_code)]
    pub agent_manager: AgentManager,
    #[allow(dead_code)]
    pub event_rx: broadcast::Receiver<SessionEventMessage>,
    pub session_id: String,
}

impl TestHarness {
    /// Create a new test harness with a fresh session
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let session_manager = temp_session_manager();

        let (event_tx, event_rx) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));

        let agent_manager = AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
        });

        let session = session_manager
            .create_session(
                SessionType::Chat,
                vec![temp_dir.path().to_path_buf()],
                None,
                None,
            )
            .await
            .expect("failed to create session");

        let session_id = session.id.clone();

        Self {
            temp_dir,
            session_manager,
            agent_manager,
            event_rx,
            session_id,
        }
    }
}

/// Helper to collect events from broadcast receiver with timeout
///
/// Loops over broadcast events, filtering by event name, with 2-second timeout.
pub async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(2), async {
        loop {
            // `Err(Closed)` must not be discarded. A closed broadcast returns it
            // IMMEDIATELY and forever, so `if let Ok(..)` swallowing it leaves a
            // loop with nothing that can park: it pegs a core and, on the
            // current-thread runtime `#[tokio::test]` builds, starves the timer
            // driver that the `timeout` wrapping this relies on. The intended
            // "timed out waiting for X" panic then never arrives and the test
            // hangs until the harness kills it.
            match rx.recv().await {
                Ok(event) if event.event == event_name => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Self-correcting: the cursor is repositioned and the next
                    // recv parks or delivers. Worth saying, since a dropped event
                    // could be the one being waited for.
                    eprintln!(
                        "event stream lagged, dropped {n} events while waiting for {event_name}"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    panic!("event stream closed while waiting for {event_name}")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_harness_creates_session() {
        let harness = TestHarness::new().await;
        assert!(!harness.session_id.is_empty());

        let session = harness
            .session_manager
            .get_session(&harness.session_id)
            .expect("failed to get session");

        assert_eq!(session.id, harness.session_id);
        assert_eq!(session.session_type, SessionType::Chat);
    }
}
