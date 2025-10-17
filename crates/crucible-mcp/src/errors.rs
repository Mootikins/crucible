/// Comprehensive Error Handling System for Crucible MCP
///
/// This module provides structured error handling with:
/// - Specific error types for different components
/// - Rich error context and metadata
/// - Error categorization and severity levels
/// - Recovery suggestions and error codes
/// - Error aggregation for batch operations

use std::collections::HashMap;
use std::fmt;
use serde::{Serialize, Deserialize};

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Critical error that prevents operation
    Critical = 1,
    /// Error that affects functionality but system can continue
    Error = 2,
    /// Warning that should be addressed but doesn't stop operation
    Warning = 3,
    /// Informational message
    Info = 4,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Warning => write!(f, "WARNING"),
            ErrorSeverity::Info => write!(f, "INFO"),
        }
    }
}

/// Error categories for better organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Network and transport related errors
    Network,
    /// MCP protocol related errors
    Protocol,
    /// Database and storage errors
    Database,
    /// Rune script compilation and execution errors
    Rune,
    /// Schema validation errors
    Validation,
    /// Tool discovery and registration errors
    ToolDiscovery,
    /// Authentication and authorization errors
    Authentication,
    /// Configuration errors
    Configuration,
    /// Resource limitation errors
    Resource,
    /// Input/output errors
    Io,
    /// Uncategorized errors
    Other,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Network => write!(f, "NETWORK"),
            ErrorCategory::Protocol => write!(f, "PROTOCOL"),
            ErrorCategory::Database => write!(f, "DATABASE"),
            ErrorCategory::Rune => write!(f, "RUNE"),
            ErrorCategory::Validation => write!(f, "VALIDATION"),
            ErrorCategory::ToolDiscovery => write!(f, "TOOL_DISCOVERY"),
            ErrorCategory::Authentication => write!(f, "AUTHENTICATION"),
            ErrorCategory::Configuration => write!(f, "CONFIGURATION"),
            ErrorCategory::Resource => write!(f, "RESOURCE"),
            ErrorCategory::Io => write!(f, "IO"),
            ErrorCategory::Other => write!(f, "OTHER"),
        }
    }
}

