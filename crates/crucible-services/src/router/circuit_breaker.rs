use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::traits::*;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Default circuit breaker implementation
pub struct DefaultServiceCircuitBreaker {
    /// Current circuit state
    state: Arc<RwLock<CircuitBreakerState>>,
    /// Number of consecutive failures
    failure_count: Arc<AtomicU32>,
    /// Number of consecutive successes
    success_count: Arc<AtomicU32>,
    /// Total request count
    request_count: Arc<AtomicU64>,
    /// Total success count
    total_success_count: Arc<AtomicU64>,
    /// Last failure time
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    /// Circuit breaker configuration
    config: CircuitBreakerConfig,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,
    /// Number of consecutive successes before closing circuit
    pub success_threshold: u32,
    /// Timeout before attempting to close circuit (half-open state)
    pub timeout: Duration,
    /// Minimum number of requests before starting to calculate error rate
    pub min_requests: u32,
    /// Error rate threshold (0.0 to 1.0)
    pub error_rate_threshold: f64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(60),
            min_requests: 10,
            error_rate_threshold: 0.5,
        }
    }
}

impl DefaultServiceCircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a new circuit breaker with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            total_success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Check if circuit should be opened
    async fn should_open_circuit(&self) -> bool {
        let requests = self.request_count.load(Ordering::Relaxed);

        if requests < self.config.min_requests as u64 {
            return false;
        }

        let failures = self.failure_count.load(Ordering::Relaxed);
        let error_rate = failures as f64 / requests as f64;

        error_rate >= self.config.error_rate_threshold ||
        failures >= self.config.failure_threshold
    }

    /// Transition to half-open state
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitBreakerState::HalfOpen;
        self.success_count.store(0, Ordering::Relaxed);
    }

    /// Check if timeout has expired for opening circuit
    async fn is_timeout_expired(&self) -> bool {
        if let Some(last_failure) = *self.last_failure_time.read().await {
            last_failure.elapsed() >= self.config.timeout
        } else {
            false
        }
    }
}

#[async_trait]
impl ServiceCircuitBreaker for DefaultServiceCircuitBreaker {
    async fn execute<F, T>(&self, operation: F) -> ServiceResult<T>
    where
        F: std::future::Future<Output = ServiceResult<T>> + Send,
    {
        // Check circuit state
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::Open => {
                // Check if we can transition to half-open
                drop(state);
                if self.is_timeout_expired().await {
                    self.transition_to_half_open().await;
                } else {
                    return Err(ServiceError::service_unavailable(
                        "Circuit breaker is open"
                    ));
                }
            }
            CircuitBreakerState::Closed => {
                // Normal operation
            }
            CircuitBreakerState::HalfOpen => {
                // Allow a single request through
            }
        }

        // Execute the operation
        let result = operation.await;

