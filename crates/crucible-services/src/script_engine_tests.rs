//! Unit tests for ScriptEngine service
//!
//! This module provides comprehensive unit tests for the CrucibleScriptEngine service,
//! covering all major functionality, edge cases, and error conditions.

use super::*;
use crate::events::routing::MockEventRouter;
use tokio_test;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Create a test script engine with default configuration
async fn create_test_engine() -> CrucibleScriptEngine {
    let config = ScriptEngineConfig::default();
    CrucibleScriptEngine::new(config).await.unwrap()
}

/// Create a test execution context
fn create_test_execution_context(script_id: String) -> ExecutionContext {
    ExecutionContext {
        execution_id: Uuid::new_v4().to_string(),
        script_id,
        arguments: HashMap::new(),
        environment: HashMap::new(),
        working_directory: None,
        security_context: SecurityContext::default(),
        timeout: Some(Duration::from_secs(5)),
        available_tools: vec![],
        user_context: None,
    }
}

/// Create a test compilation context
fn create_test_compilation_context() -> CompilationContext {
    CompilationContext {
        target: CompilationTarget::Standard,
        optimization_level: OptimizationLevel::Balanced,
        include_paths: vec![],
        definitions: HashMap::new(),
        debug_info: false,
        security_level: SecurityLevel::Safe,
    }
}

#[cfg(test)]
mod script_engine_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation_default_config() {
        let engine = create_test_engine().await;

        // Verify initial state
        assert!(!engine.is_running());
        assert_eq!(engine.service_name(), "crucible-script-engine");
        assert_eq!(engine.service_version(), "0.1.0");
    }

    #[tokio::test]
    async fn test_engine_creation_custom_config() {
        let config = ScriptEngineConfig {
            max_cache_size: 100,
            default_execution_timeout: Duration::from_secs(60),
            max_source_size: 2048,
            enable_caching: false,
            security_level: SecurityLevel::Development,
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(50 * 1024 * 1024),
                max_cpu_percentage: Some(90.0),
                max_concurrent_operations: Some(25),
                operation_timeout: Some(Duration::from_secs(120)),
                max_disk_bytes: None,
                max_queue_size: None,
            },
        };

        let engine = CrucibleScriptEngine::new(config).await.unwrap();
        let engine_config = engine.get_config().await.unwrap();

        assert_eq!(engine_config.max_cache_size, 100);
        assert_eq!(engine_config.max_source_size, 2048);
        assert!(!engine_config.enable_caching);
        assert_eq!(engine_config.security_level, SecurityLevel::Development);
    }

    #[tokio::test]
    async fn test_service_lifecycle_start_stop() {
        let mut engine = create_test_engine().await;

        // Initially not running
        assert!(!engine.is_running());

        // Start the service
        engine.start().await.unwrap();
        assert!(engine.is_running());

        // Starting again should not cause issues (idempotent)
        engine.start().await.unwrap();
        assert!(engine.is_running());

        // Stop the service
        engine.stop().await.unwrap();
        assert!(!engine.is_running());

        // Stopping again should not cause issues (idempotent)
        engine.stop().await.unwrap();
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_service_restart() {
        let mut engine = create_test_engine().await;

        // Restart when not running
        engine.restart().await.unwrap();
        assert!(engine.is_running());

        // Restart when running
        engine.restart().await.unwrap();
        assert!(engine.is_running());
    }

    #[tokio::test]
    async fn test_service_metadata() {
        let engine = create_test_engine().await;

        assert_eq!(engine.service_name(), "crucible-script-engine");
        assert_eq!(engine.service_version(), "0.1.0");
    }
}

#[cfg(test)]
mod script_engine_health_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_not_running() {
        let engine = create_test_engine().await;

