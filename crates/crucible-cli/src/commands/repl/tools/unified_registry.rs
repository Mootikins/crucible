//! Unified Tool Registry for REPL
//!
//! This module implements a unified tool registry that combines:
//! - System tools (via SystemToolGroup)
//! - Rune tools (via the existing Rune tool system)
//! - Future MCP server tools
//!
//! It provides backward compatibility while enabling the new tool group architecture.

use super::registry::ToolRegistry;
use super::system_tool_group::SystemToolGroup;
use super::tool_group::{ToolGroupCacheConfig, ToolGroupMetrics, ToolGroupRegistry};
use super::types::ToolResult;
use crate::common::CrucibleToolManager;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Unified Tool Registry that combines multiple tool sources with performance optimization
///
/// This registry provides a bridge between the old Rune tool system
/// and the new ToolGroup-based system, ensuring backward compatibility
/// while enabling the unified tool architecture with lazy loading and caching.
#[derive(Debug)]
pub struct UnifiedToolRegistry {
    /// New tool group system (for system tools, MCP tools, etc.)
    group_registry: ToolGroupRegistry,

    /// Legacy Rune tool registry (for backward compatibility)
    rune_registry: ToolRegistry,

    /// Whether to use the unified system or fall back to legacy
    use_unified: bool,

    /// Registry initialization time
    initialization_time_ms: Option<u64>,

    /// Performance statistics
    stats: UnifiedRegistryStats,
}

/// Performance statistics for the unified registry
#[derive(Debug, Default, Clone)]
pub struct UnifiedRegistryStats {
    /// Total tool execution requests
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Total time spent in executions (milliseconds)
    pub total_execution_time_ms: u64,
    /// Cache hit rate across all groups
    pub aggregate_cache_hit_rate: f64,
    /// Number of tool discovery operations
    pub tool_discoveries: u64,
}

/// Comprehensive performance metrics for the unified registry
#[derive(Debug, Clone)]
pub struct UnifiedRegistryMetrics {
    /// Registry initialization time
    pub initialization_time_ms: Option<u64>,
    /// Registry-level metrics
    pub registry_metrics: super::tool_group::RegistryMetrics,
    /// Individual group metrics
    pub group_metrics: std::collections::HashMap<String, ToolGroupMetrics>,
    /// High-level statistics
    pub stats: UnifiedRegistryStats,
}

/// Convert CLI arguments to proper tool parameters
fn convert_args_to_parameters(tool_name: &str, args: &[String]) -> serde_json::Value {
    match tool_name {
        // System tools
        "list_files" | "read_file" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"path": args[0]})
            }
        }
        "execute_command" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"command": args.join(" ")})
            }
        }
        "get_environment" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"variable": args[0]})
            }
        }

        // Vault tools
        "search_by_properties" | "search_by_tags" | "search_by_folder" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"query": args[0]})
            }
        }
        "create_note" | "update_note" | "delete_note" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"path": args[0]})
            }
        }

        // Database and search tools
        "semantic_search" | "search_by_content" | "search_by_filename" => {
            if args.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"query": args[0]})
            }
        }

        // Tools that don't take parameters or have complex parameter structures
        _ => {
            // For tools with no args, pass empty object
            if args.is_empty() {
                serde_json::json!({})
            } else {
                // For tools with multiple args, pass as array
                serde_json::json!({"args": args})
            }
        }
    }
}

impl UnifiedToolRegistry {
    /// Create a new unified tool registry with lazy loading
    pub async fn new(tool_dir: PathBuf) -> Result<Self> {
        let registry = Self::with_cache_config(tool_dir, ToolGroupCacheConfig::default()).await?;

        // Trigger tool discovery for all groups to ensure tools are available
        let _ = registry.list_tools().await;

        Ok(registry)
    }

