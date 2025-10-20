//! Static tool registration
//!
//! This module provides static tool registration functionality that allows
//! tools to be registered at startup and discovered dynamically.

use crate::system_tools::{Tool, ToolManager};
use crate::types::ToolCategory;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tracing::{debug, info, warn};

/// Global tool registry for static registration
static TOOL_REGISTRY: OnceLock<Arc<ToolRegistry>> = OnceLock::new();

/// Tool registry for static registration and discovery
pub struct ToolRegistry {
    pub(super) tools: HashMap<String, Box<dyn Fn() -> Box<dyn Tool> + Send + Sync>>,
    pub(super) categories: HashMap<ToolCategory, Vec<String>>,
    pub(super) metadata: HashMap<String, ToolMetadata>,
}

/// Tool metadata for registration
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    pub name: String,
    pub category: ToolCategory,
    pub description: String,
    pub version: String,
    pub deprecated: bool,
    pub registration_order: usize,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Register a tool factory function
    pub fn register<F>(&mut self, factory: F, metadata: ToolMetadata)
    where
        F: Fn() -> Box<dyn Tool> + Send + Sync + 'static,
    {
        let name = metadata.name.clone();
        let category = metadata.category.clone();

        self.tools.insert(name.clone(), Box::new(factory));
        self.categories.entry(category.clone()).or_insert_with(Vec::new).push(name.clone());
        self.metadata.insert(name.clone(), metadata);

        debug!("Registered tool: {} ({})", name, category);
    }

    /// Get a tool instance by name
    pub fn get_tool(&self, name: &str) -> Option<Box<dyn Tool>> {
        self.tools.get(name).map(|factory| factory())
    }

    /// List all registered tool names
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// List tools by category
    pub fn list_tools_by_category(&self, category: &ToolCategory) -> Vec<&str> {
        self.categories
            .get(category)
            .map(|names| names.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get tool metadata
    pub fn get_metadata(&self, name: &str) -> Option<&ToolMetadata> {
        self.metadata.get(name)
    }

    /// Check if a tool is registered
    pub fn is_registered(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<&ToolCategory> {
        self.categories.keys().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the global tool registry
pub fn initialize_registry() -> Arc<ToolRegistry> {
    TOOL_REGISTRY.get_or_init(|| {
        let mut registry = ToolRegistry::new();

        // Register all built-in tools
        register_built_in_tools(&mut registry);

        Arc::new(registry)
    }).clone()
}

/// Get the global tool registry
pub fn get_registry() -> Option<Arc<ToolRegistry>> {
    TOOL_REGISTRY.get().cloned()
}

/// Register all built-in tools
pub fn register_built_in_tools(registry: &mut ToolRegistry) {
    info!("Registering built-in tools...");

    // Register vault tools
    register_vault_tools(registry);
    register_database_tools(registry);
    register_search_tools(registry);

    info!("Registered {} built-in tools", registry.tools.len());
}

/// Register vault tools
pub fn register_vault_tools(registry: &mut ToolRegistry) {
    let _tools = vec![
        ToolMetadata {
            name: "search_by_properties".to_string(),
            category: ToolCategory::Vault,
            description: "Search notes by frontmatter properties".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 0,
        },
        ToolMetadata {
            name: "search_by_tags".to_string(),
            category: ToolCategory::Vault,
            description: "Search notes by tags".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 1,
        },
        ToolMetadata {
            name: "search_by_folder".to_string(),
            category: ToolCategory::Vault,
            description: "Search notes in a specific folder".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 2,
        },
        ToolMetadata {
            name: "index_vault".to_string(),
            category: ToolCategory::Vault,
            description: "Index all vault files for search and retrieval".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 3,
        },
        ToolMetadata {
            name: "get_note_metadata".to_string(),
            category: ToolCategory::Vault,
            description: "Get metadata for a specific note".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 4,
        },
    ];

    for _metadata in _tools {
        // registry.register(
        //     move || Box::new(crate::vault_tools::create_tool("")),
        //     metadata,
        // );
    }
}

/// Register database tools
pub fn register_database_tools(_registry: &mut ToolRegistry) {
    let _tools = vec![
        ToolMetadata {
            name: "semantic_search".to_string(),
            category: ToolCategory::Database,
            description: "Perform semantic search using embeddings".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 0,
        },
        ToolMetadata {
            name: "search_by_content".to_string(),
            category: ToolCategory::Database,
            description: "Full-text search in note contents".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 1,
        },
        ToolMetadata {
            name: "search_by_filename".to_string(),
            category: ToolCategory::Database,
            description: "Search notes by filename pattern".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 2,
        },
        ToolMetadata {
            name: "update_note_properties".to_string(),
            category: ToolCategory::Database,
            description: "Update frontmatter properties of a note".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 3,
        },
        ToolMetadata {
            name: "index_document".to_string(),
            category: ToolCategory::Database,
            description: "Index a specific document for search".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 4,
        },
        ToolMetadata {
            name: "get_document_stats".to_string(),
            category: ToolCategory::Database,
            description: "Get document statistics from the database".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 5,
        },
        ToolMetadata {
            name: "sync_metadata".to_string(),
            category: ToolCategory::Database,
            description: "Sync metadata from external source to database".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 6,
        },
    ];

    // for metadata in tools {
    //     let name = metadata.name.clone();
    //     registry.register(
    //         move || Box::new(crate::database_tools::create_tool(&name)),
    //         metadata,
    //     );
    // }
}

/// Register search tools
pub fn register_search_tools(registry: &mut ToolRegistry) {
    let _tools = vec![
        ToolMetadata {
            name: "search_documents".to_string(),
            category: ToolCategory::Search,
            description: "Search documents using semantic similarity".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 0,
        },
        ToolMetadata {
            name: "rebuild_index".to_string(),
            category: ToolCategory::Search,
            description: "Rebuild search indexes for all documents".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 1,
        },
        ToolMetadata {
            name: "get_index_stats".to_string(),
            category: ToolCategory::Search,
            description: "Get statistics about search indexes".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 2,
        },
        ToolMetadata {
            name: "optimize_index".to_string(),
            category: ToolCategory::Search,
            description: "Optimize search indexes for better performance".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 3,
        },
        ToolMetadata {
            name: "advanced_search".to_string(),
            category: ToolCategory::Search,
            description: "Advanced search with multiple criteria and ranking".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 4,
        },
    ];

    // for metadata in tools {
    //     let name = metadata.name.clone();
    //     registry.register(
    //         move || Box::new(crate::search_tools::create_tool(&name)),
    //         metadata,
    //     );
    // }
}

/// Tool discovery utilities
pub mod discovery {
    use super::*;

    /// Discover tools by category
    pub fn discover_tools_by_category(category: &ToolCategory) -> Vec<String> {
        if let Some(registry) = get_registry() {
            registry
                .list_tools_by_category(category)
                .into_iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            warn!("Tool registry not initialized");
            Vec::new()
        }
    }

    /// Discover tools by name pattern
    pub fn discover_tools_by_pattern(pattern: &str) -> Vec<String> {
        if let Some(registry) = get_registry() {
            registry
                .list_tools()
                .into_iter()
                .filter(|name| name.contains(pattern))
                .map(|s| s.to_string())
                .collect()
        } else {
            warn!("Tool registry not initialized");
            Vec::new()
        }
    }

    /// Get tool metadata by name
    pub fn get_tool_metadata(name: &str) -> Option<ToolMetadata> {
        if let Some(registry) = get_registry() {
            registry.get_metadata(name).cloned()
        } else {
            warn!("Tool registry not initialized");
            None
        }
    }

    /// List all available tool categories
    pub fn list_categories() -> Vec<ToolCategory> {
        if let Some(registry) = get_registry() {
            registry
                .get_categories()
                .into_iter()
                .cloned()
                .collect()
        } else {
            warn!("Tool registry not initialized");
            Vec::new()
        }
    }

    /// Check if a tool is deprecated
    pub fn is_tool_deprecated(name: &str) -> bool {
        if let Some(metadata) = get_tool_metadata(name) {
            metadata.deprecated
        } else {
            false
        }
    }
}

/// Initialize tool manager with registry
pub fn create_tool_manager_from_registry() -> ToolManager {
    let manager = ToolManager::new();

    if let Some(registry) = get_registry() {
        info!("Creating tool manager from registry with {} tools", registry.tools.len());

        // Sort tools by registration order for consistent behavior
        let mut tools: Vec<_> = registry.metadata.iter().collect();
        tools.sort_by_key(|(_, metadata)| metadata.registration_order);

        for (name, _metadata) in tools {
            if let Some(tool) = registry.get_tool(name) {
                // Note: This won't work with the current structure. We need to redesign this.
                // For now, let's just log that we would register this tool.
                debug!("Would register tool: {}", name);
            } else {
                warn!("Failed to create tool instance for: {}", name);
            }
        }

        info!("Tool manager created with {} tools", manager.list_tools().len());
    } else {
        warn!("Tool registry not initialized, creating empty manager");
    }

    manager
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();

        let metadata = ToolMetadata {
            name: "test_tool".to_string(),
            category: ToolCategory::System,
            description: "Test tool".to_string(),
            version: "1.0.0".to_string(),
            deprecated: false,
            registration_order: 0,
        };

        registry.register(
            || Box::new(crate::system_tools::BaseTool::new(
                ToolDefinition {
                    name: "test_tool".to_string(),
                    description: "Test tool".to_string(),
                    category: ToolCategory::System,
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    deprecated: false,
                    version: "1.0.0".to_string(),
                },
                |_params, _context| {
                    Ok(crate::types::ToolExecutionResult {
                        success: true,
                        data: None,
                        error: None,
                        execution_time_ms: None,
                    })
                },
            )),
            metadata,
        );

        assert!(registry.is_registered("test_tool"));
        assert_eq!(registry.list_tools().len(), 1);
    }

    #[test]
    fn test_initialize_registry() {
        let registry = initialize_registry();
        assert!(!registry.list_tools().is_empty());

        // Should have tools from all categories
        assert!(registry.list_tools_by_category(&ToolCategory::Vault).len() > 0);
        assert!(registry.list_tools_by_category(&ToolCategory::Database).len() > 0);
        assert!(registry.list_tools_by_category(&ToolCategory::Search).len() > 0);
    }

    #[test]
    fn test_discovery() {
        // Initialize registry first
        initialize_registry();

        let vault_tools = discovery::discover_tools_by_category(&ToolCategory::Vault);
        assert!(!vault_tools.is_empty());

        let search_tools = discovery::discover_tools_by_pattern("search");
        assert!(!search_tools.is_empty());

        let categories = discovery::list_categories();
        assert!(categories.contains(&ToolCategory::Vault));
        assert!(categories.contains(&ToolCategory::Database));
        assert!(categories.contains(&ToolCategory::Search));
    }

    #[test]
    fn test_create_tool_manager_from_registry() {
        // Initialize registry first
        initialize_registry();

        let manager = create_tool_manager_from_registry();
        assert!(!manager.list_tools().is_empty());

        let vault_tools = manager.list_tools_by_category(&ToolCategory::Vault);
        assert!(!vault_tools.is_empty());
    }
}