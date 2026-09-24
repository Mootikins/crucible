//! An ordered queue to one consumer that drops no item, and a wait for that
//! consumer.
//!
//! The daemon's broadcast bus drops the oldest events for a receiver that
//! falls behind. That is correct for a client, which can read the state again,
//! and wrong for an owner of stored state: the session log cannot read a lost
//! event again. Its writer reads a queue of this kind instead.
//!
//! The queue is unbounded. Its senders include synchronous code (the event
//! emit path, Lua callbacks), which cannot wait for capacity, and a runtime
//! thread that blocks can be the thread that drains the queue.
//!
//! [`Waiter::wait`] returns when the consumer finished each item that was
//! sent before the call. A reader of the stored state calls it first, so that
//! it sees each change that the daemon already announced. The consumer marks
//! an item finished when its [`Entry`] drops, so it cannot forget one.

use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::{mpsc, watch};

/// A queue and its one consumer.
pub(crate) fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (done_tx, done) = watch::channel(0);
    (
        Sender {
            tx,
            count: Counter {
                sent: Arc::new(Mutex::new(0)),
                done,
            },
        },
        Receiver { rx, done: done_tx },
    )
}

/// The count of items sent and the count that the consumer finished.
#[derive(Clone)]
struct Counter {
    sent: Arc<Mutex<u64>>,
    done: watch::Receiver<u64>,
}

/// The sending side. Clones send to the same queue.
pub(crate) struct Sender<T> {
    tx: mpsc::UnboundedSender<T>,
    count: Counter,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            count: self.count.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Make an item with `make`, queue it, and run `then` with the rest of
    /// what `make` gave, all before a concurrent sender can queue.
    ///
    /// A caller that numbers the item or publishes it elsewhere does so in
    /// `make` and `then`, so that its order and the queue order are one order.
    pub(crate) fn send_then<U, R>(
        &self,
        make: impl FnOnce() -> (T, U),
        then: impl FnOnce(U) -> R,
    ) -> (bool, R) {
        let mut sent = self
            .count
            .sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (item, rest) = make();
        // A stopped consumer does not count the item, so that a wait does not
        // wait for an item that nobody will read.
        let queued = self.tx.send(item).is_ok();
        if queued {
            *sent += 1;
        }
        (queued, then(rest))
    }

    /// A wait for the consumer of this queue.
    pub(crate) fn waiter(&self) -> Waiter {
        Waiter(Some(self.count.clone()))
    }
}

/// Waits until the consumer finished each item sent before the wait.
///
/// The default waiter has no queue, and its wait returns at once.
#[derive(Clone, Default)]
pub(crate) struct Waiter(Option<Counter>);

impl Waiter {
    /// Return when each item sent before this call is finished, or when the
    /// consumer stopped.
    pub(crate) async fn wait(&self) {
        let Some(count) = self.0.as_ref() else {
            return;
        };
        let target = *count.sent.lock().unwrap_or_else(PoisonError::into_inner);
        let mut done = count.done.clone();
        // An error means that the consumer stopped, and nothing will finish
        // the rest.
        let _ = done.wait_for(|done| *done >= target).await;
    }
}

/// The one consumer of a queue.
pub(crate) struct Receiver<T> {
    rx: mpsc::UnboundedReceiver<T>,
    done: watch::Sender<u64>,
}

impl<T> Receiver<T> {
    /// The next item, or `None` when every sender is gone.
    pub(crate) async fn recv(&mut self) -> Option<Entry<T>> {
        let item = self.rx.recv().await?;
        Some(self.entry(item))
    }

    /// The next item that is already queued.
    pub(crate) fn try_recv(&mut self) -> Option<Entry<T>> {
        let item = self.rx.try_recv().ok()?;
        Some(self.entry(item))
    }

    /// How many items are queued.
    pub(crate) fn len(&self) -> usize {
        self.rx.len()
    }

    fn entry(&self, item: T) -> Entry<T> {
        Entry {
            item,
            done: self.done.clone(),
        }
    }
}

/// One item from the queue. It counts as finished when it drops.
pub(crate) struct Entry<T> {
    item: T,
    done: watch::Sender<u64>,
}

impl<T> std::ops::Deref for Entry<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.item
    }
}

impl<T> Drop for Entry<T> {
    fn drop(&mut self) {
        self.done.send_modify(|done| *done += 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send<T>(tx: &Sender<T>, item: T) -> bool {
        tx.send_then(|| (item, ()), |()| ()).0
    }

    #[tokio::test]
    async fn a_wait_returns_only_after_the_items_before_it_are_finished() {
        let (tx, mut rx) = channel();
        assert!(send(&tx, 1));
        assert!(send(&tx, 2));
        let waiter = tx.waiter();
        let mut wait = Box::pin(waiter.wait());

        let first = rx.recv().await.expect("an item");
        assert_eq!(*first, 1);
        drop(first);
        assert!(
            futures::poll!(wait.as_mut()).is_pending(),
            "one of two items is finished"
        );

        let second = rx.try_recv().expect("an item");
        assert!(
            futures::poll!(wait.as_mut()).is_pending(),
            "an item that is received is not yet finished"
        );
        drop(second);
        assert!(futures::poll!(wait.as_mut()).is_ready());
    }

    #[tokio::test]
    async fn a_wait_does_not_wait_for_a_stopped_consumer() {
        let (tx, rx) = channel::<u8>();
        assert!(send(&tx, 1));
        drop(rx);
        assert!(!send(&tx, 2), "a stopped consumer takes no item");
        tx.waiter().wait().await;
    }
}
