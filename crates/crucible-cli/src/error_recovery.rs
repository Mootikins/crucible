//! Error Recovery and Graceful Degradation Module
//!
//! This module provides comprehensive error recovery mechanisms for the Crucible CLI,
//! including circuit breakers, retry logic with exponential backoff, fallback strategies,
//! and graceful degradation when services are unavailable.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::CliConfig;
use crucible_llm::embeddings::error::EmbeddingError;

/// Circuit breaker state for preventing cascading failures
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed and requests are allowed
    Closed,
    /// Circuit is open and requests are blocked
    Open,
    /// Circuit is half-open and testing if service has recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,
    /// Duration to keep circuit open before attempting recovery
    pub recovery_timeout: Duration,
    /// Number of successful requests needed to close circuit in half-open state
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }

    pub async fn is_request_allowed(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has passed
                let last_failure = self.last_failure_time.read().await;
                if let Some(last_failure) = *last_failure {
                    if last_failure.elapsed() > self.config.recovery_timeout {
                        drop(state);
                        // Move to half-open state
                        *self.state.write().await = CircuitState::HalfOpen;
                        *self.success_count.write().await = 0;
                        info!("Circuit breaker moving to half-open state");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub async fn record_success(&self) {
        let state = self.state.read().await;
        match *state {
            CircuitState::HalfOpen => {
                let mut success_count = self.success_count.write().await;
                *success_count += 1;

                if *success_count >= self.config.success_threshold {
                    drop(state);
                    *self.state.write().await = CircuitState::Closed;
                    *self.failure_count.write().await = 0;
                    info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Closed | CircuitState::Open => {
                // Reset failure count on success
                *self.failure_count.write().await = 0;
            }
        }
    }

    pub async fn record_failure(&self) {
        let mut failure_count = self.failure_count.write().await;
        *failure_count += 1;

        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => {
                if *failure_count >= self.config.failure_threshold {
                    drop(state);
                    *self.state.write().await = CircuitState::Open;
                    *self.last_failure_time.write().await = Some(Instant::now());
                    warn!(
                        "Circuit breaker opened after {} consecutive failures",
                        *failure_count
                    );
                }
            }
            CircuitState::HalfOpen => {
                drop(state);
                *self.state.write().await = CircuitState::Open;
                *self.last_failure_time.write().await = Some(Instant::now());
                warn!("Circuit breaker opened again after failure in half-open state");
            }
            CircuitState::Open => {
                // Already open, just update the failure time
                *self.last_failure_time.write().await = Some(Instant::now());
            }
        }
    }

    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, T, E>(
    operation: F,
    config: RetryConfig,
    is_retryable: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
    E: std::fmt::Debug,
{
    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        debug!("Retry attempt {} of {}", attempt, config.max_attempts);

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    info!("Operation succeeded on attempt {}", attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                error!("Attempt {} failed: {:?}", attempt, error);

                if !is_retryable(&error) {
                    warn!("Error is not retryable, giving up: {:?}", error);
                    return Err(error);
                }

                last_error = Some(error);

                if attempt < config.max_attempts {
                    // Calculate delay using exponential backoff
                    let delay_ms = config.base_delay.as_millis() as f64
                        * config.backoff_multiplier.powi(attempt as i32 - 1);
                    let delay_ms = delay_ms.min(config.max_delay.as_millis() as f64) as u64;
                    let delay = Duration::from_millis(delay_ms);

                    debug!("Waiting {:?} before retry attempt {}", delay, attempt + 1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    error!("All {} retry attempts failed", config.max_attempts);
    Err(last_error.expect("at least one retry attempt should have occurred"))
}

/// Service health status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceHealth {
    /// Service is healthy and responding
    Healthy,
    /// Service is degraded (slow but responding)
    Degraded,
    /// Service is unhealthy (not responding)
    Unhealthy,
    /// Service status is unknown
    Unknown,
}

/// Service health monitor
#[derive(Debug)]
pub struct ServiceHealthMonitor {
    health_status: Arc<RwLock<HashMap<String, ServiceHealth>>>,
    last_check: Arc<RwLock<HashMap<String, Instant>>>,
}

impl Default for ServiceHealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceHealthMonitor {
    pub fn new() -> Self {
        Self {
            health_status: Arc::new(RwLock::new(HashMap::new())),
            last_check: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn update_health(&self, service: &str, health: ServiceHealth) {
        debug!("Updating health for service '{}': {:?}", service, health);
        self.health_status
            .write()
            .await
            .insert(service.to_string(), health);
        self.last_check
            .write()
            .await
            .insert(service.to_string(), Instant::now());
    }

    pub async fn get_health(&self, service: &str) -> ServiceHealth {
        self.health_status
            .read()
            .await
            .get(service)
            .cloned()
            .unwrap_or(ServiceHealth::Unknown)
    }

    pub async fn is_healthy(&self, service: &str) -> bool {
        matches!(
            self.get_health(service).await,
            ServiceHealth::Healthy | ServiceHealth::Degraded
        )
    }

    pub async fn get_all_health(&self) -> HashMap<String, ServiceHealth> {
        self.health_status.read().await.clone()
    }
}

/// Search fallback strategy
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStrategy {
    /// Semantic search with embeddings (highest quality)
    Semantic,
    /// Fuzzy search with string matching
    Fuzzy,
    /// Basic text search
    Text,
    /// File listing only
    FileListing,
}

/// Search fallback manager
#[derive(Debug)]
pub struct SearchFallbackManager {
    config: SearchFallbackConfig,
    health_monitor: Arc<ServiceHealthMonitor>,
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

/// Search fallback configuration
#[derive(Debug, Clone)]
pub struct SearchFallbackConfig {
    /// Enable automatic fallback
    pub enabled: bool,
    /// Maximum fallback depth
    pub max_fallback_depth: u32,
    /// Strategies in order of preference
    pub strategies: Vec<SearchStrategy>,
}

impl Default for SearchFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fallback_depth: 3,
            strategies: vec![
                SearchStrategy::Semantic,
                SearchStrategy::Fuzzy,
                SearchStrategy::Text,
                SearchStrategy::FileListing,
            ],
        }
    }
}

impl SearchFallbackManager {
    pub fn new(config: SearchFallbackConfig, health_monitor: Arc<ServiceHealthMonitor>) -> Self {
        Self {
            config,
            health_monitor,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_circuit_breaker(&self, service: &str) -> Arc<CircuitBreaker> {
        let mut circuit_breakers = self.circuit_breakers.write().await;

        if !circuit_breakers.contains_key(service) {
            let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
            circuit_breakers.insert(service.to_string(), circuit_breaker.clone());
        }

        circuit_breakers.get(service).unwrap().clone()
    }

    pub async fn execute_search_with_fallback<F, T>(
        &self,
        strategies: &[SearchStrategy],
        search_fn: F,
    ) -> Result<(T, SearchStrategy)>
    where
        F: Fn(
            SearchStrategy,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        if !self.config.enabled {
            // Fallback disabled, try the first strategy only
            return search_fn(strategies[0].clone())
                .await
                .map(|result| (result, strategies[0].clone()));
        }

        let mut last_error = None;

        for (index, strategy) in strategies.iter().enumerate() {
            if index >= self.config.max_fallback_depth as usize {
                info!("Reached maximum fallback depth, stopping");
                break;
            }

            info!("Attempting search with strategy: {:?}", strategy);

            // Check if strategy is available based on service health
            let service_name = self.get_service_for_strategy(strategy);
            let circuit_breaker = self.get_circuit_breaker(&service_name).await;

            // Check circuit breaker
            if !circuit_breaker.is_request_allowed().await {
                warn!(
                    "Circuit breaker is open for service '{}', skipping strategy: {:?}",
                    service_name, strategy
                );
                continue;
            }

            // Check service health
            if !self.health_monitor.is_healthy(&service_name).await {
                warn!(
                    "Service '{}' is unhealthy, skipping strategy: {:?}",
                    service_name, strategy
                );
                continue;
            }

            match search_fn(strategy.clone()).await {
                Ok(result) => {
                    info!("Search succeeded with strategy: {:?}", strategy);
                    circuit_breaker.record_success().await;
                    self.health_monitor
                        .update_health(&service_name, ServiceHealth::Healthy)
                        .await;
                    return Ok((result, strategy.clone()));
                }
                Err(error) => {
                    warn!("Search failed with strategy {:?}: {:?}", strategy, error);
                    let is_temporary = self.is_temporary_error(&error);
                    last_error = Some(error);
                    circuit_breaker.record_failure().await;

                    // Update service health based on error type
                    if is_temporary {
                        self.health_monitor
                            .update_health(&service_name, ServiceHealth::Degraded)
                            .await;
                    } else {
                        self.health_monitor
                            .update_health(&service_name, ServiceHealth::Unhealthy)
                            .await;
                    }
                }
            }
        }

        error!("All search strategies failed");
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No search strategies available")))
    }

    fn get_service_for_strategy(&self, strategy: &SearchStrategy) -> String {
        match strategy {
            SearchStrategy::Semantic => "embedding_service".to_string(),
            SearchStrategy::Fuzzy => "search_service".to_string(),
            SearchStrategy::Text => "file_system".to_string(),
            SearchStrategy::FileListing => "file_system".to_string(),
        }
    }

    fn is_temporary_error(&self, error: &anyhow::Error) -> bool {
        // Check if this is a temporary error that might resolve itself
        let error_msg = error.to_string().to_lowercase();

        error_msg.contains("timeout")
            || error_msg.contains("connection")
            || error_msg.contains("network")
            || error_msg.contains("rate limit")
            || error_msg.contains("temporary")
    }

    pub async fn get_available_strategies(&self) -> Vec<SearchStrategy> {
        let mut available = Vec::new();

        for strategy in &self.config.strategies {
            let service_name = self.get_service_for_strategy(strategy);
            let circuit_breaker = self.get_circuit_breaker(&service_name).await;

            // Only block if circuit breaker is explicitly open
            let circuit_allowed = circuit_breaker.is_request_allowed().await;

            // For unknown health, assume healthy (conservative approach)
            let service_healthy = !matches!(
                self.health_monitor.get_health(&service_name).await,
                ServiceHealth::Unhealthy
            );

            if circuit_allowed && service_healthy {
                available.push(strategy.clone());
            }
        }

        available
    }
}

/// Global error recovery manager
#[derive(Debug)]
pub struct ErrorRecoveryManager {
    health_monitor: Arc<ServiceHealthMonitor>,
    search_fallback: SearchFallbackManager,
}

impl ErrorRecoveryManager {
    pub fn new(_config: &CliConfig) -> Self {
        let health_monitor = Arc::new(ServiceHealthMonitor::new());
        let search_fallback_config = SearchFallbackConfig::default();

        Self {
            health_monitor: health_monitor.clone(),
            search_fallback: SearchFallbackManager::new(search_fallback_config, health_monitor),
        }
    }

    pub fn health_monitor(&self) -> Arc<ServiceHealthMonitor> {
        self.health_monitor.clone()
    }

    pub fn search_fallback(&self) -> &SearchFallbackManager {
        &self.search_fallback
    }

    pub async fn get_system_health(&self) -> HashMap<String, ServiceHealth> {
        self.health_monitor.get_all_health().await
    }

    pub async fn get_system_status_summary(&self) -> String {
        let health_map = self.health_monitor.get_all_health().await;
        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut unhealthy_count = 0;
        let mut unknown_count = 0;

        for health in health_map.values() {
            match health {
                ServiceHealth::Healthy => healthy_count += 1,
                ServiceHealth::Degraded => degraded_count += 1,
                ServiceHealth::Unhealthy => unhealthy_count += 1,
                ServiceHealth::Unknown => unknown_count += 1,
            }
        }

        format!(
            "System Status: {} healthy, {} degraded, {} unhealthy, {} unknown",
            healthy_count, degraded_count, unhealthy_count, unknown_count
        )
    }
}

/// Check if an embedding error is retryable
pub fn is_embedding_error_retryable(error: &EmbeddingError) -> bool {
    match error {
        EmbeddingError::Timeout { .. } => true,
        EmbeddingError::RateLimitExceeded { .. } => true,
        EmbeddingError::HttpError(http_err) => {
            // Network errors and 5xx server errors are retryable
            http_err.is_timeout()
                || http_err.is_connect()
                || http_err.is_request()
                || http_err
                    .status()
                    .map(|s| s.is_server_error())
                    .unwrap_or(false)
        }
        _ => false,
    }
}

/// Get retry delay for embedding errors
pub fn get_embedding_error_retry_delay(error: &EmbeddingError) -> Option<Duration> {
    match error {
        EmbeddingError::RateLimitExceeded { retry_after_secs } => {
            Some(Duration::from_secs(*retry_after_secs))
        }
        EmbeddingError::Timeout { .. } => Some(Duration::from_secs(2)),
        EmbeddingError::HttpError(_) => Some(Duration::from_secs(1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_circuit_breaker_basic_operation() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_millis(100),
            success_threshold: 2,
        };

        let circuit_breaker = CircuitBreaker::new(config);

        // Initially closed
        assert!(circuit_breaker.is_request_allowed().await);
        assert_eq!(circuit_breaker.get_state().await, CircuitState::Closed);

        // Record failures
        circuit_breaker.record_failure().await;
        circuit_breaker.record_failure().await;
        assert!(circuit_breaker.is_request_allowed().await);

        // Third failure should open circuit
        circuit_breaker.record_failure().await;
        assert!(!circuit_breaker.is_request_allowed().await);
        assert_eq!(circuit_breaker.get_state().await, CircuitState::Open);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be half-open now
        assert!(circuit_breaker.is_request_allowed().await);
        assert_eq!(circuit_breaker.get_state().await, CircuitState::HalfOpen);

        // Record successes to close circuit
        circuit_breaker.record_success().await;
        circuit_breaker.record_success().await;
        assert!(circuit_breaker.is_request_allowed().await);
        assert_eq!(circuit_breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_service_health_monitor() {
        let monitor = ServiceHealthMonitor::new();

        assert_eq!(
            monitor.get_health("test_service").await,
            ServiceHealth::Unknown
        );
        assert!(!monitor.is_healthy("test_service").await);

        monitor
            .update_health("test_service", ServiceHealth::Healthy)
            .await;
        assert_eq!(
            monitor.get_health("test_service").await,
            ServiceHealth::Healthy
        );
        assert!(monitor.is_healthy("test_service").await);

        monitor
            .update_health("test_service", ServiceHealth::Unhealthy)
            .await;
        assert_eq!(
            monitor.get_health("test_service").await,
            ServiceHealth::Unhealthy
        );
        assert!(!monitor.is_healthy("test_service").await);
    }

    #[tokio::test]
    async fn test_search_fallback_manager() {
        let health_monitor = Arc::new(ServiceHealthMonitor::new());
        let config = SearchFallbackConfig::default();
        let fallback_manager = SearchFallbackManager::new(config, health_monitor.clone());

        // All services initially unknown, so all strategies should be available
        let available = fallback_manager.get_available_strategies().await;
        assert!(!available.is_empty());

        // Mark embedding service as unhealthy
        health_monitor
            .update_health("embedding_service", ServiceHealth::Unhealthy)
            .await;
        let available = fallback_manager.get_available_strategies().await;

        // Should still have other strategies available
        assert!(available.len() > 0);
        assert!(!available.contains(&SearchStrategy::Semantic));
    }

    #[test]
    fn test_embedding_error_retryable() {
        let retryable_errors = vec![
            EmbeddingError::Timeout { timeout_secs: 30 },
            EmbeddingError::RateLimitExceeded {
                retry_after_secs: 60,
            },
        ];

        let non_retryable_errors = vec![
            EmbeddingError::AuthenticationError("Invalid key".to_string()),
            EmbeddingError::InvalidResponse("Malformed JSON".to_string()),
            EmbeddingError::ConfigError("Invalid config".to_string()),
        ];

        for error in retryable_errors {
            assert!(
                is_embedding_error_retryable(&error),
                "Error should be retryable: {:?}",
                error
            );
            assert!(
                get_embedding_error_retry_delay(&error).is_some(),
                "Retryable error should have delay: {:?}",
                error
            );
        }

        for error in non_retryable_errors {
            assert!(
                !is_embedding_error_retryable(&error),
                "Error should not be retryable: {:?}",
                error
            );
            assert!(
                get_embedding_error_retry_delay(&error).is_none(),
                "Non-retryable error should not have delay: {:?}",
                error
            );
        }
    }
}
