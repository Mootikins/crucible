//! Subscription management for daemon session events.
//!
//! Tracks which clients are subscribed to which sessions, enabling
//! efficient event broadcasting. Supports wildcard subscriptions for
//! clients that want to receive all session events.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Counter for generating unique client IDs.
static CLIENT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for a connected client.
///
/// Each client connection receives a unique ID that persists for the
/// duration of the connection. IDs are never reused within a daemon
/// process lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(u64);

impl ClientId {
    /// Create a new unique client ID.
    ///
    /// IDs are generated using an atomic counter and are guaranteed
    /// to be unique within the current process.
    pub fn new() -> Self {
        Self(CLIENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "client-{}", self.0)
    }
}

/// Wildcard session ID for subscribing to all sessions.
pub const WILDCARD_SESSION: &str = "*";

/// Manages client subscriptions to session events.
///
/// Thread-safe manager that tracks which clients are subscribed to which
/// sessions. Supports both specific session subscriptions and wildcard
/// subscriptions that receive all session events.
///
/// # Example
///
/// ```
/// use crucible_daemon::subscription::{SubscriptionManager, ClientId};
///
/// let manager = SubscriptionManager::new();
/// let client = ClientId::new();
///
/// // Subscribe to a specific session
/// manager.subscribe(client, "chat-2025-01-08");
/// assert!(manager.is_subscribed(client, "chat-2025-01-08"));
///
/// // Wildcard subscription receives all events
/// manager.subscribe_all(client);
/// assert!(manager.is_subscribed(client, "any-session-id"));
/// ```
pub struct SubscriptionManager {
    /// Map from session_id -> set of subscribed client IDs
    subscriptions: RwLock<HashMap<String, HashSet<ClientId>>>,
    /// Reverse map from client_id -> set of session IDs (for cleanup)
    client_sessions: RwLock<HashMap<ClientId, HashSet<String>>>,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    pub fn new() -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            client_sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Subscribe a client to a specific session.
    ///
    /// The client will receive events for this session until they
    /// unsubscribe or disconnect.
    pub fn subscribe(&self, client_id: ClientId, session_id: &str) {
        // Add to subscriptions map
        {
            let mut subs = self.subscriptions.write().unwrap();
            subs.entry(session_id.to_string())
                .or_default()
                .insert(client_id);
        }

        // Add to reverse map
        {
            let mut client_subs = self.client_sessions.write().unwrap();
            client_subs
                .entry(client_id)
                .or_default()
                .insert(session_id.to_string());
        }
    }

    /// Unsubscribe a client from a specific session.
    ///
    /// The client will no longer receive events for this session.
    pub fn unsubscribe(&self, client_id: ClientId, session_id: &str) {
        // Remove from subscriptions map
        {
            let mut subs = self.subscriptions.write().unwrap();
            if let Some(clients) = subs.get_mut(session_id) {
                clients.remove(&client_id);
                // Clean up empty entries
                if clients.is_empty() {
                    subs.remove(session_id);
                }
            }
        }

        // Remove from reverse map
        {
            let mut client_subs = self.client_sessions.write().unwrap();
            if let Some(sessions) = client_subs.get_mut(&client_id) {
                sessions.remove(session_id);
                // Clean up empty entries
                if sessions.is_empty() {
                    client_subs.remove(&client_id);
                }
            }
        }
    }

    /// Subscribe a client to all sessions (wildcard subscription).
    ///
    /// The client will receive events for all sessions, current and future.
    pub fn subscribe_all(&self, client_id: ClientId) {
        self.subscribe(client_id, WILDCARD_SESSION);
    }

    #[allow(dead_code)]
    pub fn get_subscribers(&self, session_id: &str) -> Vec<ClientId> {
        let subs = self.subscriptions.read().unwrap();

        let mut subscribers = HashSet::new();

        // Add clients subscribed to this specific session
        if let Some(clients) = subs.get(session_id) {
            subscribers.extend(clients);
        }

        // Add clients with wildcard subscription
        if let Some(wildcard_clients) = subs.get(WILDCARD_SESSION) {
            subscribers.extend(wildcard_clients);
        }

        subscribers.into_iter().collect()
    }

