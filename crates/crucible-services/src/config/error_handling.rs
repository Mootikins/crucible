//! Comprehensive error handling utilities for configuration management
//!
//! This module provides enhanced error handling capabilities including error recovery,
//! error reporting, and integration with the existing service error systems.

use super::validation::{ValidationError, ValidationResult, ValidationContext, ValidationSeverity};
use crate::errors::{ServiceError, ServiceResult};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Enhanced error handler for configuration issues
#[derive(Debug, Clone)]
pub struct ConfigErrorHandler {
    /// Error recovery strategies
    recovery_strategies: HashMap<String, RecoveryStrategy>,
    /// Error reporting configuration
    reporting_config: ErrorReportingConfig,
    /// Error history
    error_history: Vec<ErrorRecord>,
    /// Maximum error history size
    max_history_size: usize,
}

impl ConfigErrorHandler {
    /// Create new error handler
    pub fn new() -> Self {
        Self {
            recovery_strategies: Self::default_recovery_strategies(),
            reporting_config: ErrorReportingConfig::default(),
            error_history: Vec::new(),
            max_history_size: 1000,
        }
    }

    /// Create error handler with custom configuration
    pub fn with_config(config: ErrorReportingConfig) -> Self {
        Self {
            recovery_strategies: Self::default_recovery_strategies(),
            reporting_config: config,
            error_history: Vec::new(),
            max_history_size: 1000,
        }
    }

    /// Handle configuration validation error
    pub fn handle_validation_error(&mut self, error: ValidationError) -> ErrorHandlingResult {
        // Record the error
        let record = ErrorRecord::from_validation_error(&error);
        self.record_error(record.clone());

        // Determine if recovery is possible
        if let Some(strategy) = self.recovery_strategies.get(error.field().unwrap_or("unknown")) {
            match strategy.attempt_recovery(&error) {
                Ok(recovery_result) => {
                    info!(
                        field = %error.field().unwrap_or("unknown"),
                        strategy = %strategy.name(),
                        "Configuration error recovered successfully"
                    );
                    return ErrorHandlingResult::Recovered(recovery_result);
                }
                Err(recovery_error) => {
                    warn!(
                        field = %error.field().unwrap_or("unknown"),
                        strategy = %strategy.name(),
                        error = %recovery_error,
                        "Configuration error recovery failed"
                    );
                }
            }
        }

        // If no recovery strategy or recovery failed, handle based on severity
        match error.severity() {
            ValidationSeverity::Critical => {
                error!(error = %error, "Critical configuration error detected");
                ErrorHandlingResult::Critical(error)
            }
            ValidationSeverity::Error => {
                error!(error = %error, "Configuration error detected");
                ErrorHandlingResult::Failed(error)
            }
            ValidationSeverity::Warning => {
                warn!(error = %error, "Configuration warning detected");
                ErrorHandlingResult::Warning(error)
            }
            ValidationSeverity::Info => {
                debug!(error = %error, "Configuration info detected");
                ErrorHandlingResult::Info(error)
            }
        }
    }

    /// Handle multiple validation errors
    pub fn handle_validation_result(&mut self, result: ValidationResult) -> ErrorHandlingResult {
        if result.is_valid && result.warnings.is_empty() {
            return ErrorHandlingResult::Success;
        }

        // Group errors by severity
        let mut critical_errors = Vec::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for error in result.errors {
            match error.severity() {
                ValidationSeverity::Critical => critical_errors.push(error),
                _ => errors.push(error),
            }
        }

        warnings = result.warnings;

        // Handle critical errors first
        if !critical_errors.is_empty() {
            let error_records: Vec<ErrorRecord> = critical_errors
                .iter()
                .map(ErrorRecord::from_validation_error)
                .collect();

            for record in &error_records {
                self.record_error(record.clone());
            }

            return ErrorHandlingResult::Critical(ValidationError::MultipleErrors {
                count: critical_errors.len(),
                errors: critical_errors,
            });
        }

        // Handle regular errors
        if !errors.is_empty() {
            let error_records: Vec<ErrorRecord> = errors
                .iter()
                .map(ErrorRecord::from_validation_error)
                .collect();

            for record in &error_records {
                self.record_error(record.clone());
            }

            return ErrorHandlingResult::Failed(ValidationError::MultipleErrors {
                count: errors.len(),
                errors,
            });
        }

        // Handle warnings
        if !warnings.is_empty() {
            let warning_records: Vec<ErrorRecord> = warnings
                .iter()
                .map(ErrorRecord::from_validation_error)
                .collect();

            for record in &warning_records {
                self.record_error(record.clone());
            }

            return ErrorHandlingResult::Warning(ValidationError::MultipleErrors {
                count: warnings.len(),
                errors: warnings,
            });
        }

        ErrorHandlingResult::Success
    }

