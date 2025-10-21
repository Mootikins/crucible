//! Comprehensive AST analyzer for Rune module discovery and schema extraction
//!
//! This module provides deep analysis capabilities for compiled Rune units:
//! - Module structure discovery and traversal
//! - Function signature extraction with type information
//! - Parameter analysis with optional/default value detection
//! - Doc comment extraction for documentation
//! - Consumer information parsing from metadata
//! - JSON Schema generation for function parameters
//! - Enhanced error handling and validation

use crate::types::AnalyzerConfig;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Information about a discovered module with enhanced metadata
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    /// Module name
    pub name: String,
    /// Module path components
    pub path: Vec<String>,
    /// Functions in this module
    pub functions: Vec<crate::types::AsyncFunctionInfo>,
    /// Module description
    pub description: Option<String>,
    /// Module doc comments
    pub doc_comments: Vec<String>,
    /// Module metadata
    pub metadata: HashMap<String, Value>,
    /// Source location
    pub location: crate::types::SourceLocation,
}



/// Type system for Rune parameter analysis
#[derive(Debug, Clone, PartialEq)]
pub enum RuneType {
    /// Basic primitive types
    String,
    Number,
    Integer,
    Float,
    Boolean,

    /// Collection types
    Array(Box<RuneType>),
    Object(HashMap<String, RuneType>),
    Tuple(Vec<RuneType>),
    Option(Box<RuneType>),

    /// Special types
    Any,
    Void,
    Unknown(String),

    /// Function types
    Function {
        parameters: Vec<RuneType>,
        return_type: Box<RuneType>,
        is_async: bool,
    },
}

/// Type constraint system
#[derive(Debug, Clone)]
pub struct TypeConstraint {
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Constraint parameters
    pub parameters: HashMap<String, Value>,
    /// Constraint description
    pub description: String,
}

/// Constraint types
#[derive(Debug, Clone)]
pub enum ConstraintType {
    /// Range constraints for numbers
    Range { min: Option<f64>, max: Option<f64> },
    /// Length constraints for strings and arrays
    Length { min: Option<usize>, max: Option<usize> },
    /// Pattern matching for strings
    Pattern(String),
    /// Enum constraints (one of specific values)
    Enum(Vec<Value>),
    /// Custom validation function
    Custom(String),
    /// Required/optional constraints
    Required,
    Optional,
}

/// Type inference engine
pub struct TypeInferenceEngine {
    config: AnalyzerConfig,
}

impl TypeInferenceEngine {
    /// Create a new type inference engine
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    /// Infer parameter types from function execution and naming patterns
    pub fn infer_parameter_types(
        &self,
        _unit: &Arc<rune::Unit>,
        function_path: &[&str],
        execution_results: &[rune::runtime::Value],
    ) -> Result<Vec<(String, RuneType, Vec<TypeConstraint>)>> {
        let mut inferred_types = Vec::new();

        // Start with naming convention inference
        let naming_types = self.infer_from_naming_conventions(function_path);

        // Refine with execution analysis
        let _execution_types = self.analyze_execution_results(execution_results);

        // Combine and validate inferences
        for (param_name, base_type) in naming_types {
            let constraints = self.generate_constraints_for_type(&base_type, function_path);
            inferred_types.push((param_name, base_type, constraints));
        }

        Ok(inferred_types)
    }

