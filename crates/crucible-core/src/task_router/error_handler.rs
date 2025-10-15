use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use std::collections::HashMap;
use uuid::Uuid;
use rand::Rng;

use super::types::*;

/// Error Handler & Recovery System - handles task failures and implements recovery strategies
#[derive(Debug)]
pub struct ErrorHandler {
    /// Error patterns and their recovery strategies
    error_patterns: HashMap<String, ErrorPattern>,
    /// Recovery strategies
    recovery_strategies: HashMap<String, RecoveryStrategy>,
    /// Error history
    error_history: Vec<ErrorRecord>,
    /// Circuit breaker status for agents
    circuit_breakers: HashMap<Uuid, CircuitBreaker>,
    /// Retry policies
    retry_policies: HashMap<String, RetryPolicy>,
    /// Error statistics
    stats: ErrorStatistics,
    /// Configuration
    config: ErrorHandlerConfig,
}

/// Pattern for recognizing specific types of errors
#[derive(Debug, Clone)]
struct ErrorPattern {
    /// Pattern name
    name: String,
    /// Regex pattern to match error messages
    pattern: String,
    /// Error category
    category: ErrorCategory,
    /// Severity level
    severity: ErrorSeverity,
    /// Suggested recovery actions
    recovery_actions: Vec<String>,
    /// Default recovery strategy
    default_strategy: String,
}

/// Recovery strategy for handling errors
#[derive(Debug, Clone)]
struct RecoveryStrategy {
    /// Strategy name
    name: String,
    /// Strategy type
    strategy_type: RecoveryType,
    /// Maximum retry attempts
    max_retries: u8,
    /// Retry delay configuration
    retry_delay: RetryDelay,
    /// Fallback actions
    fallback_actions: Vec<FallbackAction>,
    /// Success criteria
    success_criteria: Vec<String>,
}

/// Type of recovery strategy
#[derive(Debug, Clone, PartialEq)]
enum RecoveryType {
    /// Simple retry with exponential backoff
    Retry,
    /// Try with different agent
    SwitchAgent,
    /// Break down task into smaller parts
    DecomposeTask,
    /// Use alternative approach
    AlternativeApproach,
    /// Request human intervention
    HumanIntervention,
    /// Graceful degradation
    GracefulDegradation,
    /// Circuit breaker (temporarily stop trying)
    CircuitBreaker,
}

/// Retry delay configuration
#[derive(Debug, Clone)]
struct RetryDelay {
    /// Initial delay in milliseconds
    initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    max_delay_ms: u64,
    /// Backoff multiplier
    backoff_multiplier: f32,
    /// Jitter factor (0-1)
    jitter_factor: f32,
}

/// Fallback action when recovery fails
#[derive(Debug, Clone)]
struct FallbackAction {
    /// Action name
    name: String,
    /// Action type
    action_type: FallbackType,
    /// Action parameters
    parameters: HashMap<String, String>,
}

/// Type of fallback action
#[derive(Debug, Clone)]
enum FallbackType {
    /// Return default response
    DefaultResponse,
    /// Return partial results
    PartialResults,
    /// Escalate to human
    EscalateToHuman,
    /// Try different tools
    TryDifferentTools,
    /// Skip task
    SkipTask,
}

/// Circuit breaker for failing agents
#[derive(Debug, Clone)]
struct CircuitBreaker {
    /// Agent ID
    agent_id: Uuid,
    /// Current state
    state: CircuitBreakerState,
    /// Failure count
    failure_count: u32,
    /// Success count
    success_count: u32,
    /// Last failure time
    last_failure_time: Option<DateTime<Utc>>,
    /// Last success time
    last_success_time: Option<DateTime<Utc>>,
    /// Configuration
    config: CircuitBreakerConfig,
}

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (rejecting requests)
    Open,
    /// Circuit is half-open (testing if recovered)
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    failure_threshold: u32,
    /// Success threshold to close circuit
    success_threshold: u32,
    /// Timeout to try half-open state
    timeout_ms: u64,
}

