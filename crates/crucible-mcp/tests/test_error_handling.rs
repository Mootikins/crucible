/// Comprehensive Error Handling System Tests
///
/// Tests the enhanced error handling system for Rune tools including:
/// - Error categorization and severity levels
/// - Error recovery mechanisms
/// - Circuit breaker pattern
/// - Error aggregation and reporting
/// - Error logging and monitoring

use anyhow::Result;
use crucible_mcp::rune_tools::{ToolRegistry, RuneErrorHandler, ErrorRecoveryManager, CircuitBreaker};
use crucible_mcp::errors::{CrucibleError, ErrorContext, ErrorCategory, ErrorSeverity, errors};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_rune_error_handler_basic() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create registry
    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new(tool_dir.clone(), context)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Create error handler
    let mut error_handler = RuneErrorHandler::new(registry_arc);

    // Initially no errors
    let stats = error_handler.get_error_stats();
    assert_eq!(stats.total_errors, 0);
    assert_eq!(stats.rune_errors, 0);

    // Add compilation error
    let error1 = error_handler.handle_compilation_error(
        "test_tool",
        tool_dir.join("test_tool.rn").as_path(),
        "syntax error: unexpected token",
        Some("pub fn NAME() { \"test\"")
    );

    // Add execution error
    let error2 = error_handler.handle_execution_error(
        "test_tool",
        "call",
        "runtime error: division by zero",
        &json!({"input": 0})
    );

    // Check stats
    let stats = error_handler.get_error_stats();
    assert_eq!(stats.total_errors, 2);
    assert_eq!(stats.rune_errors, 2);
    assert_eq!(stats.critical_errors, 0);

    // Check error details
    let all_errors = error_handler.get_all_errors();
    assert_eq!(all_errors.len(), 2);

    // Check compilation error
    let compilation_errors = error_handler.get_errors_by_category(ErrorCategory::Rune);
    assert_eq!(compilation_errors.len(), 2);

    // Generate error report
    let report = error_handler.generate_error_report();
    assert!(report.contains("Total Errors: 2"));
    assert!(report.contains("Rune Errors: 2"));

    Ok(())
}

#[tokio::test]
async fn test_error_recovery_manager() -> Result<()> {
    let mut recovery_manager = ErrorRecoveryManager::new();

    // Test compilation error recovery
    let compilation_error = errors::rune_compilation_failed(
        "test_tool",
        "syntax error: missing semicolon",
        "/path/to/tool.rn"
    );

    let recovery_attempt = recovery_manager.attempt_recovery(&compilation_error);
    assert!(recovery_attempt.is_some());

    let attempt = recovery_attempt.unwrap();
    assert!(attempt.action.contains("recompile"));
    assert!(attempt.action.contains("test_tool"));

    // Test execution error recovery
    let execution_error = errors::rune_execution_failed(
        "test_tool",
        "runtime error: null pointer",
        "TestComponent"
    );

    let recovery_attempt = recovery_manager.attempt_recovery(&execution_error);
    assert!(recovery_attempt.is_some());

    let attempt = recovery_attempt.unwrap();
    assert!(attempt.action.contains("Retry execution"));
    assert!(attempt.action.contains("test_tool"));

    // Test discovery error recovery
    let discovery_error = errors::tool_discovery_failed(
        "/tools",
        "permission denied"
    );

    let recovery_attempt = recovery_manager.attempt_recovery(&discovery_error);
    assert!(recovery_attempt.is_some());

    let attempt = recovery_attempt.unwrap();
    assert!(attempt.action.contains("Refresh tool discovery"));

    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_pattern() -> Result<()> {
    let mut circuit_breaker = CircuitBreaker::new(3, chrono::Duration::seconds(1));

    // Initially closed
    assert!(!circuit_breaker.is_open());
    assert_eq!(circuit_breaker.get_failure_count(), 0);

    // Test successful calls
    let result = circuit_breaker.call(|| Ok("success"));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(circuit_breaker.get_failure_count(), 0);
    assert!(!circuit_breaker.is_open());

    // Add failures up to threshold
    for i in 1..=3 {
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| {
            Err(errors::rune_tool_not_found("test_tool", "test_component"))
        });
        assert!(result.is_err());
        assert_eq!(circuit_breaker.get_failure_count(), i);

        if i < 3 {
            assert!(!circuit_breaker.is_open());
        } else {
            assert!(circuit_breaker.is_open());
        }
    }

    // Circuit should be open now
    assert!(circuit_breaker.is_open());

    // Calls should be blocked when circuit is open
    let result = circuit_breaker.call(|| Ok("should not execute"));
    assert!(result.is_err());
    assert!(circuit_breaker.is_open());

    // Reset circuit breaker
    circuit_breaker.reset();
    assert!(!circuit_breaker.is_open());
    assert_eq!(circuit_breaker.get_failure_count(), 0);

    Ok(())
}

