//! Rune context building and management
//!
//! This module provides utilities for building and managing Rune contexts
//! with custom modules, security policies, and execution environments.

use crate::errors::{RuneError, RuneResult};
use crate::stdlib::build_crucible_module;
use rune::Context;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Configuration for Rune context
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Whether to include standard library modules
    pub include_stdlib: bool,
    /// Whether to include HTTP module
    pub include_http: bool,
    /// Whether to include file system module
    pub include_file: bool,
    /// Whether to include JSON module
    pub include_json: bool,
    /// Whether to include math module
    pub include_math: bool,
    /// Whether to include validation module
    pub include_validation: bool,
    /// Custom modules to include
    pub custom_modules: HashMap<String, rune::Module>,
    /// Security configuration
    pub security: SecurityConfig,
    /// Execution limits
    pub limits: ExecutionLimits,
}

/// Security configuration for Rune context
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Whether sandbox mode is enabled
    pub sandbox_enabled: bool,
    /// Allowed modules in sandbox mode
    pub allowed_modules: Vec<String>,
    /// Blocked modules
    pub blocked_modules: Vec<String>,
    /// Whether to limit file system access
    pub limit_file_access: bool,
    /// Allowed file paths (empty = no restrictions)
    pub allowed_paths: Vec<std::path::PathBuf>,
    /// Whether to limit network access
    pub limit_network_access: bool,
    /// Allowed domains (empty = no network access)
    pub allowed_domains: Vec<String>,
    /// Maximum recursion depth
    pub max_recursion_depth: usize,
}

/// Execution limits for Rune context
#[derive(Debug, Clone)]
pub struct ExecutionLimits {
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,
    /// Maximum number of function calls
    pub max_function_calls: usize,
    /// Maximum stack depth
    pub max_stack_depth: usize,
    /// Maximum string length
    pub max_string_length: usize,
    /// Maximum array length
    pub max_array_length: usize,
    /// Maximum object size
    pub max_object_size: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            include_stdlib: true,
            include_http: true,
            include_file: true,
            include_json: true,
            include_math: true,
            include_validation: true,
            custom_modules: HashMap::new(),
            security: SecurityConfig::default(),
            limits: ExecutionLimits::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            sandbox_enabled: false,
            allowed_modules: vec![
                "math".to_string(),
                "json".to_string(),
                "string".to_string(),
                "time".to_string(),
                "validate".to_string(),
            ],
            blocked_modules: vec![
                "fs".to_string(),
                "net".to_string(),
                "process".to_string(),
                "env".to_string(),
            ],
            limit_file_access: false,
            allowed_paths: Vec::new(),
            limit_network_access: false,
            allowed_domains: Vec::new(),
            max_recursion_depth: 100,
        }
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_execution_time_ms: 30_000, // 30 seconds
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            max_function_calls: 10_000,
            max_stack_depth: 100,
            max_string_length: 1_000_000,
            max_array_length: 10_000,
            max_object_size: 10_000,
        }
    }
}

/// Builder for creating Rune contexts
pub struct ContextBuilder {
    config: ContextConfig,
}

impl ContextBuilder {
    /// Create a new context builder
    pub fn new() -> Self {
        Self {
            config: ContextConfig::default(),
        }
    }

    /// Configure whether to include stdlib
    pub fn with_stdlib(mut self, include: bool) -> Self {
        self.config.include_stdlib = include;
        self
    }

    /// Configure whether to include HTTP module
    pub fn with_http(mut self, include: bool) -> Self {
        self.config.include_http = include;
        self
    }

    /// Configure whether to include file system module
    pub fn with_file(mut self, include: bool) -> Self {
        self.config.include_file = include;
        self
    }

    /// Configure whether to include JSON module
    pub fn with_json(mut self, include: bool) -> Self {
        self.config.include_json = include;
        self
    }

    /// Configure whether to include math module
    pub fn with_math(mut self, include: bool) -> Self {
        self.config.include_math = include;
        self
    }

    /// Configure whether to include validation module
    pub fn with_validation(mut self, include: bool) -> Self {
        self.config.include_validation = include;
        self
    }

    /// Add a custom module
    pub fn with_custom_module(mut self, name: String, module: rune::Module) -> Self {
        self.config.custom_modules.insert(name, module);
        self
    }

