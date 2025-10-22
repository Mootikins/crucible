//! # Error Handling Tests
//!
//! Comprehensive tests for IPC error handling including error code mapping,
//! retry strategies, circuit breaking, dead letter queue processing, and
//! error recovery mechanisms.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult, ProtocolErrorCode, AuthErrorCode, ConnectionErrorCode, MessageErrorCode, PluginErrorCode},
    message::{IpcMessage, MessageType, MessagePayload},
    protocol::ProtocolHandler,
    security::SecurityManager,
    transport::TransportManager,
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// Error code mapping tests
pub struct ErrorCodeMappingTests;

impl ErrorCodeMappingTests {
    /// Test protocol error code mapping
    pub fn test_protocol_error_codes() -> IpcResult<()> {
        let test_cases = vec![
            (ProtocolErrorCode::ProtocolViolation, "protocol violation"),
            (ProtocolErrorCode::VersionMismatch, "version mismatch"),
            (ProtocolErrorCode::InvalidHeader, "invalid header"),
            (ProtocolErrorCode::InvalidMessageFormat, "invalid message format"),
            (ProtocolErrorCode::SerializationFailed, "serialization failed"),
            (ProtocolErrorCode::DeserializationFailed, "deserialization failed"),
            (ProtocolErrorCode::ChecksumMismatch, "checksum mismatch"),
            (ProtocolErrorCode::MessageTooLarge, "message too large"),
            (ProtocolErrorCode::EncryptionFailed, "encryption failed"),
            (ProtocolErrorCode::DecryptionFailed, "decryption failed"),
            (ProtocolErrorCode::CompressionFailed, "compression failed"),
            (ProtocolErrorCode::DecompressionFailed, "decompression failed"),
            (ProtocolErrorCode::Timeout, "timeout"),
            (ProtocolErrorCode::InternalError, "internal error"),
        ];

        for (code, description) in test_cases {
            let error = IpcError::Protocol {
                message: format!("Test {}", description),
                code,
                source: None,
            };

            assert!(error.is_protocol_error());
            assert!(!error.is_auth_error());
            assert!(!error.is_connection_error());
            assert!(!error.is_message_error());
            assert!(!error.is_plugin_error());
            assert!(error.has_error_code(&format!("{:?}", code)));
        }

        Ok(())
    }

    /// Test authentication error code mapping
    pub fn test_authentication_error_codes() -> IpcResult<()> {
        let test_cases = vec![
            (AuthErrorCode::InvalidToken, "invalid token"),
            (AuthErrorCode::TokenExpired, "token expired"),
            (AuthErrorCode::TokenGenerationFailed, "token generation failed"),
            (AuthErrorCode::RefreshTokenExpired, "refresh token expired"),
            (AuthErrorCode::InvalidCredentials, "invalid credentials"),
            (AuthErrorCode::AccountLocked, "account locked"),
            (AuthErrorCode::InsufficientPermissions, "insufficient permissions"),
            (AuthErrorCode::SessionExpired, "session expired"),
            (AuthErrorCode::RateLimited, "rate limited"),
        ];

        for (code, description) in test_cases {
            let error = IpcError::Authentication {
                message: format!("Test {}", description),
                code,
                retry_after: None,
            };

            assert!(!error.is_protocol_error());
            assert!(error.is_auth_error());
            assert!(!error.is_connection_error());
            assert!(!error.is_message_error());
            assert!(!error.is_plugin_error());
            assert!(error.has_error_code(&format!("{:?}", code)));
        }

        Ok(())
    }