/// Error context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Component where the error occurred
    pub component: String,
    /// Operation being performed
    pub operation: Option<String>,
    /// Tool name if applicable
    pub tool_name: Option<String>,
    /// File path if applicable
    pub file_path: Option<String>,
    /// Line number if applicable
    pub line_number: Option<u32>,
    /// Additional key-value context
    pub metadata: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new(component: &str) -> Self {
        Self {
            component: component.to_string(),
            operation: None,
            tool_name: None,
            file_path: None,
            line_number: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_operation(mut self, operation: &str) -> Self {
        self.operation = Some(operation.to_string());
        self
    }

    pub fn with_tool_name(mut self, tool_name: &str) -> Self {
        self.tool_name = Some(tool_name.to_string());
        self
    }

    pub fn with_file_path(mut self, file_path: &str) -> Self {
        self.file_path = Some(file_path.to_string());
        self
    }

    pub fn with_line_number(mut self, line_number: u32) -> Self {
        self.line_number = Some(line_number);
        self
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Recovery suggestions for errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySuggestion {
    /// Primary suggestion
    pub suggestion: String,
    /// Additional suggestions
    pub alternatives: Vec<String>,
    /// Documentation URL if available
    pub documentation_url: Option<String>,
    /// Whether recovery is possible automatically
    pub auto_recoverable: bool,
}

impl RecoverySuggestion {
    pub fn new(suggestion: &str) -> Self {
        Self {
            suggestion: suggestion.to_string(),
            alternatives: Vec::new(),
            documentation_url: None,
            auto_recoverable: false,
        }
    }

    pub fn with_alternative(mut self, alternative: &str) -> Self {
        self.alternatives.push(alternative.to_string());
        self
    }

    pub fn with_documentation(mut self, url: &str) -> Self {
        self.documentation_url = Some(url.to_string());
        self
    }

    pub fn auto_recoverable(mut self) -> Self {
        self.auto_recoverable = true;
        self
    }
}

/// Comprehensive error type for Crucible MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrucibleError {
    /// Unique error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Error category
    pub category: ErrorCategory,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Error context
    pub context: ErrorContext,
    /// Recovery suggestions
    pub recovery: Option<RecoverySuggestion>,
    /// Original cause if available
    pub cause: Option<String>,
    /// Timestamp when error occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Error ID for tracking
    pub error_id: String,
}

impl CrucibleError {
    /// Create a new error
    pub fn new(
        code: &str,
        message: &str,
        category: ErrorCategory,
        severity: ErrorSeverity,
        context: ErrorContext,
    ) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            category,
            severity,
            context,
            recovery: None,
            cause: None,
            timestamp: chrono::Utc::now(),
            error_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Add recovery suggestion
    pub fn with_recovery(mut self, recovery: RecoverySuggestion) -> Self {
        self.recovery = Some(recovery);
        self
    }

    /// Add cause
    pub fn with_cause(mut self, cause: &str) -> Self {
        self.cause = Some(cause.to_string());
        self
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        self.recovery
            .as_ref()
            .map(|r| r.auto_recoverable)
            .unwrap_or(false)
    }

    /// Get error summary for logging
    pub fn summary(&self) -> String {
        format!("[{}] {} ({}): {}",
            self.severity,
            self.category,
            self.code,
            self.message
        )
    }

    /// Get detailed error information
    pub fn details(&self) -> String {
        let mut details = format!(
            "Error ID: {}\nCode: {}\nMessage: {}\nCategory: {}\nSeverity: {}\nComponent: {}\nTimestamp: {}",
            self.error_id,
            self.code,
            self.message,
            self.category,
            self.severity,
            self.context.component,
            self.timestamp.to_rfc3339()
        );

        if let Some(operation) = &self.context.operation {
            details.push_str(&format!("\nOperation: {}", operation));
        }

        if let Some(tool_name) = &self.context.tool_name {
            details.push_str(&format!("\nTool: {}", tool_name));
        }

        if let Some(file_path) = &self.context.file_path {
            details.push_str(&format!("\nFile: {}", file_path));
        }

        if let Some(line_number) = self.context.line_number {
            details.push_str(&format!("\nLine: {}", line_number));
        }

        if let Some(cause) = &self.cause {
            details.push_str(&format!("\nCause: {}", cause));
        }

        if let Some(recovery) = &self.recovery {
            details.push_str(&format!("\nRecovery: {}", recovery.suggestion));
            for alt in &recovery.alternatives {
                details.push_str(&format!("\nAlternative: {}", alt));
            }
        }

        details
    }
}

impl fmt::Display for CrucibleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary())
    }
}

impl std::error::Error for CrucibleError {}

/// Error builder for convenient error creation
pub struct ErrorBuilder {
    code: String,
    message: String,
    category: ErrorCategory,
    severity: ErrorSeverity,
    context: ErrorContext,
    cause: Option<String>,
}

impl ErrorBuilder {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            category: ErrorCategory::Other,
            severity: ErrorSeverity::Error,
            context: ErrorContext::new("unknown"),
            cause: None,
        }
    }

    pub fn category(mut self, category: ErrorCategory) -> Self {
        self.category = category;
        self
    }

    pub fn severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn context(mut self, context: ErrorContext) -> Self {
        self.context = context;
        self
    }

    pub fn component(mut self, component: &str) -> Self {
        self.context = self.context.with_operation(component);
        self
    }

    pub fn operation(mut self, operation: &str) -> Self {
        self.context = self.context.with_operation(operation);
        self
    }

    pub fn tool_name(mut self, tool_name: &str) -> Self {
        self.context = self.context.with_tool_name(tool_name);
        self
    }

    pub fn file_path(mut self, file_path: &str) -> Self {
        self.context = self.context.with_file_path(file_path);
        self
    }

    pub fn line_number(mut self, line_number: u32) -> Self {
        self.context = self.context.with_line_number(line_number);
        self
    }

    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.context = self.context.with_metadata(key, value);
        self
    }

    pub fn with_cause(mut self, cause: &str) -> Self {
        self.cause = Some(cause.to_string());
        self
    }

    pub fn build(self) -> CrucibleError {
        let mut error = CrucibleError::new(
            &self.code,
            &self.message,
            self.category,
            self.severity,
            self.context,
        );
        if let Some(cause) = self.cause {
            error = error.with_cause(&cause);
        }
        error
    }

    pub fn build_with_recovery(self, recovery: RecoverySuggestion) -> CrucibleError {
        let mut error = CrucibleError::new(
            &self.code,
            &self.message,
            self.category,
            self.severity,
            self.context,
        ).with_recovery(recovery);
        if let Some(cause) = self.cause {
            error = error.with_cause(&cause);
        }
        error
    }
}