    /// Configure security settings
    pub fn with_security(mut self, security: SecurityConfig) -> Self {
        self.config.security = security;
        self
    }

    /// Configure execution limits
    pub fn with_limits(mut self, limits: ExecutionLimits) -> Self {
        self.config.limits = limits;
        self
    }

    /// Enable sandbox mode
    pub fn sandbox(mut self, enabled: bool) -> Self {
        self.config.security.sandbox_enabled = enabled;
        self
    }

    /// Set maximum execution time
    pub fn max_execution_time(mut self, time_ms: u64) -> Self {
        self.config.limits.max_execution_time_ms = time_ms;
        self
    }

    /// Set maximum memory usage
    pub fn max_memory(mut self, bytes: u64) -> Self {
        self.config.limits.max_memory_bytes = bytes;
        self
    }

    /// Add allowed path for file access
    pub fn allow_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.config.security.allowed_paths.push(path.into());
        self
    }

    /// Add allowed domain for network access
    pub fn allow_domain<S: Into<String>>(mut self, domain: S) -> Self {
        self.config.security.allowed_domains.push(domain.into());
        self
    }

    /// Build the context
    pub fn build(self) -> RuneResult<Arc<rune::Context>> {
        build_context_with_config(self.config)
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a context with custom configuration
pub fn build_context_with_config(config: ContextConfig) -> RuneResult<Arc<rune::Context>> {
    info!("Building Rune context with configuration");
    debug!("Include stdlib: {}", config.include_stdlib);
    debug!("Include HTTP: {}", config.include_http);
    debug!("Include file: {}", config.include_file);
    debug!("Sandbox enabled: {}", config.security.sandbox_enabled);

    let mut context = Context::with_default_modules()
        .map_err(|e| RuneError::ContextError {
            message: format!("Failed to create base context: {}", e),
            context_type: Some("base".to_string()),
        })?;

    // Install Crucible standard library if enabled
    if config.include_stdlib {
        debug!("Installing Crucible standard library");
        let crucible_module = build_crucible_module()
            .map_err(|e| RuneError::ContextError {
                message: format!("Failed to build Crucible module: {}", e),
                context_type: Some("crucible".to_string()),
            })?;

        // Filter modules based on configuration
        let filtered_module = filter_crucible_module(crucible_module, &config)?;
        context.install(&filtered_module)
            .map_err(|e| RuneError::ContextError {
                message: format!("Failed to install Crucible module: {}", e),
                context_type: Some("crucible".to_string()),
            })?;
    }

    // Install custom modules
    for (name, module) in config.custom_modules {
        debug!("Installing custom module: {}", name);
        context.install(&module)
            .map_err(|e| RuneError::ContextError {
                message: format!("Failed to install custom module '{}': {}", name, e),
                context_type: Some(name),
            })?;
    }

    // Apply security restrictions
    if config.security.sandbox_enabled {
        debug!("Applying sandbox security restrictions");
        apply_sandbox_restrictions(&mut context, &config.security)?;
    }

    let context = Arc::new(context);
    info!("Successfully built Rune context");

    Ok(context)
}

/// Filter Crucible module based on configuration
fn filter_crucible_module(mut module: rune::Module, config: &ContextConfig) -> Result<rune::Module, RuneError> {
    // This is a simplified filtering - in a real implementation,
    // you'd want more sophisticated module filtering
    if !config.include_http {
        // Remove HTTP functions
        module.remove_item(["http"]).ok_or_else(|| {
            RuneError::ContextError {
                message: "Failed to remove HTTP module".to_string(),
                context_type: Some("filter".to_string()),
            }
        })?;
    }

    if !config.include_file {
        // Remove file system functions
        module.remove_item(["file"]).ok_or_else(|| {
            RuneError::ContextError {
                message: "Failed to remove file module".to_string(),
                context_type: Some("filter".to_string()),
            }
        })?;
    }

    if !config.include_json {
        // Remove JSON functions
        module.remove_item(["json"]).ok_or_else(|| {
            RuneError::ContextError {
                message: "Failed to remove JSON module".to_string(),
                context_type: Some("filter".to_string()),
            }
        })?;
    }

    if !config.include_math {
        // Remove math functions
        module.remove_item(["math"]).ok_or_else(|| {
            RuneError::ContextError {
                message: "Failed to remove math module".to_string(),
                context_type: Some("filter".to_string()),
            }
        })?;
    }

    if !config.include_validation {
        // Remove validation functions
        module.remove_item(["validate"]).ok_or_else(|| {
            RuneError::ContextError {
                message: "Failed to remove validation module".to_string(),
                context_type: Some("filter".to_string()),
            }
        })?;
    }

    Ok(module)
}

/// Apply sandbox restrictions to context
fn apply_sandbox_restrictions(context: &mut rune::Context, security: &SecurityConfig) -> Result<(), RuneError> {
    // Block dangerous modules
    for blocked_module in &security.blocked_modules {
        debug!("Blocking module: {}", blocked_module);
        context.remove_module(blocked_module).ok_or_else(|| {
            RuneError::ContextError {
                message: format!("Failed to block module: {}", blocked_module),
                context_type: Some("sandbox".to_string()),
            }
        })?;
    }

    // Only allow specific modules in strict sandbox mode
    if !security.allowed_modules.is_empty() {
        let installed_modules: Vec<String> = context.iter_modules()
            .map(|(name, _)| name.to_string())
            .collect();

        for module_name in installed_modules {
            if !security.allowed_modules.contains(&module_name) {
                debug!("Removing unauthorized module: {}", module_name);
                context.remove_module(&module_name).ok_or_else(|| {
                    RuneError::ContextError {
                        message: format!("Failed to remove unauthorized module: {}", module_name),
                        context_type: Some("sandbox".to_string()),
                    }
                })?;
            }
        }
    }

    Ok(())
}

/// Create a safe context for tool execution
pub fn create_safe_context() -> RuneResult<Arc<rune::Context>> {
    let security = SecurityConfig {
        sandbox_enabled: true,
        allowed_modules: vec![
            "math".to_string(),
            "json".to_string(),
            "string".to_string(),
            "time".to_string(),
            "validate".to_string(),
        ],
        blocked_modules: vec![
            "fs".to_string(),
            "net".to_string(),
            "process".to_string(),
            "env".to_string(),
        ],
        limit_file_access: true,
        allowed_paths: Vec::new(),
        limit_network_access: true,
        allowed_domains: Vec::new(),
        max_recursion_depth: 50,
    };

    let limits = ExecutionLimits {
        max_execution_time_ms: 10_000, // 10 seconds
        max_memory_bytes: 50 * 1024 * 1024, // 50MB
        max_function_calls: 1_000,
        max_stack_depth: 50,
        max_string_length: 100_000,
        max_array_length: 1_000,
        max_object_size: 1_000,
    };

    let config = ContextConfig {
        include_stdlib: true,
        include_http: false,
        include_file: false,
        include_json: true,
        include_math: true,
        include_validation: true,
        custom_modules: HashMap::new(),
        security,
        limits,
    };

    build_context_with_config(config)
}

/// Create a development context with full capabilities
pub fn create_development_context() -> RuneResult<Arc<rune::Context>> {
    let config = ContextConfig {
        include_stdlib: true,
        include_http: true,
        include_file: true,
        include_json: true,
        include_math: true,
        include_validation: true,
        custom_modules: HashMap::new(),
        security: SecurityConfig {
            sandbox_enabled: false,
            allowed_modules: Vec::new(),
            blocked_modules: Vec::new(),
            limit_file_access: false,
            allowed_paths: Vec::new(),
            limit_network_access: false,
            allowed_domains: Vec::new(),
            max_recursion_depth: 1000,
        },
        limits: ExecutionLimits {
            max_execution_time_ms: 300_000, // 5 minutes
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
            max_function_calls: 100_000,
            max_stack_depth: 1000,
            max_string_length: 10_000_000,
            max_array_length: 100_000,
            max_object_size: 100_000,
        },
    };

    build_context_with_config(config)
}

/// Create a production context with balanced security and functionality
pub fn create_production_context() -> RuneResult<Arc<rune::Context>> {
    let config = ContextConfig {
        include_stdlib: true,
        include_http: false,
        include_file: false,
        include_json: true,
        include_math: true,
        include_validation: true,
        custom_modules: HashMap::new(),
        security: SecurityConfig {
            sandbox_enabled: true,
            allowed_modules: vec![
                "math".to_string(),
                "json".to_string(),
                "string".to_string(),
                "time".to_string(),
                "validate".to_string(),
            ],
            blocked_modules: vec![
                "fs".to_string(),
                "net".to_string(),
                "process".to_string(),
                "env".to_string(),
            ],
            limit_file_access: true,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
            limit_network_access: true,
            allowed_domains: Vec::new(),
            max_recursion_depth: 100,
        },
        limits: ExecutionLimits {
            max_execution_time_ms: 30_000, // 30 seconds
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            max_function_calls: 10_000,
            max_stack_depth: 100,
            max_string_length: 1_000_000,
            max_array_length: 10_000,
            max_object_size: 10_000,
        },
    };

    build_context_with_config(config)
}

/// Context manager for reusing contexts
pub struct ContextManager {
    contexts: HashMap<String, Arc<rune::Context>>,
    default_config: ContextConfig,
}

impl ContextManager {
    /// Create a new context manager
    pub fn new(default_config: ContextConfig) -> Self {
        Self {
            contexts: HashMap::new(),
            default_config,
        }
    }

    /// Get or create a context with the given name
    pub fn get_context(&mut self, name: &str) -> RuneResult<Arc<rune::Context>> {
        if let Some(context) = self.contexts.get(name) {
            return Ok(Arc::clone(context));
        }

        let context = build_context_with_config(self.default_config.clone())?;
        self.contexts.insert(name.to_string(), Arc::clone(&context));
        Ok(context)
    }

    /// Create a context with custom configuration
    pub fn create_context(&mut self, name: &str, config: ContextConfig) -> RuneResult<Arc<rune::Context>> {
        let context = build_context_with_config(config)?;
        self.contexts.insert(name.to_string(), Arc::clone(&context));
        Ok(context)
    }

    /// Remove a context from the manager
    pub fn remove_context(&mut self, name: &str) -> bool {
        self.contexts.remove(name).is_some()
    }

    /// Clear all contexts
    pub fn clear(&mut self) {
        self.contexts.clear();
    }

    /// Get the number of managed contexts
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    /// Check if the manager is empty
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder_default() {
        let builder = ContextBuilder::new();
        let config = builder.config;
        assert!(config.include_stdlib);
        assert!(config.include_http);
        assert!(config.include_file);
    }

    #[test]
    fn test_context_builder_configuration() {
        let builder = ContextBuilder::new()
            .with_http(false)
            .with_file(false)
            .sandbox(true)
            .max_execution_time(5000);

        let config = builder.config;
        assert!(!config.include_http);
        assert!(!config.include_file);
        assert!(config.security.sandbox_enabled);
        assert_eq!(config.limits.max_execution_time_ms, 5000);
    }

    #[test]
    fn test_create_safe_context() {
        let context = create_safe_context();
        assert!(context.is_ok());
    }

    #[test]
    fn test_create_development_context() {
        let context = create_development_context();
        assert!(context.is_ok());
    }

    #[test]
    fn test_create_production_context() {
        let context = create_production_context();
        assert!(context.is_ok());
    }

    #[test]
    fn test_context_manager() {
        let mut manager = ContextManager::new(ContextConfig::default());
        assert_eq!(manager.len(), 0);

        let context1 = manager.get_context("test").unwrap();
        assert_eq!(manager.len(), 1);

        let context2 = manager.get_context("test").unwrap();
        assert!(Arc::ptr_eq(&context1, &context2));

        let removed = manager.remove_context("test");
        assert!(removed);
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_security_config_defaults() {
        let config = SecurityConfig::default();
        assert!(!config.sandbox_enabled);
        assert!(!config.limit_file_access);
        assert!(!config.limit_network_access);
        assert!(!config.blocked_modules.is_empty());
    }

    #[test]
    fn test_execution_limits_defaults() {
        let limits = ExecutionLimits::default();
        assert_eq!(limits.max_execution_time_ms, 30_000);
        assert_eq!(limits.max_memory_bytes, 100 * 1024 * 1024);
        assert_eq!(limits.max_function_calls, 10_000);
    }
}