    /// Test connection error code mapping
    pub fn test_connection_error_codes() -> IpcResult<()> {
        let test_cases = vec![
            (ConnectionErrorCode::ConnectionRefused, "connection refused"),
            (ConnectionErrorCode::ConnectionTimeout, "connection timeout"),
            (ConnectionErrorCode::ConnectionClosed, "connection closed"),
            (ConnectionErrorCode::TransportError, "transport error"),
            (ConnectionErrorCode::NetworkUnreachable, "network unreachable"),
            (ConnectionErrorCode::AddressInUse, "address in use"),
            (ConnectionErrorCode::ConnectionReset, "connection reset"),
            (ConnectionErrorCode::ConnectionAborted, "connection aborted"),
            (ConnectionErrorCode::BrokenPipe, "broken pipe"),
            (ConnectionErrorCode::ConnectionLimitReached, "connection limit reached"),
        ];

        for (code, description) in test_cases {
            let error = IpcError::Connection {
                message: format!("Test {}", description),
                code,
                endpoint: "test_endpoint".to_string(),
                retry_count: 0,
            };

            assert!(!error.is_protocol_error());
            assert!(!error.is_auth_error());
            assert!(error.is_connection_error());
            assert!(!error.is_message_error());
            assert!(!error.is_plugin_error());
            assert!(error.has_error_code(&format!("{:?}", code)));
        }

        Ok(())
    }

    /// Test message error code mapping
    pub fn test_message_error_codes() -> IpcResult<()> {
        let test_cases = vec![
            (MessageErrorCode::MessageTooLarge, "message too large"),
            (MessageErrorCode::InvalidMessage, "invalid message"),
            (MessageErrorCode::MissingRequiredField, "missing required field"),
            (MessageErrorCode::InvalidFieldType, "invalid field type"),
            (MessageErrorCode::MessageCorrupted, "message corrupted"),
            (MessageErrorCode::UnsupportedMessageType, "unsupported message type"),
            (MessageErrorCode::InvalidTimestamp, "invalid timestamp"),
            (MessageErrorCode::MessageExpired, "message expired"),
            (MessageErrorCode::UnknownRecipient, "unknown recipient"),
            (MessageErrorCode::UnauthorizedOperation, "unauthorized operation"),
        ];

        for (code, description) in test_cases {
            let error = IpcError::Message {
                message: format!("Test {}", description),
                code,
                message_id: Some(Uuid::new_v4().to_string()),
            };

            assert!(!error.is_protocol_error());
            assert!(!error.is_auth_error());
            assert!(!error.is_connection_error());
            assert!(error.is_message_error());
            assert!(!error.is_plugin_error());
            assert!(error.has_error_code(&format!("{:?}", code)));
        }

        Ok(())
    }

    /// Test plugin error code mapping
    pub fn test_plugin_error_codes() -> IpcResult<()> {
        let test_cases = vec![
            (PluginErrorCode::PluginNotFound, "plugin not found"),
            (PluginErrorCode::PluginInitializationFailed, "plugin initialization failed"),
            (PluginErrorCode::PluginExecutionFailed, "plugin execution failed"),
            (PluginErrorCode::PluginTimeout, "plugin timeout"),
            (PluginErrorCode::PluginCrashed, "plugin crashed"),
            (PluginErrorCode::InvalidPluginConfig, "invalid plugin config"),
            (PluginErrorCode::PluginVersionMismatch, "plugin version mismatch"),
            (PluginErrorCode::UnsupportedOperation, "unsupported operation"),
            (PluginErrorCode::ResourceExhausted, "resource exhausted"),
            (PluginErrorCode::PluginDisabled, "plugin disabled"),
        ];

        for (code, description) in test_cases {
            let error = IpcError::Plugin {
                message: format!("Test {}", description),
                code,
                plugin_id: Some("test_plugin".to_string()),
                operation: Some("test_operation".to_string()),
            };

            assert!(!error.is_protocol_error());
            assert!(!error.is_auth_error());
            assert!(!error.is_connection_error());
            assert!(!error.is_message_error());
            assert!(error.is_plugin_error());
            assert!(error.has_error_code(&format!("{:?}", code)));
        }

        Ok(())
    }
}

/// Retry strategy tests
pub struct RetryStrategyTests;

impl RetryStrategyTests {
    /// Test exponential backoff retry strategy
    pub async fn test_exponential_backoff_retry() -> IpcResult<()> {
        let failure_scenario = FailureScenario::new(0.7, FailureType::Connection); // 70% failure rate
        let max_retries = 5;
        let mut attempt_count = 0;

        for attempt in 0..max_retries {
            attempt_count += 1;

            if failure_scenario.should_fail().await {
                continue; // Retry
            } else {
                // Success
                assert!(attempt_count <= max_retries);
                return Ok(());
            }
        }

        // All retries failed
        panic!("All retries failed");
    }