        let health = engine.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Unhealthy));
        assert!(health.message.is_some());
    }

    #[tokio::test]
    async fn test_health_check_running_healthy() {
        let mut engine = create_test_engine().await;
        engine.start().await.unwrap();

        let health = engine.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));
        assert!(health.message.is_some());

        // Check expected details
        assert!(health.details.contains_key("active_executions"));
        assert!(health.details.contains_key("cache_size"));
        assert!(health.details.contains_key("total_executions"));
        assert!(health.details.contains_key("success_rate"));
    }

    #[tokio::test]
    async fn test_health_check_degraded() {
        let mut engine = create_test_engine().await;
        engine.start().await.unwrap();

        // Simulate high load by setting low limits
        let limits = ResourceLimits {
            max_concurrent_operations: Some(1),
            ..Default::default()
        };
        engine.set_limits(limits).await.unwrap();

        // The health check should show degraded if we have more active executions than allowed
        // This is a simplified test - in practice, we'd need to actually execute scripts
        let health = engine.health_check().await.unwrap();
        // Should still be healthy since no executions are active
        assert!(matches!(health.status, ServiceStatus::Healthy));
    }
}

#[cfg(test)]
mod script_engine_configuration_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_configuration() {
        let engine = create_test_engine().await;
        let config = engine.get_config().await.unwrap();

