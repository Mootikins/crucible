/// Enhanced Error Handling for Rune Tools
///
/// This module provides specialized error handling for Rune tool operations,
/// including compilation, execution, discovery, and validation errors.

use crate::errors::{CrucibleError, ErrorContext, ErrorCategory, ErrorSeverity,
                  ErrorAggregator, RecoverySuggestion, errors, error_codes};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;

use super::ToolRegistry;

/// Enhanced error handler for Rune operations
pub struct RuneErrorHandler {
    registry: Arc<RwLock<ToolRegistry>>,
    error_aggregator: ErrorAggregator,
}

impl RuneErrorHandler {
    pub fn new(registry: Arc<RwLock<ToolRegistry>>) -> Self {
        Self {
            registry,
            error_aggregator: ErrorAggregator::new(),
        }
    }

    /// Handle tool compilation errors
    pub fn handle_compilation_error(&mut self,
                                   tool_name: &str,
                                   file_path: &Path,
                                   error: &str,
                                   source_code: Option<&str>) -> CrucibleError {
        let context = ErrorContext::new("RuneErrorHandler")
            .with_operation("compile_tool")
            .with_tool_name(tool_name)
            .with_file_path(file_path.to_str().unwrap_or("unknown"));

        if let Some(code) = source_code {
            let _context = context.with_metadata("source_code_length", &code.len().to_string());
        }

        let error = errors::rune_compilation_failed(tool_name, error, file_path.to_str().unwrap_or("unknown"));
        self.error_aggregator.add_error(error.clone());
        error
    }

    /// Handle tool execution errors
    pub fn handle_execution_error(&mut self,
                                  tool_name: &str,
                                  operation: &str,
                                  error: &str,
                                  parameters: &Value) -> CrucibleError {
        let _context = ErrorContext::new("RuneErrorHandler")
            .with_operation(operation)
            .with_tool_name(tool_name)
            .with_metadata("parameters", &parameters.to_string());

        let error = errors::rune_execution_failed(tool_name, error, "RuneErrorHandler");
        self.error_aggregator.add_error(error.clone());
        error
    }

    /// Handle tool discovery errors
    pub fn handle_discovery_error(&mut self,
                                 directory: &Path,
                                 operation: &str,
                                 error: &str) -> CrucibleError {
        let _context = ErrorContext::new("RuneErrorHandler")
            .with_operation(operation)
            .with_file_path(directory.to_str().unwrap_or("unknown"));

        let error = errors::tool_discovery_failed(directory.to_str().unwrap_or("unknown"), error);
        self.error_aggregator.add_error(error.clone());
        error
    }

    /// Handle validation errors
    pub fn handle_validation_error(&mut self,
                                  tool_name: &str,
                                  parameter_name: &str,
                                  expected: &str,
                                  actual: &str) -> CrucibleError {
        let _context = ErrorContext::new("RuneErrorHandler")
            .with_operation("validate_parameters")
            .with_tool_name(tool_name)
            .with_metadata("parameter", parameter_name);

        let error = errors::validation_parameter_invalid(parameter_name, expected, actual);
        self.error_aggregator.add_error(error.clone());
        error
    }

    /// Handle tool loading errors
    pub fn handle_loading_error(&mut self,
                               file_path: &Path,
                               operation: &str,
                               error: &str) -> CrucibleError {
        let context = ErrorContext::new("RuneErrorHandler")
            .with_operation(operation)
            .with_file_path(file_path.to_str().unwrap_or("unknown"));

        let error = CrucibleError::new(
            "TOOL_LOADING_ERROR",
            &format!("Failed to load tool from {}: {}", file_path.display(), error),
            ErrorCategory::ToolDiscovery,
            ErrorSeverity::Error,
            context,
        ).with_cause(error)
          .with_recovery(
              RecoverySuggestion::new("Check file format and permissions")
                  .with_alternative("Verify file contains valid Rune syntax")
                  .with_alternative("Ensure all required functions are defined")
          );

        self.error_aggregator.add_error(error.clone());
        error
    }