/// Error record for tracking
#[derive(Debug, Clone)]
struct ErrorRecord {
    /// Error ID
    error_id: Uuid,
    /// Task ID
    task_id: Uuid,
    /// Agent ID
    agent_id: Uuid,
    /// Error type
    error_type: ErrorType,
    /// Error message
    message: String,
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Recovery attempted
    recovery_attempted: bool,
    /// Recovery successful
    recovery_successful: Option<bool>,
    /// Recovery strategy used
    recovery_strategy: Option<String>,
}

/// Retry policy for different error types
#[derive(Debug, Clone)]
struct RetryPolicy {
    /// Policy name
    name: String,
    /// Applicable error types
    error_types: Vec<ErrorType>,
    /// Maximum retry attempts
    max_attempts: u8,
    /// Retry delay strategy
    delay_strategy: RetryDelay,
    /// Conditions for retry
    retry_conditions: Vec<String>,
}

/// Error statistics
#[derive(Debug, Clone, Default)]
struct ErrorStatistics {
    /// Total errors encountered
    total_errors: u64,
    /// Errors by type
    errors_by_type: HashMap<ErrorType, u64>,
    /// Errors by agent
    errors_by_agent: HashMap<Uuid, u64>,
    /// Recovery success rate
    recovery_success_rate: f32,
    /// Average recovery time
    avg_recovery_time_ms: u64,
    /// Most common error patterns
    common_patterns: Vec<(String, u64)>,
}

/// Error handler configuration
#[derive(Debug, Clone)]
struct ErrorHandlerConfig {
    /// Enable automatic recovery
    enable_auto_recovery: bool,
    /// Maximum retry attempts globally
    global_max_retries: u8,
    /// Error timeout in milliseconds
    error_timeout_ms: u64,
    /// Enable circuit breakers
    enable_circuit_breakers: bool,
    /// Error retention period in days
    error_retention_days: u32,
}

impl ErrorHandler {
    /// Create a new error handler
    pub fn new() -> Self {
        let config = ErrorHandlerConfig::default();

        let mut handler = Self {
            error_patterns: HashMap::new(),
            recovery_strategies: HashMap::new(),
            error_history: Vec::new(),
            circuit_breakers: HashMap::new(),
            retry_policies: HashMap::new(),
            stats: ErrorStatistics::default(),
            config,
        };

        handler.initialize_error_patterns();
        handler.initialize_recovery_strategies();
        handler.initialize_retry_policies();

        handler
    }

    /// Handle an error that occurred during task execution
    pub async fn handle_error(&mut self, task_id: &Uuid, agent_id: &Uuid,
                            error: &TaskError) -> Result<ErrorHandlingResult> {
        let error_id = Uuid::new_v4();
        let start_time = Utc::now();

        // Step 1: Record the error
        self.record_error(error_id, *task_id, *agent_id, error).await;

        // Step 2: Identify error pattern and category
        let pattern = self.identify_error_pattern(error).await?;

        // Step 3: Select appropriate recovery strategy
        let strategy = self.select_recovery_strategy(&pattern, error).await?;

        // Step 4: Check circuit breaker status
        if self.config.enable_circuit_breakers && self.is_circuit_breaker_open(agent_id).await? {
            return Ok(ErrorHandlingResult {
                error_id,
                action: ErrorAction::SkipTask,
                message: "Agent is temporarily unavailable due to repeated failures".to_string(),
                retry_after: Some(self.get_circuit_breaker_retry_time(agent_id).await?),
                fallback_result: Some("Task skipped due to agent unavailability".to_string()),
            });
        }

        // Step 5: Attempt recovery
        let recovery_result = self.attempt_recovery(*task_id, *agent_id, error, &strategy).await?;

        // Step 6: Update statistics and circuit breakers
        let handling_time = Utc::now().signed_duration_since(start_time).num_milliseconds() as u64;
        self.update_error_statistics(&pattern, &recovery_result, handling_time).await;
        self.update_circuit_breaker(*agent_id, &recovery_result).await;

        tracing::info!("Error handling completed for task {} - action: {:?}", task_id, recovery_result.action);
        Ok(recovery_result)
    }

