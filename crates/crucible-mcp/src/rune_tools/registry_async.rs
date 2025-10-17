/// Async-compatible ToolRegistry with enhanced discovery support
///
/// This module provides a way to create ToolRegistry instances that use
/// enhanced discovery without requiring async constructors.

use super::{ToolRegistry, ToolMetadata};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Async Tool Registry wrapper that properly uses enhanced discovery
pub struct AsyncToolRegistry {
    pub registry: Arc<RwLock<ToolRegistry>>,
}

impl AsyncToolRegistry {
    /// Create a new AsyncToolRegistry with enhanced discovery
    pub async fn new_with_enhanced_discovery(
        tool_dir: PathBuf,
        context: Arc<rune::Context>,
        enhanced_mode: bool
    ) -> Result<Self> {
        let discovery = super::ToolDiscovery::new(context.clone());
        let mut registry = ToolRegistry {
            tools: HashMap::new(),
            tool_dir,
            context,
            discovery,
            enhanced_mode,
        };

        // Use async discovery if enhanced mode is enabled
        if enhanced_mode {
            tracing::info!("Using enhanced discovery (async) for tool loading");
            let loaded = registry.scan_and_load().await
                .context("Enhanced discovery failed")?;
            tracing::info!("Enhanced discovery loaded {} tools", loaded.len());
        } else {
            tracing::info!("Using traditional discovery for tool loading");
            let loaded = registry.sync_scan_and_load()
                .context("Traditional discovery failed")?;
            tracing::info!("Traditional discovery loaded {} tools", loaded.len());
        }

        Ok(Self {
            registry: Arc::new(RwLock::new(registry)),
        })
    }

    /// Create a new AsyncToolRegistry (defaults to enhanced discovery)
    pub async fn new(tool_dir: PathBuf, context: Arc<rune::Context>) -> Result<Self> {
        Self::new_with_enhanced_discovery(tool_dir, context, true).await
    }

    /// Create a new AsyncToolRegistry with Crucible stdlib and enhanced discovery
    pub async fn new_with_stdlib(
        tool_dir: PathBuf,
        db: Arc<crate::database::EmbeddingDatabase>,
        obsidian: Arc<crate::obsidian_client::ObsidianClient>,
    ) -> Result<Self> {
        // Build context with default modules + Crucible stdlib
        let mut context = rune::Context::with_default_modules()?;
        let crucible_module = super::build_crucible_module(db, obsidian)?;
        context.install(crucible_module)?;

        Self::new_with_enhanced_discovery(tool_dir, Arc::new(context), true).await
    }

    /// Get the underlying registry
    pub async fn get_registry(&self) -> tokio::sync::RwLockReadGuard<'_, ToolRegistry> {
        self.registry.read().await
    }

    /// Get the underlying registry for mutable access
    pub async fn get_registry_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ToolRegistry> {
        self.registry.write().await
    }

    /// Check if a tool exists
    pub async fn has_tool(&self, name: &str) -> bool {
        self.registry.read().await.has_tool(name)
    }

    /// Get tool count
    pub async fn tool_count(&self) -> usize {
        self.registry.read().await.tool_count()
    }

    /// List all tools
    pub async fn list_tools(&self) -> Vec<ToolMetadata> {
        self.registry.read().await.list_tools()
    }

    /// Get a tool by name
    pub async fn get_tool(&self, name: &str) -> Option<crate::rune_tools::RuneTool> {
        self.registry.read().await.get_tool(name).cloned()
    }

    /// Check if enhanced mode is enabled
    pub async fn is_enhanced_mode(&self) -> bool {
        self.registry.read().await.is_enhanced_mode()
    }

    /// Get the context
    pub async fn context(&self) -> Arc<rune::Context> {
        self.registry.read().await.context().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[tokio::test]
    async fn test_async_enhanced_discovery() -> Result<()> {
        let temp_dir = tempdir()?;
        let tool_dir = temp_dir.path().to_path_buf();

        // Create a test tool with traditional structure for reliable testing
        let test_tool = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool for async discovery" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        message: #{ type: "string" }
                    },
                    required: ["message"]
                }
            }

            pub async fn call(args) {
                #{ success: true, message: args.message }
            }
        "#;

        fs::write(tool_dir.join("test_tool.rn"), test_tool)?;

        let context = Arc::new(rune::Context::with_default_modules()?);
        let async_registry = AsyncToolRegistry::new(tool_dir.clone(), context).await?;

        // Verify tools were discovered
        assert!(async_registry.tool_count().await > 0);

        // Check what tools were actually discovered
        let tools = async_registry.list_tools().await;
        println!("Discovered tools:");
        for tool in &tools {
            println!("  - '{}': {}", tool.name, tool.description);
        }

        // Check for expected tool
        let tool_names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"test_tool"), "Should have test_tool");

        // Verify enhanced mode
        assert!(async_registry.is_enhanced_mode().await);

        println!("✅ AsyncToolRegistry with enhanced discovery works!");
        println!("✅ Found {} tools", async_registry.tool_count().await);

        let tools = async_registry.list_tools().await;
        for tool in &tools {
            println!("  - {}: {}", tool.name, tool.description);
        }

        Ok(())
    }
}