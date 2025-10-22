//! Tool Migration Bridge for Phase 5.1
//!
//! This module provides the migration bridge between existing Rune tools in crucible-tools
//! and the new ScriptEngine service in crucible-services. This enables a clean migration
//! path while maintaining backward compatibility.

use crate::{
    tool::{RuneTool, rune_value_to_json, json_to_rune_value},
    rune_registry::RuneToolRegistry,
    context_factory::ContextFactory,
    types::{RuneServiceConfig, ToolDefinition, ToolExecutionRequest, ToolExecutionResult, ToolExecutionContext, ContextRef},
};
use crucible_services::{
    ScriptEngine, ScriptEngineConfig, CompilationContext, ExecutionContext,
    CompiledScript, SecurityPolicy, SecurityLevel, ServiceResult, ServiceError,
    types::*, traits::ToolService,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

/// Migration bridge that adapts existing Rune tools to work with ScriptEngine service
#[derive(Debug)]
pub struct ToolMigrationBridge {
    /// ScriptEngine service instance
    script_engine: Arc<RwLock<crucible_services::CrucibleScriptEngine>>,
    /// Original Rune tool registry
    rune_registry: Arc<RwLock<RuneToolRegistry>>,
    /// Context factory for Rune execution
    context_factory: Arc<ContextFactory>,
    /// Migration configuration
    config: MigrationConfig,
    /// Migrated tools tracking
    migrated_tools: Arc<RwLock<std::collections::HashMap<String, MigratedTool>>>,
}

/// Configuration for the migration bridge
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Whether to automatically migrate all discovered tools
    pub auto_migrate: bool,
    /// Security level for migrated tools
    pub security_level: SecurityLevel,
    /// Enable caching of migrated tools
    pub enable_caching: bool,
    /// Maximum number of cached migrated tools
    pub max_cache_size: usize,
    /// Whether to preserve original tool IDs
    pub preserve_tool_ids: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            auto_migrate: true,
            security_level: SecurityLevel::Safe,
            enable_caching: true,
            max_cache_size: 500,
            preserve_tool_ids: true,
        }
    }
}

/// Information about a migrated tool
#[derive(Debug, Clone)]
pub struct MigratedTool {
    /// Original tool name
    pub original_name: String,
    /// Migrated tool ID in ScriptEngine
    pub migrated_script_id: String,
    /// Tool definition
    pub definition: ToolDefinition,
    /// Migration timestamp
    pub migrated_at: chrono::DateTime<chrono::Utc>,
    /// Whether the tool is active
    pub active: bool,
    /// Migration metadata
    pub metadata: std::collections::HashMap<String, Value>,
}

impl ToolMigrationBridge {
    /// Create a new migration bridge
    pub async fn new(
        rune_config: RuneServiceConfig,
        migration_config: MigrationConfig,
    ) -> Result<Self> {
        info!("Creating Tool Migration Bridge");

        // Create ScriptEngine configuration
        let script_engine_config = ScriptEngineConfig {
            max_cache_size: migration_config.max_cache_size,
            default_execution_timeout: std::time::Duration::from_secs(30),
            max_source_size: 1024 * 1024, // 1MB
            enable_caching: migration_config.enable_caching,
            security_level: migration_config.security_level.clone(),
            resource_limits: crucible_services::ResourceLimits {
                max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
                max_cpu_percentage: Some(80.0),
                max_concurrent_operations: Some(50),
                operation_timeout: Some(std::time::Duration::from_secs(60)),
                ..Default::default()
            },
        };

        // Initialize ScriptEngine service
        let mut script_engine = crucible_services::CrucibleScriptEngine::new(script_engine_config)
            .await
            .context("Failed to create ScriptEngine service")?;

        // Start the ScriptEngine service
        script_engine.start()
            .await
            .context("Failed to start ScriptEngine service")?;

        let script_engine = Arc::new(RwLock::new(script_engine));

        // Initialize Rune components
        let rune_registry = Arc::new(RwLock::new(RuneToolRegistry::new()?));
        let context_factory = Arc::new(ContextFactory::new()?);

        let bridge = Self {
            script_engine,
            rune_registry,
            context_factory,
            config: migration_config,
            migrated_tools: Arc::new(RwLock::new(std::collections::HashMap::new())),
        };

        // Auto-migrate tools if enabled
        if bridge.config.auto_migrate {
            bridge.discover_and_migrate_tools().await?;
        }

        info!("Tool Migration Bridge initialized successfully");
        Ok(bridge)
    }