    /// Record error in history
    async fn record_error(&mut self, error_id: Uuid, task_id: Uuid, agent_id: Uuid, error: &TaskError) {
        let record = ErrorRecord {
            error_id,
            task_id,
            agent_id,
            error_type: error.error_type.clone(),
            message: error.message.clone(),
            timestamp: Utc::now(),
            recovery_attempted: false,
            recovery_successful: None,
            recovery_strategy: None,
        };

        self.error_history.push(record.clone());
        self.stats.total_errors += 1;

        // Update error counts
        *self.stats.errors_by_type.entry(error.error_type.clone()).or_insert(0) += 1;
        *self.stats.errors_by_agent.entry(agent_id).or_insert(0) += 1;

        // Cleanup old errors if needed
        self.cleanup_old_errors().await;
    }

    /// Identify error pattern from error message
    async fn identify_error_pattern(&self, error: &TaskError) -> Result<ErrorPattern> {
        let message_lower = error.message.to_lowercase();

        // Try to match against known patterns
        for pattern in self.error_patterns.values() {
            // Simple pattern matching (would use regex in production)
            if message_lower.contains(&pattern.pattern.to_lowercase()) {
                return Ok(pattern.clone());
            }
        }

        // Default pattern for unknown errors
        Ok(ErrorPattern {
            name: "Unknown Error".to_string(),
            pattern: ".*".to_string(),
            category: ErrorCategory::Unknown,
            severity: ErrorSeverity::Medium,
            recovery_actions: vec!["Retry with different approach".to_string()],
            default_strategy: "retry".to_string(),
        })
    }