    /// Check if a client is subscribed to a session.
    ///
    /// Returns true if the client has a specific subscription to the
    /// session OR has a wildcard subscription.
    pub fn is_subscribed(&self, client_id: ClientId, session_id: &str) -> bool {
        let client_subs = self.client_sessions.read().unwrap();

        if let Some(sessions) = client_subs.get(&client_id) {
            // Check for specific subscription or wildcard
            sessions.contains(session_id) || sessions.contains(WILDCARD_SESSION)
        } else {
            false
        }
    }

    /// Remove a client and all their subscriptions.
    ///
    /// Called when a client disconnects to clean up all subscription state.
    pub fn remove_client(&self, client_id: ClientId) {
        // Get all sessions this client was subscribed to
        let sessions_to_clean: Vec<String> = {
            let mut client_subs = self.client_sessions.write().unwrap();
            client_subs
                .remove(&client_id)
                .map(|s| s.into_iter().collect())
                .unwrap_or_default()
        };

        // Remove client from each session's subscriber set
        {
            let mut subs = self.subscriptions.write().unwrap();
            for session_id in sessions_to_clean {
                if let Some(clients) = subs.get_mut(&session_id) {
                    clients.remove(&client_id);
                    if clients.is_empty() {
                        subs.remove(&session_id);
                    }
                }
            }
        }
    }

    /// Get the number of subscriptions for a session.
    #[cfg(test)]
    fn subscription_count(&self, session_id: &str) -> usize {
        let subs = self.subscriptions.read().unwrap();
        subs.get(session_id).map(|s| s.len()).unwrap_or(0)
    }

    /// Get the number of sessions a client is subscribed to.
    #[cfg(test)]
    fn client_subscription_count(&self, client_id: ClientId) -> usize {
        let client_subs = self.client_sessions.read().unwrap();
        client_subs.get(&client_id).map(|s| s.len()).unwrap_or(0)
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_id_uniqueness() {
        let id1 = ClientId::new();
        let id2 = ClientId::new();
        let id3 = ClientId::new();

        // All IDs should be unique
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        // IDs should be monotonically increasing
        assert!(id1.as_u64() < id2.as_u64());
        assert!(id2.as_u64() < id3.as_u64());
    }

    #[test]
    fn test_client_id_display() {
        let id = ClientId::new();
        let display = format!("{}", id);
        assert!(display.starts_with("client-"));
    }

    #[test]
    fn test_subscription_manager_subscribe() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();
        let session_id = "chat-2025-01-08T1530-abc123";

        // Initially not subscribed
        assert!(!manager.is_subscribed(client, session_id));

        // Subscribe
        manager.subscribe(client, session_id);

        // Now subscribed
        assert!(manager.is_subscribed(client, session_id));

        // Check internal state
        assert_eq!(manager.subscription_count(session_id), 1);
        assert_eq!(manager.client_subscription_count(client), 1);
    }

    #[test]
    fn test_subscription_manager_multiple_clients() {
        let manager = SubscriptionManager::new();
        let client1 = ClientId::new();
        let client2 = ClientId::new();
        let client3 = ClientId::new();
        let session_id = "chat-test";

        manager.subscribe(client1, session_id);
        manager.subscribe(client2, session_id);
        manager.subscribe(client3, session_id);

        let subscribers = manager.get_subscribers(session_id);
        assert_eq!(subscribers.len(), 3);
        assert!(subscribers.contains(&client1));
        assert!(subscribers.contains(&client2));
        assert!(subscribers.contains(&client3));
    }

    #[test]
    fn test_subscription_manager_multiple_sessions() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();

        manager.subscribe(client, "session-1");
        manager.subscribe(client, "session-2");
        manager.subscribe(client, "session-3");

        assert!(manager.is_subscribed(client, "session-1"));
        assert!(manager.is_subscribed(client, "session-2"));
        assert!(manager.is_subscribed(client, "session-3"));
        assert!(!manager.is_subscribed(client, "session-4"));