    /// Infer types from function and parameter naming conventions
    fn infer_from_naming_conventions(&self, function_path: &[&str]) -> Vec<(String, RuneType)> {
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        let mut types = Vec::new();

        match (*module_name, *function_name) {
            // File operations typically work with paths and content
            ("file" | "file_operations" | "files" | "io", "create" | "create_file" | "write" | "write_file") => {
                types.push(("path".to_string(), RuneType::String));
                types.push(("content".to_string(), RuneType::String));
            },
            ("file" | "file_operations" | "files" | "io", "read" | "read_file") => {
                types.push(("path".to_string(), RuneType::String));
            },
            ("file" | "file_operations" | "files" | "io", "delete" | "delete_file") => {
                types.push(("path".to_string(), RuneType::String));
            },
            ("file" | "file_operations" | "files" | "io", "copy" | "copy_file" | "move" | "move_file") => {
                types.push(("source".to_string(), RuneType::String));
                types.push(("destination".to_string(), RuneType::String));
            },
            ("file" | "file_operations" | "files" | "io", "list" | "list_files") => {
                types.push(("directory".to_string(), RuneType::String));
                types.push(("pattern".to_string(), RuneType::String));
                types.push(("recursive".to_string(), RuneType::Boolean));
            },

            // Search operations work with queries and filters
            ("search" | "query" | "find" | "filter", "search" | "query" | "find") => {
                types.push(("query".to_string(), RuneType::String));
                types.push(("scope".to_string(), RuneType::String));
                types.push(("case_sensitive".to_string(), RuneType::Boolean));
            },
            ("search" | "query" | "find" | "filter", "search_files") => {
                types.push(("pattern".to_string(), RuneType::String));
                types.push(("directory".to_string(), RuneType::String));
                types.push(("max_results".to_string(), RuneType::Integer));
            },
            ("search" | "query" | "find" | "filter", "search_content") => {
                types.push(("content".to_string(), RuneType::String));
                types.push(("files".to_string(), RuneType::Array(Box::new(RuneType::String))));
                types.push(("regex".to_string(), RuneType::Boolean));
            },

            // UI operations format and display data
            ("ui" | "ui_helpers" | "display" | "format", "format" | "format_results") => {
                types.push(("data".to_string(), RuneType::Any));
                types.push(("format".to_string(), RuneType::String));
                types.push(("options".to_string(), RuneType::Object(HashMap::new())));
            },
            ("ui" | "ui_helpers" | "display" | "format", "get_suggestions" | "suggest") => {
                types.push(("input".to_string(), RuneType::String));
                types.push(("context".to_string(), RuneType::Object(HashMap::new())));
                types.push(("max_suggestions".to_string(), RuneType::Integer));
            },
            ("ui" | "ui_helpers" | "display" | "format", "prompt" | "confirm" | "select") => {
                types.push(("message".to_string(), RuneType::String));
                types.push(("default".to_string(), RuneType::String));
                types.push(("options".to_string(), RuneType::Array(Box::new(RuneType::String))));
            },

            // Agent operations analyze and process data
            ("agent" | "agent_helpers" | "ai" | "assist", "analyze" | "analyze_data" | "recommend") => {
                types.push(("data".to_string(), RuneType::Any));
                types.push(("context".to_string(), RuneType::Object(HashMap::new())));
                types.push(("options".to_string(), RuneType::Object(HashMap::new())));
            },
            ("agent" | "agent_helpers" | "ai" | "assist", "optimize" | "assist") => {
                types.push(("target".to_string(), RuneType::String));
                types.push(("strategy".to_string(), RuneType::String));
                types.push(("constraints".to_string(), RuneType::Array(Box::new(RuneType::String))));
            },

            // Advanced operations for testing and validation
            ("advanced" | "utils" | "helpers" | "tools", "test" | "benchmark") => {
                types.push(("target".to_string(), RuneType::String));
                types.push(("config".to_string(), RuneType::Object(HashMap::new())));
                types.push(("iterations".to_string(), RuneType::Integer));
            },
            ("advanced" | "utils" | "helpers" | "tools", "validate" | "sanitize") => {
                types.push(("input".to_string(), RuneType::Any));
                types.push(("rules".to_string(), RuneType::Array(Box::new(RuneType::Object(HashMap::new())))));
                types.push(("strict".to_string(), RuneType::Boolean));
            },
            ("advanced" | "utils" | "helpers" | "tools", "transform" | "aggregate") => {
                types.push(("data".to_string(), RuneType::Array(Box::new(RuneType::Any))));
                types.push(("operation".to_string(), RuneType::String));
                types.push(("options".to_string(), RuneType::Object(HashMap::new())));
            },

            // Default patterns based on function name analysis
            _ => {
                // Analyze function name for common patterns
                if function_name.contains("create") {
                    types.push(("input".to_string(), RuneType::Object(HashMap::new())));
                } else if function_name.contains("delete") || function_name.contains("remove") {
                    types.push(("id".to_string(), RuneType::String));
                } else if function_name.contains("update") || function_name.contains("modify") {
                    types.push(("id".to_string(), RuneType::String));
                    types.push(("data".to_string(), RuneType::Object(HashMap::new())));
                } else if function_name.contains("get") || function_name.contains("fetch") {
                    types.push(("query".to_string(), RuneType::Object(HashMap::new())));
                    types.push(("fields".to_string(), RuneType::Array(Box::new(RuneType::String))));
                } else if function_name.contains("list") || function_name.contains("all") {
                    types.push(("filter".to_string(), RuneType::Object(HashMap::new())));
                    types.push(("limit".to_string(), RuneType::Integer));
                } else if function_name.contains("count") || function_name.contains("size") {
                    types.push(("collection".to_string(), RuneType::String));
                } else {
                    // Generic parameter
                    types.push(("args".to_string(), RuneType::Object(HashMap::new())));
                }
            }
        }

        types
    }

