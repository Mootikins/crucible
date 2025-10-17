use super::{RuneTool, ToolMetadata, ToolDiscovery, DiscoveredTool};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Registry for managing Rune-based tools
///
/// The ToolRegistry scans a directory for .rn files, compiles them into RuneTool instances,
/// and provides methods for listing, retrieving, and managing tools.
/// Enhanced with AST-based discovery and schema generation.
pub struct ToolRegistry {
    pub tools: HashMap<String, RuneTool>,
    pub tool_dir: PathBuf,
    pub context: Arc<rune::Context>,
    pub discovery: ToolDiscovery,
    pub enhanced_mode: bool, // Whether to use enhanced discovery
}

/// Get access to the Rune context for external use
impl ToolRegistry {
    /// Get a reference to the Rune context
    pub fn context(&self) -> &Arc<rune::Context> {
        &self.context
    }

    /// Get the enhanced discovery system
    pub fn discovery(&self) -> &ToolDiscovery {
        &self.discovery
    }

    /// Check if enhanced mode is enabled
    pub fn is_enhanced_mode(&self) -> bool {
        self.enhanced_mode
    }
}

impl ToolRegistry {
    /// Create a new ToolRegistry with Crucible stdlib
    ///
    /// This builds a Rune context with the Crucible standard library installed,
    /// giving tools access to crucible::db, crucible::obsidian, and crucible::log.
    pub fn new_with_stdlib(
        tool_dir: PathBuf,
        db: Arc<crate::database::EmbeddingDatabase>,
        obsidian: Arc<crate::obsidian_client::ObsidianClient>,
    ) -> Result<Self> {
        // Build context with default modules + Crucible stdlib
        let mut context = rune::Context::with_default_modules()?;

        let crucible_module = super::build_crucible_module(db, obsidian)?;
        context.install(crucible_module)?;

        Self::new_with_enhanced_discovery(tool_dir, Arc::new(context), true)
    }

    /// Create a new ToolRegistry with enhanced discovery enabled
    pub fn new_with_enhanced_discovery(
        tool_dir: PathBuf,
        context: Arc<rune::Context>,
        enhanced_mode: bool
    ) -> Result<Self> {
        let discovery = ToolDiscovery::new(context.clone());
        let mut registry = Self {
            tools: HashMap::new(),
            tool_dir,
            context,
            discovery,
            enhanced_mode,
        };

        // Use synchronous scanning for backwards compatibility
        registry.sync_scan_and_load()?;

        Ok(registry)
    }
}

impl ToolRegistry {
    /// Create a new ToolRegistry and scan for tools
    ///
    /// This will immediately scan the tool directory and load all .rn files.
    /// Uses enhanced discovery by default for better schema generation.
    pub fn new(tool_dir: PathBuf, context: Arc<rune::Context>) -> Result<Self> {
        Self::new_with_enhanced_discovery(tool_dir, context, true)
    }

    /// Synchronous scan the tool directory and load all .rn files
    ///
    /// Returns the list of successfully loaded tool names.
    /// Uses enhanced discovery when enabled for better schema generation and module discovery.
    pub fn sync_scan_and_load(&mut self) -> Result<Vec<String>> {
        use std::fs;

        // Create directory if it doesn't exist
        if !self.tool_dir.exists() {
            fs::create_dir_all(&self.tool_dir)
                .with_context(|| format!("Failed to create tool directory: {:?}", self.tool_dir))?;
            tracing::info!("Created tool directory: {:?}", self.tool_dir);
            return Ok(Vec::new());
        }

        let loaded: Vec<String> = Vec::new();

        // For now, use traditional loading to avoid runtime issues in sync context
        // TODO: Implement proper async initialization support
        tracing::info!("Using traditional tool loading for backwards compatibility");
        let loaded = self.fallback_scan_and_load()?;

        tracing::info!("Loaded {} Rune tools from {:?}", loaded.len(), self.tool_dir);

        Ok(loaded)
    }