    /// Record error in history
    fn record_error(&mut self, record: ErrorRecord) {
        self.error_history.push(record);

        // Trim history if needed
        if self.error_history.len() > self.max_history_size {
            self.error_history.drain(0..self.error_history.len() - self.max_history_size);
        }

        // Send error report if configured
        if self.reporting_config.enable_reporting {
            self.send_error_report(&self.error_history.last().unwrap());
        }
    }

    /// Send error report
    fn send_error_report(&self, record: &ErrorRecord) {
        // In a real implementation, this could send to external monitoring
        info!(
            error_id = %record.id,
            error_type = %record.error_type,
            message = %record.message,
            "Configuration error reported"
        );
    }

    /// Get error statistics
    pub fn get_error_statistics(&self) -> ErrorStatistics {
        let mut stats = ErrorStatistics::default();

        for record in &self.error_history {
            stats.total_errors += 1;

            match record.severity {
                ValidationSeverity::Critical => stats.critical_errors += 1,
                ValidationSeverity::Error => stats.error_errors += 1,
                ValidationSeverity::Warning => stats.warnings += 1,
                ValidationSeverity::Info => stats.info_messages += 1,
            }

            if let Some(field) = &record.field {
                *stats.field_error_counts.entry(field.clone()).or_insert(0) += 1;
            }
        }

        stats
    }

    /// Clear error history
    pub fn clear_history(&mut self) {
        self.error_history.clear();
        info!("Error history cleared");
    }

    /// Get recent errors
    pub fn get_recent_errors(&self, limit: usize) -> Vec<ErrorRecord> {
        let start = if self.error_history.len() > limit {
            self.error_history.len() - limit
        } else {
            0
        };

        self.error_history[start..].to_vec()
    }

    /// Add custom recovery strategy
    pub fn add_recovery_strategy(&mut self, field: impl Into<String>, strategy: RecoveryStrategy) {
        self.recovery_strategies.insert(field.into(), strategy);
    }

    /// Get default recovery strategies
    fn default_recovery_strategies() -> HashMap<String, RecoveryStrategy> {
        let mut strategies = HashMap::new();

        // Log level recovery strategy
        strategies.insert(
            "logging.level".to_string(),
            RecoveryStrategy::DefaultValue("info".to_string()),
        );

        // Log format recovery strategy
        strategies.insert(
            "logging.format".to_string(),
            RecoveryStrategy::DefaultValue("json".to_string()),
        );

        // Max concurrent events recovery strategy
        strategies.insert(
            "event_routing.max_concurrent_events".to_string(),
            RecoveryStrategy::DefaultValue("1000".to_string()),
        );

        // Max event age recovery strategy
        strategies.insert(
            "event_routing.max_event_age_seconds".to_string(),
            RecoveryStrategy::DefaultValue("300".to_string()),
        );

        // Service environment recovery strategy
        strategies.insert(
            "service.environment".to_string(),
            RecoveryStrategy::DefaultValue("development".to_string()),
        );

        strategies
    }
}

