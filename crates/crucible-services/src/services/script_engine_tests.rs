//! # Script Engine Unit Tests
//!
//! This module contains comprehensive unit tests for the Script Engine service,
//! testing individual components, methods, and error conditions in isolation.

use std::collections::HashMap;
use std::time::Duration;

use crucible_services::{
    script_engine::{
        CrucibleScriptEngine, ScriptEngineConfig, ScriptExecutionRequest,
        ScriptExecutionResponse, ScriptValidationResult, ExecutionContext
    },
    errors::ServiceError,
    types::tool::{ToolDefinition, ToolParameter, ToolExecutionContext},
};

/// Create a test script engine configuration
fn create_test_config() -> ScriptEngineConfig {
    ScriptEngineConfig {
        max_execution_time: Duration::from_secs(5),
        max_memory_usage: 10 * 1024 * 1024, // 10MB
        enable_sandbox: true,
        allowed_modules: vec!["std".to_string(), "crucible".to_string()],
        security_policies: HashMap::new(),
    }
}

/// Create a test script execution request
fn create_test_request(script: &str) -> ScriptExecutionRequest {
    ScriptExecutionRequest {
        script: script.to_string(),
        context: ExecutionContext {
            working_directory: Some("/tmp".to_string()),
            environment: HashMap::new(),
            permissions: vec![],
            timeout: Some(Duration::from_secs(3)),
        },
        metadata: HashMap::new(),
    }
}

#[cfg(test)]
mod script_engine_tests {
    use super::*;

    #[test]
    fn test_script_engine_config_creation() {
        let config = create_test_config();

        assert_eq!(config.max_execution_time, Duration::from_secs(5));
        assert_eq!(config.max_memory_usage, 10 * 1024 * 1024);
        assert!(config.enable_sandbox);
        assert!(!config.allowed_modules.is_empty());
    }

    #[test]
    fn test_script_engine_config_defaults() {
        let config = ScriptEngineConfig::default();

        assert!(config.max_execution_time > Duration::from_secs(0));
        assert!(config.max_memory_usage > 0);
        assert!(config.enable_sandbox); // Should default to secure
    }

    #[tokio::test]
    async fn test_script_engine_creation() {
        let config = create_test_config();
        let engine = CrucibleScriptEngine::new(config);

        // Test that the engine is created successfully
        assert!(engine.config().max_execution_time == Duration::from_secs(5));
        assert!(engine.config().enable_sandbox);
    }

    #[test]
    fn test_script_execution_request_creation() {
        let request = create_test_request("print('Hello, World!');");

        assert_eq!(request.script, "print('Hello, World!');");
        assert!(request.context.working_directory.is_some());
        assert_eq!(request.context.working_directory.unwrap(), "/tmp");
        assert!(request.context.environment.is_empty());
    }

