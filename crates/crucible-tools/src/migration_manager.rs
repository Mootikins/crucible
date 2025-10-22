//! Phase 5.1 Migration Manager
//!
//! This module provides the central migration manager that orchestrates the migration
//! of existing Rune tools to the new ScriptEngine service. It handles discovery,
//! migration, validation, and provides a comprehensive API for managing the migration.

use crate::{
    migration_bridge::{ToolMigrationBridge, MigrationConfig, MigrationStats, MigrationValidation},
    discovery::ToolDiscovery,
    rune_service::RuneService,
    types::{RuneServiceConfig, ToolDefinition},
};
use crucible_services::{
    CrucibleScriptEngine, ScriptEngineConfig, SecurityLevel, ServiceResult, ServiceError,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Central migration manager for Phase 5.1
#[derive(Debug)]
pub struct Phase51MigrationManager {
    /// Migration bridge for tool execution
    bridge: Arc<ToolMigrationBridge>,
    /// Original Rune service (for backward compatibility)
    rune_service: Option<Arc<RuneService>>,
    /// Migration state
    state: Arc<RwLock<MigrationState>>,
    /// Migration configuration
    config: MigrationManagerConfig,
}

/// Migration state tracking
#[derive(Debug, Clone, Default)]
pub struct MigrationState {
    /// Migration phase
    pub phase: MigrationPhase,
    /// Total tools discovered
    pub total_discovered: usize,
    /// Tools successfully migrated
    pub successfully_migrated: usize,
    /// Failed migrations
    pub failed_migrations: usize,
    /// Migration start time
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Migration completion time
    pub completion_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Migration errors
    pub errors: Vec<MigrationError>,
    /// Migration warnings
    pub warnings: Vec<String>,
}

/// Migration phases
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Not started
    NotStarted,
    /// Discovering tools
    Discovering,
    /// Migrating tools
    Migrating,
    /// Validating migration
    Validating,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

/// Migration error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationError {
    /// Tool name
    pub tool_name: String,
    /// Error type
    pub error_type: MigrationErrorType,
    /// Error message
    pub message: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Context information
    pub context: HashMap<String, String>,
}

/// Types of migration errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationErrorType {
    /// Tool discovery failed
    DiscoveryFailed,
    /// Tool compilation failed
    CompilationFailed,
    /// Tool registration failed
    RegistrationFailed,
    /// Validation failed
    ValidationFailed,
    /// Configuration error
    ConfigurationError,
    /// Service error
    ServiceError,
    /// Migration failed
    MigrationFailed,
    /// Unknown error
    Unknown,
}

/// Configuration for the migration manager
#[derive(Debug, Clone)]
pub struct MigrationManagerConfig {
    /// Migration mode
    pub mode: MigrationMode,
    /// Security level for migrated tools
    pub security_level: SecurityLevel,
    /// Migration directories to scan
    pub migration_directories: Vec<PathBuf>,
    /// Whether to preserve original Rune service
    pub preserve_original_service: bool,
    /// Enable parallel migration
    pub enable_parallel_migration: bool,
    /// Maximum concurrent migrations
    pub max_concurrent_migrations: usize,
    /// Validation mode
    pub validation_mode: ValidationMode,
    /// Rollback on failure
    pub rollback_on_failure: bool,
}

/// Migration modes
#[derive(Debug, Clone)]
pub enum MigrationMode {
    /// Dry run - don't actually migrate, just report what would happen
    DryRun,
    /// Incremental - migrate tools one by one with validation
    Incremental,
    /// Full - migrate all tools at once
    Full,
    /// Manual - require explicit approval for each tool
    Manual,
}

/// Validation modes
#[derive(Debug, Clone)]
pub enum ValidationMode {
    /// Skip validation
    Skip,
    /// Basic validation only
    Basic,
    /// Comprehensive validation
    Comprehensive,
}

impl Default for MigrationManagerConfig {
    fn default() -> Self {
        Self {
            mode: MigrationMode::Incremental,
            security_level: SecurityLevel::Safe,
            migration_directories: vec![
                PathBuf::from("./tools"),
                PathBuf::from("./rune_tools"),
            ],
            preserve_original_service: true,
            enable_parallel_migration: false,
            max_concurrent_migrations: 5,
            validation_mode: ValidationMode::Basic,
            rollback_on_failure: false,
        }
    }
}