/// Convenience functions for common error types
pub mod error_codes {

    // Rune errors
    pub const RUNE_TOOL_NOT_FOUND: &str = "RUNE_001";
    pub const RUNE_COMPILATION_FAILED: &str = "RUNE_002";
    pub const RUNE_EXECUTION_FAILED: &str = "RUNE_003";
    pub const RUNE_TIMEOUT: &str = "RUNE_004";
    pub const RUNE_INVALID_SCHEMA: &str = "RUNE_005";

    // Tool discovery errors
    pub const TOOL_DISCOVERY_FAILED: &str = "TOOL_001";
    pub const TOOL_REGISTRATION_FAILED: &str = "TOOL_002";
    pub const TOOL_VALIDATION_FAILED: &str = "TOOL_003";
    pub const TOOL_HANDLER_NOT_FOUND: &str = "TOOL_004";

    // Database errors
    pub const DATABASE_CONNECTION_FAILED: &str = "DB_001";
    pub const DATABASE_QUERY_FAILED: &str = "DB_002";
    pub const DATABASE_TRANSACTION_FAILED: &str = "DB_003";

    // Protocol errors
    pub const PROTOCOL_PARSE_ERROR: &str = "PROTO_001";
    pub const PROTOCOL_INVALID_REQUEST: &str = "PROTO_002";
    pub const PROTOCOL_RESPONSE_FAILED: &str = "PROTO_003";

    // Validation errors
    pub const VALIDATION_SCHEMA_INVALID: &str = "VAL_001";
    pub const VALIDATION_PARAMETER_INVALID: &str = "VAL_002";
    pub const VALIDATION_TYPE_MISMATCH: &str = "VAL_003";

    // Configuration errors
    pub const CONFIG_MISSING_FIELD: &str = "CFG_001";
    pub const CONFIG_INVALID_VALUE: &str = "CFG_002";
    pub const CONFIG_FILE_NOT_FOUND: &str = "CFG_003";

    // Resource errors
    pub const RESOURCE_MEMORY_EXHAUSTED: &str = "RES_001";
    pub const RESOURCE_TIMEOUT: &str = "RES_002";
    pub const RESOURCE_LIMIT_EXCEEDED: &str = "RES_003";
}

/// Error creation helper functions
pub mod errors {
    use super::*;