        // Update circuit state based on result
        match result {
            Ok(value) => {
                // Success
                self.total_success_count.fetch_add(1, Ordering::Relaxed);

                let current_state = self.state.read().await;
                match *current_state {
                    CircuitBreakerState::HalfOpen => {
                        drop(current_state);
                        let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                        if successes >= self.config.success_threshold {
                            // Close the circuit
                            let mut state = self.state.write().await;
                            *state = CircuitBreakerState::Closed;
                            self.failure_count.store(0, Ordering::Relaxed);
                        }
                    }
                    _ => {
                        // Reset failure count on success
                        self.failure_count.store(0, Ordering::Relaxed);
                    }
                }

                Ok(value)
            }
            Err(error) => {
                // Failure
                self.request_count.fetch_add(1, Ordering::Relaxed);
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

                // Record failure time
                let mut last_failure = self.last_failure_time.write().await;
                *last_failure = Some(Instant::now());

                let current_state = self.state.read().await;
                match *current_state {
                    CircuitBreakerState::Closed => {
                        drop(current_state);
                        if self.should_open_circuit().await {
                            // Open the circuit
                            let mut state = self.state.write().await;
                            *state = CircuitBreakerState::Open;
                        }
                    }
                    CircuitBreakerState::HalfOpen => {
                        drop(current_state);
                        // Immediately open circuit on failure in half-open state
                        let mut state = self.state.write().await;
                        *state = CircuitBreakerState::Open;
                    }
                    CircuitBreakerState::Open => {
                        // Already open
                    }
                }

                Err(error)
            }
        }
    }

    async fn state(&self) -> CircuitBreakerState {
        *self.state.read().await
    }

    async fn reset(&self) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        *state = CircuitBreakerState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.request_count.store(0, Ordering::Relaxed);
        self.total_success_count.store(0, Ordering::Relaxed);
        let mut last_failure = self.last_failure_time.write().await;
        *last_failure = None;
        Ok(())
    }

    async fn metrics(&self) -> CircuitBreakerMetrics {
        let requests = self.request_count.load(Ordering::Relaxed);
        let successes = self.total_success_count.load(Ordering::Relaxed);
        let failures = self.failure_count.load(Ordering::Relaxed);

        let failure_rate = if requests > 0 {
            failures as f64 / requests as f64
        } else {
            0.0
        };

        let time_to_next_state = match *self.state.read().await {
            CircuitBreakerState::Open => {
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    let elapsed = last_failure.elapsed();
                    if elapsed < self.config.timeout {
                        Some(self.config.timeout.as_millis() as u64 - elapsed.as_millis() as u64)
                    } else {
                        Some(0)
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        CircuitBreakerMetrics {
            total_requests: requests,
            successful_requests: successes,
            failed_requests: failures,
            failure_rate,
            time_to_next_state_ms: time_to_next_state,
        }
    }
}

/// Circuit breaker factory
pub struct CircuitBreakerFactory;

impl CircuitBreakerFactory {
    /// Create circuit breaker with default configuration
    pub fn create_default() -> Arc<dyn ServiceCircuitBreaker> {
        Arc::new(DefaultServiceCircuitBreaker::new())
    }

    /// Create circuit breaker with custom configuration
    pub fn create_with_config(config: CircuitBreakerConfig) -> Arc<dyn ServiceCircuitBreaker> {
        Arc::new(DefaultServiceCircuitBreaker::with_config(config))
    }

    /// Create circuit breaker with preset configurations
    pub fn create_preset(preset: CircuitBreakerPreset) -> Arc<dyn ServiceCircuitBreaker> {
        let config = match preset {
            CircuitBreakerPreset::Lenient => CircuitBreakerConfig {
                failure_threshold: 10,
                success_threshold: 2,
                timeout: Duration::from_secs(120),
                min_requests: 20,
                error_rate_threshold: 0.7,
            },
            CircuitBreakerPreset::Balanced => CircuitBreakerConfig::default(),
            CircuitBreakerPreset::Strict => CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 5,
                timeout: Duration::from_secs(30),
                min_requests: 5,
                error_rate_threshold: 0.3,
            },
        };

        Arc::new(DefaultServiceCircuitBreaker::with_config(config))
    }
}

/// Circuit breaker presets
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitBreakerPreset {
    /// Lenient circuit breaker (more tolerant of failures)
    Lenient,
    /// Balanced circuit breaker (default settings)
    Balanced,
    /// Strict circuit breaker (less tolerant of failures)
    Strict,
}

/// Metrics collector for circuit breakers
pub struct CircuitBreakerMetricsCollector {
    /// Circuit breaker metrics by service
    metrics: Arc<RwLock<HashMap<uuid::Uuid, CircuitBreakerMetrics>>>,
    /// Historical metrics
    historical_metrics: Arc<RwLock<Vec<MetricsSnapshot>>>,
}

impl CircuitBreakerMetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            historical_metrics: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record circuit breaker metrics
    pub async fn record_metrics(&self, service_id: uuid::Uuid, metrics: CircuitBreakerMetrics) {
        let mut all_metrics = self.metrics.write().await;
        all_metrics.insert(service_id, metrics.clone());

        // Store historical snapshot
        let mut historical = self.historical_metrics.write().await;
        historical.push(MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            service_id,
            metrics,
        });

        // Keep only last 1000 snapshots
        if historical.len() > 1000 {
            historical.remove(0);
        }
    }

    /// Get current metrics for all services
    pub async fn get_all_metrics(&self) -> HashMap<uuid::Uuid, CircuitBreakerMetrics> {
        self.metrics.read().await.clone()
    }

    /// Get metrics for a specific service
    pub async fn get_service_metrics(&self, service_id: uuid::Uuid) -> Option<CircuitBreakerMetrics> {
        self.metrics.read().await.get(&service_id).cloned()
    }

    /// Get historical metrics
    pub async fn get_historical_metrics(&self, limit: Option<usize>) -> Vec<MetricsSnapshot> {
        let historical = self.historical_metrics.read().await;
        match limit {
            Some(limit) => historical.iter().rev().take(limit).cloned().collect(),
            None => historical.clone(),
        }
    }
}

/// Metrics snapshot
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Snapshot timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Service ID
    pub service_id: uuid::Uuid,
    /// Circuit breaker metrics
    pub metrics: CircuitBreakerMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_basic_operation() {
        let circuit_breaker = DefaultServiceCircuitBreaker::new();

        // Should start in closed state
        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Closed);

        // Successful operation should keep circuit closed
        let result = circuit_breaker.execute(async { Ok::<_, ServiceError>("success") }).await;
        assert!(result.is_ok());
        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            min_requests: 3,
            error_rate_threshold: 0.5,
        };

        let circuit_breaker = DefaultServiceCircuitBreaker::with_config(config);

        // Fail enough times to open circuit
        for _ in 0..3 {
            let result = circuit_breaker.execute(async {
                Err::<(), ServiceError>(ServiceError::internal_error("test error"))
            }).await;
            assert!(result.is_err());
        }

        // Circuit should now be open
        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Open);

        // Next request should fail immediately
        let result = circuit_breaker.execute(async { Ok::<_, ServiceError>("success") }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            min_requests: 2,
            error_rate_threshold: 0.5,
        };

        let circuit_breaker = DefaultServiceCircuitBreaker::with_config(config);

        // Open the circuit
        for _ in 0..2 {
            let _ = circuit_breaker.execute(async {
                Err::<(), ServiceError>(ServiceError::internal_error("test error"))
            }).await;
        }

        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Open);

        // Wait for timeout
        sleep(Duration::from_millis(60)).await;

        // Next request should work and transition to half-open
        let result = circuit_breaker.execute(async { Ok::<_, ServiceError>("success") }).await;
        assert!(result.is_ok());
        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::HalfOpen);

        // Another success should close the circuit
        let result = circuit_breaker.execute(async { Ok::<_, ServiceError>("success") }).await;
        assert!(result.is_ok());
        assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Closed);
    }
}