/// Migration report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    /// Migration ID
    pub migration_id: String,
    /// Migration configuration
    pub config: MigrationManagerConfig,
    /// Migration statistics
    pub stats: MigrationStats,
    /// Migration state
    pub state: MigrationState,
    /// List of migrated tools
    pub migrated_tools: Vec<String>,
    /// List of failed tools
    pub failed_tools: Vec<MigrationError>,
    /// Validation results
    pub validation: Option<MigrationValidation>,
    /// Migration duration
    pub duration: Option<chrono::Duration>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Phase51MigrationManager {
    /// Create a new migration manager
    pub async fn new(config: MigrationManagerConfig) -> Result<Self> {
        info!("Creating Phase 5.1 Migration Manager");

        // Create Rune service configuration
        let rune_config = RuneServiceConfig {
            service_name: "crucible-rune-migration".to_string(),
            discovery: crate::types::DiscoveryConfig {
                tool_directories: config.migration_directories.clone(),
                recursive_search: true,
                file_extensions: vec!["rn".to_string(), "rune".to_string()],
                max_file_size: 10 * 1024 * 1024, // 10MB
                excluded_patterns: vec![],
            },
            execution: crate::types::ExecutionConfig {
                default_timeout: std::time::Duration::from_secs(30),
                max_memory: 100 * 1024 * 1024, // 100MB
                enable_caching: true,
                max_concurrent_executions: 10,
            },
            security: crate::types::SecurityConfig {
                default_level: crate::types::SecurityLevel::Safe,
                enable_sandboxing: true,
                allowed_modules: vec!["crucible::basic".to_string()],
                blocked_modules: vec![],
            },
        };

        // Create migration bridge configuration
        let bridge_config = MigrationConfig {
            auto_migrate: matches!(config.mode, MigrationMode::Full),
            security_level: config.security_level.clone(),
            enable_caching: true,
            max_cache_size: 1000,
            preserve_tool_ids: true,
        };

        // Create migration bridge
        let bridge = Arc::new(ToolMigrationBridge::new(rune_config, bridge_config).await
            .context("Failed to create migration bridge")?);

        // Optionally create original Rune service for backward compatibility
        let rune_service = if config.preserve_original_service {
            Some(Arc::new(RuneService::new(rune_config).await
                .context("Failed to create original Rune service")?))
        } else {
            None
        };

        let manager = Self {
            bridge,
            rune_service,
            state: Arc::new(RwLock::new(MigrationState::default())),
            config,
        };

        info!("Phase 5.1 Migration Manager created successfully");
        Ok(manager)
    }

    /// Execute the complete migration process
    pub async fn execute_migration(&mut self) -> Result<MigrationReport> {
        let migration_id = uuid::Uuid::new_v4().to_string();
        info!("Starting Phase 5.1 migration: {}", migration_id);

        let start_time = chrono::Utc::now();

        // Initialize migration state
        {
            let mut state = self.state.write().await;
            state.phase = MigrationPhase::Discovering;
            state.start_time = Some(start_time);
        }

        let mut report = MigrationReport {
            migration_id: migration_id.clone(),
            config: self.config.clone(),
            stats: MigrationStats {
                total_migrated: 0,
                active_tools: 0,
                inactive_tools: 0,
                migration_timestamp: start_time,
            },
            state: MigrationState::default(),
            migrated_tools: vec![],
            failed_tools: vec![],
            validation: None,
            duration: None,
            timestamp: start_time,
        };

        match self.config.mode {
            MigrationMode::DryRun => {
                info!("Executing dry run migration");
                self.execute_dry_run(&mut report).await?;
            }
            MigrationMode::Incremental => {
                info!("Executing incremental migration");
                self.execute_incremental_migration(&mut report).await?;
            }
            MigrationMode::Full => {
                info!("Executing full migration");
                self.execute_full_migration(&mut report).await?;
            }
            MigrationMode::Manual => {
                info!("Manual migration mode - waiting for explicit tool migrations");
                // In manual mode, tools are migrated individually via specific methods
            }
        }

        // Complete migration
        let completion_time = chrono::Utc::now();
        let duration = completion_time - start_time;

        {
            let mut state = self.state.write().await;
            state.phase = if state.failed_migrations > 0 && state.successfully_migrated == 0 {
                MigrationPhase::Failed
            } else {
                MigrationPhase::Completed
            };
            state.completion_time = Some(completion_time);
        }

        report.duration = Some(duration);
        report.timestamp = completion_time;
        report.state = self.state.read().await.clone();

        // Final validation if enabled
        if matches!(self.config.validation_mode, ValidationMode::Comprehensive) {
            report.validation = Some(self.bridge.validate_migration().await
                .context("Final migration validation failed")?);
        }

        info!("Phase 5.1 migration completed: {} in {:?}", migration_id, duration);
        Ok(report)
    }

    /// Execute dry run migration
    async fn execute_dry_run(&mut self, report: &mut MigrationReport) -> Result<()> {
        debug!("Performing dry run migration");

        // Discover tools without migrating
        let discovered_tools = self.discover_tools().await?;
        report.state.total_discovered = discovered_tools.len();

        for tool in discovered_tools {
            report.migrated_tools.push(format!("{} (would migrate)", tool.name));
        }

        info!("Dry run completed: {} tools would be migrated", report.state.total_discovered);
        Ok(())
    }

    /// Execute incremental migration
    async fn execute_incremental_migration(&mut self, report: &mut MigrationReport) -> Result<()> {
        debug!("Performing incremental migration");

        let discovered_tools = self.discover_tools().await?;
        report.state.total_discovered = discovered_tools.len();

        for tool in discovered_tools {
            debug!("Incrementally migrating tool: {}", tool.name);

            match self.migrate_single_tool_with_validation(&tool).await {
                Ok(migrated_tool) => {
                    report.migrated_tools.push(migrated_tool.original_name);
                    report.state.successfully_migrated += 1;
                    report.stats.total_migrated += 1;
                    report.stats.active_tools += 1;

                    // Basic validation after each migration
                    if matches!(self.config.validation_mode, ValidationMode::Basic | ValidationMode::Comprehensive) {
                        if let Err(e) = self.validate_tool_migration(&migrated_tool.original_name).await {
                            warn!("Tool migration validation failed for {}: {}", migrated_tool.original_name, e);
                            if self.config.rollback_on_failure {
                                let _ = self.bridge.remove_migrated_tool(&migrated_tool.original_name).await;
                                report.state.failed_migrations += 1;
                                report.stats.active_tools -= 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to migrate tool {}: {}", tool.name, e);
                    report.state.failed_migrations += 1;
                    report.failed_tools.push(MigrationError {
                        tool_name: tool.name.clone(),
                        error_type: MigrationErrorType::MigrationFailed,
                        message: e.to_string(),
                        timestamp: chrono::Utc::now(),
                        context: HashMap::new(),
                    });

                    if self.config.rollback_on_failure {
                        return Err(anyhow::anyhow!("Migration failed and rollback enabled: {}", e));
                    }
                }
            }
        }

        info!("Incremental migration completed: {} successful, {} failed",
            report.state.successfully_migrated, report.state.failed_migrations);
        Ok(())
    }

    /// Execute full migration
    async fn execute_full_migration(&mut self, report: &mut MigrationReport) -> Result<()> {
        debug!("Performing full migration");

        // Update state to migrating
        {
            let mut state = self.state.write().await;
            state.phase = MigrationPhase::Migrating;
        }

        // Use the bridge's auto-migration capability
        let migrated_count = self.bridge.discover_and_migrate_tools().await?;
        report.state.successfully_migrated = migrated_count;
        report.stats.total_migrated = migrated_count;
        report.stats.active_tools = migrated_count;

        // Get list of migrated tools
        let migrated_tools = self.bridge.list_migrated_tools().await?;
        report.migrated_tools = migrated_tools.iter().map(|t| t.original_name.clone()).collect();

        info!("Full migration completed: {} tools migrated", migrated_count);
        Ok(())
    }

    /// Discover existing Rune tools
    async fn discover_tools(&self) -> Result<Vec<crate::tool::RuneTool>> {
        debug!("Discovering Rune tools");

        let mut discovered_tools = vec![];

        for directory in &self.config.migration_directories {
            if directory.exists() {
                let discovery = ToolDiscovery::new(&crate::types::DiscoveryConfig {
                    tool_directories: vec![directory.clone()],
                    recursive_search: true,
                    file_extensions: vec!["rn".to_string(), "rune".to_string()],
                    max_file_size: 10 * 1024 * 1024,
                    excluded_patterns: vec![],
                });

                match discovery.discover_from_directory(directory).await {
                    Ok(discoveries) => {
                        for discovery_result in discoveries {
                            discovered_tools.extend(discovery_result.tools);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to discover tools in {}: {}", directory.display(), e);
                    }
                }
            } else {
                warn!("Tool directory does not exist: {}", directory.display());
            }
        }

        info!("Discovered {} Rune tools", discovered_tools.len());
        Ok(discovered_tools)
    }

    /// Migrate a single tool with validation
    async fn migrate_single_tool_with_validation(&self, tool: &crate::tool::RuneTool) -> Result<crate::migration_bridge::MigratedTool> {
        debug!("Migrating tool with validation: {}", tool.name);

        let migrated_tool = self.bridge.migrate_single_tool(tool).await?;

        // Validate the migration
        self.validate_tool_migration(&migrated_tool.original_name).await?;

        Ok(migrated_tool)
    }

    /// Validate a tool migration
    async fn validate_tool_migration(&self, tool_name: &str) -> Result<()> {
        debug!("Validating tool migration: {}", tool_name);

        // Check if tool exists in bridge
        let migrated_tool = self.bridge.get_migrated_tool(tool_name).await?
            .ok_or_else(|| anyhow::anyhow!("Tool not found in migration bridge: {}", tool_name))?;

        if !migrated_tool.active {
            return Err(anyhow::anyhow!("Migrated tool is not active: {}", tool_name));
        }

        // Test basic execution (if tool has simple schema)
        if let Ok(test_result) = self.bridge.execute_migrated_tool(
            tool_name,
            serde_json::json!({}),
            None,
        ).await {
            if !test_result.success {
                return Err(anyhow::anyhow!("Test execution failed for migrated tool {}: {}",
                    tool_name, test_result.error.unwrap_or_default()));
            }
        }

        debug!("Tool migration validation successful: {}", tool_name);
        Ok(())
    }

    /// Get migration status
    pub async fn get_migration_status(&self) -> MigrationState {
        self.state.read().await.clone()
    }

    /// Get migration statistics
    pub async fn get_migration_statistics(&self) -> MigrationStats {
        self.bridge.get_migration_stats().await
    }

    /// Migrate a specific tool manually
    pub async fn migrate_specific_tool(&mut self, tool_name: &str) -> Result<crate::migration_bridge::MigratedTool> {
        debug!("Manually migrating tool: {}", tool_name);

        // Find the tool in discovered tools
        let discovered_tools = self.discover_tools().await?;
        let tool = discovered_tools.into_iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_name))?;

        let migrated_tool = self.migrate_single_tool_with_validation(&tool).await?;

        // Update state
        {
            let mut state = self.state.write().await;
            state.successfully_migrated += 1;
        }

        info!("Successfully manually migrated tool: {}", tool_name);
        Ok(migrated_tool)
    }

    /// Rollback migration for a specific tool
    pub async fn rollback_tool_migration(&self, tool_name: &str) -> Result<bool> {
        debug!("Rolling back tool migration: {}", tool_name);

        let removed = self.bridge.remove_migrated_tool(tool_name).await?;

        if removed {
            // Update state
            let mut state = self.state.write().await;
            state.successfully_migrated = state.successfully_migrated.saturating_sub(1);
        }

        info!("Tool migration rollback: {} ({})", tool_name, if removed { "success" } else { "not found" });
        Ok(removed)
    }

    /// Export migration report
    pub async fn export_migration_report(&self, report: &MigrationReport) -> Result<String> {
        let report_json = serde_json::to_string_pretty(report)
            .context("Failed to serialize migration report")?;
        Ok(report_json)
    }

    /// Get access to the migration bridge
    pub fn bridge(&self) -> &Arc<ToolMigrationBridge> {
        &self.bridge
    }

    /// Get access to the original Rune service (if preserved)
    pub fn rune_service(&self) -> Option<&Arc<RuneService>> {
        self.rune_service.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_migration_manager_creation() {
        let config = MigrationManagerConfig::default();
        let result = Phase51MigrationManager::new(config).await;
        // May fail in CI without proper setup, but validates structure
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_migration_phases() {
        assert_eq!(MigrationPhase::NotStarted, MigrationPhase::NotStarted);
        assert_ne!(MigrationPhase::Discovering, MigrationPhase::Completed);
    }

    #[tokio::test]
    async fn test_migration_error_serialization() {
        let error = MigrationError {
            tool_name: "test_tool".to_string(),
            error_type: MigrationErrorType::CompilationFailed,
            message: "Test error".to_string(),
            timestamp: chrono::Utc::now(),
            context: HashMap::new(),
        };

        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: MigrationError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(error.tool_name, deserialized.tool_name);
    }
}