impl Default for ConfigErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Error handling result
#[derive(Debug, Clone)]
pub enum ErrorHandlingResult {
    /// No errors
    Success,
    /// Error was recovered
    Recovered(RecoveryResult),
    /// Warning encountered
    Warning(ValidationError),
    /// Error encountered
    Failed(ValidationError),
    /// Critical error
    Critical(ValidationError),
    /// Info message
    Info(ValidationError),
}

impl ErrorHandlingResult {
    /// Check if the result is successful (no critical errors)
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success | Self::Recovered(_) | Self::Info(_) | Self::Warning(_))
    }

    /// Check if the result has errors
    pub fn has_errors(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::Critical(_))
    }

    /// Convert to ServiceResult
    pub fn into_service_result(self) -> ServiceResult<()> {
        match self {
            Self::Success | Self::Recovered(_) | Self::Info(_) | Self::Warning(_) => Ok(()),
            Self::Failed(error) => Err(ServiceError::ValidationError(error.to_string())),
            Self::Critical(error) => Err(ServiceError::ValidationError(format!("CRITICAL: {}", error))),
        }
    }

    /// Get error message if any
    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::Failed(error) | Self::Critical(error) | Self::Warning(error) | Self::Info(error) => {
                Some(error.to_string())
            }
            Self::Recovered(result) => Some(format!("Recovered: {}", result.description)),
            Self::Success => None,
        }
    }
}

/// Recovery strategy for configuration errors
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Use default value
    DefaultValue(String),
    /// Use environment variable
    EnvironmentVariable(String),
    /// Use computed value
    ComputedValue(String),
    /// Skip field (make optional)
    SkipField,
    /// Custom recovery function
    Custom(String),
}

impl RecoveryStrategy {
    /// Attempt to recover from error
    pub fn attempt_recovery(&self, error: &ValidationError) -> ServiceResult<RecoveryResult> {
        match self {
            Self::DefaultValue(default_value) => {
                Ok(RecoveryResult {
                    strategy: self.name(),
                    original_value: error.field().map(|f| f.to_string()),
                    recovered_value: default_value.clone(),
                    description: format!("Applied default value: {}", default_value),
                })
            }
            Self::EnvironmentVariable(env_var) => {
                match std::env::var(env_var) {
                    Ok(value) => {
                        Ok(RecoveryResult {
                            strategy: self.name(),
                            original_value: error.field().map(|f| f.to_string()),
                            recovered_value: value.clone(),
                            description: format!("Applied environment variable {} = {}", env_var, value),
                        })
                    }
                    Err(_) => {
                        Err(ServiceError::ValidationError(
                            format!("Environment variable {} not found for recovery", env_var)
                        ))
                    }
                }
            }
            Self::ComputedValue(computation) => {
                Ok(RecoveryResult {
                    strategy: self.name(),
                    original_value: error.field().map(|f| f.to_string()),
                    recovered_value: computation.clone(),
                    description: format!("Applied computed value: {}", computation),
                })
            }
            Self::SkipField => {
                Ok(RecoveryResult {
                    strategy: self.name(),
                    original_value: error.field().map(|f| f.to_string()),
                    recovered_value: "[SKIPPED]".to_string(),
                    description: "Field skipped - not required".to_string(),
                })
            }
            Self::Custom(description) => {
                Ok(RecoveryResult {
                    strategy: self.name(),
                    original_value: error.field().map(|f| f.to_string()),
                    recovered_value: "[CUSTOM]".to_string(),
                    description: description.clone(),
                })
            }
        }
    }

    /// Get strategy name
    pub fn name(&self) -> &'static str {
        match self {
            Self::DefaultValue(_) => "default_value",
            Self::EnvironmentVariable(_) => "environment_variable",
            Self::ComputedValue(_) => "computed_value",
            Self::SkipField => "skip_field",
            Self::Custom(_) => "custom",
        }
    }
}

/// Recovery result
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Recovery strategy used
    pub strategy: &'static str,
    /// Original value
    pub original_value: Option<String>,
    /// Recovered value
    pub recovered_value: String,
    /// Recovery description
    pub description: String,
}