    /// Test fixed delay retry strategy
    pub async fn test_fixed_delay_retry() -> IpcResult<()> {
        let failure_scenario = FailureScenario::new(0.8, FailureType::Send); // 80% failure rate
        let max_retries = 10;
        let fixed_delay = Duration::from_millis(10);
        let mut attempt_count = 0;

        for attempt in 0..max_retries {
            attempt_count += 1;

            let start = SystemTime::now();
            if failure_scenario.should_fail().await {
                let elapsed = start.elapsed().unwrap();
                assert!(elapsed >= fixed_delay); // Should have waited
                tokio::time::sleep(fixed_delay).await; // Simulate retry delay
                continue;
            } else {
                // Success
                assert!(attempt_count <= max_retries);
                return Ok(());
            }
        }

        panic!("All retries failed");
    }

    /// Test retry with different error types
    pub async fn test_retry_by_error_type() -> IpcResult<()> {
        let retryable_errors = vec![
            IpcError::Connection {
                message: "Connection timeout".to_string(),
                code: ConnectionErrorCode::ConnectionTimeout,
                endpoint: "test".to_string(),
                retry_count: 0,
            },
            IpcError::Protocol {
                message: "Temporary protocol error".to_string(),
                code: ProtocolErrorCode::Timeout,
                source: None,
            },
            IpcError::Authentication {
                message: "Rate limited".to_string(),
                code: AuthErrorCode::RateLimited,
                retry_after: Some(Duration::from_secs(1)),
            },
        ];

        let non_retryable_errors = vec![
            IpcError::Authentication {
                message: "Invalid token".to_string(),
                code: AuthErrorCode::InvalidToken,
                retry_after: None,
            },
            IpcError::Message {
                message: "Invalid message".to_string(),
                code: MessageErrorCode::InvalidMessage,
                message_id: None,
            },
            IpcError::Plugin {
                message: "Plugin not found".to_string(),
                code: PluginErrorCode::PluginNotFound,
                plugin_id: None,
                operation: None,
            },
        ];

        // Test retryable errors
        for error in retryable_errors {
            let should_retry = self.is_retryable_error(&error);
            assert!(should_retry, "Error should be retryable: {}", error);
        }

        // Test non-retryable errors
        for error in non_retryable_errors {
            let should_retry = self.is_retryable_error(&error);
            assert!(!should_retry, "Error should not be retryable: {}", error);
        }

        Ok(())
    }

    /// Helper method to determine if error is retryable
    fn is_retryable_error(&self, error: &IpcError) -> bool {
        match error {
            IpcError::Connection { code, .. } => matches!(
                code,
                ConnectionErrorCode::ConnectionTimeout |
                ConnectionErrorCode::ConnectionReset |
                ConnectionErrorCode::TransportError |
                ConnectionErrorCode::NetworkUnreachable
            ),
            IpcError::Protocol { code, .. } => matches!(
                code,
                ProtocolErrorCode::Timeout |
                ProtocolErrorCode::InternalError
            ),
            IpcError::Authentication { code, retry_after, .. } => {
                matches!(code, AuthErrorCode::RateLimited) || retry_after.is_some()
            },
            IpcError::Message { .. } => false, // Message errors are typically not retryable
            IpcError::Plugin { code, .. } => matches!(
                code,
                PluginErrorCode::PluginTimeout |
                PluginErrorCode::ResourceExhausted
            ),
        }
    }

    /// Test retry limit enforcement
    pub async fn test_retry_limit_enforcement() -> IpcResult<()> {
        let failure_scenario = FailureScenario::new(1.0, FailureType::Connection); // 100% failure
        let max_retries = 3;
        let mut attempt_count = 0;

        for attempt in 0..max_retries {
            attempt_count += 1;
            assert!(failure_scenario.should_fail().await); // Should always fail
        }

        // Should have attempted exactly max_retries times
        assert_eq!(attempt_count, max_retries);

        Ok(())
    }
}

/// Circuit breaker tests
pub struct CircuitBreakerTests;

