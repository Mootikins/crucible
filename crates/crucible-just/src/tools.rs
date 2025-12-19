//! JustTools - MCP-compatible wrapper for justfile recipes
//!
//! Provides a high-level interface for discovering and executing just recipes
//! as MCP tools.

use crate::{execute_recipe, load_justfile, ExecutionResult, JustError, Justfile, McpTool, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// MCP-compatible wrapper for justfile recipes
///
/// JustTools discovers recipes from a justfile and exposes them as MCP tools
/// with auto-generated JSON schemas from recipe parameters.
pub struct JustTools {
    /// Directory containing the justfile
    justfile_dir: PathBuf,
    /// Cached justfile (lazily loaded)
    justfile: RwLock<Option<Justfile>>,
}

impl JustTools {
    /// Create a new JustTools instance for the given directory
    ///
    /// The justfile is not loaded until first access (lazy loading).
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            justfile_dir: dir.as_ref().to_path_buf(),
            justfile: RwLock::new(None),
        }
    }

    /// Get the justfile directory
    pub fn dir(&self) -> &Path {
        &self.justfile_dir
    }

    /// Check if a justfile exists in the directory
    pub fn has_justfile(&self) -> bool {
        self.justfile_dir.join("justfile").exists()
            || self.justfile_dir.join("Justfile").exists()
            || self.justfile_dir.join(".justfile").exists()
    }

    /// Refresh the justfile cache
    ///
    /// Call this when the justfile may have changed.
    pub async fn refresh(&self) -> Result<()> {
        let jf = load_justfile(&self.justfile_dir).await?;
        let mut cache = self
            .justfile
            .write()
            .map_err(|_| JustError::CommandError("Failed to acquire justfile lock".to_string()))?;
        *cache = Some(jf);
        Ok(())
    }

    /// Get the cached justfile, loading it if necessary
    async fn get_justfile(&self) -> Result<Justfile> {
        // Check cache first
        {
            let cache = self.justfile.read().map_err(|_| {
                JustError::CommandError("Failed to acquire justfile lock".to_string())
            })?;
            if let Some(jf) = cache.as_ref() {
                return Ok(jf.clone());
            }
        }

        // Load and cache
        self.refresh().await?;

        let cache = self
            .justfile
            .read()
            .map_err(|_| JustError::CommandError("Failed to acquire justfile lock".to_string()))?;
        cache
            .clone()
            .ok_or_else(|| JustError::CommandError("Justfile not loaded".to_string()))
    }

    /// List all available recipes as MCP tools
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        if !self.has_justfile() {
            return Ok(vec![]);
        }

        let jf = self.get_justfile().await?;
        Ok(jf.to_mcp_tools())
    }

    /// Get tool count without loading full tool list
    pub async fn tool_count(&self) -> Result<usize> {
        if !self.has_justfile() {
            return Ok(0);
        }

        let jf = self.get_justfile().await?;
        Ok(jf.public_recipes().len())
    }

    /// Execute a recipe by name with arguments
    ///
    /// # Arguments
    ///
    /// * `recipe_name` - Name of the recipe (without `just_` prefix)
    /// * `args` - JSON object with parameter values
    ///
    /// # Returns
    ///
    /// ExecutionResult containing stdout, stderr, and exit code
    pub async fn execute(&self, recipe_name: &str, args: Value) -> Result<ExecutionResult> {
        // Get justfile to validate recipe exists and get parameter info
        let jf = self.get_justfile().await?;

        // Find the recipe
        let recipe = jf.recipes.get(recipe_name).ok_or_else(|| {
            JustError::CommandError(format!("Recipe '{}' not found", recipe_name))
        })?;

        // Build argument list from JSON object
        let mut arg_list = Vec::new();

        if let Value::Object(obj) = args {
            // Match arguments to parameters in order
            for param in &recipe.parameters {
                let key = param.name.to_lowercase();
                if let Some(value) = obj.get(&key) {
                    match value {
                        Value::String(s) => arg_list.push(s.clone()),
                        Value::Number(n) => arg_list.push(n.to_string()),
                        Value::Bool(b) => arg_list.push(b.to_string()),
                        _ => arg_list.push(value.to_string()),
                    }
                } else if param.default.is_none() && param.kind == "singular" {
                    return Err(JustError::CommandError(format!(
                        "Missing required parameter: {}",
                        param.name
                    )));
                }
            }
        }

        // Execute the recipe
        execute_recipe(&self.justfile_dir, recipe_name, &arg_list).await
    }

    /// Execute a recipe with timeout
    pub async fn execute_with_timeout(
        &self,
        recipe_name: &str,
        args: Value,
        timeout_secs: u64,
    ) -> Result<ExecutionResult> {
        use crate::execute_recipe_with_timeout;

        let jf = self.get_justfile().await?;
        let recipe = jf.recipes.get(recipe_name).ok_or_else(|| {
            JustError::CommandError(format!("Recipe '{}' not found", recipe_name))
        })?;

        let mut arg_list = Vec::new();
        if let Value::Object(obj) = args {
            for param in &recipe.parameters {
                let key = param.name.to_lowercase();
                if let Some(value) = obj.get(&key) {
                    match value {
                        Value::String(s) => arg_list.push(s.clone()),
                        Value::Number(n) => arg_list.push(n.to_string()),
                        Value::Bool(b) => arg_list.push(b.to_string()),
                        _ => arg_list.push(value.to_string()),
                    }
                }
            }
        }

        execute_recipe_with_timeout(&self.justfile_dir, recipe_name, &arg_list, timeout_secs).await
    }

    /// Get recipe names (for debugging/listing)
    pub async fn recipe_names(&self) -> Result<Vec<String>> {
        if !self.has_justfile() {
            return Ok(vec![]);
        }

        let jf = self.get_justfile().await?;
        Ok(jf.public_recipes().iter().map(|r| r.name.clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn repo_root() -> PathBuf {
        env::var("CARGO_MANIFEST_DIR")
            .map(|p| {
                PathBuf::from(p)
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_path_buf()
            })
            .unwrap()
    }

    #[tokio::test]
    async fn test_just_tools_new() {
        let tools = JustTools::new("/tmp");
        assert_eq!(tools.dir(), Path::new("/tmp"));
    }

    #[tokio::test]
    async fn test_just_tools_has_justfile() {
        let root = repo_root();
        if root.join("justfile").exists() {
            let tools = JustTools::new(&root);
            assert!(tools.has_justfile());
        }
    }

    #[tokio::test]
    #[ignore = "Requires `just` binary to be installed"]
    async fn test_just_tools_list_tools() {
        let root = repo_root();
        if root.join("justfile").exists() {
            let tools = JustTools::new(&root);
            let mcp_tools = tools.list_tools().await.unwrap();
            assert!(!mcp_tools.is_empty());

            // All tool names should have just_ prefix
            for tool in &mcp_tools {
                assert!(tool.name.starts_with("just_"));
            }
        }
    }

    #[tokio::test]
    async fn test_just_tools_no_justfile() {
        let tools = JustTools::new("/nonexistent");
        assert!(!tools.has_justfile());
        let mcp_tools = tools.list_tools().await.unwrap();
        assert!(mcp_tools.is_empty());
    }
}