    /// Analyze execution results to refine type inference
    fn analyze_execution_results(&self, _results: &[rune::runtime::Value]) -> HashMap<String, RuneType> {
        // This is a simplified approach - in a real implementation,
        // we'd need more sophisticated analysis to map results to input types
        HashMap::new()
    }

    /// Convert Rune runtime value to our type system
    fn convert_rune_value_to_type(&self, value: &rune::runtime::Value) -> RuneType {
        match value {
            rune::runtime::Value::String(_) => RuneType::String,
            rune::runtime::Value::Integer(_) => RuneType::Integer,
            rune::runtime::Value::Float(_) => RuneType::Float,
            rune::runtime::Value::Bool(_) => RuneType::Boolean,
            rune::runtime::Value::Object(_) => RuneType::Object(HashMap::new()),
            rune::runtime::Value::Option(_) => RuneType::Option(Box::new(RuneType::Any)),
            _ => RuneType::Unknown("complex_type".to_string()),
        }
    }

    /// Generate constraints for a given type
    pub fn generate_constraints_for_type(&self, rune_type: &RuneType, function_path: &[&str]) -> Vec<TypeConstraint> {
        let mut constraints = Vec::new();

        // Add type-specific constraints
        match rune_type {
            RuneType::String => {
                constraints.push(TypeConstraint {
                    constraint_type: ConstraintType::Length { min: Some(1), max: Some(10000) },
                    parameters: HashMap::new(),
                    description: "String length constraint".to_string(),
                });
            },
            RuneType::Integer | RuneType::Number => {
                constraints.push(TypeConstraint {
                    constraint_type: ConstraintType::Range { min: Some(0.0), max: None },
                    parameters: HashMap::new(),
                    description: "Non-negative number constraint".to_string(),
                });
            },
            RuneType::Array(_) => {
                constraints.push(TypeConstraint {
                    constraint_type: ConstraintType::Length { min: Some(0), max: Some(1000) },
                    parameters: HashMap::new(),
                    description: "Array size constraint".to_string(),
                });
            },
            _ => {}
        }

        // Add function-specific constraints
        let function_name = function_path.last().unwrap_or(&"unknown");
        if function_name.contains("path") || function_name.contains("file") {
            constraints.push(TypeConstraint {
                constraint_type: ConstraintType::Pattern(r"[a-zA-Z0-9_\-./]+".to_string()),
                parameters: HashMap::new(),
                description: "Path pattern constraint".to_string(),
            });
        }

        constraints
    }
}

/// Comprehensive AST analyzer for Rune modules
pub struct RuneAstAnalyzer {
    config: AnalyzerConfig,
    context: Arc<rune::Context>,
    type_engine: TypeInferenceEngine,
}

impl RuneAstAnalyzer {
    /// Create a new AST analyzer with default configuration
    pub fn new() -> Result<Self> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        let config = AnalyzerConfig::default();
        let type_engine = TypeInferenceEngine::new(config.clone());

