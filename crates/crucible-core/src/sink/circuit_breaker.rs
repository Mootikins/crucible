//! Circuit breaker implementation for fault isolation

use std::time::{Duration, Instant};

/// Circuit breaker for protecting sinks from cascading failures
///
/// Implements the circuit breaker pattern to prevent cascading failures when
/// a sink is experiencing issues. The circuit has three states:
///
/// - **Closed**: Normal operation, all requests pass through
/// - **Open**: Sink is failing, reject requests immediately
/// - **Half-Open**: Testing if sink has recovered
///
/// # State Transitions
///
/// ```text
/// Closed --[failure_threshold]--> Open
///   ↑                              |
///   |                              |
///   └--[success_threshold]-- Half-Open <--[reset_timeout]--┘
/// ```
///
/// See `CircuitBreaker::new()` and state management methods for usage.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current circuit state
    state: CircuitState,

    /// Consecutive failure/success count
    count: u32,

    /// Last state change timestamp
    last_state_change: Option<Instant>,

    /// Configuration
    config: CircuitBreakerConfig,
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation
    Closed,

    /// Failing, reject requests
    Open,

    /// Testing recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Time to wait before transitioning from Open to Half-Open
    pub reset_timeout: Duration,

    /// Number of consecutive successes before closing circuit
    pub success_threshold: u32,

    /// Optional: Maximum time to stay in half-open state
    pub half_open_timeout: Option<Duration>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with configuration
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            count: 0,
            last_state_change: None,
            config,
        }
    }

    /// Check if requests can be executed
    ///
    /// Returns `true` if the circuit is Closed or Half-Open,
    /// `false` if the circuit is Open.
    ///
    /// If the circuit is Open and the reset timeout has elapsed,
    /// this method transitions to Half-Open and returns `true`.
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => {
                // Check half-open timeout
                if let Some(timeout) = self.config.half_open_timeout {
                    if let Some(last_change) = self.last_state_change {
                        if last_change.elapsed() >= timeout {
                            // Timed out in half-open, reopen circuit
                            self.transition_to_open();
                            return false;
                        }
                    }
                }
                true
            }
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(last_change) = self.last_state_change {
                    if last_change.elapsed() >= self.config.reset_timeout {
                        self.transition_to_half_open();
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Record a successful operation
    ///
    /// In Half-Open state, increments success count and closes circuit
    /// if threshold is reached.
    ///
    /// In Closed state, resets failure count.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.count = 0;
            }
            CircuitState::HalfOpen => {
                self.count += 1;
                if self.count >= self.config.success_threshold {
                    self.transition_to_closed();
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset anyway
                tracing::warn!("Recorded success while circuit open");
            }
        }
    }

    /// Record a failed operation
    ///
    /// In Closed state, increments failure count and opens circuit
    /// if threshold is reached.
    ///
    /// In Half-Open state, immediately reopens circuit.
    pub fn record_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.count += 1;
                if self.count >= self.config.failure_threshold {
                    self.transition_to_open();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open reopens circuit
                self.transition_to_open();
            }
            CircuitState::Open => {
                // Already open, just update timestamp
                self.last_state_change = Some(Instant::now());
            }
        }
    }

    /// Get current circuit state
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Get current count (failures in Closed, successes in Half-Open)
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Force circuit to open state (for testing/manual control)
    pub fn force_open(&mut self) {
        self.transition_to_open();
    }

    /// Force circuit to closed state (for testing/manual control)
    pub fn force_close(&mut self) {
        self.transition_to_closed();
    }

    /// Reset circuit to initial state
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.count = 0;
        self.last_state_change = None;
    }

    // Internal state transition methods

    fn transition_to_closed(&mut self) {
        tracing::info!("Circuit breaker: Open -> Closed");
        self.state = CircuitState::Closed;
        self.count = 0;
        self.last_state_change = Some(Instant::now());
    }

    fn transition_to_open(&mut self) {
        tracing::warn!("Circuit breaker: {} -> Open", self.state_name());
        self.state = CircuitState::Open;
        self.count = 0;
        self.last_state_change = Some(Instant::now());
    }

    fn transition_to_half_open(&mut self) {
        tracing::info!("Circuit breaker: Open -> Half-Open");
        self.state = CircuitState::HalfOpen;
        self.count = 0;
        self.last_state_change = Some(Instant::now());
    }

    fn state_name(&self) -> &'static str {
        match self.state {
            CircuitState::Closed => "Closed",
            CircuitState::Open => "Open",
            CircuitState::HalfOpen => "Half-Open",
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 3,
            half_open_timeout: Some(Duration::from_secs(10)),
        }
    }
}

impl CircuitBreakerConfig {
    /// Aggressive configuration (fail fast)
    pub fn aggressive() -> Self {
        Self {
            failure_threshold: 3,
            reset_timeout: Duration::from_secs(10),
            success_threshold: 2,
            half_open_timeout: Some(Duration::from_secs(5)),
        }
    }

    /// Lenient configuration (tolerate more failures)
    pub fn lenient() -> Self {
        Self {
            failure_threshold: 10,
            reset_timeout: Duration::from_secs(60),
            success_threshold: 5,
            half_open_timeout: Some(Duration::from_secs(20)),
        }
    }

    /// Create a custom configuration
    pub fn custom(failure_threshold: u32, reset_timeout: Duration, success_threshold: u32) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            success_threshold,
            half_open_timeout: None,
        }
    }
}

impl CircuitState {
    /// Check if circuit is closed
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// Check if circuit is open
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// Check if circuit is half-open
    pub fn is_half_open(&self) -> bool {
        matches!(self, Self::HalfOpen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_circuit_breaker_closed_to_open() {
        let config = CircuitBreakerConfig::custom(3, Duration::from_millis(100), 2);
        let mut breaker = CircuitBreaker::new(config);

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.can_execute());

        // Record failures
        breaker.record_failure();
        assert!(breaker.can_execute()); // Still closed
        breaker.record_failure();
        assert!(breaker.can_execute()); // Still closed
        breaker.record_failure();

        // Should be open now
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.can_execute());
    }

    #[test]
    fn test_circuit_breaker_open_to_half_open() {
        let config = CircuitBreakerConfig::custom(1, Duration::from_millis(50), 2);
        let mut breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for reset timeout
        sleep(Duration::from_millis(60));

        // Should transition to half-open
        assert!(breaker.can_execute());
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_half_open_to_closed() {
        let config = CircuitBreakerConfig::custom(1, Duration::from_millis(50), 2);
        let mut breaker = CircuitBreaker::new(config);

        // Move to half-open
        breaker.force_open();
        sleep(Duration::from_millis(60));
        assert!(breaker.can_execute());
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Record successes
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        breaker.record_success();

        // Should be closed now
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure() {
        let config = CircuitBreakerConfig::custom(1, Duration::from_millis(50), 2);
        let mut breaker = CircuitBreaker::new(config);

        // Move to half-open
        breaker.force_open();
        sleep(Duration::from_millis(60));
        assert!(breaker.can_execute());
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Record a failure - should reopen
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.can_execute());
    }

    #[test]
    fn test_circuit_breaker_success_resets_count() {
        let config = CircuitBreakerConfig::custom(3, Duration::from_secs(1), 2);
        let mut breaker = CircuitBreaker::new(config);

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.count(), 2);

        // Success should reset count
        breaker.record_success();
        assert_eq!(breaker.count(), 0);
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig::default());

        breaker.force_open();
        assert_eq!(breaker.state(), CircuitState::Open);

        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.count(), 0);
    }
}