    /// Get error statistics
    pub fn get_error_stats(&self) -> ErrorStats {
        ErrorStats {
            total_errors: self.error_aggregator.errors.len(),
            critical_errors: self.error_aggregator.summary.critical_count,
            rune_errors: self.error_aggregator.get_errors_by_category(ErrorCategory::Rune).len(),
            tool_discovery_errors: self.error_aggregator.get_errors_by_category(ErrorCategory::ToolDiscovery).len(),
            validation_errors: self.error_aggregator.get_errors_by_category(ErrorCategory::Validation).len(),
            database_errors: self.error_aggregator.get_errors_by_category(ErrorCategory::Database).len(),
        }
    }

    /// Get all errors
    pub fn get_all_errors(&self) -> &[CrucibleError] {
        &self.error_aggregator.errors
    }

    /// Get errors by category
    pub fn get_errors_by_category(&self, category: ErrorCategory) -> Vec<&CrucibleError> {
        self.error_aggregator.get_errors_by_category(category)
    }

    /// Clear error history
    pub fn clear_errors(&mut self) {
        self.error_aggregator = ErrorAggregator::new();
    }

    /// Check if there are critical errors
    pub fn has_critical_errors(&self) -> bool {
        self.error_aggregator.has_critical_errors()
    }

    /// Generate error report
    pub fn generate_error_report(&self) -> String {
        let stats = self.get_error_stats();
        let mut report = format!("=== Rune Tool Error Report ===\n");
        report.push_str(&format!("Total Errors: {}\n", stats.total_errors));
        report.push_str(&format!("Critical Errors: {}\n", stats.critical_errors));
        report.push_str(&format!("Rune Errors: {}\n", stats.rune_errors));
        report.push_str(&format!("Tool Discovery Errors: {}\n", stats.tool_discovery_errors));
        report.push_str(&format!("Validation Errors: {}\n", stats.validation_errors));
        report.push_str(&format!("Database Errors: {}\n\n", stats.database_errors));

        if !self.error_aggregator.errors.is_empty() {
            report.push_str("=== Recent Errors ===\n");
            for error in self.error_aggregator.errors.iter().take(10) {
                report.push_str(&format!("[{}] {}\n", error.severity, error.message));
                if let Some(tool_name) = &error.context.tool_name {
                    report.push_str(&format!("  Tool: {}\n", tool_name));
                }
                if let Some(recovery) = &error.recovery {
                    report.push_str(&format!("  Recovery: {}\n", recovery.suggestion));
                }
                report.push_str("\n");
            }
        }

        report
    }
}

/// Error statistics for Rune operations
#[derive(Debug, Clone)]
pub struct ErrorStats {
    pub total_errors: usize,
    pub critical_errors: usize,
    pub rune_errors: usize,
    pub tool_discovery_errors: usize,
    pub validation_errors: usize,
    pub database_errors: usize,
}

/// Enhanced error recovery mechanisms
pub struct ErrorRecoveryManager {
    recovery_strategies: Vec<RecoveryStrategy>,
}

impl ErrorRecoveryManager {
    pub fn new() -> Self {
        Self {
            recovery_strategies: vec![
                RecoveryStrategy::new(
                    ErrorCategory::Rune,
                    error_codes::RUNE_COMPILATION_FAILED,
                    |error| {
                        // Try to fix common compilation issues
                        if let Some(tool_name) = &error.context.tool_name {
                            format!("Attempt to recompile tool '{}' with additional validation", tool_name)
                        } else {
                            "Unable to determine tool name for recovery".to_string()
                        }
                    }
                ),
                RecoveryStrategy::new(
                    ErrorCategory::Rune,
                    error_codes::RUNE_EXECUTION_FAILED,
                    |error| {
                        // Try to re-execute with different parameters
                        if let Some(tool_name) = &error.context.tool_name {
                            format!("Retry execution of tool '{}' with sanitized parameters", tool_name)
                        } else {
                            "Unable to determine tool name for recovery".to_string()
                        }
                    }
                ),
                RecoveryStrategy::new(
                    ErrorCategory::ToolDiscovery,
                    error_codes::TOOL_DISCOVERY_FAILED,
                    |_error| {
                        // Try to refresh tool discovery
                        "Refresh tool discovery and re-scan directory".to_string()
                    }
                ),
            ],
        }
    }