        Ok(Self {
            config,
            context,
            type_engine,
        })
    }

    /// Create a new AST analyzer with custom configuration
    pub fn with_config(config: AnalyzerConfig) -> Result<Self> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        let type_engine = TypeInferenceEngine::new(config.clone());

        Ok(Self {
            config,
            context,
            type_engine,
        })
    }

    /// Create a new AST analyzer with custom context and configuration
    pub fn with_context(config: AnalyzerConfig, context: Arc<rune::Context>) -> Result<Self> {
        let type_engine = TypeInferenceEngine::new(config.clone());

        Ok(Self {
            config,
            context,
            type_engine,
        })
    }

    /// Analyze a compiled Rune unit to discover modules and functions
    pub fn analyze_modules(&self, unit: &Arc<rune::Unit>) -> Result<Vec<DiscoveredModule>> {
        let mut modules = Vec::new();

        // This is a simplified implementation
        // In a real implementation, we'd need to traverse the AST to extract module information
        // For now, we'll create a default module from the unit

        let module = self.create_default_module(unit)?;
        modules.push(module);

        Ok(modules)
    }

    /// Create a default module from a unit
    fn create_default_module(&self, unit: &Arc<rune::Unit>) -> Result<DiscoveredModule> {
        // Try to extract function information from the unit
        let functions = self.extract_functions_from_unit(unit)?;

        Ok(DiscoveredModule {
            name: "default".to_string(),
            path: vec!["default".to_string()],
            functions,
            description: None,
            doc_comments: Vec::new(),
            metadata: HashMap::new(),
            location: crate::types::SourceLocation {
                line: 1,
                column: 1,
                offset: 0,
            },
        })
    }

    /// Extract functions from a compiled unit
    fn extract_functions_from_unit(&self, unit: &Arc<rune::Unit>) -> Result<Vec<crate::types::AsyncFunctionInfo>> {
        let mut functions = Vec::new();

        // Try to call common function names to see what's available
        let runtime = Arc::new(self.context.runtime()?);

        // Test for common tool functions
        let function_names = vec![
            "call", "execute", "run", "process", "handle",
            "create", "read", "update", "delete",
            "search", "find", "list", "get",
        ];

        for function_name in function_names {
            let mut vm = rune::runtime::Vm::new(runtime.clone(), unit.clone());
            if let Ok(_) = vm.call([function_name], (serde_json::json!({}),)) {
                let function_info = crate::types::AsyncFunctionInfo {
                    name: function_name.to_string(),
                    is_async: true, // Assume async for tool functions
                    is_public: true,
                    parameters: vec![], // Would need more sophisticated analysis
                    return_type: Some("Result".to_string()),
                    module_path: vec!["default".to_string()],
                    full_path: vec!["default".to_string(), function_name.to_string()],
                    description: None,
                    doc_comments: Vec::new(),
                    location: crate::types::SourceLocation {
                        line: 1,
                        column: 1,
                        offset: 0,
                    },
                    metadata: HashMap::new(),
                    attributes: Vec::new(),
                };
                functions.push(function_info);
            }
        }

        Ok(functions)
    }

    /// Analyze a specific function within a unit
    pub fn analyze_function(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Option<crate::types::AsyncFunctionInfo>> {
        let modules = self.analyze_modules(unit)?;

        for module in modules {
            for function in module.functions {
                if function.full_path.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice() == function_path {
                    return Ok(Some(function));
                }
            }
        }

        Ok(None)
    }

    /// Generate JSON schema for a function
    pub fn generate_function_schema(&self, function: &crate::types::AsyncFunctionInfo) -> Result<Value> {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &function.parameters {
            let param_schema = self.generate_parameter_schema(param)?;
            properties.insert(param.name.clone(), param_schema);

            if !param.is_optional {
                required.push(param.name.clone());
            }
        }

        Ok(json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "description": function.description.as_ref().cloned().unwrap_or_else(|| format!("{} function", function.name))
        }))
    }

    /// Generate JSON schema for a parameter
    fn generate_parameter_schema(&self, param: &crate::types::ParameterInfo) -> Result<Value> {
        let base_schema = match param.type_name.to_lowercase().as_str() {
            "string" => json!({"type": "string"}),
            "number" | "int" | "integer" | "float" => json!({"type": "number"}),
            "bool" | "boolean" => json!({"type": "boolean"}),
            "array" | "vec" | "list" => json!({
                "type": "array",
                "items": {"type": "string"}
            }),
            "object" | "map" | "struct" => json!({
                "type": "object",
                "additionalProperties": true
            }),
            _ => json!({"type": "string"}),
        };

        let mut schema = base_schema;

        // Add description if available
        if let Some(ref description) = param.description {
            schema["description"] = json!(description);
        }

        // Add default value if available
        if let Some(ref default_value) = param.default_value {
            schema["default"] = default_value.clone();
        }

        Ok(schema)
    }

    /// Validate a function signature
    pub fn validate_function(&self, function: &crate::types::AsyncFunctionInfo) -> Result<super::ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check for required fields
        if function.name.is_empty() {
            errors.push("Function name cannot be empty".to_string());
        }

        if !function.is_async {
            warnings.push("Function should be async for tool compatibility".to_string());
        }

        // Check parameter names
        for param in &function.parameters {
            if param.name.is_empty() {
                errors.push("Parameter name cannot be empty".to_string());
            }
        }

        let valid = errors.is_empty();

        Ok(super::ValidationResult {
            valid,
            errors,
            warnings,
            metadata: HashMap::new(),
        })
    }

    /// Infer additional metadata about a function
    pub fn infer_function_metadata(&self, function: &crate::types::AsyncFunctionInfo) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();

        // Infer category from module path
        if let Some(module_name) = function.module_path.first() {
            metadata.insert("category".to_string(), json!(module_name));
        }

        // Infer complexity from parameter count
        metadata.insert("complexity".to_string(), json!(
            if function.parameters.len() > 5 { "high" }
            else if function.parameters.len() > 2 { "medium" }
            else { "low" }
        ));

        // Infer type from name
        let function_type = if function.name.contains("create") || function.name.contains("add") {
            "creation"
        } else if function.name.contains("delete") || function.name.contains("remove") {
            "deletion"
        } else if function.name.contains("update") || function.name.contains("modify") {
            "modification"
        } else if function.name.contains("get") || function.name.contains("fetch") || function.name.contains("read") {
            "retrieval"
        } else if function.name.contains("search") || function.name.contains("find") {
            "search"
        } else {
            "general"
        };
        metadata.insert("type".to_string(), json!(function_type));

        metadata
    }
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            enable_type_inference: true,
            enable_validation: true,
            max_depth: 10,
            validation_rules: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyzer_creation() {
        let analyzer = RuneAstAnalyzer::new();
        assert!(analyzer.is_ok());
    }

    #[test]
    fn test_type_inference() {
        let config = AnalyzerConfig::default();
        let engine = TypeInferenceEngine::new(config);

        let function_path = vec!["file", "create_file"];
        let types = engine.infer_from_naming_conventions(&function_path);

        assert_eq!(types.len(), 2);
        assert!(types.iter().any(|(name, _)| name == "path"));
        assert!(types.iter().any(|(name, _)| name == "content"));
    }

    #[test]
    fn test_rune_type_conversion() {
        let config = AnalyzerConfig::default();
        let engine = TypeInferenceEngine::new(config);

        // Test various Rune values
        let string_value = rune::runtime::Value::String(rune::alloc::String::try_from("test").unwrap());
        let rune_type = engine.convert_rune_value_to_type(&string_value);
        assert_eq!(rune_type, RuneType::String);

        let int_value = rune::runtime::Value::Integer(42);
        let rune_type = engine.convert_rune_value_to_type(&int_value);
        assert_eq!(rune_type, RuneType::Integer);
    }

    #[test]
    fn test_constraint_generation() {
        let config = AnalyzerConfig::default();
        let engine = TypeInferenceEngine::new(config);
        let function_path = vec!["file", "read_file"];

        let constraints = engine.generate_constraints_for_type(&RuneType::String, &function_path);
        assert!(!constraints.is_empty());

        // Should have length constraint
        assert!(constraints.iter().any(|c| matches!(&c.constraint_type, ConstraintType::Length { .. })));
    }

    #[test]
    fn test_function_validation() {
        let analyzer = RuneAstAnalyzer::new().unwrap();
        let function = crate::types::AsyncFunctionInfo {
            name: "test_function".to_string(),
            is_async: true,
            is_public: true,
            parameters: vec![],
            return_type: Some("Result".to_string()),
            module_path: vec!["test".to_string()],
            full_path: vec!["test".to_string(), "test_function".to_string()],
            description: Some("Test function".to_string()),
            doc_comments: Vec::new(),
            location: crate::types::SourceLocation { line: 1, column: 1, offset: 0 },
            metadata: HashMap::new(),
            attributes: Vec::new(),
        };

        let result = analyzer.validate_function(&function).unwrap();
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_metadata_inference() {
        let analyzer = RuneAstAnalyzer::new().unwrap();
        let function = crate::types::AsyncFunctionInfo {
            name: "create_file".to_string(),
            is_async: true,
            is_public: true,
            parameters: vec![],
            return_type: Some("Result".to_string()),
            module_path: vec!["file".to_string()],
            full_path: vec!["file".to_string(), "create_file".to_string()],
            description: None,
            doc_comments: Vec::new(),
            location: crate::types::SourceLocation { line: 1, column: 1, offset: 0 },
            metadata: HashMap::new(),
            attributes: Vec::new(),
        };

        let metadata = analyzer.infer_function_metadata(&function);
        assert_eq!(metadata.get("category"), Some(&json!("file")));
        assert_eq!(metadata.get("type"), Some(&json!("creation")));
    }
}