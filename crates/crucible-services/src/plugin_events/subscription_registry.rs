//! Subscription registry for tracking and managing active subscriptions

use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::{SubscriptionConfig, SubscriptionId, SubscriptionStatus, SubscriptionStats},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Weak};
use tracing::{debug, error, info, warn};

/// Registry for managing active subscriptions
#[derive(Clone)]
pub struct SubscriptionRegistry {
    /// Inner registry state
    inner: Arc<RwLock<SubscriptionRegistryInner>>,
}

/// Internal registry state
struct SubscriptionRegistryInner {
    /// Active subscriptions by ID
    subscriptions: HashMap<SubscriptionId, SubscriptionConfig>,

    /// Subscriptions by plugin ID
    plugin_subscriptions: HashMap<String, Vec<SubscriptionId>>,

    /// Subscription statistics
    stats: HashMap<SubscriptionId, SubscriptionStats>,

    /// Index for efficient event routing
    event_type_index: HashMap<String, Vec<SubscriptionId>>,
    source_index: HashMap<String, Vec<SubscriptionId>>,

    /// Health tracking
    subscription_health: HashMap<SubscriptionId, SubscriptionHealth>,

    /// Registry metrics
    metrics: RegistryMetrics,
}

/// Subscription health information
#[derive(Debug, Clone)]
struct SubscriptionHealth {
    /// Last activity timestamp
    last_activity: DateTime<Utc>,

    /// Consecutive failure count
    failure_count: u32,

    /// Last error
    last_error: Option<String>,

    /// Health status
    status: HealthStatus,
}

/// Health status
#[derive(Debug, Clone, PartialEq)]
enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Registry metrics
#[derive(Debug, Clone, Default)]
struct RegistryMetrics {
    total_subscriptions: u64,
    active_subscriptions: u64,
    suspended_subscriptions: u64,
    terminated_subscriptions: u64,
    total_events_processed: u64,
    registry_created_at: DateTime<Utc>,
}