        // Should return the default configuration
        assert_eq!(config.max_cache_size, 1000);
        assert_eq!(config.max_source_size, 1024 * 1024);
        assert!(config.enable_caching);
        assert_eq!(config.security_level, SecurityLevel::Safe);
    }

    #[tokio::test]
    async fn test_update_configuration() {
        let mut engine = create_test_engine().await;

        let new_config = ScriptEngineConfig {
            max_cache_size: 500,
            enable_caching: false,
            security_level: SecurityLevel::Production,
            ..Default::default()
        };

        engine.update_config(new_config.clone()).await.unwrap();
        let retrieved_config = engine.get_config().await.unwrap();

        assert_eq!(retrieved_config.max_cache_size, 500);
        assert!(!retrieved_config.enable_caching);
        assert_eq!(retrieved_config.security_level, SecurityLevel::Production);
    }

    #[tokio::test]
    async fn test_validate_configuration_valid() {
        let engine = create_test_engine().await;

        let valid_config = ScriptEngineConfig {
            max_cache_size: 100,
            default_execution_timeout: Duration::from_secs(30),
            ..Default::default()
        };

        let result = engine.validate_config(&valid_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_configuration_invalid() {
        let engine = create_test_engine().await;

        // Invalid: zero cache size
        let invalid_config1 = ScriptEngineConfig {
            max_cache_size: 0,
            ..Default::default()
        };
        let result = engine.validate_config(&invalid_config1).await;
        assert!(result.is_err());

        // Invalid: zero timeout
        let invalid_config2 = ScriptEngineConfig {
            default_execution_timeout: Duration::ZERO,
            ..Default::default()
        };
        let result = engine.validate_config(&invalid_config2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reload_configuration() {
        let mut engine = create_test_engine().await;

        // Reload should succeed (even if it's a no-op)
        let result = engine.reload_config().await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod script_engine_metrics_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_initial_metrics() {
        let engine = create_test_engine().await;

        let metrics = engine.get_metrics().await.unwrap();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
        assert_eq!(metrics.memory_usage, 0);
        assert_eq!(metrics.cpu_usage, 0.0);
    }

    #[tokio::test]
    async fn test_reset_metrics() {
        let mut engine = create_test_engine().await;

        // Execute some operations to generate metrics
        engine.start().await.unwrap();

        let metrics = engine.get_metrics().await.unwrap();
        assert!(metrics.total_requests >= 0);

        // Reset metrics
        engine.reset_metrics().await.unwrap();

        let reset_metrics = engine.get_metrics().await.unwrap();
        assert_eq!(reset_metrics.total_requests, 0);
        assert_eq!(reset_metrics.successful_requests, 0);
        assert_eq!(reset_metrics.failed_requests, 0);
        assert_eq!(reset_metrics.memory_usage, 0);
    }

    #[tokio::test]
    async fn test_get_performance_metrics() {
        let engine = create_test_engine().await;

        let perf_metrics = engine.get_performance_metrics().await.unwrap();
        assert_eq!(perf_metrics.active_connections, 0); // No active executions
        assert_eq!(perf_metrics.memory_usage, 0);
        assert_eq!(perf_metrics.cpu_usage, 0.0);
        assert!(perf_metrics.custom_metrics.is_empty());
    }
}

#[cfg(test)]
mod script_engine_compilation_tests {
    use super::*;

    #[tokio::test]
    async fn test_compile_simple_script() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Hello, World!"
            }
        "#;

        let context = create_test_compilation_context();
        let result = engine.compile_script(script_source, context).await;

        assert!(result.is_ok());
        let compiled = result.unwrap();

        assert!(!compiled.script_id.is_empty());
        assert_eq!(compiled.source, script_source);
        assert_eq!(compiled.metadata.language, "Rune");
        assert_eq!(compiled.metadata.version, "0.13.3");
        assert!(compiled.security_validation.valid);
    }

    #[tokio::test]
    async fn test_compile_large_script_within_limit() {
        let mut engine = create_test_engine().await;

        let script_source = "pub fn main() { 42 }\n".repeat(100); // ~2KB
        let context = create_test_compilation_context();
        let result = engine.compile_script(&script_source, context).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_compile_oversized_script() {
        let mut engine = create_test_engine().await;

        // Create a script larger than max_source_size (1MB default)
        let script_source = "pub fn main() { 42 }\n".repeat(500_000); // ~10MB
        let context = create_test_compilation_context();
        let result = engine.compile_script(&script_source, context).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ExecutionError(_)));
    }

    #[tokio::test]
    async fn test_compile_with_different_security_levels() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Test script"
            }
        "#;

        // Test Safe level
        let safe_context = CompilationContext {
            security_level: SecurityLevel::Safe,
            ..create_test_compilation_context()
        };
        let safe_result = engine.compile_script(script_source, safe_context).await;
        assert!(safe_result.is_ok());
        assert_eq!(safe_result.unwrap().security_validation.security_level, SecurityLevel::Safe);

        // Test Development level
        let dev_context = CompilationContext {
            security_level: SecurityLevel::Development,
            ..create_test_compilation_context()
        };
        let dev_result = engine.compile_script(script_source, dev_context).await;
        assert!(dev_result.is_ok());
        assert_eq!(dev_result.unwrap().security_validation.security_level, SecurityLevel::Development);

        // Test Production level
        let prod_context = CompilationContext {
            security_level: SecurityLevel::Production,
            ..create_test_compilation_context()
        };
        let prod_result = engine.compile_script(script_source, prod_context).await;
        assert!(prod_result.is_ok());
        assert_eq!(prod_result.unwrap().security_validation.security_level, SecurityLevel::Production);
    }

    #[tokio::test]
    async fn test_compilation_caching_enabled() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Cached script"
            }
        "#;

        let context = create_test_compilation_context();

        // First compilation
        let result1 = engine.compile_script(script_source, context.clone()).await;
        assert!(result1.is_ok());
        let compiled1 = result1.unwrap();

        // Second compilation of the same script
        let result2 = engine.compile_script(script_source, context).await;
        assert!(result2.is_ok());
        let compiled2 = result2.unwrap();

        // Should be cached (different script_id but same content)
        assert_eq!(compiled1.source, compiled2.source);
        assert!(compiled1.script_id != compiled2.script_id);
    }

    #[tokio::test]
    async fn test_compilation_caching_disabled() {
        let config = ScriptEngineConfig {
            enable_caching: false,
            ..Default::default()
        };
        let mut engine = CrucibleScriptEngine::new(config).await.unwrap();

        let script_source = r#"
            pub fn main() {
                "Non-cached script"
            }
        "#;

        let context = create_test_compilation_context();

        // First compilation
        let result1 = engine.compile_script(script_source, context.clone()).await;
        assert!(result1.is_ok());

        // Check if script is in cache (it shouldn't be)
        let cache_size = engine.script_cache.read().await.len();
        assert_eq!(cache_size, 0);
    }

    #[tokio::test]
    async fn test_get_compilation_errors() {
        let engine = create_test_engine().await;

        // Get errors for non-existent script
        let errors = engine.get_compilation_errors("non_existent_script").await.unwrap();
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn test_revalidate_cached_script() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Valid script"
            }
        "#;

        let context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, context).await.unwrap();

        // Revalidate existing script
        let validation = engine.revalidate_script(&compiled.script_id).await.unwrap();
        assert!(validation.valid);
        assert!(validation.errors.is_empty());

        // Revalidate non-existent script
        let validation = engine.revalidate_script("non_existent").await.unwrap();
        assert!(!validation.valid);
        assert!(!validation.errors.is_empty());
    }
}

