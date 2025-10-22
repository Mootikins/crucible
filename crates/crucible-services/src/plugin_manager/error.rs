//! # Plugin Manager Error Handling
//!
//! This module defines comprehensive error types for the PluginManager system,
//! covering all aspects of plugin lifecycle, execution, and management.

use super::types::*;
use thiserror::Error;
use std::collections::HashMap;

/// ============================================================================
/// PLUGIN ERROR TYPES
/// ============================================================================

/// Comprehensive plugin error type
#[derive(Error, Debug, Clone)]
pub enum PluginError {
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Manifest errors
    #[error("Manifest error: {0}")]
    ManifestError(String),

    /// Discovery errors
    #[error("Discovery error: {0}")]
    DiscoveryError(String),

    /// Installation errors
    #[error("Installation error: {0}")]
    InstallationError(String),

    /// Lifecycle errors
    #[error("Lifecycle error: {0}")]
    LifecycleError(String),

    /// Process management errors
    #[error("Process error: {0}")]
    ProcessError(String),

    /// Resource management errors
    #[error("Resource error: {0}")]
    ResourceError(String),

    /// Security errors
    #[error("Security error: {0}")]
    SecurityError(String),

    /// Communication errors
    #[error("Communication error: {0}")]
    CommunicationError(String),

    /// Timeout errors
    #[error("Timeout error: {0}")]
    TimeoutError(String),

    /// Dependency errors
    #[error("Dependency error: {0}")]
    DependencyError(String),

    /// Compatibility errors
    #[error("Compatibility error: {0}")]
    CompatibilityError(String),

    /// Execution errors
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// Registry errors
    #[error("Registry error: {0}")]
    RegistryError(String),

    /// Health monitoring errors
    #[error("Health monitoring error: {0}")]
    HealthMonitoringError(String),

    /// I/O errors
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// UUID generation errors
    #[error("UUID error: {0}")]
    UuidError(#[from] uuid::Error),

    /// System time errors
    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),

    /// Generic errors
    #[error("Plugin error: {0}")]
    Generic(String),
}

/// Plugin result type
pub type PluginResult<T> = Result<T, PluginError>;

/// ============================================================================
/// ERROR CONTEXT AND DETAILS
/// ============================================================================

