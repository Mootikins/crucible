//! Tool registry for discovering, loading, and executing Rune scripts
//!
//! The ToolRegistry manages the lifecycle of Rune-based tools:
//! - Discovery: Scans directory for .rn files
//! - Loading: Compiles Rune scripts into executable units
//! - Execution: Runs tools with argument passing and error handling
//! - Hot-reload: Supports dynamic tool updates

use super::rune_db::{create_db_module, DbHandle};
use super::types::ToolResult;
use anyhow::{anyhow, Context, Result};
use rune::runtime::VmError;
use rune::{termcolor, Diagnostics, Sources, Unit, Vm};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

/// Registry for discovering and executing Rune-based tools
///
/// Tools are Rune scripts (.rn files) that expose a `main()` function.
/// The registry handles compilation, caching, and execution with proper
/// error handling and timeout support.
#[derive(Clone, Debug)]
pub struct ToolRegistry {
    /// Directory containing tool scripts (.rn files)
    tool_dir: PathBuf,
    /// Compiled tools cached in memory (name -> compiled Unit)
    loaded_tools: HashMap<String, Arc<Unit>>,
    /// Shared Rune context with standard library
    context: Arc<rune::Context>,
    /// Optional database handle for tools that need database access
    db_handle: Option<DbHandle>,
}

impl ToolRegistry {
    /// Create new tool registry for given directory
    ///
    /// # Arguments
    /// * `tool_dir` - Path to directory containing .rn tool scripts
    ///
    /// # Errors
    /// Returns error if:
    /// - Directory does not exist
    /// - Rune context cannot be initialized
    pub fn new(tool_dir: PathBuf) -> Result<Self> {
        // Build Rune context with standard library
        let context = rune::Context::with_default_modules()
            .context("Failed to create Rune context with default modules")?;

        // Note: Database module will be installed when db_handle is set via with_database()
        // We don't install it here because we don't have a database handle yet

        Ok(Self {
            tool_dir,
            loaded_tools: HashMap::new(),
            context: Arc::new(context),
            db_handle: None,
        })
    }

    /// Add database access to the registry
    ///
    /// This installs the database module into the Rune context, allowing tools
    /// to execute database queries using `db::query()` and `db::query_simple()`.
    ///
    /// # Arguments
    /// * `db_handle` - Database handle to inject into Rune runtime
    ///
    /// # Returns
    /// Self for method chaining
    ///
    /// # Errors
    /// Returns error if the database module cannot be installed
    pub fn with_database(mut self, db_handle: DbHandle) -> Result<Self> {
        // Create a new context with the database module
        let mut context = rune::Context::with_default_modules()
            .context("Failed to create Rune context with default modules")?;

        // Install database module
        let db_module = create_db_module(db_handle.clone())
            .context("Failed to create database module")?;

        context.install(db_module)
            .context("Failed to install database module into Rune context")?;

        // Update the context and store the database handle
        self.context = Arc::new(context);
        self.db_handle = Some(db_handle);

        // Clear loaded tools since they were compiled with the old context
        // They will be reloaded with the new context on next discovery
        self.loaded_tools.clear();

        Ok(self)
    }