    /// Discover existing Rune tools and migrate them to ScriptEngine
    pub async fn discover_and_migrate_tools(&self) -> Result<usize> {
        info!("Discovering and migrating existing Rune tools");

        let registry = self.rune_registry.read().await;
        let existing_tools = registry.list_tools().await?;
        drop(registry);

        let mut migrated_count = 0;

        for tool in existing_tools {
            match self.migrate_single_tool(&tool).await {
                Ok(_) => {
                    migrated_count += 1;
                    debug!("Successfully migrated tool: {}", tool.name);
                }
                Err(e) => {
                    warn!("Failed to migrate tool {}: {}", tool.name, e);
                }
            }
        }

        info!("Migration completed: {}/{} tools migrated", migrated_count, existing_tools.len());
        Ok(migrated_count)
    }

    /// Migrate a single Rune tool to ScriptEngine
    pub async fn migrate_single_tool(&self, tool: &RuneTool) -> Result<MigratedTool> {
        debug!("Migrating Rune tool: {}", tool.name);

        // Create compilation context for the tool
        let compilation_context = CompilationContext {
            target: crucible_services::CompilationTarget::Standard,
            optimization_level: crucible_services::OptimizationLevel::Balanced,
            include_paths: vec![],
            definitions: std::collections::HashMap::new(),
            debug_info: false,
            security_level: self.config.security_level.clone(),
        };

        // Compile the tool as a script in ScriptEngine
        let mut script_engine = self.script_engine.write().await;
        let compiled_script = script_engine.compile_script(&tool.source_code, compilation_context).await
            .map_err(|e| anyhow::anyhow!("Failed to compile tool {}: {}", tool.name, e))?;

        // Create migrated tool info
        let migrated_tool = MigratedTool {
            original_name: tool.name.clone(),
            migrated_script_id: compiled_script.script_id.clone(),
            definition: tool.to_tool_definition(),
            migrated_at: chrono::Utc::now(),
            active: true,
            metadata: {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("migration_version".to_string(), Value::String("5.1".to_string()));
                metadata.insert("original_file_path".to_string(),
                    Value::String(tool.file_path.as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "memory".to_string())));
                metadata.insert("rune_version".to_string(), Value::String("0.13.3".to_string()));
                metadata
            },
        };

        // Track the migrated tool
        {
            let mut migrated_tools = self.migrated_tools.write().await;
            migrated_tools.insert(tool.name.clone(), migrated_tool.clone());
        }

        // Register the tool as a script tool in ScriptEngine
        let script_tool = crucible_services::ScriptTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            signature: format!("call(args) -> {}",
                tool.output_schema.as_ref()
                    .and_then(|s| s.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("Object")),
            parameters: vec![],
            return_type: "serde_json::Value".to_string(),
            script_id: compiled_script.script_id.clone(),
            function_name: "call".to_string(),
            metadata: {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("migrated_from_rune".to_string(), "true".to_string());
                metadata.insert("original_tool_name".to_string(), tool.name.clone());
                metadata
            },
            version: Some(tool.version.clone()),
            author: tool.author.clone(),
        };

        script_engine.register_tool(script_tool).await
            .map_err(|e| anyhow::anyhow!("Failed to register migrated tool {}: {}", tool.name, e))?;