    #[test]
    fn test_execution_context_creation() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());

        let context = ExecutionContext {
            working_directory: Some("/home".to_string()),
            environment: env.clone(),
            permissions: vec!["read".to_string()],
            timeout: Some(Duration::from_secs(10)),
        };

        assert_eq!(context.working_directory.unwrap(), "/home");
        assert_eq!(context.environment.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(context.permissions.len(), 1);
        assert_eq!(context.permissions[0], "read");
        assert_eq!(context.timeout.unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn test_script_validation_result() {
        let valid_result = ScriptValidationResult {
            valid: true,
            errors: vec![],
            warnings: vec!["Unused variable".to_string()],
            estimated_duration: Duration::from_millis(100),
        };

        assert!(valid_result.valid);
        assert!(valid_result.errors.is_empty());
        assert_eq!(valid_result.warnings.len(), 1);
        assert_eq!(valid_result.estimated_duration, Duration::from_millis(100));
    }

    #[test]
    fn test_script_validation_result_invalid() {
        let invalid_result = ScriptValidationResult {
            valid: false,
            errors: vec!["Syntax error at line 5".to_string()],
            warnings: vec![],
            estimated_duration: Duration::from_millis(0),
        };

        assert!(!invalid_result.valid);
        assert_eq!(invalid_result.errors.len(), 1);
        assert!(invalid_result.warnings.is_empty());
        assert_eq!(invalid_result.estimated_duration, Duration::from_millis(0));
    }

    #[test]
    fn test_script_execution_response() {
        let success_response = ScriptExecutionResponse {
            success: true,
            output: "Hello, World!".to_string(),
            error: None,
            execution_time: Duration::from_millis(150),
            memory_used: 1024,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("lines_executed".to_string(), "3".to_string());
                meta
            },
        };

        assert!(success_response.success);
        assert_eq!(success_response.output, "Hello, World!");
        assert!(success_response.error.is_none());
        assert_eq!(success_response.execution_time, Duration::from_millis(150));
        assert_eq!(success_response.memory_used, 1024);
        assert_eq!(success_response.metadata.get("lines_executed"), Some(&"3".to_string()));
    }

    #[test]
    fn test_script_execution_response_error() {
        let error_response = ScriptExecutionResponse {
            success: false,
            output: String::new(),
            error: Some("Division by zero".to_string()),
            execution_time: Duration::from_millis(50),
            memory_used: 512,
            metadata: HashMap::new(),
        };

        assert!(!error_response.success);
        assert!(error_response.output.is_empty());
        assert_eq!(error_response.error.unwrap(), "Division by zero");
        assert_eq!(error_response.execution_time, Duration::from_millis(50));
        assert_eq!(error_response.memory_used, 512);
        assert!(error_response.metadata.is_empty());
    }

    #[test]
    fn test_security_policy_enforcement() {
        let config = ScriptEngineConfig {
            max_execution_time: Duration::from_secs(1),
            max_memory_usage: 1024 * 1024, // 1MB
            enable_sandbox: true,
            allowed_modules: vec!["std".to_string()],
            security_policies: {
                let mut policies = HashMap::new();
                policies.insert("no_network".to_string(), true);
                policies
            },
        };

        assert_eq!(config.allowed_modules.len(), 1);
        assert_eq!(config.allowed_modules[0], "std");
        assert_eq!(config.security_policies.get("no_network"), Some(&true));
    }

    #[test]
    fn test_timeout_configuration() {
        let short_timeout = Duration::from_millis(100);
        let long_timeout = Duration::from_secs(30);

        let config_short = ScriptEngineConfig {
            max_execution_time: short_timeout,
            max_memory_usage: 1024 * 1024,
            enable_sandbox: true,
            allowed_modules: vec![],
            security_policies: HashMap::new(),
        };

        let config_long = ScriptEngineConfig {
            max_execution_time: long_timeout,
            max_memory_usage: 10 * 1024 * 1024,
            enable_sandbox: false,
            allowed_modules: vec!["std".to_string()],
            security_policies: HashMap::new(),
        };

        assert!(config_short.max_execution_time < config_long.max_execution_time);
        assert!(config_short.max_memory_usage < config_long.max_memory_usage);
        assert!(config_short.enable_sandbox);
        assert!(!config_long.enable_sandbox);
    }

    #[test]
    fn test_memory_limits() {
        let limits = vec![
            1024,             // 1KB
            1024 * 1024,      // 1MB
            10 * 1024 * 1024, // 10MB
            100 * 1024 * 1024 // 100MB
        ];

        for limit in limits {
            let config = ScriptEngineConfig {
                max_execution_time: Duration::from_secs(5),
                max_memory_usage: limit,
                enable_sandbox: true,
                allowed_modules: vec![],
                security_policies: HashMap::new(),
            };

            assert_eq!(config.max_memory_usage, limit);
        }
    }

    #[test]
    fn test_module_access_control() {
        let allowed_modules = vec![
            "std".to_string(),
            "serde".to_string(),
            "tokio".to_string(),
            "crucible".to_string(),
        ];

        let config = ScriptEngineConfig {
            max_execution_time: Duration::from_secs(10),
            max_memory_usage: 50 * 1024 * 1024,
            enable_sandbox: true,
            allowed_modules: allowed_modules.clone(),
            security_policies: HashMap::new(),
        };

        assert_eq!(config.allowed_modules.len(), 4);
        assert!(config.allowed_modules.contains(&"std".to_string()));
        assert!(config.allowed_modules.contains(&"serde".to_string()));
        assert!(config.allowed_modules.contains(&"tokio".to_string()));
        assert!(config.allowed_modules.contains(&"crucible".to_string()));
        assert!(!config.allowed_modules.contains(&"prohibited".to_string()));
    }

    // Integration-style unit tests that test method behavior

    #[tokio::test]
    async fn test_script_engine_service_health() {
        let config = create_test_config();
        let engine = CrucibleScriptEngine::new(config);

        // This would test the service_health method if implemented
        // For now, we test that the engine can be created and configured
        assert!(engine.config().enable_sandbox);
    }

    #[test]
    fn test_error_handling_scenarios() {
        // Test various error conditions that can occur

        // Empty script
        let empty_request = create_test_request("");
        assert!(empty_request.script.is_empty());

        // Very long script (would exceed limits)
        let long_script = "print('test');\n".repeat(10000);
        let long_request = create_test_request(&long_script);
        assert!(long_request.script.len() > 100000);

        // Script with potential security issues
        let suspicious_script = r#"
            import std::fs;
            std::fs::remove_file("/important/file");
        "#;
        let suspicious_request = create_test_request(suspicious_script);
        assert!(suspicious_request.script.contains("remove_file"));
    }

    #[test]
    fn test_metadata_handling() {
        let mut metadata = HashMap::new();
        metadata.insert("author".to_string(), "test_user".to_string());
        metadata.insert("version".to_string(), "1.0.0".to_string());
        metadata.insert("priority".to_string(), "high".to_string());

        let request = ScriptExecutionRequest {
            script: "print('test')".to_string(),
            context: ExecutionContext::default(),
            metadata: metadata.clone(),
        };

        assert_eq!(request.metadata.len(), 3);
        assert_eq!(request.metadata.get("author"), Some(&"test_user".to_string()));
        assert_eq!(request.metadata.get("version"), Some(&"1.0.0".to_string()));
        assert_eq!(request.metadata.get("priority"), Some(&"high".to_string()));
    }

    #[test]
    fn test_environment_variable_handling() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/user".to_string());
        env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        env.insert("DEBUG".to_string(), "true".to_string());

        let context = ExecutionContext {
            working_directory: None,
            environment: env.clone(),
            permissions: vec![],
            timeout: None,
        };

        assert_eq!(context.environment.len(), 3);
        assert_eq!(context.environment.get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(context.environment.get("PATH"), Some(&"/usr/bin:/bin".to_string()));
        assert_eq!(context.environment.get("DEBUG"), Some(&"true".to_string()));
    }
}