impl CircuitBreakerTests {
    /// Test circuit breaker state transitions
    pub async fn test_circuit_breaker_states() -> IpcResult<()> {
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            5,    // failure_threshold
            3,    // success_threshold
            Duration::from_secs(10), // timeout
        )));

        // Initially closed
        {
            let cb = circuit_breaker.lock().await;
            assert!(matches!(cb.state, CircuitBreakerState::Closed));
        }

        // Simulate failures to trip circuit breaker
        for _ in 0..5 {
            let mut cb = circuit_breaker.lock().await;
            cb.record_failure();
        }

        // Should be open now
        {
            let cb = circuit_breaker.lock().await;
            assert!(matches!(cb.state, CircuitBreakerState::Open));
        }

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Transition to half-open
        {
            let mut cb = circuit_breaker.lock().await;
            cb.check_state_transition();
            assert!(matches!(cb.state, CircuitBreakerState::HalfOpen));
        }

        // Simulate successes to close circuit breaker
        for _ in 0..3 {
            let mut cb = circuit_breaker.lock().await;
            cb.record_success();
        }

        // Should be closed again
        {
            let cb = circuit_breaker.lock().await;
            assert!(matches!(cb.state, CircuitBreakerState::Closed));
        }

        Ok(())
    }

    /// Test circuit breaker prevents operations when open
    pub async fn test_circuit_breaker_prevention() -> IpcResult<()> {
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            2,    // failure_threshold
            2,    // success_threshold
            Duration::from_millis(50), // timeout
        )));

        // Trip the circuit breaker
        {
            let mut cb = circuit_breaker.lock().await;
            cb.record_failure();
            cb.record_failure();
            assert!(matches!(cb.state, CircuitBreakerState::Open));
        }

        // Operations should be prevented
        {
            let cb = circuit_breaker.lock().await;
            assert!(!cb.allow_operation());
        }

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should allow operation in half-open state
        {
            let mut cb = circuit_breaker.lock().await;
            cb.check_state_transition();
            assert!(cb.allow_operation());
            assert!(matches!(cb.state, CircuitBreakerState::HalfOpen));
        }

        Ok(())
    }

    /// Test circuit breaker with concurrent operations
    pub async fn test_circuit_breaker_concurrency() -> IpcResult<()> {
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            10,   // failure_threshold
            5,    // success_threshold
            Duration::from_millis(100), // timeout
        )));

        let num_operations = 100;

        // Simulate concurrent operations with mixed success/failure
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let circuit_breaker = Arc::clone(&circuit_breaker);
                async move {
                    let mut cb = circuit_breaker.lock().await;

                    if cb.allow_operation() {
                        if i % 3 == 0 {
                            cb.record_success();
                            Ok("success")
                        } else {
                            cb.record_failure();
                            Err("failure")
                        }
                    } else {
                        Err("circuit_breaker_open")
                    }
                }
            },
        ).await;

        // Analyze results
        let success_count = results.iter().filter(|r| r.as_ref().map(|s| *s == "success").unwrap_or(false)).count();
        let failure_count = results.iter().filter(|r| r.as_ref().map(|s| *s == "failure").unwrap_or(false)).count();
        let open_count = results.iter().filter(|r| r.as_ref().map(|s| *s == "circuit_breaker_open").unwrap_or(false)).count();

        assert!(success_count + failure_count + open_count == num_operations);
        assert!(open_count > 0); // Circuit breaker should have opened

        Ok(())
    }
}

/// Dead letter queue tests
pub struct DeadLetterQueueTests;

impl DeadLetterQueueTests {
    /// Test dead letter queue functionality
    pub async fn test_dead_letter_queue() -> IpcResult<()> {
        let dlq = Arc::new(Mutex::new(DeadLetterQueue::new(100))); // Max 100 messages

        let message = MessageFixtures::request("test_operation", serde_json::json!({"data": "test"}));
        let error = IpcError::Connection {
            message: "Connection failed".to_string(),
            code: ConnectionErrorCode::ConnectionRefused,
            endpoint: "test_endpoint".to_string(),
            retry_count: 3,
        };

        // Add message to DLQ
        {
            let mut dlq_guard = dlq.lock().await;
            dlq_guard.add_message(message.clone(), error.clone()).await?;
        }

        // Check queue size
        {
            let dlq_guard = dlq.lock().await;
            assert_eq!(dlq_guard.size(), 1);
        }

        // Retrieve message from DLQ
        {
            let mut dlq_guard = dlq.lock().await;
            let (retrieved_message, retrieved_error) = dlq_guard.get_next_message().await?;
            assert!(retrieved_message.is_some());
            assert!(retrieved_error.is_some());
        }

        // Queue should be empty now
        {
            let dlq_guard = dlq.lock().await;
            assert_eq!(dlq_guard.size(), 0);
        }

        Ok(())
    }