    /// Select recovery strategy based on error pattern
    async fn select_recovery_strategy(&self, pattern: &ErrorPattern, error: &TaskError) -> Result<RecoveryStrategy> {
        let strategy_name = if error.recoverable {
            &pattern.default_strategy
        } else {
            "graceful_degradation"
        };

        self.recovery_strategies.get(strategy_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Recovery strategy not found: {}", strategy_name))
    }

    /// Check if circuit breaker is open for an agent
    async fn is_circuit_breaker_open(&self, agent_id: &Uuid) -> Result<bool> {
        if let Some(breaker) = self.circuit_breakers.get(agent_id) {
            Ok(breaker.state == CircuitBreakerState::Open)
        } else {
            Ok(false)
        }
    }

    /// Get retry time from circuit breaker
    async fn get_circuit_breaker_retry_time(&self, agent_id: &Uuid) -> Result<DateTime<Utc>> {
        if let Some(breaker) = self.circuit_breakers.get(agent_id) {
            if let Some(last_failure) = breaker.last_failure_time {
                Ok(last_failure + Duration::milliseconds(breaker.config.timeout_ms as i64))
            } else {
                Ok(Utc::now() + Duration::minutes(5))
            }
        } else {
            Ok(Utc::now() + Duration::minutes(1))
        }
    }

    /// Attempt error recovery
    async fn attempt_recovery(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                            strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        match strategy.strategy_type {
            RecoveryType::Retry => {
                self.attempt_retry(task_id, agent_id, error, strategy).await
            }
            RecoveryType::SwitchAgent => {
                self.attempt_agent_switch(task_id, agent_id, error, strategy).await
            }
            RecoveryType::DecomposeTask => {
                self.attempt_task_decomposition(task_id, agent_id, error, strategy).await
            }
            RecoveryType::AlternativeApproach => {
                self.attempt_alternative_approach(task_id, agent_id, error, strategy).await
            }
            RecoveryType::HumanIntervention => {
                self.attempt_human_intervention(task_id, agent_id, error, strategy).await
            }
            RecoveryType::GracefulDegradation => {
                self.attempt_graceful_degradation(task_id, agent_id, error, strategy).await
            }
            RecoveryType::CircuitBreaker => {
                self.attempt_circuit_breaker(task_id, agent_id, error, strategy).await
            }
        }
    }

    /// Attempt retry recovery
    async fn attempt_retry(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                         strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        let delay = self.calculate_retry_delay(strategy, 1).await?;

        if error.recoverable && strategy.max_retries > 0 {
            Ok(ErrorHandlingResult {
                error_id: Uuid::new_v4(),
                action: ErrorAction::Retry,
                message: format!("Retrying task after {}ms delay", delay),
                retry_after: Some(Utc::now() + Duration::milliseconds(delay as i64)),
                fallback_result: None,
            })
        } else {
            Ok(ErrorHandlingResult {
                error_id: Uuid::new_v4(),
                action: ErrorAction::Fail,
                message: "Retry not possible - error is not recoverable or max retries exceeded".to_string(),
                retry_after: None,
                fallback_result: Some("Task failed after recovery attempts".to_string()),
            })
        }
    }

    /// Attempt agent switch recovery
    async fn attempt_agent_switch(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::SwitchAgent,
            message: "Switching to different agent for task execution".to_string(),
            retry_after: Some(Utc::now() + Duration::seconds(10)),
            fallback_result: None,
        })
    }

    /// Attempt task decomposition recovery
    async fn attempt_task_decomposition(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                     strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::DecomposeTask,
            message: "Breaking down task into smaller subtasks".to_string(),
            retry_after: Some(Utc::now() + Duration::seconds(5)),
            fallback_result: None,
        })
    }

    /// Attempt alternative approach recovery
    async fn attempt_alternative_approach(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                       strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::AlternativeApproach,
            message: "Trying alternative approach to complete task".to_string(),
            retry_after: Some(Utc::now() + Duration::seconds(15)),
            fallback_result: None,
        })
    }

    /// Attempt human intervention recovery
    async fn attempt_human_intervention(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                     strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::HumanIntervention,
            message: "Task requires human intervention to proceed".to_string(),
            retry_after: None,
            fallback_result: Some("Task paused pending human review".to_string()),
        })
    }

    /// Attempt graceful degradation recovery
    async fn attempt_graceful_degradation(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                       strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::GracefulDegradation,
            message: "Providing partial results due to error".to_string(),
            retry_after: None,
            fallback_result: Some("Partial results provided - some functionality may be limited".to_string()),
        })
    }

    /// Attempt circuit breaker recovery
    async fn attempt_circuit_breaker(&self, task_id: Uuid, agent_id: Uuid, error: &TaskError,
                                   strategy: &RecoveryStrategy) -> Result<ErrorHandlingResult> {
        Ok(ErrorHandlingResult {
            error_id: Uuid::new_v4(),
            action: ErrorAction::CircuitBreaker,
            message: "Agent temporarily disabled due to repeated failures".to_string(),
            retry_after: Some(Utc::now() + Duration::minutes(5)),
            fallback_result: Some("Task deferred due to agent unavailability".to_string()),
        })
    }

    /// Calculate retry delay with exponential backoff
    async fn calculate_retry_delay(&self, strategy: &RecoveryStrategy, attempt: u8) -> Result<u64> {
        let base_delay = strategy.retry_delay.initial_delay_ms;
        let delay = base_delay * strategy.retry_delay.backoff_multiplier.powi(attempt as i32);
        let jittered_delay = delay * (1.0 + strategy.retry_delay.jitter_factor * rand::random::<f32>());

        Ok(jittered_delay as u64)
    }

    /// Update error statistics
    async fn update_error_statistics(&mut self, pattern: &ErrorPattern, result: &ErrorHandlingResult,
                                  handling_time: u64) {
        // Update recovery success rate
        let successful = matches!(result.action, ErrorAction::Retry | ErrorAction::SwitchAgent |
                                 ErrorAction::AlternativeApproach | ErrorAction::DecomposeTask);

        if self.stats.total_errors > 0 {
            self.stats.recovery_success_rate = (self.stats.recovery_success_rate * 0.9) +
                (if successful { 1.0 } else { 0.0 }) * 0.1;
        }

        // Update average recovery time
        if self.stats.avg_recovery_time_ms == 0 {
            self.stats.avg_recovery_time_ms = handling_time;
        } else {
            self.stats.avg_recovery_time_ms = ((self.stats.avg_recovery_time_ms as f64 * 0.9) + (handling_time as f64 * 0.1)) as u64;
        }
    }

    /// Update circuit breaker status
    async fn update_circuit_breaker(&mut self, agent_id: Uuid, result: &ErrorHandlingResult) {
        if !self.config.enable_circuit_breakers {
            return;
        }

        let breaker = self.circuit_breakers.entry(agent_id)
            .or_insert_with(|| CircuitBreaker::new(agent_id));

        let is_failure = matches!(result.action, ErrorAction::Fail | ErrorAction::CircuitBreaker);

        if is_failure {
            breaker.failure_count += 1;
            breaker.last_failure_time = Some(Utc::now());

            if breaker.failure_count >= breaker.config.failure_threshold {
                breaker.state = CircuitBreakerState::Open;
                tracing::warn!("Circuit breaker opened for agent {}", agent_id);
            }
        } else {
            breaker.success_count += 1;
            breaker.last_success_time = Some(Utc::now());

            match breaker.state {
                CircuitBreakerState::Open => {
                    // Check if we should try half-open
                    if let Some(last_failure) = breaker.last_failure_time {
                        if Utc::now() > last_failure + Duration::milliseconds(breaker.config.timeout_ms as i64) {
                            breaker.state = CircuitBreakerState::HalfOpen;
                            tracing::info!("Circuit breaker half-open for agent {}", agent_id);
                        }
                    }
                }
                CircuitBreakerState::HalfOpen => {
                    if breaker.success_count >= breaker.config.success_threshold {
                        breaker.state = CircuitBreakerState::Closed;
                        breaker.failure_count = 0;
                        breaker.success_count = 0;
                        tracing::info!("Circuit breaker closed for agent {}", agent_id);
                    }
                }
                CircuitBreakerState::Closed => {
                    // Normal operation
                }
            }
        }
    }

    /// Clean up old error records
    async fn cleanup_old_errors(&mut self) {
        let cutoff_date = Utc::now() - Duration::days(self.config.error_retention_days as i64);
        self.error_history.retain(|record| record.timestamp > cutoff_date);
    }

    /// Get error statistics
    pub fn get_error_statistics(&self) -> &ErrorStatistics {
        &self.stats
    }

    /// Get recent errors
    pub fn get_recent_errors(&self, limit: usize) -> Vec<&ErrorRecord> {
        self.error_history.iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Get errors by agent
    pub fn get_errors_by_agent(&self, agent_id: &Uuid) -> Vec<&ErrorRecord> {
        self.error_history.iter()
            .filter(|record| record.agent_id == *agent_id)
            .collect()
    }

    /// Reset circuit breaker for an agent
    pub async fn reset_circuit_breaker(&mut self, agent_id: &Uuid) -> Result<()> {
        if let Some(breaker) = self.circuit_breakers.get_mut(agent_id) {
            breaker.state = CircuitBreakerState::Closed;
            breaker.failure_count = 0;
            breaker.success_count = 0;
            tracing::info!("Circuit breaker reset for agent {}", agent_id);
        }
        Ok(())
    }

    /// Initialize error patterns
    fn initialize_error_patterns(&mut self) {
        // Network errors
        self.error_patterns.insert("network_error".to_string(), ErrorPattern {
            name: "Network Error".to_string(),
            pattern: "network|connection|timeout|unreachable".to_string(),
            category: ErrorCategory::Network,
            severity: ErrorSeverity::Medium,
            recovery_actions: vec!["Retry with exponential backoff".to_string()],
            default_strategy: "retry".to_string(),
        });

        // Authentication errors
        self.error_patterns.insert("auth_error".to_string(), ErrorPattern {
            name: "Authentication Error".to_string(),
            pattern: "auth|login|credential|unauthorized|forbidden".to_string(),
            category: ErrorCategory::Authentication,
            severity: ErrorSeverity::High,
            recovery_actions: vec!["Refresh credentials".to_string(), "Try different authentication method".to_string()],
            default_strategy: "alternative_approach".to_string(),
        });

        // Resource errors
        self.error_patterns.insert("resource_error".to_string(), ErrorPattern {
            name: "Resource Error".to_string(),
            pattern: "resource|memory|disk|capacity|limit".to_string(),
            category: ErrorCategory::Resource,
            severity: ErrorSeverity::Medium,
            recovery_actions: vec!["Free up resources".to_string(), "Try with smaller scope".to_string()],
            default_strategy: "graceful_degradation".to_string(),
        });

        // Timeout errors
        self.error_patterns.insert("timeout_error".to_string(), ErrorPattern {
            name: "Timeout Error".to_string(),
            pattern: "timeout|time out|slow|hang".to_string(),
            category: ErrorCategory::Performance,
            severity: ErrorSeverity::Medium,
            recovery_actions: vec!["Increase timeout".to_string(), "Break down task".to_string()],
            default_strategy: "decompose_task".to_string(),
        });
    }

    /// Initialize recovery strategies
    fn initialize_recovery_strategies(&mut self) {
        self.recovery_strategies.insert("retry".to_string(), RecoveryStrategy {
            name: "Retry".to_string(),
            strategy_type: RecoveryType::Retry,
            max_retries: 3,
            retry_delay: RetryDelay {
                initial_delay_ms: 1000,
                max_delay_ms: 30000,
                backoff_multiplier: 2.0,
                jitter_factor: 0.1,
            },
            fallback_actions: vec![],
            success_criteria: vec!["Task completes successfully".to_string()],
        });

        self.recovery_strategies.insert("switch_agent".to_string(), RecoveryStrategy {
            name: "Switch Agent".to_string(),
            strategy_type: RecoveryType::SwitchAgent,
            max_retries: 1,
            retry_delay: RetryDelay {
                initial_delay_ms: 5000,
                max_delay_ms: 10000,
                backoff_multiplier: 1.0,
                jitter_factor: 0.2,
            },
            fallback_actions: vec![FallbackAction {
                name: "Queue for later".to_string(),
                action_type: FallbackType::SkipTask,
                parameters: HashMap::new(),
            }],
            success_criteria: vec!["New agent accepts task".to_string()],
        });

        self.recovery_strategies.insert("graceful_degradation".to_string(), RecoveryStrategy {
            name: "Graceful Degradation".to_string(),
            strategy_type: RecoveryType::GracefulDegradation,
            max_retries: 0,
            retry_delay: RetryDelay {
                initial_delay_ms: 0,
                max_delay_ms: 0,
                backoff_multiplier: 1.0,
                jitter_factor: 0.0,
            },
            fallback_actions: vec![FallbackAction {
                name: "Return partial results".to_string(),
                action_type: FallbackType::PartialResults,
                parameters: HashMap::new(),
            }],
            success_criteria: vec!["Partial results provided".to_string()],
        });
    }

    /// Initialize retry policies
    fn initialize_retry_policies(&mut self) {
        self.retry_policies.insert("default".to_string(), RetryPolicy {
            name: "Default".to_string(),
            error_types: vec![
                ErrorType::NetworkError,
                ErrorType::Timeout,
            ],
            max_attempts: 3,
            delay_strategy: RetryDelay {
                initial_delay_ms: 1000,
                max_delay_ms: 10000,
                backoff_multiplier: 2.0,
                jitter_factor: 0.1,
            },
            retry_conditions: vec!["Error is recoverable".to_string()],
        });
    }
}