#[tokio::test]
async fn test_error_context_enrichment() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new(tool_dir.clone(), context)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    let mut error_handler = RuneErrorHandler::new(registry_arc);

    // Test validation error with rich context
    let validation_error = error_handler.handle_validation_error(
        "test_tool",
        "input_string",
        "string",
        "null"
    );

    assert_eq!(validation_error.context.tool_name, Some("test_tool".to_string()));
    assert_eq!(validation_error.context.operation, Some("validate_parameters".to_string()));
    assert_eq!(validation_error.context.metadata.get("parameter"), Some(&"input_string".to_string()));

    // Test discovery error with file context
    let discovery_error = error_handler.handle_discovery_error(
        tool_dir.as_path(),
        "scan_directory",
        "permission denied"
    );

    assert_eq!(discovery_error.context.operation, Some("scan_directory".to_string()));
    assert_eq!(discovery_error.context.file_path, Some(tool_dir.to_str().unwrap().to_string()));

    // Test compilation error with source context
    let source_code = r#"
pub fn NAME() { "test" }
pub fn DESCRIPTION() { "Test tool" }
pub async fn call(args) {
    // This has a syntax error
    args.input.
"#;

    let compilation_error = error_handler.handle_compilation_error(
        "test_tool",
        tool_dir.join("test_tool.rn").as_path(),
        "unexpected token",
        Some(source_code)
    );

    assert_eq!(compilation_error.context.tool_name, Some("test_tool".to_string()));
    assert_eq!(compilation_error.context.metadata.get("source_code_length"), Some(&source_code.len().to_string()));

    Ok(())
}

#[tokio::test]
async fn test_error_aggregation_and_reporting() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new(tool_dir.clone(), context)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    let mut error_handler = RuneErrorHandler::new(registry_arc);

    // Add errors of different types and severities
    let compilation_error = error_handler.handle_compilation_error(
        "tool1",
        tool_dir.join("tool1.rn").as_path(),
        "syntax error",
        None
    );

    let execution_error = error_handler.handle_execution_error(
        "tool2",
        "call",
        "runtime error",
        &json!({"test": "value"})
    );

    let validation_error = error_handler.handle_validation_error(
        "tool3",
        "param1",
        "string",
        "number"
    );

    let loading_error = error_handler.handle_loading_error(
        tool_dir.join("missing_tool.rn").as_path(),
        "load_tool",
        "file not found"
    );

    // Verify aggregation
    let stats = error_handler.get_error_stats();
    assert_eq!(stats.total_errors, 4);
    assert_eq!(stats.rune_errors, 2); // compilation + execution
    assert_eq!(stats.validation_errors, 1);
    assert_eq!(stats.tool_discovery_errors, 1); // loading error falls under tool discovery

    // Generate comprehensive report
    let report = error_handler.generate_error_report();
    assert!(report.contains("Total Errors: 4"));
    assert!(report.contains("Rune Errors: 2"));
    assert!(report.contains("Validation Errors: 1"));
    assert!(report.contains("Database Errors: 0")); // Should be 0 since no DB errors

    // Verify errors by category
    let rune_errors = error_handler.get_errors_by_category(ErrorCategory::Rune);
    assert_eq!(rune_errors.len(), 2);

    let validation_errors = error_handler.get_errors_by_category(ErrorCategory::Validation);
    assert_eq!(validation_errors.len(), 1);

    // Check for critical errors
    assert!(!error_handler.has_critical_errors());

    // Clear errors and verify
    error_handler.clear_errors();
    let stats_after_clear = error_handler.get_error_stats();
    assert_eq!(stats_after_clear.total_errors, 0);

    Ok(())
}

#[tokio::test]
async fn test_error_recovery_strategies() -> Result<()> {
    let mut recovery_manager = ErrorRecoveryManager::new();

    // Add custom recovery strategy
    recovery_manager.add_strategy(
        crucible_mcp::rune_tools::RecoveryStrategy::new(
            ErrorCategory::Validation,
            "VALIDATION_PARAMETER_INVALID",
            |error| {
                if let Some(param_name) = error.context.metadata.get("parameter") {
                    format!("Apply parameter transformation for '{}'", param_name)
                } else {
                    "Apply generic parameter validation fix".to_string()
                }
            }
        )
    );

    // Test custom strategy
    let validation_error = errors::validation_parameter_invalid(
        "input_field",
        "string",
        "null"
    );

    let recovery_attempt = recovery_manager.attempt_recovery(&validation_error);
    assert!(recovery_attempt.is_some());

    let attempt = recovery_attempt.unwrap();
    assert!(attempt.action.contains("parameter transformation"));
    assert!(attempt.action.contains("input_field"));

    Ok(())
}