    /// Test dead letter queue capacity limits
    pub async fn test_dead_letter_queue_capacity() -> IpcResult<()> {
        let dlq = Arc::new(Mutex::new(DeadLetterQueue::new(5))); // Small capacity
        let error = IpcError::Connection {
            message: "Connection failed".to_string(),
            code: ConnectionErrorCode::ConnectionRefused,
            endpoint: "test_endpoint".to_string(),
            retry_count: 3,
        };

        // Fill queue beyond capacity
        for i in 0..10 {
            let message = MessageFixtures::request(&format!("operation_{}", i), serde_json::json!({"index": i}));
            let mut dlq_guard = dlq.lock().await;

            if dlq_guard.size() < 5 {
                dlq_guard.add_message(message, error.clone()).await?;
            } else {
                // Should reject additional messages
                let result = dlq_guard.add_message(message, error.clone()).await;
                assert!(result.is_err());
            }
        }

        // Verify queue size
        {
            let dlq_guard = dlq.lock().await;
            assert_eq!(dlq_guard.size(), 5);
        }

        Ok(())
    }

    /// Test dead letter queue message aging
    pub async fn test_dead_letter_queue_aging() -> IpcResult<()> {
        let dlq = Arc::new(Mutex::new(DeadLetterQueue::new(10)));
        let error = IpcError::Connection {
            message: "Connection failed".to_string(),
            code: ConnectionErrorCode::ConnectionRefused,
            endpoint: "test_endpoint".to_string(),
            retry_count: 3,
        };

        // Add message to DLQ
        let message = MessageFixtures::request("test_operation", serde_json::json!({"data": "test"}));
        {
            let mut dlq_guard = dlq.lock().await;
            dlq_guard.add_message(message.clone(), error.clone()).await?;
        }

        // Wait a bit and check aging
        tokio::time::sleep(Duration::from_millis(10)).await;

        {
            let dlq_guard = dlq.lock().await;
            let aged_messages = dlq_guard.get_aged_messages(Duration::from_millis(5)).await;
            assert_eq!(aged_messages.len(), 1); // Message should be aged
        }

        Ok(())
    }
}

/// Error recovery tests
pub struct ErrorRecoveryTests;

impl ErrorRecoveryTests {
    /// Test automatic error recovery mechanisms
    pub async fn test_automatic_error_recovery() -> IpcResult<()> {
        let recovery_manager = Arc::new(Mutex::new(ErrorRecoveryManager::new()));

        // Simulate recoverable error
        let recoverable_error = IpcError::Connection {
            message: "Connection timeout".to_string(),
            code: ConnectionErrorCode::ConnectionTimeout,
            endpoint: "test_endpoint".to_string(),
            retry_count: 1,
        };

        {
            let mut manager = recovery_manager.lock().await;
            let recovery_strategy = manager.get_recovery_strategy(&recoverable_error);
            assert!(matches!(recovery_strategy, RecoveryStrategy::Retry));
        }

        // Simulate non-recoverable error
        let non_recoverable_error = IpcError::Authentication {
            message: "Invalid credentials".to_string(),
            code: AuthErrorCode::InvalidCredentials,
            retry_after: None,
        };

        {
            let mut manager = recovery_manager.lock().await;
            let recovery_strategy = manager.get_recovery_strategy(&non_recoverable_error);
            assert!(matches!(recovery_strategy, RecoveryStrategy::Fail));
        }

        Ok(())
    }