    /// Attempt error recovery
    pub fn attempt_recovery(&self, error: &CrucibleError) -> Option<RecoveryAttempt> {
        for strategy in &self.recovery_strategies {
            if strategy.matches(error) {
                return Some(strategy.execute(error));
            }
        }
        None
    }

    /// Add custom recovery strategy
    pub fn add_strategy(&mut self, strategy: RecoveryStrategy) {
        self.recovery_strategies.push(strategy);
    }
}

/// Recovery strategy for specific error types
pub struct RecoveryStrategy {
    category: ErrorCategory,
    error_code: &'static str,
    recovery_fn: Box<dyn Fn(&CrucibleError) -> String + Send + Sync>,
}

impl RecoveryStrategy {
    pub fn new<F>(category: ErrorCategory, error_code: &'static str, recovery_fn: F) -> Self
    where
        F: Fn(&CrucibleError) -> String + Send + Sync + 'static,
    {
        Self {
            category,
            error_code,
            recovery_fn: Box::new(recovery_fn),
        }
    }

    pub fn matches(&self, error: &CrucibleError) -> bool {
        error.category == self.category && error.code == self.error_code
    }

    pub fn execute(&self, error: &CrucibleError) -> RecoveryAttempt {
        let action = (self.recovery_fn)(error);
        RecoveryAttempt {
            action,
            auto_recoverable: error.is_recoverable(),
            estimated_success_rate: self.estimate_success_rate(error),
        }
    }

    fn estimate_success_rate(&self, error: &CrucibleError) -> f32 {
        match error.severity {
            ErrorSeverity::Critical => 0.1,
            ErrorSeverity::Error => 0.4,
            ErrorSeverity::Warning => 0.7,
            ErrorSeverity::Info => 0.9,
        }
    }
}

/// Recovery attempt result
pub struct RecoveryAttempt {
    pub action: String,
    pub auto_recoverable: bool,
    pub estimated_success_rate: f32,
}

/// Error logging and monitoring
pub struct ErrorLogger {
    log_file: Option<PathBuf>,
    max_log_size: u64,
}

impl ErrorLogger {
    pub fn new() -> Self {
        Self {
            log_file: None,
            max_log_size: 10 * 1024 * 1024, // 10MB
        }
    }

    pub fn with_log_file(mut self, file_path: PathBuf) -> Self {
        self.log_file = Some(file_path);
        self
    }

    pub fn log_error(&self, error: &CrucibleError) {
        let log_entry = format!(
            "[{}] [{}] [{}] {} - {} - {}",
            chrono::Utc::now().to_rfc3339(),
            error.severity,
            error.category,
            error.code,
            error.message,
            error.error_id
        );

        // Log to stderr (could be enhanced to log to file)
        eprintln!("{}", log_entry);

        // TODO: Add file logging if log_file is set
    }

    pub fn log_recovery_attempt(&self, attempt: &RecoveryAttempt, error_id: &str) {
        let log_entry = format!(
            "[{}] RECOVERY_ATTEMPT [{}] {} - {}% success rate",
            chrono::Utc::now().to_rfc3339(),
            error_id,
            attempt.action,
            (attempt.estimated_success_rate * 100.0) as u32
        );

        eprintln!("{}", log_entry);
    }
}

/// Circuit breaker pattern for preventing repeated failures
pub struct CircuitBreaker {
    failure_count: u32,
    failure_threshold: u32,
    state: CircuitState,
    last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    recovery_timeout: chrono::Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Circuit is open, blocking calls
    HalfOpen,  // Testing if system has recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: chrono::Duration) -> Self {
        Self {
            failure_count: 0,
            failure_threshold,
            state: CircuitState::Closed,
            last_failure_time: None,
            recovery_timeout,
        }
    }