        assert_eq!(manager.client_subscription_count(client), 3);
    }

    #[test]
    fn test_subscription_manager_unsubscribe() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();
        let session_id = "chat-test";

        // Subscribe then unsubscribe
        manager.subscribe(client, session_id);
        assert!(manager.is_subscribed(client, session_id));

        manager.unsubscribe(client, session_id);
        assert!(!manager.is_subscribed(client, session_id));

        // Internal state should be cleaned up
        assert_eq!(manager.subscription_count(session_id), 0);
        assert_eq!(manager.client_subscription_count(client), 0);
    }

    #[test]
    fn test_subscription_manager_unsubscribe_nonexistent() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();

        // Unsubscribing from non-existent subscription should not panic
        manager.unsubscribe(client, "nonexistent");
    }

    #[test]
    fn test_subscription_manager_wildcard() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();

        // Subscribe to wildcard
        manager.subscribe_all(client);

        // Should be subscribed to any session
        assert!(manager.is_subscribed(client, "chat-test"));
        assert!(manager.is_subscribed(client, "agent-task"));
        assert!(manager.is_subscribed(client, "workflow-123"));
        assert!(manager.is_subscribed(client, "any-random-session"));
    }

    #[test]
    fn test_subscription_manager_wildcard_in_subscribers() {
        let manager = SubscriptionManager::new();
        let specific_client = ClientId::new();
        let wildcard_client = ClientId::new();

        // One client subscribes to specific session
        manager.subscribe(specific_client, "chat-test");

        // Another client subscribes to all
        manager.subscribe_all(wildcard_client);

        // Both should appear in subscribers for that session
        let subscribers = manager.get_subscribers("chat-test");
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&specific_client));
        assert!(subscribers.contains(&wildcard_client));

        // Only wildcard client for other sessions
        let other_subscribers = manager.get_subscribers("other-session");
        assert_eq!(other_subscribers.len(), 1);
        assert!(other_subscribers.contains(&wildcard_client));
    }

    #[test]
    fn test_subscription_manager_remove_client() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();

        // Subscribe to multiple sessions
        manager.subscribe(client, "session-1");
        manager.subscribe(client, "session-2");
        manager.subscribe(client, "session-3");

        assert_eq!(manager.client_subscription_count(client), 3);

        // Remove client
        manager.remove_client(client);

        // Client should be fully cleaned up
        assert!(!manager.is_subscribed(client, "session-1"));
        assert!(!manager.is_subscribed(client, "session-2"));
        assert!(!manager.is_subscribed(client, "session-3"));
        assert_eq!(manager.client_subscription_count(client), 0);

        // Session subscriptions should be cleaned up
        assert_eq!(manager.subscription_count("session-1"), 0);
        assert_eq!(manager.subscription_count("session-2"), 0);
        assert_eq!(manager.subscription_count("session-3"), 0);
    }

    #[test]
    fn test_subscription_manager_remove_client_preserves_others() {
        let manager = SubscriptionManager::new();
        let client1 = ClientId::new();
        let client2 = ClientId::new();
        let session_id = "shared-session";

        // Both clients subscribe to same session
        manager.subscribe(client1, session_id);
        manager.subscribe(client2, session_id);

        assert_eq!(manager.subscription_count(session_id), 2);

        // Remove client1
        manager.remove_client(client1);

        // client2 should still be subscribed
        assert!(!manager.is_subscribed(client1, session_id));
        assert!(manager.is_subscribed(client2, session_id));
        assert_eq!(manager.subscription_count(session_id), 1);
    }

    #[test]
    fn test_subscription_manager_remove_nonexistent_client() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();

        // Removing non-existent client should not panic
        manager.remove_client(client);
    }

    #[test]
    fn test_subscription_manager_get_subscribers_empty() {
        let manager = SubscriptionManager::new();

        let subscribers = manager.get_subscribers("nonexistent-session");
        assert!(subscribers.is_empty());
    }

    #[test]
    fn test_subscription_manager_idempotent_subscribe() {
        let manager = SubscriptionManager::new();
        let client = ClientId::new();
        let session_id = "chat-test";

        // Subscribe multiple times
        manager.subscribe(client, session_id);
        manager.subscribe(client, session_id);
        manager.subscribe(client, session_id);

        // Should only count once
        assert_eq!(manager.subscription_count(session_id), 1);
        assert_eq!(manager.client_subscription_count(client), 1);
    }

    #[test]
    fn test_subscription_manager_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(SubscriptionManager::new());
        let mut handles = vec![];

        // Spawn multiple threads that subscribe/unsubscribe
        for _ in 0..10 {
            let mgr = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                let client = ClientId::new();
                for i in 0..100 {
                    let session = format!("session-{}", i % 10);
                    mgr.subscribe(client, &session);
                }
                for i in 0..100 {
                    let session = format!("session-{}", i % 10);
                    mgr.unsubscribe(client, &session);
                }
            }));
        }

        // All threads should complete without panic
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
