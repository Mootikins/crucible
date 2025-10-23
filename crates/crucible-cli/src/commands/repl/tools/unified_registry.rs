//! Unified Tool Registry for REPL
//!
//! This module implements a unified tool registry that combines:
//! - System tools (via SystemToolGroup)
//! - Rune tools (via the existing Rune tool system)
//! - Future MCP server tools
//!
//! It provides backward compatibility while enabling the new tool group architecture.

use super::tool_group::{ToolGroupRegistry, ToolGroup, ToolGroupResult, ToolGroupError};
use super::system_tool_group::SystemToolGroup;
use super::types::ToolResult;
use super::registry::ToolRegistry;
use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn, debug};

/// Unified Tool Registry that combines multiple tool sources
///
/// This registry provides a bridge between the old Rune tool system
/// and the new ToolGroup-based system, ensuring backward compatibility
/// while enabling the unified tool architecture.
#[derive(Debug)]
pub struct UnifiedToolRegistry {
    /// New tool group system (for system tools, MCP tools, etc.)
    group_registry: ToolGroupRegistry,

    /// Legacy Rune tool registry (for backward compatibility)
    rune_registry: ToolRegistry,

    /// Whether to use the unified system or fall back to legacy
    use_unified: bool,
}

impl UnifiedToolRegistry {
    /// Create a new unified tool registry
    pub async fn new(tool_dir: PathBuf) -> Result<Self> {
        info!("Initializing UnifiedToolRegistry with tool_dir: {:?}", tool_dir);

        // Create the new tool group registry
        let mut group_registry = ToolGroupRegistry::new();

        // Register SystemToolGroup for crucible-tools
        let mut system_group = SystemToolGroup::new();

        // Try to initialize system group, but don't fail if it doesn't work
        match system_group.initialize().await {
            Ok(()) => {
                info!("SystemToolGroup initialized successfully");
                match group_registry.register_group(Box::new(system_group)).await {
                    Ok(()) => {
                        info!("SystemToolGroup registered successfully");
                    }
                    Err(e) => {
                        warn!("Failed to register SystemToolGroup: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to initialize SystemToolGroup: {}", e);
            }
        }

        // Create the legacy Rune tool registry
        let mut rune_registry = ToolRegistry::new(tool_dir.clone())?;

        // Discover Rune tools
        match rune_registry.discover_tools().await {
            Ok(discovered) => {
                info!("Discovered {} Rune tools", discovered.len());
            }
            Err(e) => {
                warn!("Failed to discover Rune tools: {}", e);
            }
        }

        let use_unified = true; // Enable unified system by default

        info!("UnifiedToolRegistry initialized (unified: {}, groups: {})",
              use_unified, group_registry.list_groups().len());

        Ok(Self {
            group_registry,
            rune_registry,
            use_unified,
        })
    }

    /// List all available tools from all sources
    pub fn list_tools(&self) -> Vec<String> {
        if self.use_unified {
            let mut tools = self.group_registry.list_all_tools();

            // Add Rune tools if any exist
            let rune_tools = self.rune_registry.list_tools();
            tools.extend(rune_tools);

            // Remove duplicates and sort
            tools.sort();
            tools.dedup();
            tools
        } else {
            // Fall back to legacy system
            self.rune_registry.list_tools()
        }
    }

    /// List tools grouped by source
    pub fn list_tools_by_group(&self) -> std::collections::HashMap<String, Vec<String>> {
        if self.use_unified {
            let mut grouped = self.group_registry.list_tools_by_group();

            // Add Rune tools if any exist
            let rune_tools = self.rune_registry.list_tools();
            if !rune_tools.is_empty() {
                grouped.insert("rune".to_string(), rune_tools);
            }

            grouped
        } else {
            // Legacy mode - just return Rune tools
            let mut grouped = std::collections::HashMap::new();
            let rune_tools = self.rune_registry.list_tools();
            if !rune_tools.is_empty() {
                grouped.insert("rune".to_string(), rune_tools);
            }
            grouped
        }
    }

    /// Execute a tool using the unified system
    pub async fn execute_tool(&self, tool_name: &str, args: &[String]) -> Result<ToolResult> {
        debug!("Executing tool: {} with args: {:?}", tool_name, args);

        if self.use_unified {
            // First try the group registry (system tools, MCP tools, etc.)
            match self.group_registry.execute_tool(tool_name, args).await {
                Ok(result) => {
                    debug!("Tool {} executed successfully via group registry", tool_name);
                    return Ok(result);
                }
                Err(e) => {
                    debug!("Group registry execution failed for {}: {}", tool_name, e);

                    // Try Rune tools as fallback
                    match self.rune_registry.execute_tool(tool_name, args).await {
                        Ok(result) => {
                            debug!("Tool {} executed successfully via Rune registry (fallback)", tool_name);
                            return Ok(result);
                        }
                        Err(rune_err) => {
                            // Both failed, return combined error
                            let combined_error = format!(
                                "Tool '{}' not found or execution failed.\nGroup registry: {}\nRune registry: {}",
                                tool_name, e, rune_err
                            );
                            return Err(anyhow::anyhow!(combined_error));
                        }
                    }
                }
            }
        } else {
            // Legacy mode - only use Rune registry
            self.rune_registry.execute_tool(tool_name, args).await
        }
    }

    /// Get the group that owns a specific tool
    pub fn get_tool_group(&self, tool_name: &str) -> Option<String> {
        if self.use_unified {
            // Check group registry first
            if let Some(group) = self.group_registry.get_tool_group(tool_name) {
                return Some(group.to_string());
            }

            // Check Rune tools
            let rune_tools = self.rune_registry.list_tools();
            if rune_tools.contains(&tool_name.to_string()) {
                return Some("rune".to_string());
            }

            None
        } else {
            // Legacy mode - everything is "rune"
            let rune_tools = self.rune_registry.list_tools();
            if rune_tools.contains(&tool_name.to_string()) {
                Some("rune".to_string())
            } else {
                None
            }
        }
    }

    /// Get all registered groups
    pub fn list_groups(&self) -> Vec<String> {
        if self.use_unified {
            let mut groups = self.group_registry.list_groups();

            // Add Rune group if it has tools
            let rune_tools = self.rune_registry.list_tools();
            if !rune_tools.is_empty() {
                groups.push("rune".to_string());
            }

            groups
        } else {
            // Legacy mode
            let rune_tools = self.rune_registry.list_tools();
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

    /// Get statistics about the registry
    pub fn get_stats(&self) -> std::collections::HashMap<String, String> {
        let mut stats = std::collections::HashMap::new();

        stats.insert("use_unified".to_string(), self.use_unified.to_string());
        stats.insert("total_groups".to_string(), self.group_registry.list_groups().len().to_string());
        stats.insert("total_tools".to_string(), self.list_tools().len().to_string());

        let grouped = self.list_tools_by_group();
        for (group_name, tools) in grouped {
            stats.insert(format!("{}_tools", group_name), tools.len().to_string());
        }

        stats
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