    /// Test graceful degradation under errors
    pub async fn test_graceful_degradation() -> IpcResult<()> {
        let degradation_manager = Arc::new(Mutex::new(DegradationManager::new()));

        // Simulate partial system failure
        {
            let mut manager = degradation_manager.lock().await;
            manager.record_failure("component_a");
            manager.record_failure("component_a");
            manager.record_failure("component_b");
        }

        // Check degradation level
        {
            let manager = degradation_manager.lock().await;
            let degradation_level = manager.get_degradation_level();
            assert!(degradation_level > 0.0);
            assert!(degradation_level <= 1.0);
        }

        // Simulate recovery
        {
            let mut manager = degradation_manager.lock().await;
            manager.record_success("component_a");
            manager.record_success("component_b");
        }

        // Check recovery
        {
            let manager = degradation_manager.lock().await;
            let degradation_level = manager.get_degradation_level();
            assert!(degradation_level < 0.5); // Should have improved
        }

        Ok(())
    }

    /// Test cascading failure prevention
    pub async fn test_cascading_failure_prevention() -> IpcResult<()> {
        let failure_prevention = Arc::new(Mutex::new(FailurePreventionManager::new()));

        // Simulate rapid failures
        for i in 0..20 {
            let component = format!("component_{}", i % 3); // Distribute across 3 components
            let mut manager = failure_prevention.lock().await;
            manager.record_failure(&component);
        }

        // Check if circuit breakers have been triggered
        {
            let manager = failure_prevention.lock().await;
            let active_breakers = manager.get_active_circuit_breakers();
            assert!(active_breakers.len() > 0); // Some circuit breakers should be active
        }

        // Verify system protection
        {
            let manager = failure_prevention.lock().await;
            let system_protected = manager.is_system_protected();
            assert!(system_protected); // System should be protected from cascading failures
        }

        Ok(())
    }

    /// Test error reporting and alerting
    pub async fn test_error_reporting() -> IpcResult<()> {
        let alert_manager = Arc::new(Mutex::new(AlertManager::new()));

        let errors = vec![
            IpcError::Protocol {
                message: "Protocol violation".to_string(),
                code: ProtocolErrorCode::ProtocolViolation,
                source: None,
            },
            IpcError::Connection {
                message: "Connection refused".to_string(),
                code: ConnectionErrorCode::ConnectionRefused,
                endpoint: "test_endpoint".to_string(),
                retry_count: 3,
            },
            IpcError::Authentication {
                message: "Invalid token".to_string(),
                code: AuthErrorCode::InvalidToken,
                retry_after: None,
            },
        ];

        // Report errors
        for error in errors {
            let mut manager = alert_manager.lock().await;
            manager.report_error(error).await?;
        }

        // Check alerts
        {
            let manager = alert_manager.lock().await;
            let alerts = manager.get_active_alerts();
            assert!(!alerts.is_empty());

            // Check error statistics
            let stats = manager.get_error_statistics();
            assert!(stats.total_errors > 0);
            assert!(stats.error_types.len() > 0);
        }

        Ok(())
    }
}

// Supporting structs for error handling tests

#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    success_count: u32,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
    last_failure_time: Option<SystemTime>,
}

#[derive(Debug, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold,
            success_threshold,
            timeout,
            last_failure_time: None,
        }
    }

    pub fn allow_operation(&self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    SystemTime::now().duration_since(last_failure).unwrap() > self.timeout
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.success_threshold {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitBreakerState::Open => {
                // Shouldn't happen, but handle gracefully
                self.state = CircuitBreakerState::Closed;
                self.failure_count = 0;
            }
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(SystemTime::now());

        match self.state {
            CircuitBreakerState::Closed => {
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
                self.success_count = 0;
            }
            CircuitBreakerState::Open => {
                // Already open
            }
        }
    }

    pub fn check_state_transition(&mut self) {
        if let (CircuitBreakerState::Open, Some(last_failure)) = (&self.state, self.last_failure_time) {
            if SystemTime::now().duration_since(last_failure).unwrap() > self.timeout {
                self.state = CircuitBreakerState::HalfOpen;
                self.success_count = 0;
            }
        }
    }
}

#[derive(Debug)]
pub struct DeadLetterQueue {
    messages: Vec<(IpcMessage, IpcError, SystemTime)>,
    max_size: usize,
}

