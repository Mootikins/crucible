/// Rune-based tool system for Crucible MCP
///
/// This module provides support for defining MCP tools using the Rune scripting language.
/// Tools can be dynamically loaded, validated, and executed with hot-reload support.
///
/// Enhanced features:
/// - Flexible organization patterns (simple direct tools + module-based tools)
/// - Consumer awareness without restrictions
/// - Configurable naming conventions
/// - Backwards compatibility with existing tools
/// - AST-based module discovery for organized tools

mod ast_analyzer;
mod discovery;
mod error_handling;
pub mod handler_generator;
mod registry;
pub mod registry_async;
pub mod schema_generator;
mod schema_validator;
mod stdlib;
mod tool;
mod tool_macro;
mod tool_metadata_storage;

pub use ast_analyzer::{
    RuneAstAnalyzer, DiscoveredModule, AsyncFunctionInfo, ParameterInfo,
    TypeInferenceEngine, AnalyzerConfig, RuneType, TypeConstraint, ConstraintType,
    ValidationRule, SourceLocation
};
pub use discovery::{ToolDiscovery, DiscoveredTool, DiscoveredTools, ConsumerInfo};
pub use error_handling::{
    RuneErrorHandler, ErrorRecoveryManager, ErrorLogger, CircuitBreaker,
    RecoveryStrategy, RecoveryAttempt, ErrorStats
};
pub use handler_generator::{DynamicRuneToolHandler, ToolHandlerGenerator, EnhancedToolService};
pub use registry::ToolRegistry;
pub use registry_async::AsyncToolRegistry;
pub use schema_generator::generate_schema;
pub use schema_validator::{SchemaValidator, ValidationResult, ValidationError, ValidationWarning, ValidationConfig, ValidationContext};
pub use stdlib::build_crucible_module;
pub use tool::{RuneTool, ToolMetadata};
// pub use tool_macro::tool_attribute_macro; // Temporarily commented out
pub use tool_metadata_storage::{
    ParameterMetadata, ToolMacroMetadata, ToolMetadataStorage, TypeSpec,
};