    pub fn rune_tool_not_found(tool_name: &str, component: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::RUNE_TOOL_NOT_FOUND,
                         &format!("Rune tool '{}' not found", tool_name))
            .category(ErrorCategory::Rune)
            .severity(ErrorSeverity::Error)
            .component(component)
            .tool_name(tool_name)
            .build_with_recovery(
                RecoverySuggestion::new("Check if the tool file exists and is properly formatted")
                    .with_alternative("Verify the tool is registered in the tool registry")
                    .with_alternative("Check tool discovery logs for compilation errors")
            )
    }

    pub fn rune_compilation_failed(tool_name: &str, error: &str, file_path: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::RUNE_COMPILATION_FAILED,
                         &format!("Rune tool '{}' compilation failed: {}", tool_name, error))
            .category(ErrorCategory::Rune)
            .severity(ErrorSeverity::Error)
            .tool_name(tool_name)
            .file_path(file_path)
            .with_cause(error)
            .build_with_recovery(
                RecoverySuggestion::new("Fix syntax errors in the Rune script")
                    .with_alternative("Check for missing dependencies or imports")
                    .with_alternative("Verify function signatures match expected format")
            )
    }

    pub fn rune_execution_failed(tool_name: &str, error: &str, component: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::RUNE_EXECUTION_FAILED,
                         &format!("Rune tool '{}' execution failed: {}", tool_name, error))
            .category(ErrorCategory::Rune)
            .severity(ErrorSeverity::Error)
            .component(component)
            .tool_name(tool_name)
            .with_cause(error)
            .build_with_recovery(
                RecoverySuggestion::new("Check tool parameters and inputs")
                    .with_alternative("Verify external dependencies are available")
                    .with_alternative("Review tool logic for runtime errors")
            )
    }

    pub fn tool_discovery_failed(directory: &str, error: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::TOOL_DISCOVERY_FAILED,
                         &format!("Tool discovery failed in directory '{}': {}", directory, error))
            .category(ErrorCategory::ToolDiscovery)
            .severity(ErrorSeverity::Error)
            .file_path(directory)
            .with_cause(error)
            .build_with_recovery(
                RecoverySuggestion::new("Check directory permissions and accessibility")
                    .with_alternative("Verify directory contains valid .rn files")
                    .with_alternative("Check for file system errors")
            )
    }

    pub fn validation_parameter_invalid(param_name: &str, expected: &str, actual: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::VALIDATION_PARAMETER_INVALID,
                         &format!("Parameter '{}' validation failed: expected {}, got {}",
                                 param_name, expected, actual))
            .category(ErrorCategory::Validation)
            .severity(ErrorSeverity::Error)
            .metadata("parameter", param_name)
            .metadata("expected", expected)
            .metadata("actual", actual)
            .build_with_recovery(
                RecoverySuggestion::new(&format!("Provide a valid value for parameter '{}'", param_name))
                    .with_alternative("Check the tool's input schema for expected format")
            )
    }

    pub fn database_connection_failed(connection_string: &str, error: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::DATABASE_CONNECTION_FAILED,
                         &format!("Database connection failed: {}", error))
            .category(ErrorCategory::Database)
            .severity(ErrorSeverity::Critical)
            .with_cause(error)
            .metadata("connection", connection_string)
            .build_with_recovery(
                RecoverySuggestion::new("Verify database connection string and credentials")
                    .with_alternative("Check if database server is running")
                    .with_alternative("Verify network connectivity to database")
            )
    }

    pub fn protocol_invalid_request(operation: &str, details: &str) -> CrucibleError {
        ErrorBuilder::new(error_codes::PROTOCOL_INVALID_REQUEST,
                         &format!("Invalid {} request: {}", operation, details))
            .category(ErrorCategory::Protocol)
            .severity(ErrorSeverity::Error)
            .operation(operation)
            .build_with_recovery(
                RecoverySuggestion::new("Review MCP protocol specification")
                    .with_alternative("Check request format and required parameters")
                    .with_alternative("Verify tool names and parameters are valid")
            )
    }

    pub fn resource_memory_exhausted(operation: &str, used_mb: u64, limit_mb: u64) -> CrucibleError {
        ErrorBuilder::new(error_codes::RESOURCE_MEMORY_EXHAUSTED,
                         &format!("Memory exhausted during {}: {}MB used, {}MB limit",
                                 operation, used_mb, limit_mb))
            .category(ErrorCategory::Resource)
            .severity(ErrorSeverity::Critical)
            .operation(operation)
            .metadata("used_mb", &used_mb.to_string())
            .metadata("limit_mb", &limit_mb.to_string())
            .build_with_recovery(
                RecoverySuggestion::new("Free up system memory")
                    .with_alternative("Reduce operation batch size")
                    .with_alternative("Increase memory limits if possible")
            )
    }
}

/// Error aggregation for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAggregator {
    pub errors: Vec<CrucibleError>,
    pub summary: ErrorSummary,
}

impl ErrorAggregator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            summary: ErrorSummary::new(),
        }
    }

    pub fn add_error(&mut self, error: CrucibleError) {
        self.summary.add_error(&error);
        self.errors.push(error);
    }

    pub fn has_critical_errors(&self) -> bool {
        self.summary.critical_count > 0
    }

    pub fn has_errors(&self) -> bool {
        self.summary.error_count > 0
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn get_errors_by_category(&self, category: ErrorCategory) -> Vec<&CrucibleError> {
        self.errors.iter()
            .filter(|e| e.category == category)
            .collect()
    }

    pub fn get_errors_by_severity(&self, severity: ErrorSeverity) -> Vec<&CrucibleError> {
        self.errors.iter()
            .filter(|e| e.severity == severity)
            .collect()
    }
}

/// Error summary for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    pub total_count: usize,
    pub critical_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub by_category: HashMap<String, usize>,
}

impl ErrorSummary {
    pub fn new() -> Self {
        Self {
            total_count: 0,
            critical_count: 0,
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            by_category: HashMap::new(),
        }
    }