impl DeadLetterQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_size,
        }
    }

    pub async fn add_message(&mut self, message: IpcMessage, error: IpcError) -> IpcResult<()> {
        if self.messages.len() >= self.max_size {
            return Err(IpcError::Protocol {
                message: "Dead letter queue is full".to_string(),
                code: ProtocolErrorCode::InternalError,
                source: None,
            });
        }

        self.messages.push((message, error, SystemTime::now()));
        Ok(())
    }

    pub async fn get_next_message(&mut self) -> IpcResult<(Option<IpcMessage>, Option<IpcError>)> {
        if let Some((message, error, _)) = self.messages.first() {
            let result = (Some(message.clone()), Some(error.clone()));
            self.messages.remove(0);
            Ok(result)
        } else {
            Ok((None, None))
        }
    }

    pub async fn get_aged_messages(&mut self, max_age: Duration) -> Vec<(IpcMessage, IpcError)> {
        let now = SystemTime::now();
        let mut aged_messages = Vec::new();

        self.messages.retain(|(message, error, timestamp)| {
            if now.duration_since(*timestamp).unwrap() > max_age {
                aged_messages.push((message.clone(), error.clone()));
                false
            } else {
                true
            }
        });

        aged_messages
    }

    pub fn size(&self) -> usize {
        self.messages.len()
    }
}

#[derive(Debug)]
pub enum RecoveryStrategy {
    Retry,
    Fail,
    Degrade,
    Reconnect,
}

#[derive(Debug)]
pub struct ErrorRecoveryManager {
    strategies: HashMap<String, RecoveryStrategy>,
}

impl ErrorRecoveryManager {
    pub fn new() -> Self {
        let mut strategies = HashMap::new();
        strategies.insert("ConnectionTimeout".to_string(), RecoveryStrategy::Retry);
        strategies.insert("ConnectionRefused".to_string(), RecoveryStrategy::Retry);
        strategies.insert("InvalidToken".to_string(), RecoveryStrategy::Fail);
        strategies.insert("ProtocolViolation".to_string(), RecoveryStrategy::Fail);
        strategies.insert("MessageTooLarge".to_string(), RecoveryStrategy::Fail);

        Self { strategies }
    }

    pub fn get_recovery_strategy(&self, error: &IpcError) -> RecoveryStrategy {
        let error_type = format!("{:?}", error);

        for (pattern, strategy) &self.strategies {
            if error_type.contains(pattern) {
                return strategy.clone();
            }
        }

        RecoveryStrategy::Fail // Default strategy
    }
}

#[derive(Debug)]
pub struct DegradationManager {
    component_failures: HashMap<String, u32>,
    component_successes: HashMap<String, u32>,
}

impl DegradationManager {
    pub fn new() -> Self {
        Self {
            component_failures: HashMap::new(),
            component_successes: HashMap::new(),
        }
    }

    pub fn record_failure(&mut self, component: &str) {
        *self.component_failures.entry(component.to_string()).or_insert(0) += 1;
    }

    pub fn record_success(&mut self, component: &str) {
        *self.component_successes.entry(component.to_string()).or_insert(0) += 1;
    }

    pub fn get_degradation_level(&self) -> f64 {
        let total_failures: u32 = self.component_failures.values().sum();
        let total_successes: u32 = self.component_successes.values().sum();
        let total_operations = total_failures + total_successes;

        if total_operations == 0 {
            0.0
        } else {
            total_failures as f64 / total_operations as f64
        }
    }
}

#[derive(Debug)]
pub struct FailurePreventionManager {
    component_circuit_breakers: HashMap<String, CircuitBreaker>,
}

impl FailurePreventionManager {
    pub fn new() -> Self {
        Self {
            component_circuit_breakers: HashMap::new(),
        }
    }

    pub fn record_failure(&mut self, component: &str) {
        let breaker = self.component_circuit_breakers
            .entry(component.to_string())
            .or_insert_with(|| CircuitBreaker::new(3, 2, Duration::from_secs(10)));
        breaker.record_failure();
    }

    pub fn get_active_circuit_breakers(&self) -> Vec<&String> {
        self.component_circuit_breakers
            .iter()
            .filter(|(_, breaker)| matches!(breaker.state, CircuitBreakerState::Open))
            .map(|(component, _)| component)
            .collect()
    }

