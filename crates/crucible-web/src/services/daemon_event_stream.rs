//! A browser stream owns one local receiver and its upstream interest.
use super::{ReconnectingDaemon, EVENT_CHANNEL_CAPACITY};
use crucible_daemon::SessionEvent;
use futures::Stream;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

pub struct EventStream {
    receiver: Option<BroadcastStream<SessionEvent>>,
    daemon: Weak<ReconnectingDaemon>,
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
        // Drop reception before scheduling cleanup: receiver_count is the
        // lifetime authority, including cancellation during initial subscribe.
        self.receiver.take();
        let Some(daemon) = self.daemon.upgrade() else {
            return;
        };
        let session_id = self.session_id.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                daemon.release_events(&session_id).await;
            });
        }
    }
}

pub(super) fn stream_gap(session_id: &str, dropped: u64) -> SessionEvent {
    SessionEvent::typed(
        session_id,
        crucible_core::protocol::SystemPayload::StreamGap { dropped },
    )
}

impl ReconnectingDaemon {
    pub async fn subscribe_events(
        self: &Arc<Self>,
        session_id: &str,
    ) -> anyhow::Result<EventStream> {
        let _change = self.subscription_changes.lock().await;
        let (receiver, first) = {
            let mut sessions = self.broker.sessions.write().await;
            let tx = sessions
                .entry(session_id.to_owned())
                .or_insert_with(|| broadcast::channel(EVENT_CHANNEL_CAPACITY).0);
            let first = tx.receiver_count() == 0;
            (tx.subscribe(), first)
        };
        let stream = EventStream {
            receiver: Some(BroadcastStream::new(receiver)),
            daemon: Arc::downgrade(self),
            session_id: session_id.to_owned(),
        };
        if first {
            self.session_subscribe(&[session_id]).await?;
        }
        Ok(stream)
    }

    async fn release_events(&self, session_id: &str) {
        // Serialize subscribe and last-drop through the whole RPC, so a
        // delayed unsubscribe cannot undo a replacement reader's subscribe.
        let _change = self.subscription_changes.lock().await;
        {
            let mut sessions = self.broker.sessions.write().await;
            if sessions
                .get(session_id)
                .is_some_and(|tx| tx.receiver_count() > 0)
            {
                return;
            }
            if sessions.remove(session_id).is_none() {
                return;
            }
        }
        self.unsubscribe_events(session_id).await;
    }

    /// Ending or deleting a session closes its HTTP streams and releases the
    /// shared interest through the same owner as ordinary last-reader cleanup.
    pub async fn close_event_streams(&self, session_id: &str) {
        let _change = self.subscription_changes.lock().await;
        let removed = self
            .broker
            .sessions
            .write()
            .await
            .remove(session_id)
            .is_some();
        if removed {
            self.unsubscribe_events(session_id).await;
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