    /// Create a new unified tool registry with custom cache configuration
    pub async fn with_cache_config(
        tool_dir: PathBuf,
        cache_config: ToolGroupCacheConfig,
    ) -> Result<Self> {
        let start_time = Instant::now();
        info!(
            "Initializing UnifiedToolRegistry with tool_dir: {:?}",
            tool_dir
        );

        // Ensure centralized tool manager is initialized
        CrucibleToolManager::ensure_initialized_global().await?;

        // Create the new tool group registry
        let group_registry = ToolGroupRegistry::new();

        // Register SystemToolGroup for crucible-tools (lazy - no initialization yet)
        let system_group = SystemToolGroup::with_cache_config(cache_config);
        match group_registry.register_group(Box::new(system_group)).await {
            Ok(()) => {
                info!("SystemToolGroup registered successfully (lazy initialization)");
            }
            Err(e) => {
                warn!("Failed to register SystemToolGroup: {}", e);
            }
        }

        // Create the legacy Rune tool registry
        let rune_registry = ToolRegistry::new(tool_dir.clone())?;

        // Note: We don't discover Rune tools immediately anymore - this will be done lazily

        let use_unified = true; // Enable unified system by default
        let initialization_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "UnifiedToolRegistry initialized in {}ms (unified: {}, lazy loading enabled)",
            initialization_time_ms, use_unified
        );

