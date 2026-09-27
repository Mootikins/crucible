//! A browser stream owns one local receiver and its upstream interest.
//!
//! The broker's receiver count is the intent: a session wants daemon events
//! while a browser stream reads it. [`ReconnectingDaemon::reconcile`] makes the
//! daemon subscription agree with that intent, one flight at a time for each
//! session. A slow subscribe of one session (it can reconnect the daemon)
//! therefore delays only the readers of that session, and the RPC runs with
//! no lock that another session needs.
use super::{ReconnectingDaemon, EVENT_CHANNEL_CAPACITY};
use crucible_daemon::SessionEvent;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

pub struct EventStream {
    receiver: Option<BroadcastStream<SessionEvent>>,
    daemon: Weak<ReconnectingDaemon>,
    session_id: String,
}

/// One dropped stream. The message holds the daemon until the release
/// finishes, so a release outlives the last other owner of the daemon.
struct Release {
    daemon: Arc<ReconnectingDaemon>,
    session_id: String,
}

impl Stream for EventStream {
    type Item = SessionEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(self.receiver.as_mut().expect("live stream")).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => Poll::Ready(Some(event)),
            Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(n)))) => {
                tracing::warn!(session_id = %self.session_id, dropped = n, "SSE subscriber lagged");
                Poll::Ready(Some(stream_gap(&self.session_id, n)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        // Drop reception before the release: receiver_count is the lifetime
        // authority, including cancellation during the initial subscribe.
        self.receiver.take();
        // The release service owns the RPC. A send needs no runtime, so a
        // stream that drops outside one still releases its interest. With no
        // daemon left, its connection closed and took the interest with it.
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let release = Release {
            session_id: std::mem::take(&mut self.session_id),
            daemon: daemon.clone(),
        };
        let _ = daemon.interest.releases.send(release);
    }
}

pub(super) fn stream_gap(session_id: &str, dropped: u64) -> SessionEvent {
    SessionEvent::typed(
        session_id,
        crucible_core::protocol::SystemPayload::StreamGap { dropped },
    )
}

/// What the daemon forwards for one session, as far as this client knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Upstream {
    Off,
    On,
    /// An RPC started and did not answer (a cancel). The next flight sends
    /// the intent again; both RPCs are idempotent.
    Unknown,
}

/// The upstream state of each session, and the one flight that changes it.
pub(super) struct Interest {
    flights: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Upstream>>>>,
    releases: mpsc::UnboundedSender<Release>,
    /// Taken by the first subscribe, which starts the release service.
    pending_releases: std::sync::Mutex<Option<mpsc::UnboundedReceiver<Release>>>,
}

impl Interest {
    pub(super) fn new() -> Self {
        let (releases, pending) = mpsc::unbounded_channel();
        Self {
            flights: std::sync::Mutex::new(HashMap::new()),
            releases,
            pending_releases: std::sync::Mutex::new(Some(pending)),
        }
    }

    fn flight(&self, session_id: &str) -> Arc<tokio::sync::Mutex<Upstream>> {
        self.flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(session_id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Upstream::Off)))
            .clone()
    }

    /// Drop the entry of a session that nothing reads and that the daemon
    /// does not forward. A clone needs the map lock, so the count is exact.
    fn forget_idle(&self, session_id: &str, flight: Arc<tokio::sync::Mutex<Upstream>>) {
        let mut flights = self
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let idle = flights
            .get(session_id)
            .is_some_and(|held| Arc::ptr_eq(held, &flight))
            && Arc::strong_count(&flight) == 2
            && flight.try_lock().is_ok_and(|state| *state == Upstream::Off);
        if idle {
            flights.remove(session_id);
        }
    }
}

impl ReconnectingDaemon {
    pub async fn subscribe_events(
        self: &Arc<Self>,
        session_id: &str,
    ) -> anyhow::Result<EventStream> {
        self.start_release_service();
        let receiver = {
            let mut sessions = self.broker.sessions.write().await;
            sessions
                .entry(session_id.to_owned())
                .or_insert_with(|| broadcast::channel(EVENT_CHANNEL_CAPACITY).0)
                .subscribe()
        };
        let stream = EventStream {
            receiver: Some(BroadcastStream::new(receiver)),
            daemon: Arc::downgrade(self),
            session_id: session_id.to_owned(),
        };
        self.reconcile(session_id).await?;
        Ok(stream)
    }

    /// The flight of `session_id`, so a test can hold it as a slow RPC would.
    #[cfg(test)]
    pub(super) fn interest_flight_for_tests(
        &self,
        session_id: &str,
    ) -> Arc<tokio::sync::Mutex<Upstream>> {
        self.interest.flight(session_id)
    }

    fn start_release_service(self: &Arc<Self>) {
        let pending = self
            .interest
            .pending_releases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(releases) = pending {
            tokio::spawn(serve_releases(releases));
        }
    }

    /// Make the daemon subscription of `session_id` agree with the broker.
    ///
    /// One flight for each session applies the newest intent until the two
    /// agree, so a delayed unsubscribe cannot undo a later reader's
    /// subscribe. A refused subscribe answers its error and leaves the
    /// session `Off`.
    pub(super) async fn reconcile(&self, session_id: &str) -> anyhow::Result<()> {
        let flight = self.interest.flight(session_id);
        let result = {
            let mut upstream = flight.lock().await;
            loop {
                let wanted = self.wants_events(session_id).await;
                let target = if wanted { Upstream::On } else { Upstream::Off };
                if *upstream == target {
                    break Ok(());
                }
                *upstream = Upstream::Unknown;
                if wanted {
                    if let Err(error) = self.session_subscribe(&[session_id]).await {
                        *upstream = Upstream::Off;
                        break Err(error);
                    }
                } else {
                    self.unsubscribe_events(session_id).await;
                }
                *upstream = target;
            }
        };
        self.interest.forget_idle(session_id, flight);
        result
    }

    /// Whether a browser stream reads `session_id`. A local channel that no
    /// stream reads is removed here, under the same lock as the count.
    async fn wants_events(&self, session_id: &str) -> bool {
        let mut sessions = self.broker.sessions.write().await;
        match sessions.get(session_id) {
            Some(tx) if tx.receiver_count() > 0 => true,
            Some(_) => {
                sessions.remove(session_id);
                false
            }
            None => false,
        }
    }

    /// Ending or deleting a session closes its HTTP streams and releases the
    /// shared interest through the same owner as ordinary last-reader cleanup.
    pub async fn close_event_streams(&self, session_id: &str) {
        self.broker.sessions.write().await.remove(session_id);
        if let Err(error) = self.reconcile(session_id).await {
            tracing::debug!(%session_id, %error, "Could not release event subscription");
        }
    }

    async fn unsubscribe_events(&self, session_id: &str) {
        // A failed unsubscribe on a dead connection needs no replay. The next
        // connection restores only live receivers from the broker.
        if let Err(error) = self
            .daemon
            .read()
            .await
            .session_unsubscribe(&[session_id])
            .await
        {
            tracing::debug!(%session_id, %error, "Could not release event subscription");
        }
    }
}

/// Release the interest of each dropped stream. Each release runs in its own
/// task, so a slow RPC for one session does not delay another.
async fn serve_releases(mut releases: mpsc::UnboundedReceiver<Release>) {
    while let Some(Release { daemon, session_id }) = releases.recv().await {
        tokio::spawn(async move {
            if let Err(error) = daemon.reconcile(&session_id).await {
                tracing::debug!(%session_id, %error, "Could not settle event subscription");
            }
        });
    }
}