impl SubscriptionRegistry {
    /// Create a new subscription registry
    pub fn new() -> Self {
        let inner = SubscriptionRegistryInner {
            subscriptions: HashMap::new(),
            plugin_subscriptions: HashMap::new(),
            stats: HashMap::new(),
            event_type_index: HashMap::new(),
            source_index: HashMap::new(),
            subscription_health: HashMap::new(),
            metrics: RegistryMetrics {
                registry_created_at: Utc::now(),
                ..Default::default()
            },
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Register a new subscription
    pub async fn register_subscription(&self, subscription: SubscriptionConfig) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        // Check if subscription already exists
        if inner.subscriptions.contains_key(&subscription.id) {
            return Err(SubscriptionError::InvalidConfiguration(
                format!("Subscription {} already exists", subscription.id.as_string())
            ));
        }

        // Validate subscription
        self.validate_subscription(&subscription)?;

        // Add to subscriptions map
        inner.subscriptions.insert(subscription.id.clone(), subscription.clone());

        // Add to plugin subscriptions index
        let plugin_subs = inner.plugin_subscriptions
            .entry(subscription.plugin_id.clone())
            .or_insert_with(Vec::new);
        plugin_subs.push(subscription.id.clone());

        // Initialize statistics
        let stats = SubscriptionStats {
            subscription_id: subscription.id.clone(),
            ..Default::default()
        };
        inner.stats.insert(subscription.id.clone(), stats);

        // Initialize health tracking
        let health = SubscriptionHealth {
            last_activity: Utc::now(),
            failure_count: 0,
            last_error: None,
            status: HealthStatus::Healthy,
        };
        inner.subscription_health.insert(subscription.id.clone(), health);

        // Update indexes
        self.update_indexes(&mut inner, &subscription, true).await;

        // Update metrics
        inner.metrics.total_subscriptions += 1;
        if subscription.status == SubscriptionStatus::Active {
            inner.metrics.active_subscriptions += 1;
        }

        info!("Registered subscription {} for plugin {}",
              subscription.id.as_string(), subscription.plugin_id);

        Ok(())
    }

    /// Unregister a subscription
    pub async fn unregister_subscription(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        // Get subscription before removing
        let subscription = inner.subscriptions
            .remove(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        // Remove from plugin subscriptions index
        if let Some(plugin_subs) = inner.plugin_subscriptions.get_mut(&subscription.plugin_id) {
            plugin_subs.retain(|id| id != subscription_id);
            if plugin_subs.is_empty() {
                inner.plugin_subscriptions.remove(&subscription.plugin_id);
            }
        }

        // Remove statistics and health
        inner.stats.remove(subscription_id);
        inner.subscription_health.remove(subscription_id);

        // Update indexes
        self.update_indexes(&mut inner, &subscription, false).await;

        // Update metrics
        if subscription.status == SubscriptionStatus::Active {
            inner.metrics.active_subscriptions -= 1;
        }
        inner.metrics.total_subscriptions -= 1;

        info!("Unregistered subscription {} for plugin {}",
              subscription_id.as_string(), subscription.plugin_id);

        Ok(())
    }

    /// Get subscription by ID
    pub async fn get_subscription(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<SubscriptionConfig> {
        let inner = self.inner.read().await;
        inner.subscriptions
            .get(subscription_id)
            .cloned()
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))
    }

    /// Get all subscriptions for a plugin
    pub async fn get_plugin_subscriptions(&self, plugin_id: &str) -> Vec<SubscriptionConfig> {
        let inner = self.inner.read().await;
        if let Some(subscription_ids) = inner.plugin_subscriptions.get(plugin_id) {
            subscription_ids
                .iter()
                .filter_map(|id| inner.subscriptions.get(id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Update subscription status
    pub async fn update_subscription_status(
        &self,
        subscription_id: &SubscriptionId,
        status: SubscriptionStatus,
    ) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        let subscription = inner.subscriptions
            .get_mut(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        let old_status = subscription.status.clone();
        subscription.status = status.clone();
        subscription.updated_at = Utc::now();

        // Update metrics
        match (old_status, status) {
            (SubscriptionStatus::Active, _) => {
                inner.metrics.active_subscriptions -= 1;
            }
            (_, SubscriptionStatus::Active) => {
                inner.metrics.active_subscriptions += 1;
            }
            (SubscriptionStatus::Suspended { .. }, _) => {
                inner.metrics.suspended_subscriptions -= 1;
            }
            (_, SubscriptionStatus::Suspended { .. }) => {
                inner.metrics.suspended_subscriptions += 1;
            }
            (SubscriptionStatus::Terminated { .. }, _) => {
                inner.metrics.terminated_subscriptions -= 1;
            }
            (_, SubscriptionStatus::Terminated { .. }) => {
                inner.metrics.terminated_subscriptions += 1;
            }
            _ => {}
        }

        // Update health tracking
        if let Some(health) = inner.subscription_health.get_mut(subscription_id) {
            health.last_activity = Utc::now();
            match status {
                SubscriptionStatus::Active => {
                    health.status = HealthStatus::Healthy;
                    health.failure_count = 0;
                    health.last_error = None;
                }
                SubscriptionStatus::Suspended { .. } => {
                    health.status = HealthStatus::Degraded;
                }
                SubscriptionStatus::Terminated { .. } => {
                    health.status = HealthStatus::Unhealthy;
                }
                _ => {}
            }
        }

        debug!("Updated subscription {} status to {:?}",
               subscription_id.as_string(), status);

        Ok(())
    }

    /// Get subscriptions that match an event
    pub async fn get_matching_subscriptions(
        &self,
        event: &crate::events::DaemonEvent,
    ) -> Vec<SubscriptionConfig> {
        let inner = self.inner.read().await;

        // Get candidate subscriptions from indexes
        let mut candidates = std::collections::HashSet::new();

        // Check event type index
        let event_type_str = match &event.event_type {
            crate::events::EventType::Filesystem(_) => "filesystem",
            crate::events::EventType::Database(_) => "database",
            crate::events::EventType::External(_) => "external",
            crate::events::EventType::Mcp(_) => "mcp",
            crate::events::EventType::Service(_) => "service",
            crate::events::EventType::System(_) => "system",
            crate::events::EventType::Custom(name) => name,
        };

        if let Some(subs) = inner.event_type_index.get(event_type_str) {
            candidates.extend(subs.iter().cloned());
        }

        // Check source index
        if let Some(subs) = inner.source_index.get(&event.source.id) {
            candidates.extend(subs.iter().cloned());
        }

        // If no specific matches, include all active subscriptions (for broadcasts)
        if candidates.is_empty() {
            for (id, sub) in &inner.subscriptions {
                if sub.status == SubscriptionStatus::Active {
                    candidates.insert(id.clone());
                }
            }
        }

        // Filter candidates by actual matching logic
        candidates
            .into_iter()
            .filter_map(|id| {
                inner.subscriptions.get(&id).and_then(|sub| {
                    if sub.matches_event(event) {
                        Some(sub.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Update subscription statistics
    pub async fn update_stats<F>(&self, subscription_id: &SubscriptionId, updater: F) -> SubscriptionResult<()>
    where
        F: FnOnce(&mut SubscriptionStats),
    {
        let mut inner = self.inner.write().await;

        let stats = inner.stats
            .get_mut(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        updater(stats);

        // Update health tracking
        if let Some(health) = inner.subscription_health.get_mut(subscription_id) {
            health.last_activity = Utc::now();
        }

        Ok(())
    }

    /// Record subscription error
    pub async fn record_error(
        &self,
        subscription_id: &SubscriptionId,
        error: &str,
    ) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if let Some(health) = inner.subscription_health.get_mut(subscription_id) {
            health.failure_count += 1;
            health.last_error = Some(error.to_string());

            // Update health status based on failure count
            health.status = match health.failure_count {
                0 => HealthStatus::Healthy,
                1..=3 => HealthStatus::Degraded,
                _ => HealthStatus::Unhealthy,
            }

            warn!("Subscription {} error #{}: {}",
                  subscription_id.as_string(), health.failure_count, error);
        }

        Ok(())
    }

    /// Get subscription statistics
    pub async fn get_stats(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<SubscriptionStats> {
        let inner = self.inner.read().await;
        inner.stats
            .get(subscription_id)
            .cloned()
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))
    }

    /// Get registry metrics
    pub async fn get_metrics(&self) -> RegistryMetrics {
        let inner = self.inner.read().await;
        inner.metrics.clone()
    }

    /// Get health status for subscription
    pub async fn get_subscription_health(&self, subscription_id: &SubscriptionId) -> Option<SubscriptionHealthInfo> {
        let inner = self.inner.read().await;
        inner.subscription_health
            .get(subscription_id)
            .map(|health| SubscriptionHealthInfo {
                last_activity: health.last_activity,
                failure_count: health.failure_count,
                last_error: health.last_error.clone(),
                status: match health.status {
                    HealthStatus::Healthy => "healthy".to_string(),
                    HealthStatus::Degraded => "degraded".to_string(),
                    HealthStatus::Unhealthy => "unhealthy".to_string(),
                },
            })
    }

    /// Cleanup expired subscriptions
    pub async fn cleanup_expired(&self, max_age: chrono::Duration) -> SubscriptionResult<u64> {
        let mut inner = self.inner.write().await;
        let mut removed_count = 0;
        let cutoff_time = Utc::now() - max_age;

        let subscriptions_to_remove: Vec<SubscriptionId> = inner
            .subscriptions
            .iter()
            .filter(|(_, sub)| {
                // Remove subscriptions that have been terminated for longer than max_age
                if let SubscriptionStatus::Terminated { terminated_at, .. } = &sub.status {
                    *terminated_at < cutoff_time
                } else {
                    false
                }
            })
            .map(|(id, _)| id.clone())
            .collect();

        for subscription_id in subscriptions_to_remove {
            if let Ok(_) = self.unregister_subscription_internal(&mut inner, &subscription_id).await {
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {} expired subscriptions", removed_count);
        }

        Ok(removed_count)
    }

    /// Validate subscription configuration
    fn validate_subscription(&self, subscription: &SubscriptionConfig) -> SubscriptionResult<()> {
        // Check plugin ID
        if subscription.plugin_id.is_empty() {
            return Err(SubscriptionError::InvalidConfiguration(
                "Plugin ID cannot be empty".to_string()
            ));
        }

        // Check subscription name
        if subscription.name.is_empty() {
            return Err(SubscriptionError::InvalidConfiguration(
                "Subscription name cannot be empty".to_string()
            ));
        }

        // Validate delivery options
        if subscription.delivery_options.max_event_size == 0 {
            return Err(SubscriptionError::InvalidConfiguration(
                "Max event size must be greater than 0".to_string()
            ));
        }

        // Validate authorization context
        if subscription.auth_context.principal.is_empty() {
            return Err(SubscriptionError::InvalidConfiguration(
                "Authorization principal cannot be empty".to_string()
            ));
        }

        Ok(())
    }

    /// Update internal indexes when subscription is added or removed
    async fn update_indexes(
        &self,
        inner: &mut SubscriptionRegistryInner,
        subscription: &SubscriptionConfig,
        add: bool,
    ) {
        // Update event type index based on subscription filters
        if subscription.filters.is_empty() {
            // No filters - potentially interested in all event types
            for event_type in ["filesystem", "database", "external", "mcp", "service", "system"] {
                let subs = inner.event_type_index.entry(event_type.to_string()).or_insert_with(Vec::new);
                if add && !subs.contains(&subscription.id) {
                    subs.push(subscription.id.clone());
                } else if !add {
                    subs.retain(|id| id != &subscription.id);
                }
            }
        } else {
            // Add to specific event type indexes based on filters
            for filter in &subscription.filters {
                for event_type in &filter.event_types {
                    let subs = inner.event_type_index.entry(event_type.clone()).or_insert_with(Vec::new);
                    if add && !subs.contains(&subscription.id) {
                        subs.push(subscription.id.clone());
                    } else if !add {
                        subs.retain(|id| id != &subscription.id);
                    }
                }
            }
        }

        // Update source index based on subscription filters
        for filter in &subscription.filters {
            for source in &filter.sources {
                let subs = inner.source_index.entry(source.clone()).or_insert_with(Vec::new);
                if add && !subs.contains(&subscription.id) {
                    subs.push(subscription.id.clone());
                } else if !add {
                    subs.retain(|id| id != &subscription.id);
                }
            }
        }
    }

    /// Internal unregistration method (requires mutable access)
    async fn unregister_subscription_internal(
        &self,
        inner: &mut SubscriptionRegistryInner,
        subscription_id: &SubscriptionId,
    ) -> SubscriptionResult<()> {
        // Get subscription before removing
        let subscription = inner.subscriptions
            .remove(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        // Remove from plugin subscriptions index
        if let Some(plugin_subs) = inner.plugin_subscriptions.get_mut(&subscription.plugin_id) {
            plugin_subs.retain(|id| id != subscription_id);
            if plugin_subs.is_empty() {
                inner.plugin_subscriptions.remove(&subscription.plugin_id);
            }
        }

        // Remove statistics and health
        inner.stats.remove(subscription_id);
        inner.subscription_health.remove(subscription_id);

        // Update indexes
        self.update_indexes(inner, &subscription, false).await;

        // Update metrics
        if subscription.status == SubscriptionStatus::Active {
            inner.metrics.active_subscriptions -= 1;
        }
        inner.metrics.total_subscriptions -= 1;

        Ok(())
    }
}

/// Public health information for subscriptions
#[derive(Debug, Clone)]
pub struct SubscriptionHealthInfo {
    pub last_activity: DateTime<Utc>,
    pub failure_count: u32,
    pub last_error: Option<String>,
    pub status: String,
}

impl Default for SubscriptionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventSource, SourceType, EventPayload};
    use crate::plugin_events::types::{SubscriptionType, AuthContext, EventPermission, PermissionScope};

    #[tokio::test]
    async fn test_subscription_registration() {
        let registry = SubscriptionRegistry::new();

        let subscription = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "test-subscription".to_string(),
            SubscriptionType::Realtime,
            AuthContext::new("test-user".to_string(), vec![]),
        );

        assert!(registry.register_subscription(subscription.clone()).await.is_ok());
        assert_eq!(
            registry.get_subscription(&subscription.id).await.unwrap().name,
            "test-subscription"
        );
    }

    #[tokio::test]
    async fn test_plugin_subscriptions() {
        let registry = SubscriptionRegistry::new();

        let sub1 = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "sub1".to_string(),
            SubscriptionType::Realtime,
            AuthContext::new("test-user".to_string(), vec![]),
        );

        let sub2 = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "sub2".to_string(),
            SubscriptionType::Realtime,
            AuthContext::new("test-user".to_string(), vec![]),
        );

        registry.register_subscription(sub1).await.unwrap();
        registry.register_subscription(sub2).await.unwrap();

        let plugin_subs = registry.get_plugin_subscriptions("test-plugin").await;
        assert_eq!(plugin_subs.len(), 2);
    }

    #[tokio::test]
    async fn test_subscription_metrics() {
        let registry = SubscriptionRegistry::new();

        let subscription = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "test-subscription".to_string(),
            SubscriptionType::Realtime,
            AuthContext::new("test-user".to_string(), vec![]),
        );

        registry.register_subscription(subscription.clone()).await.unwrap();

        let metrics = registry.get_metrics().await;
        assert_eq!(metrics.total_subscriptions, 1);
        assert_eq!(metrics.active_subscriptions, 1);
    }
}