        Ok(Self {
            group_registry,
            rune_registry,
            use_unified,
            initialization_time_ms: Some(initialization_time_ms),
            stats: UnifiedRegistryStats::default(),
        })
    }

    /// Create a unified registry that uses the centralized tool manager
    pub async fn with_centralized_manager(tool_dir: PathBuf) -> Result<Self> {
        let start_time = Instant::now();
        info!("Initializing UnifiedToolRegistry with centralized manager");

        // Ensure centralized tool manager is initialized
        CrucibleToolManager::ensure_initialized_global().await?;

        // Create the new tool group registry (simplified - just for Rune tools now)
        let group_registry = ToolGroupRegistry::new();

        // Create the legacy Rune tool registry
        let rune_registry = ToolRegistry::new(tool_dir.clone())?;

        let use_unified = true; // Enable unified system by default
        let initialization_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "UnifiedToolRegistry with centralized manager initialized in {}ms",
            initialization_time_ms
        );

        Ok(Self {
            group_registry,
            rune_registry,
            use_unified,
            initialization_time_ms: Some(initialization_time_ms),
            stats: UnifiedRegistryStats::default(),
        })
    }

    /// Create a unified registry with no caching (for testing)
    pub async fn without_cache(tool_dir: PathBuf) -> Result<Self> {
        Self::with_cache_config(tool_dir, ToolGroupCacheConfig::no_caching()).await
    }

    /// Create a unified registry with fast caching (for development)
    pub async fn with_fast_cache(tool_dir: PathBuf) -> Result<Self> {
        Self::with_cache_config(tool_dir, ToolGroupCacheConfig::fast_cache()).await
    }

    /// List all available tools from all sources (async with lazy loading)
    pub async fn list_tools(&self) -> Vec<String> {
        if self.use_unified {
            // Get tools from centralized manager first
            if let Ok(system_tools) = CrucibleToolManager::list_tools_global().await {
                let mut tools = system_tools;

                // Add Rune tools if any exist (lazy discovery)
                let rune_tools = self.list_rune_tools_lazy().await;
                tools.extend(rune_tools);

                // Remove duplicates and sort
                tools.sort();
                tools.dedup();
                tools
            } else {
                // Fallback to group registry
                let mut tools = self.group_registry.list_all_tools().await;

                // Add Rune tools if any exist (lazy discovery)
                let rune_tools = self.list_rune_tools_lazy().await;
                tools.extend(rune_tools);

                // Remove duplicates and sort
                tools.sort();
                tools.dedup();
                tools
            }
        } else {
            // Fall back to legacy system
            self.list_rune_tools_lazy().await
        }
    }

    /// List tools grouped by source (async with lazy loading)
    pub async fn list_tools_by_group(&self) -> std::collections::HashMap<String, Vec<String>> {
        if self.use_unified {
            let mut grouped = self.group_registry.list_tools_by_group().await;

            // Add Rune tools if any exist
            let rune_tools = self.list_rune_tools_lazy().await;
            if !rune_tools.is_empty() {
                grouped.insert("rune".to_string(), rune_tools);
            }

            grouped
        } else {
            // Legacy mode - just return Rune tools
            let mut grouped = std::collections::HashMap::new();
            let rune_tools = self.list_rune_tools_lazy().await;
            if !rune_tools.is_empty() {
                grouped.insert("rune".to_string(), rune_tools);
            }
            grouped
        }
    }

    /// Lazy discovery of Rune tools
    async fn list_rune_tools_lazy(&self) -> Vec<String> {
        // Only discover Rune tools if needed
        // Note: This is a simplified approach - in a real implementation,
        // you might want to handle the mutable borrowing differently
        debug!("Lazy discovery of Rune tools (simplified)");
        vec![] // Placeholder - actual implementation would need proper mutable access
    }

    /// Execute a tool using the unified system with performance tracking
    pub async fn execute_tool(&self, tool_name: &str, args: &[String]) -> Result<ToolResult> {
        let start_time = Instant::now();
        debug!("Executing tool: {} with args: {:?}", tool_name, args);

        let result = if self.use_unified {
            // Convert CLI args to proper tool parameters
            let parameters = convert_args_to_parameters(tool_name, args);

            // First try the centralized manager (system tools)
            match CrucibleToolManager::execute_tool_global(
                tool_name,
                parameters,
                Some("repl_user".to_string()),
                Some("repl_session".to_string()),
            )
            .await
            {
                Ok(crucible_result) => {
                    // Convert crucible_tools::ToolResult to REPL ToolResult
                    debug!(
                        "Tool {} executed successfully via centralized manager",
                        tool_name
                    );
                    Ok(convert_crucible_result_to_repl_result(
                        tool_name,
                        crucible_result,
                    ))
                }
                Err(e) => {
                    debug!(
                        "Centralized manager execution failed for {}: {}",
                        tool_name, e
                    );

                    // Try group registry as fallback
                    match self.group_registry.execute_tool(tool_name, args).await {
                        Ok(result) => {
                            debug!(
                                "Tool {} executed successfully via group registry (fallback)",
                                tool_name
                            );
                            Ok(result)
                        }
                        Err(e) => {
                            debug!("Group registry execution failed for {}: {}", tool_name, e);

                            // Try Rune tools as final fallback
                            match self.rune_registry.execute_tool(tool_name, args).await {
                                Ok(result) => {
                                    debug!("Tool {} executed successfully via Rune registry (final fallback)", tool_name);
                                    Ok(result)
                                }
                                Err(rune_err) => {
                                    // All failed, return combined error
                                    let combined_error = format!(
                                        "Tool '{}' not found or execution failed.\nCentralized manager: {}\nGroup registry: {}\nRune registry: {}",
                                        tool_name, e, e, rune_err
                                    );
                                    Err(anyhow::anyhow!(combined_error))
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Legacy mode - only use Rune registry
            self.rune_registry.execute_tool(tool_name, args).await
        };

        // Update performance metrics
        let execution_time = start_time.elapsed();
        match &result {
            Ok(_) => {
                debug!(
                    "Tool {} executed successfully in {}ms",
                    tool_name,
                    execution_time.as_millis()
                );
            }
            Err(e) => {
                debug!(
                    "Tool {} execution failed in {}ms: {}",
                    tool_name,
                    execution_time.as_millis(),
                    e
                );
            }
        }

        result
    }

    /// Get the group that owns a specific tool
    pub async fn get_tool_group(&self, tool_name: &str) -> Option<String> {
        if self.use_unified {
            // Check group registry first
            if let Some(group) = self.group_registry.get_tool_group(tool_name).await {
                return Some(group);
            }

            // Check Rune tools
            let rune_tools = self.list_rune_tools_lazy().await;
            if rune_tools.contains(&tool_name.to_string()) {
                return Some("rune".to_string());
            }

            None
        } else {
            // Legacy mode - everything is "rune"
            let rune_tools = self.list_rune_tools_lazy().await;
            if rune_tools.contains(&tool_name.to_string()) {
                Some("rune".to_string())
            } else {
                None
            }
        }
    }

    /// Get all registered groups
    pub async fn list_groups(&self) -> Vec<String> {
        if self.use_unified {
            let mut groups = self.group_registry.list_groups().await;

            // Add Rune group if it has tools
            let rune_tools = self.list_rune_tools_lazy().await;
            if !rune_tools.is_empty() {
                groups.push("rune".to_string());
            }

            groups
        } else {
            // Legacy mode
            let rune_tools = self.list_rune_tools_lazy().await;
            if rune_tools.is_empty() {
                Vec::new()
            } else {
                vec!["rune".to_string()]
            }
        }
    }

    /// Force refresh of all tool sources
    pub async fn refresh(&mut self) -> Result<()> {
        info!("Refreshing UnifiedToolRegistry");

        // Refresh Rune tools
        match self.rune_registry.discover_tools().await {
            Ok(discovered) => {
                info!("Refreshed {} Rune tools", discovered.len());
            }
            Err(e) => {
                warn!("Failed to refresh Rune tools: {}", e);
            }
        }

        // Note: ToolGroupRegistry doesn't have a refresh method currently
        // This could be added in the future if needed

        info!("UnifiedToolRegistry refresh completed");
        Ok(())
    }

    /// Get statistics about the registry (async version)
    pub async fn get_stats(&self) -> std::collections::HashMap<String, String> {
        let mut stats = std::collections::HashMap::new();

        stats.insert("use_unified".to_string(), self.use_unified.to_string());
        stats.insert(
            "total_groups".to_string(),
            self.group_registry.list_groups().await.len().to_string(),
        );
        stats.insert(
            "total_tools".to_string(),
            self.list_tools().await.len().to_string(),
        );

        let grouped = self.list_tools_by_group().await;
        for (group_name, tools) in grouped {
            stats.insert(format!("{}_tools", group_name), tools.len().to_string());
        }

        // Add performance metrics
        if let Some(init_time) = self.initialization_time_ms {
            stats.insert("initialization_time_ms".to_string(), init_time.to_string());
        }

        stats
    }

    /// Get detailed performance metrics
    pub async fn get_performance_metrics(&self) -> UnifiedRegistryMetrics {
        let group_metrics = self.group_registry.get_detailed_metrics().await;
        let registry_metrics = self.group_registry.get_metrics();

        UnifiedRegistryMetrics {
            initialization_time_ms: self.initialization_time_ms,
            registry_metrics,
            group_metrics,
            stats: self.stats.clone(),
        }
    }

    /// Force refresh all tool groups and caches
    pub async fn refresh_all(&mut self) -> Result<()> {
        info!("Refreshing all tool groups in UnifiedToolRegistry");

        // Refresh tool groups
        self.group_registry.refresh_all().await?;

        // Refresh Rune tools
        match self.rune_registry.discover_tools().await {
            Ok(discovered) => {
                info!("Refreshed {} Rune tools", discovered.len());
            }
            Err(e) => {
                warn!("Failed to refresh Rune tools: {}", e);
            }
        }

        info!("UnifiedToolRegistry refresh completed");
        Ok(())
    }

    /// Enable or disable unified mode
    pub fn set_unified_mode(&mut self, enabled: bool) {
        info!("Setting unified mode: {}", enabled);
        self.use_unified = enabled;
    }

    /// Check if unified mode is enabled
    pub fn is_unified_enabled(&self) -> bool {
        self.use_unified
    }

    /// Get schema information for a specific tool
    pub async fn get_tool_schema(
        &self,
        tool_name: &str,
    ) -> Result<Option<super::ToolSchema>> {
        if self.use_unified {
            // Try to get schema from the group registry
            self.group_registry
                .get_tool_schema(tool_name)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get tool schema: {}", e))
        } else {
            // Legacy mode - no schema support for Rune tools yet
            Ok(None)
        }
    }
}

/// Convert crucible_tools::ToolResult to REPL ToolResult
fn convert_crucible_result_to_repl_result(
    tool_name: &str,
    crucible_result: crucible_tools::ToolResult,
) -> ToolResult {
    if crucible_result.success {
        let output = match crucible_result.data {
            Some(data) => {
                // Pretty print the data
                serde_json::to_string_pretty(&data).unwrap_or_else(|_| format!("Data: {:?}", data))
            }
            None => format!("{} executed successfully", tool_name),
        };
        ToolResult::success(output)
    } else {
        let error_msg = crucible_result
            .error
            .unwrap_or_else(|| "Unknown error".to_string());
        ToolResult::error(error_msg)
    }
}

/// Compatibility wrapper for the old ToolRegistry interface
impl From<UnifiedToolRegistry> for ToolRegistry {
    fn from(_unified: UnifiedToolRegistry) -> Self {
        // This is a simplified conversion for compatibility
        // In practice, this conversion is not used directly in the REPL
        // since we're replacing the ToolRegistry usage with UnifiedToolRegistry
        todo!("This conversion is deprecated - use UnifiedToolRegistry directly")
    }
}
