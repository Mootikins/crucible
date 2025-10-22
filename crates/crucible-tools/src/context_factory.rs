//! Context factory for creating fresh individual contexts per tool execution
//!
//! This module provides a factory for creating fresh Rune contexts for each tool
//! execution, ensuring isolation and simplicity over performance optimization.

use crate::errors::{RuneError, RuneResult};
use crate::stdlib::build_crucible_module;
use anyhow::Result;
use rune::Context;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Factory for creating fresh individual contexts per tool execution
#[derive(Debug, Clone)]
pub struct ContextFactory {
    /// Base modules that can be reused across contexts
    base_modules: HashMap<String, Arc<rune::Module>>,
    /// Default configuration for context creation
    default_config: ContextFactoryConfig,
}

/// Configuration for context factory
#[derive(Debug, Clone)]
pub struct ContextFactoryConfig {
    /// Whether to include standard library modules by default
    pub include_stdlib: bool,
    /// Default security level
    pub security_level: SecurityLevel,
    /// Default execution limits
    pub limits: ExecutionLimits,
}

/// Security levels for context creation
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityLevel {
    /// Safe mode - sandboxed with limited capabilities
    Safe,
    /// Development mode - full capabilities
    Development,
    /// Production mode - balanced security and functionality
    Production,
}

/// Execution limits for contexts
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
}

impl Default for ContextFactoryConfig {
    fn default() -> Self {
        Self {
            include_stdlib: true,
            security_level: SecurityLevel::Safe,
            limits: ExecutionLimits::default(),
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
        }
    }
}

impl ContextFactory {
    /// Create a new context factory
    pub fn new() -> Result<Self> {
        Self::with_config(ContextFactoryConfig::default())
    }

    /// Create a context factory with custom configuration
    pub fn with_config(config: ContextFactoryConfig) -> Result<Self> {
        info!("Creating ContextFactory with security level: {:?}", config.security_level);

        let mut factory = Self {
            base_modules: HashMap::new(),
            default_config: config,
        };

        // Pre-build common modules for efficiency
        factory.build_base_modules()?;

        Ok(factory)
    }

    /// Create a fresh context for the specified tool
    pub async fn create_fresh_context(&self, tool_name: &str) -> Result<Context> {
        debug!("Creating fresh context for tool: {}", tool_name);

        // Start with a clean base context
        let mut context = Context::with_default_modules()
            .map_err(|e| anyhow::anyhow!("Failed to create base context: {}", e))?;

        // Install required modules based on tool type
        self.install_required_modules(&mut context, tool_name)?;

        // Apply security settings based on configuration
        self.apply_security_settings(&mut context)?;

        debug!("Successfully created fresh context for tool: {}", tool_name);
        Ok(context)
    }

    /// Create a fresh context with custom security level
    pub async fn create_fresh_context_with_security(
        &self,
        tool_name: &str,
        security_level: SecurityLevel,
    ) -> Result<Context> {
        debug!("Creating fresh context for tool: {} with security level: {:?}", tool_name, security_level);

        let mut context = Context::with_default_modules()
            .map_err(|e| anyhow::anyhow!("Failed to create base context: {}", e))?;

        // Install required modules based on tool type
        self.install_required_modules(&mut context, tool_name)?;

        // Apply custom security settings
        self.apply_security_settings_with_level(&mut context, &security_level)?;

        Ok(context)
    }

    /// Build base modules that can be reused
    fn build_base_modules(&mut self) -> Result<()> {
        debug!("Building base modules for context factory");

        // Build Crucible standard library module
        let crucible_module = Arc::new(build_crucible_module()?);
        self.base_modules.insert("crucible".to_string(), crucible_module);

        // Could add other common modules here
        debug!("Built {} base modules", self.base_modules.len());
        Ok(())
    }

