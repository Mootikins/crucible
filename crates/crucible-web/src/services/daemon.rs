use crate::{Result, WebError};
use crucible_config::CliAppConfig;
use crucible_daemon::{DaemonClient, SessionEvent};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct AppState {
    pub daemon: Arc<DaemonClient>,
    pub events: Arc<EventBroker>,
    pub config: Arc<CliAppConfig>,
    pub http_client: reqwest::Client,
}

pub struct EventBroker {
    sessions: RwLock<HashMap<String, broadcast::Sender<SessionEvent>>>,
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBroker {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn subscribe(&self, session_id: &str) -> broadcast::Receiver<SessionEvent> {
        let mut sessions = self.sessions.write().await;
        let tx = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| broadcast::channel(EVENT_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    async fn dispatch(&self, event: SessionEvent) {
        let sessions = self.sessions.read().await;
        if let Some(tx) = sessions.get(&event.session_id) {
            let _ = tx.send(event);
        }
    }

    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }
}

pub async fn init_daemon(config: CliAppConfig) -> Result<AppState> {
    let (daemon, event_rx) = crucible_daemon::DaemonClient::connect_or_start_with_events()
        .await
        .map_err(|e| WebError::Daemon(format!("Failed to connect to daemon: {e}")))?;

    let daemon = Arc::new(daemon);
    let broker = Arc::new(EventBroker::new());

    spawn_event_router(event_rx, broker.clone());

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| WebError::Config(format!("Failed to create HTTP client: {e}")))?;

    Ok(AppState {
        daemon,
        events: broker,
        config: Arc::new(config),
        http_client,
    })
}

fn spawn_event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    broker: Arc<EventBroker>,
) {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            broker.dispatch(event).await;
        }
        tracing::warn!("Daemon event stream ended");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test SessionEvent
    fn test_event(session_id: &str, event_type: &str) -> SessionEvent {
        SessionEvent {
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            data: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn new_creates_empty_broker() {
        let broker = EventBroker::new();
        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "New broker should have no sessions");
    }

    #[tokio::test]
    async fn subscribe_creates_channel_for_new_session() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should have one session after subscribe");
        assert!(sessions.contains_key("session-1"), "Session key should exist");
    }

    #[tokio::test]
    async fn subscribe_twice_same_session_returns_two_receivers() {
        let broker = EventBroker::new();
        let rx1 = broker.subscribe("session-1").await;
        let rx2 = broker.subscribe("session-1").await;

        // Both receivers should be valid (not panicked)
        drop(rx1);
        drop(rx2);

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should still have only one session");
    }

    #[tokio::test]
    async fn dispatch_sends_event_to_subscribers() {
        let broker = Arc::new(EventBroker::new());
        let mut rx = broker.subscribe("session-1").await;

        let event = test_event("session-1", "test_event");
        broker.dispatch(event.clone()).await;

        // Receive the event
        let received = rx.recv().await;
        assert!(received.is_ok(), "Should receive event");
        let received_event = received.unwrap();
        assert_eq!(received_event.session_id, "session-1");
        assert_eq!(received_event.event_type, "test_event");
    }

    #[tokio::test]
    async fn dispatch_ignores_unsubscribed_sessions() {
        let broker = Arc::new(EventBroker::new());

        let event = test_event("unknown-session", "test_event");
        // Should not panic
        broker.dispatch(event).await;
    }

    #[tokio::test]
    async fn remove_session_deletes_channel() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        {
            let sessions = broker.sessions.read().await;
            assert_eq!(sessions.len(), 1);
        }

        broker.remove_session("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "Session should be removed");
    }

    #[tokio::test]
    async fn multiple_subscribers_both_receive_event() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-1").await;

        let event = test_event("session-1", "broadcast_test");
        broker.dispatch(event.clone()).await;

        // Both receivers should get the event
        let received1 = rx1.recv().await;
        let received2 = rx2.recv().await;

        assert!(received1.is_ok(), "Subscriber 1 should receive event");
        assert!(received2.is_ok(), "Subscriber 2 should receive event");

        assert_eq!(received1.unwrap().event_type, "broadcast_test");
        assert_eq!(received2.unwrap().event_type, "broadcast_test");
    }

    #[tokio::test]
    async fn multiple_sessions_receive_only_their_events() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-2").await;

        let event1 = test_event("session-1", "event_for_1");
        let event2 = test_event("session-2", "event_for_2");

        broker.dispatch(event1).await;
        broker.dispatch(event2).await;

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.event_type, "event_for_1");
        assert_eq!(received2.event_type, "event_for_2");
    }
}