/// Error context information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Plugin ID (if applicable)
    pub plugin_id: Option<String>,
    /// Instance ID (if applicable)
    pub instance_id: Option<String>,
    /// Operation being performed
    pub operation: Option<String>,
    /// Error source location
    pub source: Option<String>,
    /// Additional context
    pub additional_context: HashMap<String, String>,
    /// Error timestamp
    pub timestamp: std::time::SystemTime,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new() -> Self {
        Self {
            plugin_id: None,
            instance_id: None,
            operation: None,
            source: None,
            additional_context: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Set the plugin ID
    pub fn with_plugin_id(mut self, plugin_id: String) -> Self {
        self.plugin_id = Some(plugin_id);
        self
    }

    /// Set the instance ID
    pub fn with_instance_id(mut self, instance_id: String) -> Self {
        self.instance_id = Some(instance_id);
        self
    }

    /// Set the operation
    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    /// Set the source
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// Add additional context
    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.additional_context.insert(key, value);
        self
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Detailed error information with context
#[derive(Debug, Clone)]
pub struct DetailedPluginError {
    /// The base error
    pub error: PluginError,
    /// Error context
    pub context: ErrorContext,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Whether the error is recoverable
    pub recoverable: bool,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
    /// Related error IDs (for error chains)
    pub related_errors: Vec<String>,
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Informational message
    Info,
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical error
    Critical,
}

/// ============================================================================
/// CONVENIENCE CONSTRUCTORS
/// ============================================================================

impl PluginError {
    /// Create a configuration error
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    /// Create a validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::ValidationError(msg.into())
    }

    /// Create a manifest error
    pub fn manifest(msg: impl Into<String>) -> Self {
        Self::ManifestError(msg.into())
    }

    /// Create a discovery error
    pub fn discovery(msg: impl Into<String>) -> Self {
        Self::DiscoveryError(msg.into())
    }

    /// Create an installation error
    pub fn installation(msg: impl Into<String>) -> Self {
        Self::InstallationError(msg.into())
    }

    /// Create a lifecycle error
    pub fn lifecycle(msg: impl Into<String>) -> Self {
        Self::LifecycleError(msg.into())
    }

    /// Create a process error
    pub fn process(msg: impl Into<String>) -> Self {
        Self::ProcessError(msg.into())
    }

    /// Create a resource error
    pub fn resource(msg: impl Into<String>) -> Self {
        Self::ResourceError(msg.into())
    }

    /// Create a security error
    pub fn security(msg: impl Into<String>) -> Self {
        Self::SecurityError(msg.into())
    }

    /// Create a communication error
    pub fn communication(msg: impl Into<String>) -> Self {
        Self::CommunicationError(msg.into())
    }

    /// Create a timeout error
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::TimeoutError(msg.into())
    }

    /// Create a dependency error
    pub fn dependency(msg: impl Into<String>) -> Self {
        Self::DependencyError(msg.into())
    }

    /// Create a compatibility error
    pub fn compatibility(msg: impl Into<String>) -> Self {
        Self::CompatibilityError(msg.into())
    }

    /// Create an execution error
    pub fn execution(msg: impl Into<String>) -> Self {
        Self::ExecutionError(msg.into())
    }

    /// Create a registry error
    pub fn registry(msg: impl Into<String>) -> Self {
        Self::RegistryError(msg.into())
    }

    /// Create a health monitoring error
    pub fn health_monitoring(msg: impl Into<String>) -> Self {
        Self::HealthMonitoringError(msg.into())
    }

    /// Create a generic error
    pub fn generic(msg: impl Into<String>) -> Self {
        Self::Generic(msg.into())
    }

    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::IoError(_) | Self::SerializationError(_) | Self::UuidError(_) | Self::SystemTimeError(_) => {
                ErrorSeverity::Error
            }
            Self::ValidationError(_) | Self::CompatibilityError(_) => ErrorSeverity::Warning,
            Self::SecurityError(_) | Self::ConfigurationError(_) => ErrorSeverity::Critical,
            _ => ErrorSeverity::Error,
        }
    }

    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::TimeoutError(_) | Self::ProcessError(_) | Self::CommunicationError(_) => true,
            Self::ValidationError(_) | Self::CompatibilityError(_) | Self::DependencyError(_) => false,
            _ => false,
        }
    }

    /// Get suggested actions for the error
    pub fn suggested_actions(&self) -> Vec<String> {
        match self {
            Self::ValidationError(_) => vec![
                "Check plugin manifest format".to_string(),
                "Verify all required fields are present".to_string(),
            ],
            Self::ConfigurationError(_) => vec![
                "Review plugin configuration".to_string(),
                "Check configuration schema compatibility".to_string(),
            ],
            Self::DependencyError(_) => vec![
                "Install missing dependencies".to_string(),
                "Check version compatibility".to_string(),
            ],
            Self::SecurityError(_) => vec![
                "Review plugin security settings".to_string(),
                "Check plugin permissions".to_string(),
            ],
            Self::TimeoutError(_) => vec![
                "Increase timeout settings".to_string(),
                "Check plugin performance".to_string(),
            ],
            _ => vec!["Contact system administrator".to_string()],
        }
    }
}

/// ============================================================================
/// ERROR CONVERSIONS
/// ============================================================================

impl From<PluginError> for DetailedPluginError {
    fn from(error: PluginError) -> Self {
        Self {
            severity: error.severity(),
            recoverable: error.is_recoverable(),
            suggested_actions: error.suggested_actions(),
            error,
            context: ErrorContext::default(),
            related_errors: Vec::new(),
        }
    }
}

/// ============================================================================
/// ERROR HANDLING UTILITIES
/// ============================================================================

/// Error recovery strategies
#[derive(Debug, Clone)]
pub enum ErrorRecoveryStrategy {
    /// Retry the operation
    Retry { max_attempts: u32, delay: std::time::Duration },
    /// Restart the plugin instance
    RestartInstance,
    /// Disable the plugin
    DisablePlugin,
    /// Fallback to alternative implementation
    Fallback { alternative: String },
    /// Ignore the error
    Ignore,
    /// Escalate to administrator
    Escalate,
}

impl PluginError {
    /// Get recommended recovery strategy
    pub fn recovery_strategy(&self) -> ErrorRecoveryStrategy {
        match self {
            Self::TimeoutError(_) => ErrorRecoveryStrategy::Retry {
                max_attempts: 3,
                delay: std::time::Duration::from_secs(1),
            },
            Self::ProcessError(_) => ErrorRecoveryStrategy::RestartInstance,
            Self::DependencyError(_) => ErrorRecoveryStrategy::DisablePlugin,
            Self::ValidationError(_) => ErrorRecoveryStrategy::Escalate,
            Self::CommunicationError(_) => ErrorRecoveryStrategy::Retry {
                max_attempts: 2,
                delay: std::time::Duration::from_millis(500),
            },
            _ => ErrorRecoveryStrategy::Escalate,
        }
    }
}