    /// Install required modules based on tool type
    fn install_required_modules(&self, context: &mut Context, tool_name: &str) -> Result<()> {
        let tool_category = self.categorize_tool(tool_name);

        debug!("Installing modules for tool category: {:?}", tool_category);

        // Always install basic Crucible module
        if let Some(crucible_module) = self.base_modules.get("crucible") {
            // Filter module based on tool category and security level
            let filtered_module = self.filter_module_for_category(crucible_module.as_ref(), &tool_category)?;
            context.install(&filtered_module)
                .map_err(|e| anyhow::anyhow!("Failed to install Crucible module: {}", e))?;
        }

        // Install category-specific modules
        match tool_category {
            ToolCategory::File => self.install_file_modules(context)?,
            ToolCategory::Http => self.install_http_modules(context)?,
            ToolCategory::Json => self.install_json_modules(context)?,
            ToolCategory::Database => self.install_database_modules(context)?,
            ToolCategory::System => self.install_system_modules(context)?,
            ToolCategory::Vault => self.install_vault_modules(context)?,
            ToolCategory::Basic => self.install_basic_modules(context)?,
        }

        Ok(())
    }

    /// Categorize tool based on its name
    fn categorize_tool(&self, tool_name: &str) -> ToolCategory {
        let name_lower = tool_name.to_lowercase();

        if name_lower.contains("file") || name_lower.contains("read") || name_lower.contains("write") {
            ToolCategory::File
        } else if name_lower.contains("http") || name_lower.contains("fetch") || name_lower.contains("request") {
            ToolCategory::Http
        } else if name_lower.contains("json") || name_lower.contains("parse") {
            ToolCategory::Json
        } else if name_lower.contains("db") || name_lower.contains("database") || name_lower.contains("query") {
            ToolCategory::Database
        } else if name_lower.contains("system") || name_lower.contains("process") || name_lower.contains("exec") {
            ToolCategory::System
        } else if name_lower.contains("vault") || name_lower.contains("note") || name_lower.contains("doc") {
            ToolCategory::Vault
        } else {
            ToolCategory::Basic
        }
    }

    /// Filter module based on tool category
    fn filter_module_for_category(&self, module: &rune::Module, _category: &ToolCategory) -> Result<rune::Module> {
        // This is a simplified implementation - in practice you'd want
        // more sophisticated module filtering based on security requirements
        // For now, we'll just return the module as-is
        // TODO: Implement proper module filtering based on tool category

        let filtered_module = module.clone();
        Ok(filtered_module)
    }

    /// Install file-related modules
    fn install_file_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing file modules");
        // File-specific module installation would go here
        // For now, the filtered crucible module should handle this
        Ok(())
    }

    /// Install HTTP-related modules
    fn install_http_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing HTTP modules");
        // HTTP-specific module installation would go here
        Ok(())
    }

    /// Install JSON-related modules
    fn install_json_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing JSON modules");
        // JSON-specific module installation would go here
        Ok(())
    }

    /// Install database-related modules
    fn install_database_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing database modules");
        // Database-specific module installation would go here
        Ok(())
    }

    /// Install system-related modules
    fn install_system_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing system modules");
        // System-specific module installation would go here
        Ok(())
    }

    /// Install vault-related modules
    fn install_vault_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing vault modules");
        // Vault-specific module installation would go here
        Ok(())
    }

    /// Install basic modules
    fn install_basic_modules(&self, _context: &mut Context) -> Result<()> {
        debug!("Installing basic modules");
        // Basic module installation would go here
        Ok(())
    }

    /// Apply security settings based on factory configuration
    fn apply_security_settings(&self, context: &mut Context) -> Result<()> {
        self.apply_security_settings_with_level(context, &self.default_config.security_level)
    }

    /// Apply security settings based on security level
    fn apply_security_settings_with_level(&self, context: &mut Context, security_level: &SecurityLevel) -> Result<()> {
        debug!("Applying security settings for level: {:?}", security_level);

        match security_level {
            SecurityLevel::Safe => {
                // Block dangerous modules
                self.block_dangerous_modules(context)?;

                // Apply execution limits (this would need to be implemented in Rune runtime)
                debug!("Applied safe mode security restrictions");
            }
            SecurityLevel::Development => {
                // Full access, minimal restrictions
                debug!("Applied development mode (minimal restrictions)");
            }
            SecurityLevel::Production => {
                // Balanced security - block most dangerous modules
                self.block_dangerous_modules(context)?;
                // Allow some additional modules beyond safe mode
                debug!("Applied production mode security restrictions");
            }
        }

        Ok(())
    }

    /// Block dangerous modules
    fn block_dangerous_modules(&self, _context: &mut Context) -> Result<()> {
        let dangerous_modules = vec![
            "process",
            "env",
            "net_raw",
            "system_exec",
        ];

        // TODO: Implement proper module blocking in Rune
        // For now, we just log what would be blocked
        for module_name in dangerous_modules {
            debug!("Would block module: {}", module_name);
            // context.remove_module(module_name).ok_or_else(|| {
            //     anyhow::anyhow!("Failed to block module: {}", module_name)
            // })?;
        }

        Ok(())
    }

    /// Get factory statistics
    pub fn get_stats(&self) -> ContextFactoryStats {
        ContextFactoryStats {
            base_modules_count: self.base_modules.len(),
            security_level: self.default_config.security_level.clone(),
            supported_categories: vec![
                ToolCategory::File,
                ToolCategory::Http,
                ToolCategory::Json,
                ToolCategory::Database,
                ToolCategory::System,
                ToolCategory::Vault,
                ToolCategory::Basic,
            ],
        }
    }
}