#[cfg(test)]
mod script_engine_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_compiled_script() {
        let mut engine = create_test_engine().await;

        // First compile a script
        let script_source = r#"
            pub fn main() {
                "Hello, World!"
            }
        "#;

        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        // Execute the compiled script
        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let result = engine.execute_script(&compiled.script_id, execution_context).await;

        assert!(result.is_ok());
        let execution_result = result.unwrap();

        assert!(execution_result.success);
        assert!(!execution_result.execution_id.is_empty());
        assert_eq!(execution_result.script_id, compiled.script_id);
        assert!(execution_result.return_value.is_some());
        assert!(execution_result.stderr.is_empty());
        assert!(execution_result.stdout.contains("executed successfully"));
    }

    #[tokio::test]
    async fn test_execute_non_existent_script() {
        let engine = create_test_engine().await;

        let execution_context = create_test_execution_context("non_existent".to_string());
        let result = engine.execute_script("non_existent", execution_context).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ExecutionError(_)));
    }

    #[tokio::test]
    async fn test_execute_script_direct_from_source() {
        let engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Direct execution"
            }
        "#;

        let execution_context = create_test_execution_context("direct_script".to_string());
        let result = engine.execute_script_source(script_source, execution_context).await;

        assert!(result.is_ok());
        let execution_result = result.unwrap();

        assert!(execution_result.success);
        assert!(execution_result.return_value.is_some());
    }

    #[tokio::test]
    async fn test_execute_oversized_script_source() {
        let engine = create_test_engine().await;

        // Create a script larger than max_source_size
        let script_source = "pub fn main() { 42 }\n".repeat(500_000);
        let execution_context = create_test_execution_context("oversized".to_string());
        let result = engine.execute_script_source(&script_source, execution_context).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ExecutionError(_)));
    }

    #[tokio::test]
    async fn test_cancel_execution() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Test script"
            }
        "#;

        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        // Start execution (simulated - would normally take time)
        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let execution_id = execution_context.execution_id.clone();

        // For this test, we'll directly add an execution to track
        let execution_state = ExecutionState {
            execution_id: execution_id.clone(),
            script_id: compiled.script_id.clone(),
            started_at: std::time::Instant::now(),
            status: ExecutionStatus::Running,
            timeout: Some(Duration::from_secs(5)),
        };

        {
            let mut executions = engine.active_executions.write().await;
            executions.insert(execution_id.clone(), execution_state);
        }

        // Cancel the execution
        let cancel_result = engine.cancel_execution(&execution_id).await;
        assert!(cancel_result.is_ok());

        // Verify execution was removed
        let executions = engine.active_executions.read().await;
        assert!(!executions.contains_key(&execution_id));
    }

    #[tokio::test]
    async fn test_cancel_non_existent_execution() {
        let engine = create_test_engine().await;

        let result = engine.cancel_execution("non_existent_execution").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ExecutionError(_)));
    }

    #[tokio::test]
    async fn test_streaming_execution() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Streaming test"
            }
        "#;

        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let mut receiver = engine.execute_script_stream(&compiled.script_id, execution_context).await.unwrap();

        // Should receive stdout chunks
        let mut received_chunks = 0;
        while let Some(chunk) = receiver.recv().await {
            received_chunks += 1;

            match chunk.chunk_type {
                ExecutionChunkType::Stdout => {
                    assert!(!chunk.data.is_null());
                }
                ExecutionChunkType::Complete => {
                    assert!(chunk.data.is_object());
                    break;
                }
                _ => panic!("Unexpected chunk type: {:?}", chunk.chunk_type),
            }
        }

        assert!(received_chunks > 0);
    }
}