#[tokio::test]
async fn test_error_severity_levels() -> Result<()> {
    // Test error creation with different severities
    let critical_error = CrucibleError::new(
        "CRITICAL_ERROR",
        "System cannot continue",
        ErrorCategory::Database,
        ErrorSeverity::Critical,
        ErrorContext::new("TestComponent").with_operation("critical_operation")
    );

    let error_error = CrucibleError::new(
        "ERROR_ERROR",
        "Operation failed",
        ErrorCategory::Rune,
        ErrorSeverity::Error,
        ErrorContext::new("TestComponent").with_operation("error_operation")
    );

    let warning_error = CrucibleError::new(
        "WARNING_ERROR",
        "Potential issue detected",
        ErrorCategory::Validation,
        ErrorSeverity::Warning,
        ErrorContext::new("TestComponent").with_operation("warning_operation")
    );

    // Verify severity levels
    assert_eq!(critical_error.severity, ErrorSeverity::Critical);
    assert_eq!(error_error.severity, ErrorSeverity::Error);
    assert_eq!(warning_error.severity, ErrorSeverity::Warning);

    // Verify error summaries
    assert!(critical_error.summary().contains("CRITICAL"));
    assert!(error_error.summary().contains("ERROR"));
    assert!(warning_error.summary().contains("WARNING"));

    // Test error ordering
    assert!(critical_error.severity < error_error.severity);
    assert!(error_error.severity < warning_error.severity);

    Ok(())
}

#[tokio::test]
async fn test_complex_error_scenario() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    let context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new(tool_dir.clone(), context)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    let mut error_handler = RuneErrorHandler::new(registry_arc);
    let mut recovery_manager = ErrorRecoveryManager::new();
    let mut circuit_breaker = CircuitBreaker::new(2, chrono::Duration::seconds(1));

    // Simulate a complex error scenario
    // 1. Tool discovery fails
    let discovery_error = error_handler.handle_discovery_error(
        tool_dir.as_path(),
        "scan_directory",
        "permission denied"
    );

    // 2. Try recovery
    let recovery_attempt = recovery_manager.attempt_recovery(&discovery_error);
    assert!(recovery_attempt.is_some());

    // 3. Simulate repeated failures to trigger circuit breaker
    for _ in 0..3 {
        let result: Result<&str, CrucibleError> = circuit_breaker.call(|| {
            Err(errors::tool_discovery_failed(tool_dir.to_str().unwrap(), "permission denied"))
        });

        if circuit_breaker.is_open() {
            break;
        }
    }

    // Circuit should be open
    assert!(circuit_breaker.is_open());

    // 4. Generate comprehensive report
    let report = error_handler.generate_error_report();
    assert!(report.contains("Total Errors: 1"));
    assert!(report.contains("Tool Discovery Errors: 1"));

    // 5. Verify error statistics
    let stats = error_handler.get_error_stats();
    assert_eq!(stats.total_errors, 1);
    assert_eq!(stats.tool_discovery_errors, 1);

    // 6. Reset and try recovery
    circuit_breaker.reset();
    assert!(!circuit_breaker.is_open());

    Ok(())
}

#[tokio::test]
async fn test_error_recovery_auto_recovery() -> Result<()> {
    // Test errors with automatic recovery
    let auto_recoverable_error = errors::validation_parameter_invalid(
        "test_param",
        "string",
        "number"
    ).with_recovery(
        crucible_mcp::errors::RecoverySuggestion::new("Convert number to string")
            .auto_recoverable()
    );

    assert!(auto_recoverable_error.is_recoverable());
    assert!(auto_recoverable_error.recovery.is_some());
    assert!(auto_recoverable_error.recovery.as_ref().unwrap().auto_recoverable);

    // Test non-auto-recoverable error
    let manual_recovery_error = errors::rune_compilation_failed(
        "test_tool",
        "syntax error",
        "/path/to/tool.rn"
    );

    assert!(!manual_recovery_error.is_recoverable());
    assert!(manual_recovery_error.recovery.is_some());
    assert!(!manual_recovery_error.recovery.as_ref().unwrap().auto_recoverable);

    Ok(())
}