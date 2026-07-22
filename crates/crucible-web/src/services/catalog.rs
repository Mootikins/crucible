//! Cached catalog data (agent profiles, providers).
//!
//! `/api/agents` and `/api/providers` proxy daemon calls that PROBE things
//! (agent binaries, provider endpoints) and take 0.5–1s each. Every splash
//! render paid that — and the two slow requests hogged browser connections,
//! queueing the fast ones behind them. This cache serves the last known
//! value instantly and refreshes in the background once it is stale
//! (stale-while-revalidate); the daemon is only awaited on a cold cache,
//! and `warm` pre-fills that at server startup.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use super::daemon::AppState;

/// Agents/providers change on install/config edits, not per request.
const CATALOG_TTL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct SwrCache {
    entries: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    value: serde_json::Value,
    fetched_at: Instant,
    /// A background refresh is already in flight — don't pile up more.
    refreshing: bool,
}

impl SwrCache {
    /// Return the cached value for `key` if one exists — kicking off a
    /// background refresh once it is older than `ttl` — and await `fetch`
    /// inline only when the key has never been fetched.
    pub async fn get_or_fetch<F, Fut>(
        self: &Arc<Self>,
        key: &str,
        ttl: Duration,
        fetch: F,
    ) -> anyhow::Result<serde_json::Value>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<serde_json::Value>> + Send + 'static,
    {
        {
            let mut entries = self.entries.lock().await;
            if let Some(entry) = entries.get_mut(key) {
                let value = entry.value.clone();
                if entry.fetched_at.elapsed() > ttl && !entry.refreshing {
                    entry.refreshing = true;
                    let cache = Arc::clone(self);
                    let key = key.to_string();
                    tokio::spawn(async move {
                        let result = fetch().await;
                        let mut entries = cache.entries.lock().await;
                        if let Some(entry) = entries.get_mut(&key) {
                            entry.refreshing = false;
                            // Failures keep serving the stale value; bumping
                            // fetched_at spaces retries out to the TTL instead
                            // of retrying on every request.
                            entry.fetched_at = Instant::now();
                            match result {
                                Ok(value) => entry.value = value,
                                Err(e) => tracing::warn!(
                                    key = %key,
                                    error = %e,
                                    "Catalog refresh failed; serving stale value"
                                ),
                            }
                        }
                    });
                }
                return Ok(value);
            }
        }

        // Cold cache: fetch inline. Concurrent cold callers may double-fetch;
        // last write wins, which is harmless for idempotent catalog reads.
        let value = fetch().await?;
        let mut entries = self.entries.lock().await;
        entries.insert(
            key.to_string(),
            Entry {
                value: value.clone(),
                fetched_at: Instant::now(),
                refreshing: false,
            },
        );
        Ok(value)
    }
}

/// Daemon agent-profiles payload (`agents.list_profiles` result), cached.
pub async fn agents_value(state: &AppState) -> anyhow::Result<serde_json::Value> {
    let daemon = state.daemon.clone();
    state
        .swr
        .get_or_fetch("agents", CATALOG_TTL, move || async move {
            daemon.agents_list_profiles().await
        })
        .await
}

/// Providers payload (serialized `Vec<ProviderInfo>`), cached per kiln filter.
pub async fn providers_value(
    state: &AppState,
    kiln: Option<&std::path::Path>,
) -> anyhow::Result<serde_json::Value> {
    let key = match kiln {
        Some(p) => format!("providers:{}", p.display()),
        None => "providers".to_string(),
    };
    let daemon = state.daemon.clone();
    let kiln = kiln.map(|p| p.to_path_buf());
    state
        .swr
        .get_or_fetch(&key, CATALOG_TTL, move || async move {
            let providers = daemon.list_providers(kiln.as_deref()).await?;
            Ok(serde_json::to_value(providers)?)
        })
        .await
}

/// Pre-fill the slow entries at server startup so the first splash render
/// never pays the probe cost. Fire-and-forget; failures just leave the
/// cache cold for the first real request.
pub fn warm(state: AppState) {
    tokio::spawn(async move {
        if let Err(e) = agents_value(&state).await {
            tracing::warn!(error = %e, "Agent catalog warm-up failed");
        }
        if let Err(e) = providers_value(&state, None).await {
            tracing::warn!(error = %e, "Provider catalog warm-up failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counting_fetch(
        counter: &Arc<AtomicUsize>,
        value: serde_json::Value,
    ) -> impl FnOnce() -> std::pin::Pin<
        Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send>,
    > + Send
           + 'static {
        let counter = Arc::clone(counter);
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(value) })
        }
    }

    #[tokio::test]
    async fn cold_cache_awaits_fetch_then_serves_cached() {
        let cache = Arc::new(SwrCache::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let ttl = Duration::from_secs(60);

        let v = cache
            .get_or_fetch("k", ttl, counting_fetch(&calls, serde_json::json!(1)))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Fresh hit: no second fetch.
        let v = cache
            .get_or_fetch("k", ttl, counting_fetch(&calls, serde_json::json!(2)))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stale_entry_serves_old_value_and_refreshes_in_background() {
        let cache = Arc::new(SwrCache::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let ttl = Duration::from_millis(0); // instantly stale

        cache
            .get_or_fetch("k", ttl, counting_fetch(&calls, serde_json::json!("old")))
            .await
            .unwrap();

        // Stale read returns the OLD value immediately, refresh runs behind it.
        let v = cache
            .get_or_fetch("k", ttl, counting_fetch(&calls, serde_json::json!("new")))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!("old"));

        // The background refresh lands eventually.
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(2)).await;
            let v = cache
                .get_or_fetch(
                    "k",
                    Duration::from_secs(60),
                    counting_fetch(&calls, serde_json::json!("later")),
                )
                .await
                .unwrap();
            if v == serde_json::json!("new") {
                return;
            }
        }
        panic!("background refresh never landed");
    }

    #[tokio::test]
    async fn failed_refresh_keeps_serving_stale_value() {
        let cache = Arc::new(SwrCache::default());
        cache
            .get_or_fetch("k", Duration::from_millis(0), || async {
                Ok(serde_json::json!("good"))
            })
            .await
            .unwrap();

        // Stale read triggers a refresh that fails; the stale value survives.
        let v = cache
            .get_or_fetch("k", Duration::from_millis(0), || async {
                anyhow::bail!("daemon down")
            })
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!("good"));

        tokio::time::sleep(Duration::from_millis(20)).await;
        let v = cache
            .get_or_fetch("k", Duration::from_secs(60), || async {
                Ok(serde_json::json!("unused"))
            })
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!("good"));
    }
}