/// Error reporting configuration
#[derive(Debug, Clone)]
pub struct ErrorReportingConfig {
    /// Enable error reporting
    pub enable_reporting: bool,
    /// Reporting endpoint
    pub reporting_endpoint: Option<String>,
    /// Report warnings
    pub report_warnings: bool,
    /// Report info messages
    pub report_info: bool,
    /// Maximum reports per hour
    pub max_reports_per_hour: Option<u32>,
    /// Include context in reports
    pub include_context: bool,
}

impl Default for ErrorReportingConfig {
    fn default() -> Self {
        Self {
            enable_reporting: false,
            reporting_endpoint: None,
            report_warnings: false,
            report_info: false,
            max_reports_per_hour: Some(100),
            include_context: true,
        }
    }
}

/// Error record for tracking and analysis
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    /// Error ID
    pub id: String,
    /// Error type
    pub error_type: String,
    /// Error severity
    pub severity: super::validation::ValidationSeverity,
    /// Error message
    pub message: String,
    /// Field name (if applicable)
    pub field: Option<String>,
    /// Error context
    pub context: Option<ValidationContext>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Recovery attempted
    pub recovery_attempted: bool,
    /// Recovery successful
    pub recovery_successful: bool,
}

impl ErrorRecord {
    /// Create error record from validation error
    pub fn from_validation_error(error: &ValidationError) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            error_type: "validation_error".to_string(),
            severity: error.severity(),
            message: error.to_string(),
            field: error.field().map(|f| f.to_string()),
            context: error.context().cloned(),
            timestamp: chrono::Utc::now(),
            recovery_attempted: error.is_recoverable(),
            recovery_successful: false, // Will be updated if recovery succeeds
        }
    }

    /// Create error record from service error
    pub fn from_service_error(error: &ServiceError) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            error_type: "service_error".to_string(),
            severity: ValidationSeverity::Error,
            message: error.to_string(),
            field: None,
            context: None,
            timestamp: chrono::Utc::now(),
            recovery_attempted: false,
            recovery_successful: false,
        }
    }
}

/// Error statistics
#[derive(Debug, Clone, Default)]
pub struct ErrorStatistics {
    /// Total errors
    pub total_errors: u64,
    /// Critical errors
    pub critical_errors: u64,
    /// Regular errors
    pub error_errors: u64,
    /// Warnings
    pub warnings: u64,
    /// Info messages
    pub info_messages: u64,
    /// Field-specific error counts
    pub field_error_counts: HashMap<String, u64>,
}

impl ErrorStatistics {
    /// Get error rate (errors per hour)
    pub fn error_rate_per_hour(&self, hours: f64) -> f64 {
        if hours > 0.0 {
            self.total_errors as f64 / hours
        } else {
            0.0
        }
    }

    /// Get most problematic fields
    pub fn most_problematic_fields(&self, limit: usize) -> Vec<(String, u64)> {
        let mut fields: Vec<_> = self.field_error_counts.iter().collect();
        fields.sort_by(|a, b| b.1.cmp(a.1));
        fields.into_iter()
            .take(limit)
            .map(|(field, count)| (field.clone(), *count))
            .collect()
    }

    /// Check if error rate is high
    pub fn is_high_error_rate(&self, hours: f64) -> bool {
        self.error_rate_per_hour(hours) > 10.0
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "Errors: {} total ({} critical, {} errors, {} warnings, {} info)",
            self.total_errors, self.critical_errors, self.error_errors, self.warnings, self.info_messages
        )
    }
}

/// Utility functions for error handling
pub mod utils {
    use super::*;

    /// Create error context from current environment
    pub fn create_error_context(source: &str) -> ValidationContext {
        ValidationContext::new(source)
            .with_metadata("hostname", hostname::get().unwrap_or_default().to_string_lossy())
            .with_metadata("process_id", std::process::id().to_string())
            .with_metadata("timestamp", chrono::Utc::now().to_rfc3339())
    }