    /// Async scan the tool directory and load all .rn files
    ///
    /// Returns the list of successfully loaded tool names.
    /// Uses enhanced discovery when enabled for better schema generation and module discovery.
    pub async fn scan_and_load(&mut self) -> Result<Vec<String>> {
        use std::fs;

        // Create directory if it doesn't exist
        if !self.tool_dir.exists() {
            fs::create_dir_all(&self.tool_dir)
                .with_context(|| format!("Failed to create tool directory: {:?}", self.tool_dir))?;
            tracing::info!("Created tool directory: {:?}", self.tool_dir);
            return Ok(Vec::new());
        }

        let mut loaded = Vec::new();

        if self.enhanced_mode {
            // Use enhanced discovery with AST analysis
            tracing::info!("Using enhanced discovery to scan {:?} for tools", self.tool_dir);

            match self.discovery.discover_in_directory(&self.tool_dir).await {
                Ok(discoveries) => {
                    for discovery in discoveries {
                        tracing::info!("Discovered {} tools from {:?}", discovery.tools.len(), discovery.file_path);

                        for discovered_tool in discovery.tools {
                            match self.convert_discovered_tool(&discovered_tool, &discovery.file_path) {
                                Ok(tool_name) => {
                                    tracing::info!("Loaded enhanced tool: {}", tool_name);
                                    loaded.push(tool_name);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to convert discovered tool {}: {}", discovered_tool.name, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Enhanced discovery failed, falling back to traditional loading: {}", e);
                    // Fallback to traditional loading
                    loaded = self.fallback_scan_and_load()?;
                }
            }
        } else {
            // Use traditional loading
            loaded = self.fallback_scan_and_load()?;
        }

        tracing::info!("Loaded {} Rune tools from {:?}", loaded.len(), self.tool_dir);

        Ok(loaded)
    }

    /// Fallback traditional scanning method
    fn fallback_scan_and_load(&mut self) -> Result<Vec<String>> {
        use std::fs;

        let mut loaded = Vec::new();

        for entry in fs::read_dir(&self.tool_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("rn") {
                match self.load_tool(&path) {
                    Ok(name) => {
                        tracing::info!("Loaded traditional tool: {}", name);
                        loaded.push(name);
                    }
                    Err(e) => {
                        tracing::error!("Failed to load tool {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(loaded)
    }

    /// Convert a discovered tool to a RuneTool
    fn convert_discovered_tool(&mut self, discovered_tool: &DiscoveredTool, file_path: &std::path::Path) -> Result<String> {
        use std::fs;

        // Read the source code for the tool
        let source_code = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read tool file: {:?}", file_path))?;

        // Create a RuneTool from the discovered tool information
        let rune_tool = RuneTool::from_discovered(discovered_tool, &source_code, &self.context)
            .with_context(|| format!("Failed to create RuneTool from discovered tool: {}", discovered_tool.name))?;

        let name = rune_tool.name.clone();

        // Validate that the tool name is valid
        if !is_valid_tool_name(&name) {
            anyhow::bail!("Invalid tool name '{}': must be alphanumeric with underscores only", name);
        }

        self.tools.insert(name.clone(), rune_tool);

        Ok(name)
    }

    /// Load a single tool from a file
    ///
    /// This reads the file, compiles it, and adds it to the registry.
    /// If a tool with the same name already exists, it will be replaced.
    pub fn load_tool(&mut self, path: &Path) -> Result<String> {
        use std::fs;

        let source_code = fs::read_to_string(path)
            .with_context(|| format!("Failed to read tool file: {:?}", path))?;

        let tool = RuneTool::from_source(&source_code, &self.context)
            .with_context(|| format!("Failed to compile tool from {:?}", path))?;

        let name = tool.name.clone();

        // Validate that the tool name is valid (no spaces, special chars, etc.)
        if !is_valid_tool_name(&name) {
            anyhow::bail!("Invalid tool name '{}': must be alphanumeric with underscores only", name);
        }

        self.tools.insert(name.clone(), tool);

        Ok(name)
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&RuneTool> {
        self.tools.get(name)
    }

    /// Get a mutable reference to a tool by name
    pub fn get_tool_mut(&mut self, name: &str) -> Option<&mut RuneTool> {
        self.tools.get_mut(name)
    }

    /// List all tool metadata
    pub fn list_tools(&self) -> Vec<ToolMetadata> {
        self.tools.values().map(|t| t.metadata()).collect()
    }

    /// Get the number of loaded tools
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Reload a specific tool by name
    ///
    /// This finds the .rn file for the tool and recompiles it.
    pub fn reload_tool(&mut self, name: &str) -> Result<()> {
        // Find the file for this tool
        let tool_path = self.tool_dir.join(format!("{}.rn", name));

        if !tool_path.exists() {
            anyhow::bail!("Tool file not found: {:?}", tool_path);
        }

        self.load_tool(&tool_path)?;

        tracing::info!("Reloaded tool: {}", name);

        Ok(())
    }

    /// Remove a tool from the registry
    pub fn remove_tool(&mut self, name: &str) -> Option<RuneTool> {
        let removed = self.tools.remove(name);
        if removed.is_some() {
            tracing::info!("Removed tool: {}", name);
        }
        removed
    }

    /// Check if a tool exists
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all tool names
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

/// Validate that a tool name follows the required format
///
/// Tool names must:
/// - Be lowercase
/// - Start with a letter
/// - Contain only letters, numbers, and underscores
fn is_valid_tool_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First char must be lowercase letter
    if let Some(first) = chars.next() {
        if !first.is_ascii_lowercase() {
            return false;
        }
    } else {
        return false;
    }

    // Rest must be lowercase letters, digits, or underscores
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_tool_names() {
        assert!(is_valid_tool_name("search"));
        assert!(is_valid_tool_name("search_notes"));
        assert!(is_valid_tool_name("search2"));
        assert!(is_valid_tool_name("s"));
    }

    #[test]
    fn test_invalid_tool_names() {
        assert!(!is_valid_tool_name(""));
        assert!(!is_valid_tool_name("Search"));
        assert!(!is_valid_tool_name("SEARCH"));
        assert!(!is_valid_tool_name("search-notes"));
        assert!(!is_valid_tool_name("search.notes"));
        assert!(!is_valid_tool_name("2search"));
        assert!(!is_valid_tool_name("_search"));
    }

    #[test]
    fn test_registry_creation() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        let context = rune::Context::with_default_modules().unwrap();
        let registry = ToolRegistry::new(tool_dir.clone(), Arc::new(context)).unwrap();

        assert_eq!(registry.tool_count(), 0);
        assert!(tool_dir.exists());
    }

    #[tokio::test]
    async fn test_tool_loading() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        // Create a test tool file
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ name: #{ type: "string" } }
                }
            }

            pub async fn call(args) {
                #{ success: true }
            }
        "#;

        let tool_path = tool_dir.join("test_tool.rn");
        fs::create_dir_all(&tool_dir).unwrap();
        fs::write(&tool_path, tool_source).unwrap();

        let context = rune::Context::with_default_modules().unwrap();
        let registry = ToolRegistry::new(tool_dir, Arc::new(context)).unwrap();

        assert_eq!(registry.tool_count(), 1);
        assert!(registry.has_tool("test_tool"));

        let tool = registry.get_tool("test_tool").unwrap();
        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
    }
}