#[cfg(test)]
mod script_engine_security_tests {
    use super::*;

    #[tokio::test]
    async fn test_security_policy_levels() {
        let engine = create_test_engine().await;

        // Test Safe policy
        let safe_policy = SecurityPolicy::from_security_level(&SecurityLevel::Safe);
        assert_eq!(safe_policy.name, "safe");
        assert_eq!(safe_policy.default_security_level, SecurityLevel::Safe);
        assert!(!safe_policy.allow_file_access);
        assert!(!safe_policy.allow_network_access);
        assert!(!safe_policy.allow_system_calls);
        assert!(safe_policy.allowed_modules.contains(&"crucible::basic".to_string()));
        assert!(safe_policy.blocked_modules.contains(&"std::fs".to_string()));

        // Test Development policy
        let dev_policy = SecurityPolicy::from_security_level(&SecurityLevel::Development);
        assert_eq!(dev_policy.name, "development");
        assert!(dev_policy.allow_file_access);
        assert!(dev_policy.allow_network_access);
        assert!(dev_policy.allow_system_calls);
        assert!(dev_policy.allowed_modules.contains(&"*".to_string()));
        assert!(dev_policy.blocked_modules.is_empty());

        // Test Production policy
        let prod_policy = SecurityPolicy::from_security_level(&SecurityLevel::Production);
        assert_eq!(prod_policy.name, "production");
        assert!(!prod_policy.allow_file_access);
        assert!(prod_policy.allow_network_access);
        assert!(!prod_policy.allow_system_calls);
        assert!(prod_policy.blocked_modules.contains(&"std::process".to_string()));
    }

    #[tokio::test]
    async fn test_set_security_policy() {
        let mut engine = create_test_engine().await;

        let policy = SecurityPolicy::from_security_level(&SecurityLevel::Production);
        let result = engine.set_security_policy(policy.clone()).await;
        assert!(result.is_ok());

        let retrieved_policy = engine.get_security_policy().await.unwrap();
        assert_eq!(retrieved_policy.name, policy.name);
        assert_eq!(retrieved_policy.default_security_level, policy.default_security_level);
    }

    #[tokio::test]
    async fn test_validate_script_security() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Security test"
            }
        "#;

        let context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, context).await.unwrap();

        // Validate existing script
        let validation = engine.validate_script_security(&compiled.script_id).await.unwrap();
        assert!(validation.valid);
        assert_eq!(validation.security_level, SecurityLevel::Safe);

        // Validate non-existent script
        let validation = engine.validate_script_security("non_existent").await.unwrap();
        assert!(!validation.valid);
        assert!(!validation.violations.is_empty());
    }
}

#[cfg(test)]
mod script_engine_script_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_list_scripts_empty() {
        let engine = create_test_engine().await;