/// Error collector for accumulating multiple errors
#[derive(Debug, Clone, Default)]
pub struct ErrorCollector {
    errors: Vec<DetailedPluginError>,
}

impl ErrorCollector {
    /// Create a new error collector
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add an error to the collection
    pub fn add_error(&mut self, error: DetailedPluginError) {
        self.errors.push(error);
    }

    /// Add a simple error to the collection
    pub fn add(&mut self, error: PluginError, context: ErrorContext) {
        self.add_error(DetailedPluginError {
            error,
            context,
            severity: ErrorSeverity::Error,
            recoverable: false,
            suggested_actions: Vec::new(),
            related_errors: Vec::new(),
        });
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the number of errors
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Get all errors
    pub fn get_errors(&self) -> &[DetailedPluginError] {
        &self.errors
    }

    /// Get errors by severity
    pub fn get_errors_by_severity(&self, severity: ErrorSeverity) -> Vec<&DetailedPluginError> {
        self.errors.iter().filter(|e| e.severity == severity).collect()
    }

    /// Get critical errors
    pub fn get_critical_errors(&self) -> Vec<&DetailedPluginError> {
        self.get_errors_by_severity(ErrorSeverity::Critical)
    }

    /// Clear all errors
    pub fn clear(&mut self) {
        self.errors.clear();
    }

    /// Convert to result type
    pub fn into_result(self) -> PluginResult<()> {
        if self.has_errors() {
            Err(PluginError::Generic(format!(
                "Collected {} errors: {}",
                self.error_count(),
                self.errors.iter()
                    .map(|e| e.error.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        } else {
            Ok(())
        }
    }
}

/// Error metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ErrorMetrics {
    /// Total error count
    pub total_errors: u64,
    /// Errors by type
    pub errors_by_type: HashMap<String, u64>,
    /// Errors by severity
    pub errors_by_severity: HashMap<ErrorSeverity, u64>,
    /// Errors by plugin
    pub errors_by_plugin: HashMap<String, u64>,
    /// Recent errors (last hour)
    pub recent_errors: Vec<DetailedPluginError>,
    /// Last error timestamp
    pub last_error: Option<std::time::SystemTime>,
}

impl ErrorMetrics {
    /// Create new error metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an error
    pub fn record_error(&mut self, error: &DetailedPluginError) {
        self.total_errors += 1;
        self.last_error = Some(std::time::SystemTime::now());

        // Count by type
        let error_type = format!("{:?}", error.error);
        *self.errors_by_type.entry(error_type).or_insert(0) += 1;

        // Count by severity
        *self.errors_by_severity.entry(error.severity).or_insert(0) += 1;

        // Count by plugin
        if let Some(plugin_id) = &error.context.plugin_id {
            *self.errors_by_plugin.entry(plugin_id.clone()).or_insert(0) += 1;
        }

        // Add to recent errors (keep last 100)
        self.recent_errors.push(error.clone());
        if self.recent_errors.len() > 100 {
            self.recent_errors.remove(0);
        }
    }

    /// Get error rate (errors per minute)
    pub fn error_rate(&self) -> f64 {
        if let Some(last_error) = self.last_error {
            if let Ok(duration) = last_error.duration_since(std::time::SystemTime::UNIX_EPOCH) {
                let minutes = duration.as_secs() as f64 / 60.0;
                if minutes > 0.0 {
                    return self.total_errors as f64 / minutes;
                }
            }
        }
        0.0
    }

    /// Get most common error type
    pub fn most_common_error_type(&self) -> Option<(String, u64)> {
        self.errors_by_type
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(type_, &count)| (type_.clone(), count))
    }

    /// Get plugin with most errors
    pub fn plugin_with_most_errors(&self) -> Option<(String, u64)> {
        self.errors_by_plugin
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(plugin, &count)| (plugin.clone(), count))
    }

    /// Clear old errors (older than 1 hour)
    pub fn clear_old_errors(&mut self) {
        let one_hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        self.recent_errors.retain(|e| e.context.timestamp > one_hour_ago);
    }
}