    pub fn add_error(&mut self, error: &CrucibleError) {
        self.total_count += 1;

        match error.severity {
            ErrorSeverity::Critical => self.critical_count += 1,
            ErrorSeverity::Error => self.error_count += 1,
            ErrorSeverity::Warning => self.warning_count += 1,
            ErrorSeverity::Info => self.info_count += 1,
        }

        let category = error.category.to_string();
        *self.by_category.entry(category).or_insert(0) += 1;
    }
}

/// Result type alias for convenience
pub type CrucibleResult<T> = Result<T, CrucibleError>;

/// Trait for converting other error types to CrucibleError
pub trait IntoCrucibleError<T> {
    fn into_crucible_error(self, context: ErrorContext) -> CrucibleResult<T>;
}

impl<T> IntoCrucibleError<T> for Result<T, anyhow::Error> {
    fn into_crucible_error(self, context: ErrorContext) -> CrucibleResult<T> {
        self.map_err(|e| {
            ErrorBuilder::new("ANYHOW_ERROR", &e.to_string())
                .category(ErrorCategory::Other)
                .severity(ErrorSeverity::Error)
                .context(context)
                .with_cause(&e.to_string())
                .build()
        })
    }
}

impl<T> IntoCrucibleError<T> for Result<T, serde_json::Error> {
    fn into_crucible_error(self, context: ErrorContext) -> CrucibleResult<T> {
        self.map_err(|e| {
            ErrorBuilder::new("JSON_ERROR", &format!("JSON serialization/deserialization error: {}", e))
                .category(ErrorCategory::Io)
                .severity(ErrorSeverity::Error)
                .context(context)
                .with_cause(&e.to_string())
                .build()
        })
    }
}

impl<T> IntoCrucibleError<T> for Result<T, std::io::Error> {
    fn into_crucible_error(self, context: ErrorContext) -> CrucibleResult<T> {
        self.map_err(|e| {
            ErrorBuilder::new("IO_ERROR", &format!("I/O error: {}", e))
                .category(ErrorCategory::Io)
                .severity(ErrorSeverity::Error)
                .context(context)
                .with_cause(&e.to_string())
                .build()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{errors, error_codes, ErrorCategory, ErrorSeverity, ErrorAggregator, ErrorContext, RecoverySuggestion};

    #[test]
    fn test_error_creation() {
        let error = errors::rune_tool_not_found("test_tool", "test_component");

        assert_eq!(error.code, error_codes::RUNE_TOOL_NOT_FOUND);
        assert_eq!(error.category, ErrorCategory::Rune);
        assert_eq!(error.severity, ErrorSeverity::Error);
        assert!(error.recovery.is_some());
    }

    #[test]
    fn test_error_aggregator() {
        let mut aggregator = ErrorAggregator::new();

        let error1 = errors::rune_tool_not_found("tool1", "component1");
        let error2 = errors::rune_compilation_failed("tool2", "syntax error", "/path/to/tool2.rn");

        aggregator.add_error(error1);
        aggregator.add_error(error2);

        assert_eq!(aggregator.errors.len(), 2);
        assert_eq!(aggregator.summary.total_count, 2);
        assert_eq!(aggregator.summary.error_count, 2);
        assert!(!aggregator.has_critical_errors());
        assert!(aggregator.has_errors());
    }

    #[test]
    fn test_error_context() {
        let context = ErrorContext::new("test_component")
            .with_operation("test_operation")
            .with_tool_name("test_tool")
            .with_file_path("/path/to/file.rn")
            .with_line_number(42)
            .with_metadata("key1", "value1");

        assert_eq!(context.component, "test_component");
        assert_eq!(context.operation, Some("test_operation".to_string()));
        assert_eq!(context.tool_name, Some("test_tool".to_string()));
        assert_eq!(context.file_path, Some("/path/to/file.rn".to_string()));
        assert_eq!(context.line_number, Some(42));
        assert_eq!(context.metadata.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_recovery_suggestion() {
        let recovery = RecoverySuggestion::new("Primary suggestion")
            .with_alternative("Alternative 1")
            .with_alternative("Alternative 2")
            .with_documentation("https://example.com/docs")
            .auto_recoverable();

        assert_eq!(recovery.suggestion, "Primary suggestion");
        assert_eq!(recovery.alternatives.len(), 2);
        assert!(recovery.documentation_url.is_some());
        assert!(recovery.auto_recoverable);
    }
}