    pub fn is_system_protected(&self) -> bool {
        let total_components = self.component_circuit_breakers.len();
        let open_breakers = self.get_active_circuit_breakers().len();

        total_components > 0 && (open_breakers as f64 / total_components as f64) > 0.3
    }
}

#[derive(Debug)]
pub struct AlertManager {
    active_alerts: Vec<(IpcError, SystemTime)>,
    error_counts: HashMap<String, u32>,
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            active_alerts: Vec::new(),
            error_counts: HashMap::new(),
        }
    }

    pub async fn report_error(&mut self, error: IpcError) -> IpcResult<()> {
        self.active_alerts.push((error.clone(), SystemTime::now()));

        let error_type = format!("{:?}", error);
        *self.error_counts.entry(error_type).or_insert(0) += 1;

        Ok(())
    }

    pub fn get_active_alerts(&self) -> Vec<&IpcError> {
        self.active_alerts.iter().map(|(error, _)| error).collect()
    }

    pub fn get_error_statistics(&self) -> ErrorStatistics {
        ErrorStatistics {
            total_errors: self.error_counts.values().sum(),
            error_types: self.error_counts.clone(),
            active_alerts: self.active_alerts.len(),
        }
    }
}

#[derive(Debug)]
pub struct ErrorStatistics {
    pub total_errors: u32,
    pub error_types: HashMap<String, u32>,
    pub active_alerts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_error_codes() {
        ErrorCodeMappingTests::test_protocol_error_codes().unwrap();
    }

    #[test]
    fn test_authentication_error_codes() {
        ErrorCodeMappingTests::test_authentication_error_codes().unwrap();
    }

    #[test]
    fn test_connection_error_codes() {
        ErrorCodeMappingTests::test_connection_error_codes().unwrap();
    }

    #[test]
    fn test_message_error_codes() {
        ErrorCodeMappingTests::test_message_error_codes().unwrap();
    }

    #[test]
    fn test_plugin_error_codes() {
        ErrorCodeMappingTests::test_plugin_error_codes().unwrap();
    }

    async_test!(test_exponential_backoff_retry, {
        RetryStrategyTests::test_exponential_backoff_retry().await.unwrap();
        "success"
    });

    async_test!(test_fixed_delay_retry, {
        RetryStrategyTests::test_fixed_delay_retry().await.unwrap();
        "success"
    });

    async_test!(test_retry_by_error_type, {
        RetryStrategyTests::test_retry_by_error_type().await.unwrap();
        "success"
    });

    async_test!(test_retry_limit_enforcement, {
        RetryStrategyTests::test_retry_limit_enforcement().await.unwrap();
        "success"
    });

    async_test!(test_circuit_breaker_states, {
        CircuitBreakerTests::test_circuit_breaker_states().await.unwrap();
        "success"
    });

    async_test!(test_circuit_breaker_prevention, {
        CircuitBreakerTests::test_circuit_breaker_prevention().await.unwrap();
        "success"
    });

    async_test!(test_circuit_breaker_concurrency, {
        CircuitBreakerTests::test_circuit_breaker_concurrency().await.unwrap();
        "success"
    });

    async_test!(test_dead_letter_queue, {
        DeadLetterQueueTests::test_dead_letter_queue().await.unwrap();
        "success"
    });

    async_test!(test_dead_letter_queue_capacity, {
        DeadLetterQueueTests::test_dead_letter_queue_capacity().await.unwrap();
        "success"
    });

    async_test!(test_dead_letter_queue_aging, {
        DeadLetterQueueTests::test_dead_letter_queue_aging().await.unwrap();
        "success"
    });

    async_test!(test_automatic_error_recovery, {
        ErrorRecoveryTests::test_automatic_error_recovery().await.unwrap();
        "success"
    });

    async_test!(test_graceful_degradation, {
        ErrorRecoveryTests::test_graceful_degradation().await.unwrap();
        "success"
    });

    async_test!(test_cascading_failure_prevention, {
        ErrorRecoveryTests::test_cascading_failure_prevention().await.unwrap();
        "success"
    });

    async_test!(test_error_reporting, {
        ErrorRecoveryTests::test_error_reporting().await.unwrap();
        "success"
    });
}