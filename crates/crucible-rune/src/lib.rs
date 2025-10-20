//! # Crucible Rune
//!
//! A dynamic tool execution system powered by the Rune scripting language.
//!
//! This crate provides:
//! - Dynamic tool discovery and loading
//! - Hot-reload functionality for development
//! - AST-based tool analysis
//! - Service layer integration
//! - Embedding and database support
//! - Comprehensive error handling
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use crucible_rune::{RuneService, RuneServiceConfig};
//! use crucible_services::traits::tool::ToolService;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = RuneServiceConfig::default();
//!     let service = RuneService::new(config).await?;
//!
//!     // Discover tools from a directory
//!     service.discover_tools_from_directory("./tools").await?;
//!
//!     // List available tools
//!     let tools = service.list_tools().await?;
//!     println!("Found {} tools", tools.len());
//!
//!     Ok(())
//! }
//! ```

// Core modules
pub mod analyzer;
pub mod context;
pub mod discovery;
pub mod handler;
pub mod loader;
pub mod registry;
pub mod service;
pub mod stdlib;
pub mod tool;

// Supporting systems
pub mod database;
pub mod embeddings;
pub mod errors;
pub mod types;
pub mod utils;

// Re-exports for convenience
pub use analyzer::*;
pub use context::*;
pub use discovery::*;
pub use handler::*;
pub use loader::*;
pub use registry::*;
pub use service::*;
pub use stdlib::*;
pub use tool::*;

pub use database::*;
pub use embeddings::*;
pub use errors::*;
pub use types::*;
pub use utils::*;

use anyhow::Result;
use std::path::Path;

/// Default tool directories to search
pub const DEFAULT_TOOL_DIRECTORIES: &[&str] = &[
    "./tools",
    "./rune-tools",
    "./scripts",
    "./plugins",
];

/// Default file extensions for Rune tools
pub const DEFAULT_RUNE_EXTENSIONS: &[&str] = &["rn", "rune"];

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the Rune system with default configuration
pub async fn init() -> Result<RuneService> {
    let config = RuneServiceConfig::default();
    RuneService::new(config).await
}

/// Initialize the Rune system with custom configuration
pub async fn init_with_config(config: RuneServiceConfig) -> Result<RuneService> {
    RuneService::new(config).await
}

/// Discover tools from default directories
pub async fn discover_default_tools() -> Result<Vec<discovery::DiscoveredTool>> {
    let discovery_config = discovery::DiscoveryConfig::default();
    let discovery = discovery::ToolDiscovery::new(discovery_config)?;
    let mut tools = Vec::new();

    for dir in DEFAULT_TOOL_DIRECTORIES {
        if Path::new(dir).exists() {
            let discovered = discovery.discover_from_directory(dir).await?;
            for discovery in discovered {
                tools.extend(discovery.tools);
            }
        }
    }

    Ok(tools)
}

/// Validate a Rune file without loading it
pub async fn validate_rune_file<P: AsRef<Path>>(path: P) -> Result<validation::ValidationResult> {
    let discovery_config = discovery::DiscoveryConfig::default();
    let discovery = discovery::ToolDiscovery::new(discovery_config)?;
    discovery.validate_file(path).await
}

/// Get information about the Rune system
pub fn system_info() -> types::SystemInfo {
    types::SystemInfo {
        version: VERSION.to_string(),
        rune_version: rune::VERSION,
        supported_extensions: DEFAULT_RUNE_EXTENSIONS.to_vec(),
        default_directories: DEFAULT_TOOL_DIRECTORIES.iter().map(|s| s.to_string()).collect(),
    }
}