    /// Format error for logging
    pub fn format_error_for_logging(error: &ValidationError, include_context: bool) -> String {
        let mut formatted = format!("[{}] {}", error.severity(), error);

        if include_context {
            if let Some(context) = error.context() {
                formatted.push_str(&format!(" (source: {}, field: {})", context.source, context.full_field_path()));
            }
        }

        formatted
    }

    /// Create user-friendly error message
    pub fn create_user_friendly_message(error: &ValidationError) -> String {
        match error {
            ValidationError::MissingField { field, .. } => {
                format!("The required configuration field '{}' is missing. Please add it to your configuration file.", field)
            }
            ValidationError::InvalidValue { field, suggested_fix: Some(fix), .. } => {
                format!("The value for '{}' is invalid. {}", field, fix)
            }
            ValidationError::ConstraintViolation { field, details, .. } => {
                format!("The value for '{}' violates a constraint: {}", field, details)
            }
            _ => {
                error.description()
            }
        }
    }

    /// Check if error should be escalated
    pub fn should_escalate_error(error: &ValidationError) -> bool {
        matches!(error.severity(), ValidationSeverity::Critical) ||
        matches!(error, ValidationError::DependencyViolation { .. }) ||
        matches!(error, ValidationError::ParseError { .. })
    }

    /// Generate error report
    pub fn generate_error_report(errors: &[ErrorRecord]) -> String {
        if errors.is_empty() {
            return "No errors to report.".to_string();
        }

        let mut report = String::new();
        report.push_str("# Configuration Error Report\n\n");

        let stats = calculate_error_stats(errors);
        report.push_str(&format!("## Summary\n{}\n\n", stats.summary()));

        if !stats.most_problematic_fields(5).is_empty() {
            report.push_str("## Most Problematic Fields\n");
            for (field, count) in stats.most_problematic_fields(5) {
                report.push_str(&format!("- {}: {} errors\n", field, count));
            }
            report.push('\n');
        }

        report.push_str("## Recent Errors\n");
        for (i, record) in errors.iter().rev().take(10).enumerate() {
            report.push_str(&format!(
                "{}. [{}] {} - {}\n",
                i + 1,
                record.severity,
                record.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                record.message
            ));
        }

        report
    }

    /// Calculate error statistics
    fn calculate_error_stats(errors: &[ErrorRecord]) -> ErrorStatistics {
        let mut stats = ErrorStatistics::default();

        for record in errors {
            stats.total_errors += 1;

            match record.severity {
                ValidationSeverity::Critical => stats.critical_errors += 1,
                ValidationSeverity::Error => stats.error_errors += 1,
                ValidationSeverity::Warning => stats.warnings += 1,
                ValidationSeverity::Info => stats.info_messages += 1,
            }

            if let Some(field) = &record.field {
                *stats.field_error_counts.entry(field.clone()).or_insert(0) += 1;
            }
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_handler_creation() {
        let handler = ConfigErrorHandler::new();
        assert_eq!(handler.error_history.len(), 0);
    }

    #[test]
    fn test_recovery_strategy() {
        let strategy = RecoveryStrategy::DefaultValue("test".to_string());
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.recovered_value, "test");
        assert_eq!(recovery.strategy, "default_value");
    }

    #[test]
    fn test_error_statistics() {
        let mut stats = ErrorStatistics::default();
        stats.total_errors = 10;
        stats.critical_errors = 2;
        stats.error_errors = 5;
        stats.warnings = 3;

        assert_eq!(stats.error_rate_per_hour(1.0), 10.0);
        assert!(!stats.is_high_error_rate(24.0));
        assert!(stats.is_high_error_rate(0.5));
    }

    #[test]
    fn test_error_record() {
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let record = ErrorRecord::from_validation_error(&error);
        assert_eq!(record.field, Some("test_field".to_string()));
        assert_eq!(record.error_type, "validation_error");
    }

    #[test]
    fn test_user_friendly_message() {
        let error = ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let message = utils::create_user_friendly_message(&error);
        assert!(message.contains("required_field"));
        assert!(message.contains("missing"));
    }
}