/// Result of error handling
#[derive(Debug, Clone)]
pub struct ErrorHandlingResult {
    /// Error ID
    pub error_id: Uuid,
    /// Action to take
    pub action: ErrorAction,
    /// Human-readable message
    pub message: String,
    /// When to retry (if applicable)
    pub retry_after: Option<DateTime<Utc>>,
    /// Fallback result (if available)
    pub fallback_result: Option<String>,
}

/// Action to take for error recovery
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorAction {
    /// Retry the task
    Retry,
    /// Switch to a different agent
    SwitchAgent,
    /// Decompose task into smaller parts
    DecomposeTask,
    /// Try alternative approach
    AlternativeApproach,
    /// Request human intervention
    HumanIntervention,
    /// Graceful degradation
    GracefulDegradation,
    /// Circuit breaker - temporarily stop trying
    CircuitBreaker,
    /// Fail the task
    Fail,
    /// Skip the task
    SkipTask,
}

/// Error category
#[derive(Debug, Clone, PartialEq)]
enum ErrorCategory {
    Network,
    Authentication,
    Resource,
    Performance,
    Logic,
    Unknown,
}

/// Error severity
#[derive(Debug, Clone, PartialEq)]
enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    fn new(agent_id: Uuid) -> Self {
        Self {
            agent_id,
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_success_time: None,
            config: CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 3,
                timeout_ms: 60000, // 1 minute
            },
        }
    }
}

impl Default for ErrorHandlerConfig {
    fn default() -> Self {
        Self {
            enable_auto_recovery: true,
            global_max_retries: 5,
            error_timeout_ms: 300000, // 5 minutes
            enable_circuit_breakers: true,
            error_retention_days: 30,
        }
    }
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}