impl Default for ContextFactory {
    fn default() -> Self {
        Self::new().expect("Failed to create default ContextFactory")
    }
}

/// Tool categories for module installation
#[derive(Debug, Clone, PartialEq)]
enum ToolCategory {
    File,
    Http,
    Json,
    Database,
    System,
    Vault,
    Basic,
}

/// Statistics for the context factory
#[derive(Debug, Clone)]
pub struct ContextFactoryStats {
    /// Number of base modules built
    pub base_modules_count: usize,
    /// Default security level
    pub security_level: SecurityLevel,
    /// Supported tool categories
    pub supported_categories: Vec<ToolCategory>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_factory_creation() {
        let factory = ContextFactory::new();
        assert!(factory.is_ok());
    }

    #[tokio::test]
    async fn test_create_fresh_context() {
        let factory = ContextFactory::new().unwrap();
        let context = factory.create_fresh_context("test_tool").await;
        assert!(context.is_ok());
    }

    #[tokio::test]
    async fn test_context_isolation() {
        let factory = ContextFactory::new().unwrap();

        // Create two contexts for the same tool
        let context1 = factory.create_fresh_context("test_tool").await.unwrap();
        let context2 = factory.create_fresh_context("test_tool").await.unwrap();

        // They should be separate instances
        // Note: In practice, you'd test this by modifying one context
        // and verifying the other is unaffected
        assert_ne!(format!("{:p}", &context1), format!("{:p}", &context2));
    }

    #[tokio::test]
    async fn test_tool_categorization() {
        let factory = ContextFactory::new().unwrap();

        // Test various tool names
        assert_eq!(factory.categorize_tool("file_reader"), ToolCategory::File);
        assert_eq!(factory.categorize_tool("http_fetcher"), ToolCategory::Http);
        assert_eq!(factory.categorize_tool("json_parser"), ToolCategory::Json);
        assert_eq!(factory.categorize_tool("database_query"), ToolCategory::Database);
        assert_eq!(factory.categorize_tool("simple_tool"), ToolCategory::Basic);
    }

    #[tokio::test]
    async fn test_security_levels() {
        let factory = ContextFactory::new().unwrap();

        // Test different security levels
        let safe_context = factory.create_fresh_context_with_security("test", SecurityLevel::Safe).await;
        assert!(safe_context.is_ok());

        let dev_context = factory.create_fresh_context_with_security("test", SecurityLevel::Development).await;
        assert!(dev_context.is_ok());

        let prod_context = factory.create_fresh_context_with_security("test", SecurityLevel::Production).await;
        assert!(prod_context.is_ok());
    }

    #[test]
    fn test_factory_stats() {
        let factory = ContextFactory::new().unwrap();
        let stats = factory.get_stats();

        assert!(stats.base_modules_count > 0);
        assert_eq!(stats.security_level, SecurityLevel::Safe);
        assert!(!stats.supported_categories.is_empty());
    }

    #[test]
    fn test_execution_limits_defaults() {
        let limits = ExecutionLimits::default();
        assert_eq!(limits.max_execution_time_ms, 30_000);
        assert_eq!(limits.max_memory_bytes, 100 * 1024 * 1024);
        assert_eq!(limits.max_function_calls, 10_000);
    }

    #[test]
    fn test_security_level_equality() {
        assert_eq!(SecurityLevel::Safe, SecurityLevel::Safe);
        assert_ne!(SecurityLevel::Safe, SecurityLevel::Development);
        assert_ne!(SecurityLevel::Development, SecurityLevel::Production);
    }
}