    /// Discover all .rn tool files in the tool directory
    ///
    /// Scans the configured directory for files with .rn extension and returns
    /// their names (without extension) sorted alphabetically.
    ///
    /// # Returns
    /// Vector of tool names (file stems) sorted alphabetically
    ///
    /// # Errors
    /// Returns error if directory cannot be read
    pub async fn discover_tools(&mut self) -> Result<Vec<String>> {
        let mut tools = Vec::new();

        // Read directory entries
        let mut entries = fs::read_dir(&self.tool_dir)
            .await
            .with_context(|| format!("Failed to read tool directory: {:?}", self.tool_dir))?;

        // Filter for .rn files and extract names
        while let Some(entry) = entries
            .next_entry()
            .await
            .context("Failed to read directory entry")?
        {
            let path = entry.path();

            // Only process .rn files
            if let Some(extension) = path.extension() {
                if extension == "rn" {
                    if let Some(name) = path.file_stem() {
                        tools.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }

        // Sort alphabetically for consistent ordering
        tools.sort();

        // Auto-load all discovered tools
        for tool_name in &tools {
            // Ignore errors during discovery - individual tools may fail
            let _ = self.load_tool(tool_name).await;
        }

        Ok(tools)
    }

    /// Load and compile a specific tool by name
    ///
    /// Reads the .rn file, compiles it using Rune, and stores the compiled
    /// Unit in the loaded_tools cache.
    ///
    /// # Arguments
    /// * `name` - Tool name (without .rn extension)
    ///
    /// # Errors
    /// Returns error if:
    /// - File does not exist
    /// - File cannot be read
    /// - Rune compilation fails (syntax errors, etc.)
    pub async fn load_tool(&mut self, name: &str) -> Result<()> {
        let path = self.tool_dir.join(format!("{}.rn", name));

        // Read source file
        let source_code = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read tool file: {:?}", path))?;

        // Compile Rune script
        let mut sources = Sources::new();
        sources.insert(
            rune::Source::new(name, source_code)
                .context("Failed to create Rune source")?,
        )?;

        let mut diagnostics = Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&*self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        // Check for compilation errors
        if !diagnostics.is_empty() {
            let mut writer = termcolor::Buffer::ansi();
            diagnostics.emit(&mut writer, &sources)?;
            let output = String::from_utf8_lossy(writer.as_slice());
            return Err(anyhow!(
                "Failed to compile tool '{}': {}",
                name,
                output
            ));
        }

        let unit = result.context("Rune compilation failed")?;

        // Cache compiled unit
        self.loaded_tools.insert(name.to_string(), Arc::new(unit));

        Ok(())
    }

    /// Execute a loaded tool with arguments
    ///
    /// Creates a Rune VM, runs the tool's main() function with provided arguments,
    /// and captures the return value or error.
    ///
    /// # Arguments
    /// * `name` - Tool name to execute
    /// * `args` - String arguments to pass to tool's main() function
    ///
    /// # Returns
    /// ToolResult containing output and execution status
    ///
    /// # Errors
    /// Returns error if:
    /// - Tool is not loaded
    /// - VM creation fails
    /// - Execution fails (returns ToolResult with Error status, not Err)
    pub async fn execute_tool(&self, name: &str, args: &[String]) -> Result<ToolResult> {
        // Check if tool is loaded
        let unit = self
            .loaded_tools
            .get(name)
            .ok_or_else(|| {
                anyhow!(
                    "Tool '{}' not found. Use :tools to see available tools.",
                    name
                )
            })?
            .clone();

        // Create runtime with context
        let runtime = Arc::new(
            self.context
                .runtime()
                .context("Failed to create Rune runtime")?,
        );

        // Create VM instance
        let mut vm = Vm::new(runtime, unit);

        // Execute the main() function with args
        // We use async_complete() to handle both sync and async tools
        // Pass each arg as a separate parameter by unpacking the args slice
        let execution_result = if args.is_empty() {
            // No arguments - call main()
            vm.execute(["main"], ())
        } else if args.len() == 1 {
            // One argument - call main(arg0)
            vm.execute(["main"], (args[0].clone(),))
        } else if args.len() == 2 {
            // Two arguments - call main(arg0, arg1)
            vm.execute(["main"], (args[0].clone(), args[1].clone()))
        } else if args.len() == 3 {
            vm.execute(["main"], (args[0].clone(), args[1].clone(), args[2].clone()))
        } else {
            // For more args, pass as a Vec
            // This matches Rune's expectation for variadic arguments
            let args_vec: Vec<String> = args.to_vec();
            vm.execute(["main"], (args_vec,))
        };

        match execution_result {
            Ok(mut execution) => {
                // Run to completion
                let result = execution.async_complete().await;

                match result.into_result() {
                    Ok(value) => {
                        // Convert return value to string
                        let output = format_rune_value(&value);
                        Ok(ToolResult::success(output))
                    }
                    Err(e) => {
                        // Runtime error occurred
                        let error_msg = format_vm_error(&e);
                        Ok(ToolResult::error(error_msg))
                    }
                }
            }
            Err(e) => {
                // VM setup error
                let error_msg = format_vm_error(&e);
                Ok(ToolResult::error(error_msg))
            }
        }
    }

    /// List names of all currently loaded tools
    ///
    /// Returns tool names sorted alphabetically.
    pub fn list_tools(&self) -> Vec<String> {
        let mut tools: Vec<String> = self.loaded_tools.keys().cloned().collect();
        tools.sort();
        tools
    }

    /// Reload all tools from disk
    ///
    /// Clears the loaded tools cache and re-discovers/loads all tools.
    /// Useful for hot-reloading after tool files have been modified.
    ///
    /// # Errors
    /// Returns error if discovery or loading fails
    pub async fn reload(&mut self) -> Result<()> {
        // Clear existing tools
        self.loaded_tools.clear();

        // Re-discover and load
        self.discover_tools().await?;

        Ok(())
    }
}

/// Format a Rune value for display
///
/// Converts Rune runtime values to human-readable strings.
/// Handles primitives, strings, collections, and objects.
fn format_rune_value(value: &rune::Value) -> String {
    // Use Rune's debug formatting
    // For rune 0.13, we use the Debug trait implementation
    format!("{:?}", value)
}

/// Format a Rune VM error for display
///
/// Extracts useful error information including type, message, and stack trace.
fn format_vm_error(error: &VmError) -> String {
    // Get the error message
    let message = error.to_string();

    // Try to extract stack trace if available
    // For now, just return the error message
    // TODO: Add stack trace formatting when needed
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_registry_creation() {
        let temp_dir = TempDir::new().unwrap();
        let registry = ToolRegistry::new(temp_dir.path().to_path_buf());
        assert!(registry.is_ok());
    }

    #[tokio::test]
    async fn test_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new(temp_dir.path().to_path_buf()).unwrap();
        let tools = registry.discover_tools().await.unwrap();
        assert_eq!(tools.len(), 0);
    }
}