/// Create a simple test tool for demonstration
pub fn create_simple_tool(name: &str, description: &str, logic: &str) -> Result<tool::RuneTool> {
    let source_code = format!(r#"
        pub fn NAME() {{ {:?} }}
        pub fn DESCRIPTION() {{ {:?} }}
        pub fn INPUT_SCHEMA() {{
            #{{ type: "object", properties: #{{}}, required: [] }}
        }}
        pub async fn call(args) {{
            {}
        }}
    "#, name, description, logic);

    let context = context::create_safe_context()?;
    tool::RuneTool::from_source(&source_code, &context, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let info = system_info();
        assert!(!info.version.is_empty());
        assert!(!info.rune_version.is_empty());
        assert_eq!(info.supported_extensions.len(), 2);
        assert_eq!(info.default_directories.len(), 4);
    }

    #[tokio::test]
    async fn test_default_initialization() {
        // This test requires actual tool files to be present
        // In a real scenario, you'd set up test fixtures
        let result = init().await;
        // Don't assert success here as we might not have tool files
        println!("Initialization result: {:?}", result);
    }

    #[test]
    fn test_simple_tool_creation() {
        let tool = create_simple_tool(
            "test_tool",
            "A simple test tool",
            r#"{ success: true, message: "Hello from simple tool!" }"#
        );

        assert!(tool.is_ok());
        if let Ok(tool) = tool {
            assert_eq!(tool.name, "test_tool");
            assert_eq!(tool.description, "A simple test tool");
        }
    }

    #[tokio::test]
    async fn test_validation() {
        let temp_dir = tempfile::TempDir::unwrap();
        let tool_path = temp_dir.path().join("test.rn");

        // Create a valid test tool file
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ success: true } }
        "#;

        std::fs::write(&tool_path, tool_source).unwrap();

        let result = validate_rune_file(&tool_path).await.unwrap();
        assert!(result.valid);
    }

    #[tokio::test]
    async fn test_integration() -> Result<(), Box<dyn std::error::Error>> {
        // Create a simple test tool file
        let temp_dir = tempfile::TempDir::new()?;
        let tool_path = temp_dir.path().join("integration_test.rn");

        let tool_source = r#"
            pub fn NAME() { "integration_tool" }
            pub fn DESCRIPTION() { "Integration test tool" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: {
                        message: { type: "string" }
                    },
                    required: ["message"]
                }
            }
            pub async fn call(args) {
                #{
                    success: true,
                    echo: args.message,
                    timestamp: time::now()
                }
            }
        "#;

        std::fs::write(&tool_path, tool_source)?;

        // Create a service with the temp directory as a tool directory
        let config = RuneServiceConfig {
            service_name: "test-service".to_string(),
            version: "1.0.0".to_string(),
            discovery: crate::types::DiscoveryServiceConfig {
                tool_directories: vec![temp_dir.path().to_path_buf()],
                ..Default::default()
            },
            hot_reload: crate::types::HotReloadConfig {
                enabled: false,
                ..Default::default()
            },
            execution: crate::types::ExecutionConfig::default(),
            cache: crate::types::CacheConfig::default(),
            security: crate::types::SecurityConfig::default(),
        };

        let service = RuneService::new(config).await?;

        // List tools
        let tools = service.list_tools().await?;
        assert!(!tools.is_empty());

        // Find our integration tool
        let integration_tool = tools.iter()
            .find(|t| t.name == "integration_tool")
            .unwrap();

        assert_eq!(integration_tool.name, "integration_tool");
        assert_eq!(integration_tool.description, "Integration test tool");

        // Execute the tool
        let request = crucible_services::types::tool::ToolExecutionRequest {
            execution_id: "test-123".to_string(),
            tool_name: "integration_tool".to_string(),
            parameters: serde_json::json!({
                "message": "Hello from integration test!"
            }),
            context: crucible_services::types::tool::ToolExecutionContext {
                execution_id: "test-123".to_string(),
                tool_name: "integration_tool".to_string(),
                user_id: "test".to_string(),
                session_id: None,
                timestamp: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
            },
        };

        let result = service.execute_tool(request).await?;
        assert!(result.success);
        assert_eq!(result.output["success"], true);
        assert_eq!(result.output["echo"], "Hello from integration test!");

        // Check service stats
        let stats = service.get_service_stats().await?;
        assert_eq!(stats.total_tools, 1);
        assert_eq!(stats.enabled_tools, 1);
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);

        Ok(())
    }
}