    pub fn call<F, T>(&mut self, f: F) -> Result<T, CrucibleError>
    where
        F: FnOnce() -> Result<T, CrucibleError>,
    {
        match self.state {
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if chrono::Utc::now() - last_failure > self.recovery_timeout {
                        self.state = CircuitState::HalfOpen;
                    } else {
                        return Err(CrucibleError::new(
                            "CIRCUIT_BREAKER_OPEN",
                            "Circuit breaker is open, preventing repeated failures",
                            ErrorCategory::Other,
                            ErrorSeverity::Error,
                            ErrorContext::new("CircuitBreaker"),
                        ));
                    }
                }
            }
            CircuitState::HalfOpen => {
                // Allow one call to test recovery
            }
            CircuitState::Closed => {
                // Normal operation
            }
        }

        let result = f();

        match &result {
            Ok(_) => {
                self.on_success();
                result
            }
            Err(error) => {
                self.on_failure();
                Err(error.clone())
            }
        }
    }

    fn on_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.last_failure_time = None;
    }

    fn on_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(chrono::Utc::now());

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }

    pub fn get_failure_count(&self) -> u32 {
        self.failure_count
    }

    pub fn reset(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.last_failure_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use serde_json::json;

    #[test]
    fn test_error_handler_creation() {
        let temp_dir = tempdir().unwrap();
        let context = Arc::new(rune::Context::with_default_modules().unwrap());
        let registry = ToolRegistry::new(temp_dir.path().to_path_buf(), context).unwrap();
        let registry_arc = Arc::new(RwLock::new(registry));

        let handler = RuneErrorHandler::new(registry_arc);
        assert_eq!(handler.get_error_stats().total_errors, 0);
    }

    #[test]
    fn test_error_aggregation() {
        let temp_dir = tempdir().unwrap();
        let context = Arc::new(rune::Context::with_default_modules().unwrap());
        let registry = ToolRegistry::new(temp_dir.path().to_path_buf(), context).unwrap();
        let registry_arc = Arc::new(RwLock::new(registry));

        let mut handler = RuneErrorHandler::new(registry_arc);

        // Add some errors
        let error1 = handler.handle_compilation_error("test_tool", temp_dir.path(), "syntax error", None);
        let error2 = handler.handle_execution_error("test_tool", "call", "runtime error", &json!({}));

        assert_eq!(handler.get_error_stats().total_errors, 2);
        assert_eq!(handler.get_error_stats().rune_errors, 2);
        assert!(!handler.has_critical_errors());
    }

    #[test]
    fn test_circuit_breaker() {
        let mut circuit_breaker = CircuitBreaker::new(3, chrono::Duration::seconds(5));

        // Initially closed
        assert!(!circuit_breaker.is_open());
        assert_eq!(circuit_breaker.get_failure_count(), 0);

        // First failure
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| Err(errors::rune_tool_not_found("test", "test")));
        assert!(result.is_err());
        assert_eq!(circuit_breaker.get_failure_count(), 1);
        assert!(!circuit_breaker.is_open());

        // Second failure
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| Err(errors::rune_tool_not_found("test", "test")));
        assert!(result.is_err());
        assert_eq!(circuit_breaker.get_failure_count(), 2);
        assert!(!circuit_breaker.is_open());

        // Third failure - should open circuit
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| Err(errors::rune_tool_not_found("test", "test")));
        assert!(result.is_err());
        assert_eq!(circuit_breaker.get_failure_count(), 3);
        assert!(circuit_breaker.is_open());

        // Fourth call while circuit is open
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| Ok("success"));
        assert!(result.is_err());
        assert!(circuit_breaker.is_open());
    }

    #[test]
    fn test_recovery_manager() {
        let mut recovery_manager = ErrorRecoveryManager::new();
        let error = errors::rune_compilation_failed("test_tool", "syntax error", "/path/to/tool.rn");

        let attempt = recovery_manager.attempt_recovery(&error);
        assert!(attempt.is_some());

        let attempt = attempt.unwrap();
        assert!(attempt.action.contains("recompile"));
        assert!(!attempt.auto_recoverable); // Compilation errors typically require manual intervention
    }

    #[test]
    fn test_error_logger() {
        let logger = ErrorLogger::new();
        let error = errors::rune_tool_not_found("test_tool", "test_component");

        // Should not panic
        logger.log_error(&error);

        let attempt = RecoveryAttempt {
            action: "Test recovery".to_string(),
            auto_recoverable: false,
            estimated_success_rate: 0.5,
        };

        logger.log_recovery_attempt(&attempt, &error.error_id);
    }
}