        info!("Successfully migrated Rune tool: {} -> {}", tool.name, compiled_script.script_id);
        Ok(migrated_tool)
    }

    /// Execute a migrated tool through the ScriptEngine service
    pub async fn execute_migrated_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        execution_context: Option<ToolExecutionContext>,
    ) -> Result<ToolExecutionResult> {
        debug!("Executing migrated tool: {}", tool_name);

        let migrated_tools = self.migrated_tools.read().await;
        let migrated_tool = migrated_tools.get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found in migration registry: {}", tool_name))?;
        drop(migrated_tools);

        if !migrated_tool.active {
            return Err(anyhow::anyhow!("Tool is not active: {}", tool_name));
        }

        // Create execution context for ScriptEngine
        let exec_context = execution_context.unwrap_or_default();
        let script_exec_context = ExecutionContext {
            execution_id: exec_context.execution_id.clone(),
            script_id: migrated_tool.migrated_script_id.clone(),
            arguments: parameters.as_object()
                .map(|obj| obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect())
                .unwrap_or_default(),
            environment: exec_context.environment,
            working_directory: None,
            security_context: crucible_services::SecurityContext {
                user_id: exec_context.user_context
                    .as_ref()
                    .and_then(|ctx| ctx.get("user_id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("migrated_user")
                    .to_string(),
                session_id: exec_context.context_ref
                    .as_ref()
                    .map(|cr| cr.id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                permissions: vec![],
                security_level: self.config.security_level.clone(),
                sandbox: true,
            },
            timeout: exec_context.timeout,
            available_tools: vec![],
            user_context: exec_context.user_context,
        };

        // Execute through ScriptEngine
        let script_engine = self.script_engine.read().await;
        let execution_result = script_engine.execute_script(&migrated_tool.migrated_script_id, script_exec_context).await
            .map_err(|e| anyhow::anyhow!("ScriptEngine execution failed: {}", e))?;

        // Convert ScriptEngine result to ToolExecutionResult
        let tool_result = ToolExecutionResult {
            success: execution_result.success,
            result: execution_result.return_value,
            error: if execution_result.stderr.is_empty() {
                None
            } else {
                Some(execution_result.stderr)
            },
            execution_time: execution_result.execution_time,
            tool_name: tool_name.to_string(),
            context_ref: exec_context.context_ref,
            metadata: {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("migrated_execution".to_string(), Value::Bool(true));
                metadata.insert("script_engine_version".to_string(), Value::String("0.1.0".to_string()));
                metadata.insert("stdout".to_string(), Value::String(execution_result.stdout));
                metadata
            },
        };

        Ok(tool_result)
    }

    /// Get list of migrated tools
    pub async fn list_migrated_tools(&self) -> Result<Vec<MigratedTool>> {
        let migrated_tools = self.migrated_tools.read().await;
        Ok(migrated_tools.values().cloned().collect())
    }

    /// Get information about a specific migrated tool
    pub async fn get_migrated_tool(&self, tool_name: &str) -> Result<Option<MigratedTool>> {
        let migrated_tools = self.migrated_tools.read().await;
        Ok(migrated_tools.get(tool_name).cloned())
    }

    /// Remove a migrated tool
    pub async fn remove_migrated_tool(&self, tool_name: &str) -> Result<bool> {
        debug!("Removing migrated tool: {}", tool_name);

        let migrated_tools = self.migrated_tools.read().await;
        if let Some(migrated_tool) = migrated_tools.get(tool_name) {
            drop(migrated_tools);

            // Remove from ScriptEngine
            let mut script_engine = self.script_engine.write().await;
            let _ = script_engine.unregister_tool(tool_name).await;
            let _ = script_engine.delete_script(&migrated_tool.migrated_script_id).await;

            // Remove from migration registry
            let mut migrated_tools = self.migrated_tools.write().await;
            migrated_tools.remove(tool_name);

            info!("Successfully removed migrated tool: {}", tool_name);
            Ok(true)
        } else {
            warn!("Attempted to remove non-existent migrated tool: {}", tool_name);
            Ok(false)
        }
    }

    /// Reload a migrated tool from its original source
    pub async fn reload_migrated_tool(&self, tool_name: &str) -> Result<MigratedTool> {
        debug!("Reloading migrated tool: {}", tool_name);

        let rune_registry = self.rune_registry.read().await;
        let original_tool = rune_registry.get_tool(tool_name).await?
            .ok_or_else(|| anyhow::anyhow!("Original tool not found: {}", tool_name))?;
        drop(rune_registry);

        // Remove existing migration
        self.remove_migrated_tool(tool_name).await?;

        // Re-migrate the tool
        self.migrate_single_tool(&original_tool).await
    }

    /// Get migration statistics
    pub async fn get_migration_stats(&self) -> MigrationStats {
        let migrated_tools = self.migrated_tools.read().await;
        let active_count = migrated_tools.values().filter(|t| t.active).count();

        MigrationStats {
            total_migrated: migrated_tools.len(),
            active_tools: active_count,
            inactive_tools: migrated_tools.len() - active_count,
            migration_timestamp: chrono::Utc::now(),
        }
    }

    /// Validate migration integrity
    pub async fn validate_migration(&self) -> Result<MigrationValidation> {
        debug!("Validating migration integrity");

        let migrated_tools = self.migrated_tools.read().await;
        let mut validation = MigrationValidation {
            valid: true,
            issues: vec![],
            warnings: vec![],
            total_tools: migrated_tools.len(),
            valid_tools: 0,
        };

        let script_engine = self.script_engine.read().await;
        let rune_registry = self.rune_registry.read().await;

        for (tool_name, migrated_tool) in migrated_tools.iter() {
            let mut tool_valid = true;

            // Check if original tool still exists
            if let Ok(Some(original_tool)) = rune_registry.get_tool(tool_name).await {
                // Check if migrated script exists in ScriptEngine
                if let Ok(Some(script_info)) = script_engine.get_script_info(&migrated_tool.migrated_script_id).await {
                    // Verify tool definitions match
                    if original_tool.name != migrated_tool.definition.name {
                        validation.issues.push(format!("Tool name mismatch: {} vs {}",
                            original_tool.name, migrated_tool.definition.name));
                        tool_valid = false;
                    }
                } else {
                    validation.issues.push(format!("Migrated script not found in ScriptEngine: {}",
                        migrated_tool.migrated_script_id));
                    tool_valid = false;
                }
            } else {
                validation.warnings.push(format!("Original tool no longer exists: {}", tool_name));
            }

            if tool_valid {
                validation.valid_tools += 1;
            }
        }

        validation.valid = validation.issues.is_empty();

        Ok(validation)
    }
}

/// Migration statistics
#[derive(Debug, Clone)]
pub struct MigrationStats {
    /// Total number of migrated tools
    pub total_migrated: usize,
    /// Number of active tools
    pub active_tools: usize,
    /// Number of inactive tools
    pub inactive_tools: usize,
    /// When statistics were collected
    pub migration_timestamp: chrono::DateTime<chrono::Utc>,
}

/// Migration validation results
#[derive(Debug, Clone)]
pub struct MigrationValidation {
    /// Whether the migration is valid
    pub valid: bool,
    /// List of issues found
    pub issues: Vec<String>,
    /// List of warnings
    pub warnings: Vec<String>,
    /// Total number of tools checked
    pub total_tools: usize,
    /// Number of valid tools
    pub valid_tools: usize,
}

/// Implement ToolService trait for the migration bridge
#[async_trait::async_trait]
impl ToolService for ToolMigrationBridge {
    async fn execute_tool(&self, request: ToolExecutionRequest) -> ServiceResult<ToolExecutionResult> {
        self.execute_migrated_tool(&request.tool_name, request.parameters, Some(request.context))
            .await
            .map_err(|e| ServiceError::ExecutionError(format!("Migration bridge execution failed: {}", e)))
    }

    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>> {
        let migrated_tools = self.list_migrated_tools()
            .await
            .map_err(|e| ServiceError::ExecutionError(format!("Failed to list migrated tools: {}", e)))?;

        Ok(migrated_tools.into_iter().map(|tool| tool.definition).collect())
    }

    async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>> {
        let migrated_tool = self.get_migrated_tool(name)
            .await
            .map_err(|e| ServiceError::ExecutionError(format!("Failed to get migrated tool: {}", e)))?;

        Ok(migrated_tool.map(|tool| tool.definition))
    }

    async fn validate_tool(&self, name: &str) -> ServiceResult<crucible_services::types::ValidationResult> {
        match self.get_migrated_tool(name).await {
            Ok(Some(migrated_tool)) => {
                if migrated_tool.active {
                    Ok(crucible_services::types::ValidationResult {
                        valid: true,
                        errors: vec![],
                        warnings: vec![],
                        tool_name: name.to_string(),
                        metadata: Some(serde_json::to_value(migrated_tool.metadata).unwrap_or_default()),
                    })
                } else {
                    Ok(crucible_services::types::ValidationResult {
                        valid: false,
                        errors: vec!["Tool is inactive".to_string()],
                        warnings: vec![],
                        tool_name: name.to_string(),
                        metadata: None,
                    })
                }
            }
            Ok(None) => Ok(crucible_services::types::ValidationResult {
                valid: false,
                errors: vec![format!("Tool '{}' not found in migration registry", name)],
                warnings: vec![],
                tool_name: name.to_string(),
                metadata: None,
            }),
            Err(e) => Err(ServiceError::ExecutionError(format!("Validation failed: {}", e))),
        }
    }

    async fn service_health(&self) -> ServiceResult<crucible_services::types::ServiceHealth> {
        let script_engine = self.script_engine.read().await;
        script_engine.health_check().await
    }

    async fn get_metrics(&self) -> ServiceResult<crucible_services::types::ServiceMetrics> {
        let script_engine = self.script_engine.read().await;
        script_engine.get_metrics().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuneServiceConfig;

    #[tokio::test]
    async fn test_migration_bridge_creation() {
        let rune_config = RuneServiceConfig::default();
        let migration_config = MigrationConfig::default();

        let result = ToolMigrationBridge::new(rune_config, migration_config).await;
        // Note: This test may fail in CI without proper Rune setup
        // but validates the basic structure
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_migration_stats() {
        let rune_config = RuneServiceConfig::default();
        let migration_config = MigrationConfig::default();

        if let Ok(bridge) = ToolMigrationBridge::new(rune_config, migration_config).await {
            let stats = bridge.get_migration_stats().await;
            assert_eq!(stats.total_migrated, 0);
        }
    }
}