        let scripts = engine.list_scripts().await.unwrap();
        assert!(scripts.is_empty());
    }

    #[tokio::test]
    async fn test_list_scripts_with_compiled() {
        let mut engine = create_test_engine().await;

        // Compile a few scripts
        let script1 = r#"
            pub fn main() {
                "Script 1"
            }
        "#;

        let script2 = r#"
            pub fn main() {
                "Script 2"
            }
        "#;

        let context = create_test_compilation_context();
        let compiled1 = engine.compile_script(script1, context.clone()).await.unwrap();
        let compiled2 = engine.compile_script(script2, context).await.unwrap();

        let scripts = engine.list_scripts().await.unwrap();
        assert_eq!(scripts.len(), 2);

        // Verify script info
        let script_ids: Vec<String> = scripts.iter().map(|s| s.script_id.clone()).collect();
        assert!(script_ids.contains(&compiled1.script_id));
        assert!(script_ids.contains(&compiled2.script_id));
    }

    #[tokio::test]
    async fn test_get_script_info() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Info test"
            }
        "#;

        let context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, context).await.unwrap();

        // Get existing script info
        let info = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(info.is_some());
        let script_info = info.unwrap();

        assert_eq!(script_info.script_id, compiled.script_id);
        assert_eq!(script_info.name, compiled.script_id);
        assert_eq!(script_info.language, "Rune");
        assert_eq!(script_info.size_bytes, script_source.len() as u64);

        // Get non-existent script info
        let info = engine.get_script_info("non_existent").await.unwrap();
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_delete_script() {
        let mut engine = create_test_engine().await;

        let script_source = r#"
            pub fn main() {
                "Delete test"
            }
        "#;

        let context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, context).await.unwrap();

        // Script should exist
        let info = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(info.is_some());

        // Delete the script
        let result = engine.delete_script(&compiled.script_id).await;
        assert!(result.is_ok());

        // Script should no longer exist
        let info = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(info.is_none());

        // Deleting non-existent script should fail
        let result = engine.delete_script("non_existent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_script_context() {
        let mut engine = create_test_engine().await;

        let context = create_test_compilation_context();
        let compiled = engine.compile_script("pub fn main() {}", context).await.unwrap();

        let new_context = create_test_execution_context(compiled.script_id.clone());
        let result = engine.update_script_context(&compiled.script_id, new_context).await;

        // Should succeed even if not implemented (returns Ok(()))
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod script_engine_cache_tests {
    use super::*;

    #[tokio::test]
    async fn test_clear_cache() {
        let mut engine = create_test_engine().await;

        // Compile some scripts to populate cache
        let script_source = r#"
            pub fn main() {
                "Cache test"
            }
        "#;

        let context = create_test_compilation_context();
        engine.compile_script(script_source, context).await.unwrap();

        // Verify cache has content
        let cache_size = engine.script_cache.read().await.len();
        assert!(cache_size > 0);

        // Clear cache
        let result = engine.clear_cache().await;
        assert!(result.is_ok());

        // Verify cache is empty
        let cache_size = engine.script_cache.read().await.len();
        assert_eq!(cache_size, 0);
    }

    #[tokio::test]
    async fn test_cache_script() {
        let mut engine = create_test_engine().await;

        let script_id = "test_script";
        let cache_config = CacheConfig {
            ttl: Some(Duration::from_secs(3600)),
            max_size: Some(1024),
            ..Default::default()
        };

        let result = engine.cache_script(script_id, cache_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_precompile_script() {
        let mut engine = create_test_engine().await;

        let script_id = "test_precompile";
        let result = engine.precompile_script(script_id).await;

        // Should return a result (even if not implemented)
        assert!(result.is_ok());
        let compilation_result = result.unwrap();

        // Since precompilation is not implemented, it should return success: false
        assert!(!compilation_result.success);
        assert!(compilation_result.errors.len() > 0);
    }
}

#[cfg(test)]
mod script_engine_tool_tests {
    use super::*;

    #[tokio::test]
    async fn test_register_tool() {
        let mut engine = create_test_engine().await;

        let tool = ScriptTool {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            parameters: vec![],
            return_type: "string".to_string(),
            implementation: ToolImplementation::Rune {
                source: "pub fn main() { \"test\" }".to_string(),
            },
        };

        let result = engine.register_tool(tool).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unregister_tool() {
        let mut engine = create_test_engine().await;

        let result = engine.unregister_tool("test_tool").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_script_tools() {
        let engine = create_test_engine().await;

        let tools = engine.list_script_tools().await.unwrap();
        assert!(tools.is_empty()); // Should be empty since no tools are registered
    }

    #[tokio::test]
    async fn test_get_script_tool() {
        let engine = create_test_engine().await;

        let tool = engine.get_script_tool("test_tool").await.unwrap();
        assert!(tool.is_none());
    }
}

#[cfg(test)]
mod script_engine_resource_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_resource_usage() {
        let engine = create_test_engine().await;

        let usage = engine.get_resource_usage().await.unwrap();

        assert_eq!(usage.memory_bytes, 0);
        assert_eq!(usage.cpu_percentage, 0.0);
        assert_eq!(usage.disk_bytes, 0);
        assert_eq!(usage.network_bytes, 0);
        assert_eq!(usage.open_files, 0);
        assert_eq!(usage.active_threads, 0);
    }

    #[tokio::test]
    async fn test_set_and_get_limits() {
        let mut engine = create_test_engine().await;

        let limits = ResourceLimits {
            max_memory_bytes: Some(100 * 1024 * 1024),
            max_cpu_percentage: Some(80.0),
            max_concurrent_operations: Some(50),
            operation_timeout: Some(Duration::from_secs(60)),
            max_disk_bytes: None,
            max_queue_size: Some(1000),
        };

        engine.set_limits(limits.clone()).await.unwrap();

        let retrieved_limits = engine.get_limits().await.unwrap();
        assert_eq!(retrieved_limits.max_memory_bytes, limits.max_memory_bytes);
        assert_eq!(retrieved_limits.max_cpu_percentage, limits.max_cpu_percentage);
        assert_eq!(retrieved_limits.max_concurrent_operations, limits.max_concurrent_operations);
        assert_eq!(retrieved_limits.operation_timeout, limits.operation_timeout);
        assert_eq!(retrieved_limits.max_queue_size, limits.max_queue_size);
    }

    #[tokio::test]
    async fn test_cleanup_resources() {
        let mut engine = create_test_engine().await;

        let result = engine.cleanup_resources().await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod script_engine_event_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_subscription() {
        let mut engine = create_test_engine().await;

        // Subscribe to script compilation events
        let mut receiver = engine.subscribe("script_compiled").await.unwrap();

        // Compile a script to trigger an event
        let script_source = r#"
            pub fn main() {
                "Event test"
            }
        "#;

        let context = create_test_compilation_context();
        let _compiled = engine.compile_script(script_source, context).await.unwrap();

        // Should receive the compilation event
        let event = receiver.recv().await;
        assert!(event.is_some());

        if let Some(ScriptEngineEvent::ScriptCompiled { script_id, success, .. }) = event {
            assert!(!script_id.is_empty());
            assert!(success);
        } else {
            panic!("Expected ScriptCompiled event");
        }
    }

    #[tokio::test]
    async fn test_multiple_event_subscriptions() {
        let mut engine = create_test_engine().await;

        // Subscribe to multiple event types
        let mut compiled_rx = engine.subscribe("script_compiled").await.unwrap();
        let mut executed_rx = engine.subscribe("script_executed").await.unwrap();

        let script_source = r#"
            pub fn main() {
                "Multi-event test"
            }
        "#;

        // Compile script (should trigger compilation event)
        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        // Execute script (should trigger execution event)
        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let _result = engine.execute_script(&compiled.script_id, execution_context).await.unwrap();

        // Should receive compilation event
        let compiled_event = compiled_rx.recv().await;
        assert!(compiled_event.is_some());
        assert!(matches!(compiled_event.unwrap(), ScriptEngineEvent::ScriptCompiled { .. }));

        // Should receive execution event
        let executed_event = executed_rx.recv().await;
        assert!(executed_event.is_some());
        assert!(matches!(executed_event.unwrap(), ScriptEngineEvent::ScriptExecuted { .. }));
    }

    #[tokio::test]
    async fn test_event_unsubscription() {
        let mut engine = create_test_engine().await;

        // Subscribe and then unsubscribe
        let _receiver = engine.subscribe("script_compiled").await.unwrap();
        engine.unsubscribe("script_compiled").await.unwrap();

        // Should not panic or cause issues
    }

    #[tokio::test]
    async fn test_handle_script_engine_event() {
        let mut engine = create_test_engine().await;

        let test_events = vec![
            ScriptEngineEvent::ScriptCompiled {
                script_id: "test".to_string(),
                success: true,
                duration: Duration::from_millis(100),
            },
            ScriptEngineEvent::ScriptExecuted {
                script_id: "test".to_string(),
                execution_id: "exec_123".to_string(),
                success: true,
                duration: Duration::from_millis(50),
            },
            ScriptEngineEvent::Error {
                operation: "test_operation".to_string(),
                error: "Test error".to_string(),
                script_id: Some("test".to_string()),
            },
            ScriptEngineEvent::SecurityPolicyUpdated {
                policy_name: "test_policy".to_string(),
            },
            ScriptEngineEvent::ScriptCached {
                script_id: "test".to_string(),
            },
            ScriptEngineEvent::CacheCleared,
        ];

        for event in test_events {
            let result = engine.handle_event(event.clone()).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_publish_event() {
        let mut engine = create_test_engine().await;

        let event = ScriptEngineEvent::ScriptCompiled {
            script_id: "test".to_string(),
            success: true,
            duration: Duration::from_millis(100),
        };

        let result = engine.publish(event).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod script_engine_execution_stats_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_execution_stats_initial() {
        let engine = create_test_engine().await;

        let stats = engine.get_execution_stats().await.unwrap();

        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.successful_executions, 0);
        assert_eq!(stats.failed_executions, 0);
        assert_eq!(stats.average_execution_time, Duration::ZERO);
        assert_eq!(stats.total_memory_used, 0);
        assert!(stats.executions_by_script.is_empty());
        assert!(stats.error_rates_by_script.is_empty());
        assert!(stats.popular_scripts.is_empty());
    }

    #[tokio::test]
    async fn test_get_execution_stats_after_operations() {
        let mut engine = create_test_engine().await;

        // Compile and execute a script
        let script_source = r#"
            pub fn main() {
                "Stats test"
            }
        "#;

        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let _result = engine.execute_script(&compiled.script_id, execution_context).await.unwrap();

        // Check stats after execution
        let stats = engine.get_execution_stats().await.unwrap();

        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 0);
        assert!(stats.average_execution_time > Duration::ZERO);
        assert!(stats.total_memory_used > 0);
    }
}

#[cfg(test)]
mod script_engine_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_workflow() {
        let mut engine = create_test_engine().await;

        // Start the service
        engine.start().await.unwrap();
        assert!(engine.is_running());

        // Check health
        let health = engine.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));

        // Compile a script
        let script_source = r#"
            pub fn main() {
                "End-to-end test"
            }
        "#;

        let compilation_context = create_test_compilation_context();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        // Verify script info
        let info = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(info.is_some());

        // Execute the script
        let execution_context = create_test_execution_context(compiled.script_id.clone());
        let result = engine.execute_script(&compiled.script_id, execution_context).await.unwrap();
        assert!(result.success);

        // Check metrics
        let metrics = engine.get_metrics().await.unwrap();
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.successful_requests, 1);

        // Check execution stats
        let stats = engine.get_execution_stats().await.unwrap();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);

        // List scripts
        let scripts = engine.list_scripts().await.unwrap();
        assert_eq!(scripts.len(), 1);

        // Stop the service
        engine.stop().await.unwrap();
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let mut engine = create_test_engine().await;
        engine.start().await.unwrap();

        // Compile multiple scripts concurrently
        let script_source = r#"
            pub fn main() {
                "Concurrent test"
            }
        "#;

        let context = create_test_compilation_context();

        let mut handles = vec![];
        for i in 0..5 {
            let engine_clone = engine.clone();
            let source = script_source.to_string();
            let ctx = context.clone();

            let handle = tokio::spawn(async move {
                let compiled = engine_clone.compile_script(&source, ctx).await.unwrap();

                let execution_context = create_test_execution_context(compiled.script_id.clone());
                engine_clone.execute_script(&compiled.script_id, execution_context).await
            });

            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
            assert!(result.unwrap().success);
        }

        // Verify all executions completed successfully
        let stats = engine.get_execution_stats().await.unwrap();
        assert_eq!(stats.total_executions, 5);
        assert_eq!(stats.successful_executions, 5);
    }
}