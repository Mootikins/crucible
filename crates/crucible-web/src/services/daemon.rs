use crate::{Result, WebError};
use crucible_config::CliAppConfig;
use crucible_rpc::{DaemonClient, SessionEvent};
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

impl EventBroker {
    fn new() -> Self {
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
    let (daemon, event_rx) = DaemonClient::connect_or_start_with_events()
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
