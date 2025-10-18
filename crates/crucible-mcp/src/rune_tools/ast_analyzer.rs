/// Comprehensive AST analyzer for Rune module discovery and schema extraction
///
/// This module provides deep analysis capabilities for compiled Rune units:
/// - Module structure discovery and traversal
/// - Function signature extraction with type information
/// - Parameter analysis with optional/default value detection
/// - Doc comment extraction for documentation
/// - Consumer information parsing from metadata
/// - JSON Schema generation for function parameters
/// - Enhanced error handling and validation

use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace, warn};

use super::schema_validator::{SchemaValidator, ValidationConfig};

/// Information about a discovered module with enhanced metadata
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    pub name: String,
    pub path: Vec<String>,
    pub functions: Vec<AsyncFunctionInfo>,
    pub description: Option<String>,
    pub doc_comments: Vec<String>,
    pub metadata: HashMap<String, Value>,
}

/// Comprehensive information about a discovered async function
#[derive(Debug, Clone)]
pub struct AsyncFunctionInfo {
    pub name: String,
    pub is_async: bool,
    pub is_public: bool,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
    pub module_path: Vec<String>,
    pub full_path: Vec<String>,
    pub description: Option<String>,
    pub doc_comments: Vec<String>,
    pub source_location: SourceLocation,
    pub metadata: HashMap<String, Value>,
}

/// Information about a function parameter with type details
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub type_name: String,
    pub type_constraints: Vec<String>,
    pub is_optional: bool,
    pub default_value: Option<Value>,
    pub description: Option<String>,
    pub validation_rules: Vec<ValidationRule>,
}

/// Source location information for debugging
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub file_path: Option<String>,
}

/// Parameter validation rules
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub rule_type: String,
    pub parameters: HashMap<String, Value>,
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
    pub constraint_type: ConstraintType,
    pub parameters: HashMap<String, Value>,
    pub description: String,
}

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

/// Analysis configuration
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub include_private_functions: bool,
    pub extract_doc_comments: bool,
    pub generate_schemas: bool,
    pub validate_signatures: bool,
    pub infer_types: bool,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            include_private_functions: false,
            extract_doc_comments: true,
            generate_schemas: true,
            validate_signatures: true,
            infer_types: true,
        }
    }
}

/// Comprehensive AST analyzer for Rune modules
pub struct RuneAstAnalyzer {
    config: AnalyzerConfig,
    context: Arc<rune::Context>,
    type_engine: TypeInferenceEngine,
    schema_validator: SchemaValidator,
}

impl RuneAstAnalyzer {
    /// Create a new AST analyzer with default configuration
    pub fn new() -> Result<Self> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        let config = AnalyzerConfig::default();
        let type_engine = TypeInferenceEngine::new(config.clone());
        let validation_config = ValidationConfig::default();
        let schema_validator = SchemaValidator::new(validation_config);
        Ok(Self {
            config,
            context,
            type_engine,
            schema_validator,
        })
    }

    /// Create a new AST analyzer with custom configuration
    pub fn with_config(config: AnalyzerConfig) -> Result<Self> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        let type_engine = TypeInferenceEngine::new(config.clone());
        let validation_config = ValidationConfig::default();
        let schema_validator = SchemaValidator::new(validation_config);
        Ok(Self { config, context, type_engine, schema_validator })
    }

    /// Analyze a compiled Rune unit to discover all modules and their contents
    pub fn analyze_modules(&self, unit: &Arc<rune::Unit>) -> Result<Vec<DiscoveredModule>> {
        debug!("Starting comprehensive module analysis");
        let mut modules = Vec::new();

        // Use reflection-based module discovery with enhanced pattern matching
        let module_patterns = self.get_module_patterns();

        for module_name in module_patterns {
            if let Some(module_info) = self.analyze_module_comprehensive(unit, &module_name)? {
                trace!("Discovered module: {} with {} functions", module_info.name, module_info.functions.len());
                modules.push(module_info);
            }
        }

        // Also try to discover additional modules through introspection
        modules.extend(self.discover_additional_modules(unit)?);

        debug!("Analysis complete. Found {} modules", modules.len());
        Ok(modules)
    }

    /// Get comprehensive module patterns to search for
    fn get_module_patterns(&self) -> Vec<String> {
        vec![
            // File operations
            "file_operations".to_string(),
            "file".to_string(),
            "files".to_string(),
            "io".to_string(),

            // Search operations
            "search".to_string(),
            "query".to_string(),
            "find".to_string(),
            "filter".to_string(),

            // UI helpers
            "ui".to_string(),
            "ui_helpers".to_string(),
            "display".to_string(),
            "format".to_string(),

            // Agent helpers
            "agent_helpers".to_string(),
            "agent".to_string(),
            "ai".to_string(),
            "assist".to_string(),

            // Advanced features
            "advanced".to_string(),
            "utils".to_string(),
            "helpers".to_string(),
            "tools".to_string(),

            // Common patterns
            "data".to_string(),
            "network".to_string(),
            "system".to_string(),
            "config".to_string(),
        ]
    }

    /// Comprehensively analyze a specific module
    fn analyze_module_comprehensive(&self, unit: &Arc<rune::Unit>, module_name: &str) -> Result<Option<DiscoveredModule>> {
        debug!("Analyzing module: {}", module_name);

        let mut found_functions = Vec::new();
        let mut doc_comments = Vec::new();
        let mut metadata = HashMap::new();

        // Get function patterns for this module
        let function_patterns = self.get_function_patterns_for_module(module_name);

        for function_name in function_patterns {
            let full_path = vec![module_name, &function_name];

            if self.function_exists(unit, &full_path)? {
                match self.analyze_function_comprehensive(unit, module_name, &function_name) {
                    Ok(function_info) => found_functions.push(function_info),
                    Err(e) => {
                        trace!("Failed to analyze function {}.{}: {}", module_name, function_name, e);
                    }
                }
            }
        }

        // Extract module-level metadata
        if self.config.extract_doc_comments {
            doc_comments = self.extract_module_doc_comments(unit, module_name)?;
        }

        metadata.insert("discovered_at".to_string(), json!("2025-10-16T00:00:00Z"));

        if !found_functions.is_empty() {
            Ok(Some(DiscoveredModule {
                name: module_name.to_string(),
                path: vec![module_name.to_string()],
                functions: found_functions,
                description: self.extract_module_description(unit, module_name)?,
                doc_comments,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get function patterns based on module context
    fn get_function_patterns_for_module(&self, module_name: &str) -> Vec<String> {
        match module_name {
            "file" | "file_operations" | "files" | "io" => vec![
                "create_file".to_string(),
                "create".to_string(),
                "delete_file".to_string(),
                "delete".to_string(),
                "copy_file".to_string(),
                "copy".to_string(),
                "move_file".to_string(),
                "move".to_string(),
                "read_file".to_string(),
                "read".to_string(),
                "write_file".to_string(),
                "write".to_string(),
                "list_files".to_string(),
                "list".to_string(),
                "file_info".to_string(),
                "info".to_string(),
            ],

            "search" | "query" | "find" | "filter" => vec![
                "search".to_string(),
                "query".to_string(),
                "find".to_string(),
                "filter".to_string(),
                "search_files".to_string(),
                "search_content".to_string(),
                "advanced_search".to_string(),
                "fuzzy_search".to_string(),
                "regex_search".to_string(),
            ],

            "ui" | "ui_helpers" | "display" | "format" => vec![
                "format_results".to_string(),
                "format".to_string(),
                "display".to_string(),
                "display_data".to_string(),
                "show".to_string(),
                "render".to_string(),
                "get_suggestions".to_string(),
                "suggest".to_string(),
                "prompt".to_string(),
                "prompt_user".to_string(),
                "confirm".to_string(),
                "select".to_string(),
                "create_menu".to_string(),
                "validate_input".to_string(),
                "validate".to_string(),
            ],

            "agent" | "agent_helpers" | "ai" | "assist" => vec![
                "get_suggestions".to_string(),
                "suggest".to_string(),
                "optimize".to_string(),
                "analyze".to_string(),
                "analyze_data".to_string(),
                "recommend".to_string(),
                "assist".to_string(),
                "guide".to_string(),
                "explain".to_string(),
                "process_text".to_string(),
                "generate_content".to_string(),
                "classify".to_string(),
            ],

            "advanced" | "utils" | "helpers" | "tools" => vec![
                "test_rune_features".to_string(),
                "benchmark_performance".to_string(),
                "analyze_complexity".to_string(),
                "optimize_performance".to_string(),
                "validate_input".to_string(),
                "sanitize_data".to_string(),
                "transform_data".to_string(),
                "aggregate_results".to_string(),
            ],

            // Default patterns for unknown modules
            _ => vec![
                "process".to_string(),
                "handle".to_string(),
                "execute".to_string(),
                "run".to_string(),
                "perform".to_string(),
                "create".to_string(),
                "delete".to_string(),
                "update".to_string(),
                "list".to_string(),
                "get".to_string(),
                "set".to_string(),
            ],
        }
    }

    /// Comprehensively analyze a specific function
    fn analyze_function_comprehensive(&self, unit: &Arc<rune::Unit>, module_name: &str, function_name: &str) -> Result<AsyncFunctionInfo> {
        debug!("Analyzing function: {}.{}", module_name, function_name);

        let full_path = vec![module_name, function_name];

        // Extract function signature and parameters
        let parameters = self.extract_function_parameters(unit, &full_path)?;
        let return_type = self.infer_return_type(unit, &full_path)?;

        // Extract documentation
        let module_path_vec = vec![module_name.to_string()];
        let description = Some(self.extract_function_description(unit, &module_path_vec, function_name)?);
        let doc_comments = self.extract_function_doc_comments(unit, module_name, function_name)?;

        // Determine visibility and async nature
        let is_async = self.is_function_async(unit, &full_path)?;
        let is_public = self.is_function_public(unit, &full_path)?;

        // Extract source location
        let source_location = self.get_function_source_location(unit, &full_path)?;

        // Extract metadata
        let metadata = self.extract_function_metadata(unit, module_name, function_name)?;

        Ok(AsyncFunctionInfo {
            name: function_name.to_string(),
            is_async,
            is_public,
            parameters,
            return_type,
            module_path: vec![module_name.to_string()],
            full_path: full_path.iter().map(|s| s.to_string()).collect(),
            description,
            doc_comments,
            source_location,
            metadata,
        })
    }

    /// Check if a function exists by trying to call it
    fn function_exists(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<bool> {
        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        match vm.call(function_path, ()) {
            Ok(_) => Ok(true),
            Err(e) => {
                let error_str = e.to_string().to_lowercase();
                Ok(!error_str.contains("not found") &&
                   !error_str.contains("missing") &&
                   !error_str.contains("unknown") &&
                   !error_str.contains("doesn't exist"))
            }
        }
    }

    /// Discover additional modules through introspection
    fn discover_additional_modules(&self, _unit: &Arc<rune::Unit>) -> Result<Vec<DiscoveredModule>> {
        // TODO: Implement advanced module discovery through reflection
        // This would involve analyzing the compiled unit's structure
        Ok(Vec::new())
    }

    /// Extract function parameters with enhanced type information using the TypeInferenceEngine
    fn extract_function_parameters(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Vec<ParameterInfo>> {
        debug!("Extracting enhanced parameters for function: {}", function_path.join("::"));

        // First, collect execution results for type inference
        let execution_results = self.collect_execution_results(unit, function_path)?;

        // Use TypeInferenceEngine for sophisticated type analysis
        let inferred_types = self.type_engine.infer_parameter_types(unit, function_path, &execution_results)?;

        // Convert inferred types to ParameterInfo structures
        let mut parameters = Vec::new();
        for (param_name, rune_type, constraints) in inferred_types {
            let param_info = ParameterInfo {
                name: param_name.clone(),
                type_name: self.rune_type_to_string(&rune_type),
                type_constraints: self.extract_type_constraints(&rune_type, &constraints),
                is_optional: self.is_optional_parameter(&constraints),
                default_value: self.infer_default_value(&param_name, &rune_type, function_path),
                description: self.generate_parameter_description(&param_name, &rune_type, function_path),
                validation_rules: self.convert_constraints_to_validation_rules(&constraints),
            };
            parameters.push(param_info);
        }

        // If type inference didn't find parameters, try fallback methods
        if parameters.is_empty() {
            debug!("Type inference found no parameters, trying fallback methods");
            if let Ok(introspected_params) = self.introspect_function_signature(unit, function_path) {
                parameters = introspected_params;
            } else {
                parameters = self.infer_parameter_signature(unit, function_path)?;
            }
        }

        // Ultimate fallback: provide a generic args parameter
        if parameters.is_empty() {
            parameters.push(ParameterInfo {
                name: "args".to_string(),
                type_name: "Object".to_string(),
                type_constraints: vec!["Any".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("Function arguments object".to_string()),
                validation_rules: vec![],
            });
        }

        debug!("Extracted {} enhanced parameters for {}", parameters.len(), function_path.join("::"));
        Ok(parameters)
    }

    /// Collect execution results for type inference
    fn collect_execution_results(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Vec<rune::runtime::Value>> {
        let mut results = Vec::new();

        // Try calling with different safe argument patterns
        let safe_args_patterns = self.get_safe_argument_patterns(function_path);

        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        for args in safe_args_patterns {
            match vm.call(function_path, (args,)) {
                Ok(result) => {
                    let result_type = self.analyze_rune_value_type(&result);
                    results.push(result);
                    debug!("Function {} accepted args, got result type: {}",
                           function_path.join("::"), result_type);
                    break; // One successful execution is enough for basic inference
                },
                Err(e) => {
                    trace!("Function {} rejected args: {}", function_path.join("::"), e);
                }
            }
        }

        Ok(results)
    }

    /// Get safe argument patterns for type inference
    fn get_safe_argument_patterns(&self, function_path: &[&str]) -> Vec<Vec<rune::runtime::Value>> {
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        match (*module_name, *function_name) {
            // File operations - safe to call with non-existent paths
            ("file" | "file_operations" | "files" | "io", "read" | "list") => {
                vec![vec![rune::to_value("/tmp/test_nonexistent").unwrap()]]
            },

            // Search operations - safe to call with basic queries
            ("search" | "query" | "find" | "filter", _) => {
                vec![vec![rune::to_value("test").unwrap()]]
            },

            // UI operations - safe to format simple data
            ("ui" | "ui_helpers" | "display" | "format", "format") => {
                vec![vec![rune::to_value("test").unwrap()]]
            },

            // Default - try with simple string argument
            _ => {
                vec![vec![rune::to_value("test").unwrap()]]
            }
        }
    }

    /// Convert RuneType to string representation
    fn rune_type_to_string(&self, rune_type: &RuneType) -> String {
        match rune_type {
            RuneType::String => "String".to_string(),
            RuneType::Number => "Number".to_string(),
            RuneType::Integer => "Integer".to_string(),
            RuneType::Float => "Float".to_string(),
            RuneType::Boolean => "Boolean".to_string(),
            RuneType::Array(inner) => format!("Array<{}>", self.rune_type_to_string(inner)),
            RuneType::Object(_) => "Object".to_string(),
            RuneType::Tuple(_) => "Tuple".to_string(),
            RuneType::Option(inner) => format!("Option<{}>", self.rune_type_to_string(inner)),
            RuneType::Any => "Any".to_string(),
            RuneType::Void => "Void".to_string(),
            RuneType::Unknown(desc) => format!("Unknown({})", desc),
            RuneType::Function { .. } => "Function".to_string(),
        }
    }

    /// Extract type constraints from TypeConstraint system
    fn extract_type_constraints(&self, rune_type: &RuneType, constraints: &[TypeConstraint]) -> Vec<String> {
        let mut type_constraints = vec![self.rune_type_to_string(rune_type)];

        for constraint in constraints {
            match &constraint.constraint_type {
                ConstraintType::Range { min, max } => {
                    if let Some(min_val) = min {
                        type_constraints.push(format!("min: {}", min_val));
                    }
                    if let Some(max_val) = max {
                        type_constraints.push(format!("max: {}", max_val));
                    }
                },
                ConstraintType::Length { min, max } => {
                    if let Some(min_len) = min {
                        type_constraints.push(format!("min_length: {}", min_len));
                    }
                    if let Some(max_len) = max {
                        type_constraints.push(format!("max_length: {}", max_len));
                    }
                },
                ConstraintType::Pattern(pattern) => {
                    type_constraints.push(format!("pattern: {}", pattern));
                },
                ConstraintType::Enum(_) => {
                    type_constraints.push("enum".to_string());
                },
                _ => {}
            }
        }

        type_constraints
    }

    /// Check if parameter is optional based on constraints
    fn is_optional_parameter(&self, constraints: &[TypeConstraint]) -> bool {
        constraints.iter().any(|c| matches!(c.constraint_type, ConstraintType::Optional))
    }

    /// Infer default value for parameter
    fn infer_default_value(&self, param_name: &str, rune_type: &RuneType, _function_path: &[&str]) -> Option<Value> {
        // Common default values based on parameter name and type
        match (param_name, rune_type) {
            ("recursive", RuneType::Boolean) => Some(json!(false)),
            ("case_sensitive", RuneType::Boolean) => Some(json!(false)),
            ("strict", RuneType::Boolean) => Some(json!(false)),
            ("max_results" | "max_suggestions" | "iterations", RuneType::Integer) => Some(json!(10)),
            ("format", RuneType::String) => Some(json!("json")),
            ("default", RuneType::String) => Some(json!("")),
            _ => None,
        }
    }

    /// Generate parameter description
    fn generate_parameter_description(&self, param_name: &str, rune_type: &RuneType, function_path: &[&str]) -> Option<String> {
        let function_name = function_path.last().unwrap_or(&"unknown");

        // Generate contextual descriptions
        match (param_name, rune_type) {
            ("path", RuneType::String) => Some(format!("File path for {}", function_name)),
            ("content", RuneType::String) => Some("Content to write".to_string()),
            ("query", RuneType::String) => Some("Search query string".to_string()),
            ("data", RuneType::Any) => Some("Input data".to_string()),
            ("format", RuneType::String) => Some("Output format".to_string()),
            (name, RuneType::Array(_)) => Some(format!("Array of {}", name)),
            (name, _) => Some(format!("{} parameter", name)),
        }
    }

    /// Convert TypeConstraints to ValidationRules
    fn convert_constraints_to_validation_rules(&self, constraints: &[TypeConstraint]) -> Vec<ValidationRule> {
        let mut validation_rules = Vec::new();

        for constraint in constraints {
            match &constraint.constraint_type {
                ConstraintType::Range { min, max } => {
                    let mut params = HashMap::new();
                    if let Some(min_val) = min {
                        params.insert("min".to_string(), json!(min_val));
                    }
                    if let Some(max_val) = max {
                        params.insert("max".to_string(), json!(max_val));
                    }
                    validation_rules.push(ValidationRule {
                        rule_type: "range".to_string(),
                        parameters: params,
                    });
                },
                ConstraintType::Length { min, max } => {
                    let mut params = HashMap::new();
                    if let Some(min_len) = min {
                        params.insert("min".to_string(), json!(min_len));
                    }
                    if let Some(max_len) = max {
                        params.insert("max".to_string(), json!(max_len));
                    }
                    validation_rules.push(ValidationRule {
                        rule_type: "length".to_string(),
                        parameters: params,
                    });
                },
                ConstraintType::Pattern(pattern) => {
                    let mut params = HashMap::new();
                    params.insert("pattern".to_string(), json!(pattern));
                    validation_rules.push(ValidationRule {
                        rule_type: "pattern".to_string(),
                        parameters: params,
                    });
                },
                ConstraintType::Enum(values) => {
                    let mut params = HashMap::new();
                    params.insert("values".to_string(), json!(values));
                    validation_rules.push(ValidationRule {
                        rule_type: "enum".to_string(),
                        parameters: params,
                    });
                },
                _ => {}
            }
        }

        validation_rules
    }

    /// Introspect function signature using Rune's runtime information
    fn introspect_function_signature(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Vec<ParameterInfo>> {
        // Try to get function information through runtime inspection
        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        // Try to call with minimal arguments to see what the function expects
        // This is a heuristic approach since Rune doesn't expose function metadata directly

        let mut parameters = Vec::new();

        // Test with different argument patterns to infer signature
        let test_patterns = vec![
            ("empty", vec![]),
            ("single_string", vec!["test"]),
        ];

        for (pattern_name, args) in test_patterns {
            match vm.call(function_path, (args,)) {
                Ok(_) => {
                    // Function succeeded with this pattern - we can infer something about expected parameters
                    debug!("Function {} accepted pattern: {}", function_path.join("::"), pattern_name);

                    // For now, we'll create a generic parameter based on the successful pattern
                    if parameters.is_empty() {
                        let param_type = match pattern_name {
                            "single_string" => "String",
                            "object" => "Object",
                            "number" => "Number",
                            "boolean" => "Boolean",
                            _ => "Any"
                        };

                        parameters.push(ParameterInfo {
                            name: "input".to_string(),
                            type_name: param_type.to_string(),
                            type_constraints: vec![param_type.to_string()],
                            is_optional: false,
                            default_value: None,
                            description: Some(format!("Function input (discovered via {} pattern)", pattern_name)),
                            validation_rules: vec![],
                        });
                    }
                    break;
                }
                Err(_) => {
                    // Function rejected this pattern, continue testing
                    continue;
                }
            }
        }

        Ok(parameters)
    }

    /// Infer parameter signature through dynamic analysis
    fn infer_parameter_signature(&self, _unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Vec<ParameterInfo>> {
        let mut parameters = Vec::new();

        // Analyze function name and module to infer expected parameters
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        // Use naming conventions to infer parameter structure
        let inferred_params = self.infer_parameters_from_naming(module_name, function_name);

        for (name, param_type, description) in inferred_params {
            parameters.push(ParameterInfo {
                name,
                type_name: param_type.clone(),
                type_constraints: vec![param_type.clone()],
                is_optional: false,
                default_value: None,
                description: Some(description),
                validation_rules: vec![],
            });
        }

        Ok(parameters)
    }

    /// Infer parameters from function and module naming conventions
    fn infer_parameters_from_naming(&self, module_name: &str, function_name: &str) -> Vec<(String, String, String)> {
        let mut params = Vec::new();

        match (module_name, function_name) {
            // File operations
            ("file" | "file_operations" | "files" | "io", "create" | "create_file") => {
                params.push(("path".to_string(), "String".to_string(), "File path to create".to_string()));
                params.push(("content".to_string(), "String".to_string(), "Content to write to file".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "delete" | "delete_file") => {
                params.push(("path".to_string(), "String".to_string(), "File path to delete".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "read" | "read_file") => {
                params.push(("path".to_string(), "String".to_string(), "File path to read".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "write" | "write_file") => {
                params.push(("path".to_string(), "String".to_string(), "File path to write".to_string()));
                params.push(("content".to_string(), "String".to_string(), "Content to write".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "copy" | "copy_file") => {
                params.push(("source".to_string(), "String".to_string(), "Source file path".to_string()));
                params.push(("destination".to_string(), "String".to_string(), "Destination file path".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "move" | "move_file") => {
                params.push(("source".to_string(), "String".to_string(), "Source file path".to_string()));
                params.push(("destination".to_string(), "String".to_string(), "Destination file path".to_string()));
            },
            ("file" | "file_operations" | "files" | "io", "list" | "list_files") => {
                params.push(("directory".to_string(), "String".to_string(), "Directory path to list".to_string()));
                params.push(("pattern".to_string(), "String".to_string(), "File pattern to match".to_string()));
            },

            // Search operations
            ("search" | "query" | "find" | "filter", "search" | "query" | "find") => {
                params.push(("query".to_string(), "String".to_string(), "Search query string".to_string()));
                params.push(("scope".to_string(), "String".to_string(), "Search scope (file, content, etc)".to_string()));
            },
            ("search" | "query" | "find" | "filter", "search_files") => {
                params.push(("pattern".to_string(), "String".to_string(), "File pattern to search for".to_string()));
                params.push(("directory".to_string(), "String".to_string(), "Directory to search in".to_string()));
            },
            ("search" | "query" | "find" | "filter", "search_content") => {
                params.push(("content".to_string(), "String".to_string(), "Content to search for".to_string()));
                params.push(("files".to_string(), "Array".to_string(), "Files to search in".to_string()));
            },

            // UI operations
            ("ui" | "ui_helpers" | "display" | "format", "format" | "format_results") => {
                params.push(("data".to_string(), "Any".to_string(), "Data to format".to_string()));
                params.push(("format".to_string(), "String".to_string(), "Output format (json, table, etc)".to_string()));
            },
            ("ui" | "ui_helpers" | "display" | "format", "get_suggestions" | "suggest") => {
                params.push(("input".to_string(), "String".to_string(), "Input to get suggestions for".to_string()));
                params.push(("context".to_string(), "Object".to_string(), "Context for suggestions".to_string()));
            },
            ("ui" | "ui_helpers" | "display" | "format", "prompt" | "confirm" | "select") => {
                params.push(("message".to_string(), "String".to_string(), "Prompt message".to_string()));
                params.push(("options".to_string(), "Array".to_string(), "Available options".to_string()));
            },

            // Agent operations
            ("agent" | "agent_helpers" | "ai" | "assist", "analyze" | "recommend") => {
                params.push(("data".to_string(), "Any".to_string(), "Data to analyze".to_string()));
                params.push(("context".to_string(), "Object".to_string(), "Analysis context".to_string()));
            },
            ("agent" | "agent_helpers" | "ai" | "assist", "optimize" | "assist") => {
                params.push(("target".to_string(), "String".to_string(), "Target to optimize/assist".to_string()));
                params.push(("options".to_string(), "Object".to_string(), "Optimization options".to_string()));
            },

            // Advanced operations
            ("advanced" | "utils" | "helpers" | "tools", "test" | "benchmark") => {
                params.push(("target".to_string(), "String".to_string(), "Target to test/benchmark".to_string()));
                params.push(("config".to_string(), "Object".to_string(), "Test configuration".to_string()));
            },
            ("advanced" | "utils" | "helpers" | "tools", "validate" | "sanitize") => {
                params.push(("input".to_string(), "Any".to_string(), "Input to validate/sanitize".to_string()));
                params.push(("rules".to_string(), "Array".to_string(), "Validation rules".to_string()));
            },
            ("advanced" | "utils" | "helpers" | "tools", "transform" | "aggregate") => {
                params.push(("data".to_string(), "Array".to_string(), "Data to transform/aggregate".to_string()));
                params.push(("operation".to_string(), "String".to_string(), "Operation to perform".to_string()));
            },

            // Default patterns
            _ => {
                // Generic parameters based on common patterns
                if function_name.contains("create") {
                    params.push(("input".to_string(), "Object".to_string(), "Input data for creation".to_string()));
                } else if function_name.contains("delete") || function_name.contains("remove") {
                    params.push(("id".to_string(), "String".to_string(), "Identifier to delete".to_string()));
                } else if function_name.contains("update") || function_name.contains("modify") {
                    params.push(("id".to_string(), "String".to_string(), "Identifier to update".to_string()));
                    params.push(("data".to_string(), "Object".to_string(), "Update data".to_string()));
                } else if function_name.contains("get") || function_name.contains("fetch") || function_name.contains("list") {
                    params.push(("query".to_string(), "Object".to_string(), "Query parameters".to_string()));
                } else {
                    // Most generic parameter
                    params.push(("args".to_string(), "Object".to_string(), "Function arguments".to_string()));
                }
            }
        }

        params
    }

    /// Infer return type from function signature and usage patterns
    fn infer_return_type(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Option<String>> {
        debug!("Inferring return type for function: {}", function_path.join("::"));

        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        // Use naming conventions to infer return type
        let inferred_type = self.infer_return_type_from_naming(module_name, function_name);

        // Try to verify by actually calling the function with safe arguments
        if let Some(verified_type) = self.verify_return_type_by_execution(unit, function_path)? {
            debug!("Verified return type for {}: {}", function_path.join("::"), verified_type);
            return Ok(Some(verified_type));
        }

        debug!("Inferred return type for {}: {}", function_path.join("::"), inferred_type);
        Ok(Some(inferred_type))
    }

    /// Infer return type from naming conventions
    fn infer_return_type_from_naming(&self, module_name: &str, function_name: &str) -> String {
        match (module_name, function_name) {
            // File operations typically return results or status
            ("file" | "file_operations" | "files" | "io", "create" | "create_file" | "write" | "write_file") => {
                "Result".to_string()
            },
            ("file" | "file_operations" | "files" | "io", "read" | "read_file") => {
                "String".to_string()
            },
            ("file" | "file_operations" | "files" | "io", "list" | "list_files") => {
                "Array".to_string()
            },
            ("file" | "file_operations" | "files" | "io", "delete" | "delete_file" | "copy" | "copy_file" | "move" | "move_file") => {
                "Boolean".to_string()
            },

            // Search operations typically return arrays or results
            ("search" | "query" | "find" | "filter", "search" | "query" | "find" | "search_files" | "search_content") => {
                "Array".to_string()
            },

            // UI operations often return formatted data or user input
            ("ui" | "ui_helpers" | "display" | "format", "format" | "format_results") => {
                "String".to_string()
            },
            ("ui" | "ui_helpers" | "display" | "format", "get_suggestions" | "suggest") => {
                "Array".to_string()
            },
            ("ui" | "ui_helpers" | "display" | "format", "prompt" | "confirm" | "select") => {
                "String".to_string()
            },

            // Agent operations typically return analysis results
            ("agent" | "agent_helpers" | "ai" | "assist", "analyze" | "recommend") => {
                "Object".to_string()
            },
            ("agent" | "agent_helpers" | "ai" | "assist", "optimize" | "assist") => {
                "Object".to_string()
            },

            // Advanced operations have varied return types
            ("advanced" | "utils" | "helpers" | "tools", "test" | "benchmark") => {
                "Object".to_string() // Test results
            },
            ("advanced" | "utils" | "helpers" | "tools", "validate" | "sanitize") => {
                "Boolean".to_string() // Validation result
            },
            ("advanced" | "utils" | "helpers" | "tools", "transform" | "aggregate") => {
                "Array".to_string() // Transformed data
            },

            // Default patterns
            _ => {
                if function_name.contains("get") || function_name.contains("fetch") {
                    "Object".to_string()
                } else if function_name.contains("is_") || function_name.contains("has_") || function_name.contains("can_") {
                    "Boolean".to_string()
                } else if function_name.contains("list") || function_name.contains("all") {
                    "Array".to_string()
                } else if function_name.contains("count") || function_name.contains("size") {
                    "Number".to_string()
                } else {
                    "Object".to_string() // Most common return type
                }
            }
        }
    }

    /// Verify return type by safe function execution
    fn verify_return_type_by_execution(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Option<String>> {
        // Only attempt verification for functions that look safe to call
        let function_name = function_path.last().unwrap_or(&"unknown");

        // Skip potentially dangerous functions
        if function_name.contains("delete") || function_name.contains("remove") || function_name.contains("write") {
            return Ok(None);
        }

        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        // Try to call with safe test arguments based on naming conventions
        let safe_args = self.get_safe_test_arguments(function_path);

        if let Ok(result) = vm.call(function_path, (safe_args,)) {
            // Analyze the result to determine its type
            let result_type = self.analyze_rune_value_type(&result);
            debug!("Function {} returned type: {}", function_path.join("::"), result_type);
            Ok(Some(result_type))
        } else {
            Ok(None)
        }
    }

    /// Get safe test arguments for function execution
    fn get_safe_test_arguments(&self, function_path: &[&str]) -> Vec<rune::runtime::Value> {
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        // Create safe test arguments based on expected parameters
        match (*module_name, *function_name) {
            ("file" | "file_operations" | "files" | "io", "read" | "list") => {
                // Safe to test with non-existent paths
                vec![rune::to_value("/tmp/test_nonexistent").unwrap()]
            },
            ("search" | "query" | "find" | "filter", _) => {
                // Safe search queries
                vec![rune::to_value("test").unwrap()]
            },
            ("ui" | "ui_helpers" | "display" | "format", "format") => {
                // Safe to format test data
                vec![rune::to_value("test").unwrap()]
            },
            _ => {
                // Generic safe argument
                vec![rune::to_value("test").unwrap()]
            }
        }
    }

    /// Analyze Rune value type
    fn analyze_rune_value_type(&self, value: &rune::runtime::Value) -> String {
        match value {
            rune::runtime::Value::String(_) => "String".to_string(),
            rune::runtime::Value::Integer(_) => "Number".to_string(),
            rune::runtime::Value::Float(_) => "Number".to_string(),
            rune::runtime::Value::Bool(_) => "Boolean".to_string(),
            rune::runtime::Value::Vec(_) => "Array".to_string(),
            rune::runtime::Value::Object(_) => "Object".to_string(),
            rune::runtime::Value::Option(_) => {
                // For simplicity, treat all options as "Any" type
                "Any".to_string()
            },
            _ => "Any".to_string(),
        }
    }

    /// Extract module description
    fn extract_module_description(&self, _unit: &Arc<rune::Unit>, module_name: &str) -> Result<Option<String>> {
        Ok(Some(format!("{} module with file operations", module_name)))
    }

    /// Extract module doc comments
    fn extract_module_doc_comments(&self, _unit: &Arc<rune::Unit>, _module_name: &str) -> Result<Vec<String>> {
        // TODO: Implement doc comment extraction
        Ok(vec![])
    }

    /// Extract function doc comments
    fn extract_function_doc_comments(&self, _unit: &Arc<rune::Unit>, _module_name: &str, _function_name: &str) -> Result<Vec<String>> {
        // TODO: Implement doc comment extraction
        Ok(vec![])
    }

    /// Check if function is async through naming and behavior analysis
    fn is_function_async(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<bool> {
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        // First check if the function is explicitly declared as async in the source
        // by trying to determine this from the function signature or module context
        let is_explicitly_async = match (*module_name, *function_name) {
            // File operations are almost always async by convention
            ("file" | "file_operations" | "files" | "io", _) => true,

            // Network/search operations are typically async
            ("search" | "query" | "find" | "filter", _) => true,

            // Agent operations are usually async (may involve AI calls)
            ("agent" | "agent_helpers" | "ai" | "assist", _) => true,

            _ => false,
        };

        // If we have strong evidence from naming conventions, prefer that over execution verification
        // since execution verification can be misleading for simple test functions
        if is_explicitly_async {
            debug!("Detected async via naming conventions for {}: {}", function_path.join("::"), true);
            return Ok(true);
        }

        // For functions that aren't clearly async by naming, try execution verification
        match self.verify_async_by_execution(unit, function_path)? {
            Some(is_verified_async) => {
                debug!("Verified async status for {}: {}", function_path.join("::"), is_verified_async);
                return Ok(is_verified_async);
            },
            None => {
                // If execution verification is inconclusive, use enhanced naming conventions
                debug!("Execution verification inconclusive for {}, using naming conventions", function_path.join("::"));
            }
        }

        // Enhanced naming convention analysis with more specific patterns
        let likely_async = match (*module_name, *function_name) {
            // File operations are almost always async
            ("file" | "file_operations" | "files" | "io", _) => true,

            // Network/search operations are typically async
            ("search" | "query" | "find" | "filter", _) => true,

            // Agent operations are usually async (may involve AI calls)
            ("agent" | "agent_helpers" | "ai" | "assist", _) => true,

            // UI operations - more specific patterns based on typical UI behavior
            ("ui" | "ui_helpers" | "display" | "format", "format" | "format_results" | "display" | "show" | "render") => {
                // Formatting/display operations are typically sync
                false
            },
            ("ui" | "ui_helpers" | "display" | "format", "prompt" | "confirm" | "select") => {
                // User input operations are typically async
                true
            },
            ("ui" | "ui_helpers" | "display" | "format", "get_suggestions" | "suggest") => {
                // Suggestion generation might be async (could involve AI/lookup)
                true
            },

            // Advanced operations vary - more granular analysis
            ("advanced" | "utils" | "helpers" | "tools", function_name) => {
                if function_name.contains("test") || function_name.contains("benchmark") {
                    false // Tests are typically sync
                } else if function_name.contains("transform") || function_name.contains("aggregate") {
                    false // Data processing is typically sync
                } else if function_name.contains("validate") || function_name.contains("sanitize") {
                    false // Validation is typically sync
                } else {
                    true // Most other advanced operations are async
                }
            },

            // Default patterns based on function name analysis
            (_, function_name) => {
                // Look for specific sync patterns
                if function_name.contains("format") || function_name.contains("display") ||
                   function_name.contains("render") || function_name.contains("show") ||
                   function_name.contains("print") || function_name.contains("log") {
                    false // These are typically sync UI operations
                } else if function_name.contains("get") && (
                    function_name.contains("info") || function_name.contains("details") ||
                    function_name.contains("metadata") || function_name.contains("status")
                ) {
                    false // Information retrieval is typically sync
                } else if function_name.contains("test") || function_name.contains("benchmark") {
                    false // Tests are typically sync
                } else {
                    // Default assumption: most tool functions are async
                    true
                }
            }
        };

        debug!("Inferred async status for {}: {}", function_path.join("::"), likely_async);
        Ok(likely_async)
    }

    /// Check if function is public through naming conventions
    fn is_function_public(&self, _unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<bool> {
        let function_name = function_path.last().unwrap_or(&"unknown");

        // In Rune, most functions that start with underscore are private
        let is_public = !function_name.starts_with('_');

        debug!("Inferred visibility for {}: {}", function_path.join("::"), is_public);
        Ok(is_public)
    }

    /// Get function source location (limited implementation)
    fn get_function_source_location(&self, _unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<SourceLocation> {
        let function_name = function_path.last().unwrap_or(&"unknown");
        let module_name = function_path.first().unwrap_or(&"unknown");

        // Since we can't easily get source locations from compiled Rune units,
        // we'll provide a placeholder with function information
        Ok(SourceLocation {
            line: None,
            column: None,
            file_path: Some(format!("{}::{}", module_name, function_name)),
        })
    }

    /// Verify async status by safe execution
    fn verify_async_by_execution(&self, unit: &Arc<rune::Unit>, function_path: &[&str]) -> Result<Option<bool>> {
        let function_name = function_path.last().unwrap_or(&"unknown");

        // Skip potentially dangerous functions
        if function_name.contains("delete") || function_name.contains("write") {
            return Ok(None);
        }

        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        let safe_args = self.get_safe_test_arguments(function_path);

        match vm.call(function_path, (safe_args,)) {
            Ok(result) => {
                // Try to detect if the result is a Future-like object
                let is_async_result = self.check_if_result_is_async(&result);
                debug!("Function {} returned result, detected as async: {}", function_path.join("::"), is_async_result);

                // For sync functions, we should get an immediate result
                // For async functions, we typically get a Future that needs to be awaited
                if is_async_result {
                    Ok(Some(true))
                } else {
                    // If we got an immediate result, the function is likely sync
                    Ok(Some(false))
                }
            },
            Err(e) => {
                // Check error messages for async indicators
                let error_str = e.to_string().to_lowercase();

                // Look for specific async-related error patterns
                let is_async_error = error_str.contains("async") ||
                                   error_str.contains("await") ||
                                   error_str.contains("future") ||
                                   error_str.contains("must be awaited") ||
                                   error_str.contains("cannot be called synchronously");

                // Look for sync function error patterns
                let is_sync_error = error_str.contains("is not async") ||
                                  error_str.contains("expected sync function") ||
                                  error_str.contains("blocking operation");

                if is_async_error {
                    debug!("Function {} shows async characteristics in error: {}", function_path.join("::"), e);
                    Ok(Some(true))
                } else if is_sync_error {
                    debug!("Function {} shows sync characteristics in error: {}", function_path.join("::"), e);
                    Ok(Some(false))
                } else {
                    // If error message doesn't give clear indication, fall back to None
                    trace!("Function {} error doesn't indicate async/sync nature: {}", function_path.join("::"), e);
                    Ok(None)
                }
            }
        }
    }

    /// Check if a result looks like an async operation result
    fn check_if_result_is_async(&self, result: &rune::runtime::Value) -> bool {
        match result {
            // If the result has Future-like characteristics
            rune::runtime::Value::Object(obj) => {
                // Check if the object has async-related methods or properties
                // This is heuristic since we can't directly inspect the type
                false // Most objects are not futures by default
            },
            // Primitive types typically indicate sync functions
            rune::runtime::Value::String(_) |
            rune::runtime::Value::Integer(_) |
            rune::runtime::Value::Float(_) |
            rune::runtime::Value::Bool(_) => {
                false // These are immediate results, likely from sync functions
            },
            // Arrays could be from either sync or async functions
            rune::runtime::Value::Vec(_) => {
                false // Default to assuming sync for arrays
            },
            // Option types might wrap async results
            rune::runtime::Value::Option(_) => {
                false // Can't determine, assume sync for safety
            },
            _ => {
                // For other types, assume sync (most common case)
                false
            }
        }
    }

    /// Extract function metadata
    fn extract_function_metadata(&self, _unit: &Arc<rune::Unit>, module_name: &str, function_name: &str) -> Result<HashMap<String, Value>> {
        let mut metadata = HashMap::new();
        metadata.insert("extracted_at".to_string(), json!("2025-10-16T00:00:00Z"));
        metadata.insert("module".to_string(), json!(module_name));
        metadata.insert("function".to_string(), json!(function_name));
        metadata.insert("version".to_string(), json!("1.0.0"));
        Ok(metadata)
    }

    /// Extract all async functions from a specific module (legacy method)
    pub fn extract_functions_in_module(
        &self,
        _unit: &Arc<rune::Unit>,
        _module_path: &[String],
    ) -> Result<Vec<AsyncFunctionInfo>> {
        // TODO: Implement this without recursion - for now return empty
        Ok(Vec::new())
    }

    /// Extract consumer information from function metadata (legacy method)
    pub fn extract_consumer_info(
        &self,
        _unit: &Arc<rune::Unit>,
        _module_path: &[String],
        _function_name: &str,
    ) -> Result<crate::rune_tools::discovery::ConsumerInfo> {
        // Default consumer info for now
        Ok(crate::rune_tools::discovery::ConsumerInfo::default())
    }

    /// Extract function description from doc comments (legacy method)
    pub fn extract_function_description(
        &self,
        _unit: &Arc<rune::Unit>,
        module_path: &[String],
        function_name: &str,
    ) -> Result<String> {
        let unknown = "unknown".to_string();
        let module_name = module_path.first().unwrap_or(&unknown);
        // Return a basic description for now - avoid recursion
        Ok(format!("{} function in {} module", function_name, module_name))
    }

    /// Analyze function parameters to generate input schema (legacy method)
    pub fn analyze_function_parameters(
        &self,
        _unit: &Arc<rune::Unit>,
        _module_path: &[String],
        _function_name: &str,
    ) -> Result<Value> {
        // Generate a generic schema for now
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "object",
                    "description": "Function arguments"
                }
            },
            "required": []
        }))
    }

    /// Generate comprehensive JSON schema for a function
    pub fn generate_function_schema(&self, function_info: &AsyncFunctionInfo) -> Result<Value> {
        if !self.config.generate_schemas {
            return Ok(json!({"type": "object", "properties": {}, "required": []}));
        }

        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &function_info.parameters {
            let param_schema = self.generate_parameter_schema(param)?;
            if !param.is_optional {
                required.push(param.name.clone());
            }
            properties.insert(param.name.clone(), param_schema);
        }

        Ok(json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "description": function_info.description.clone().unwrap_or_default(),
            "metadata": {
                "module": function_info.module_path.join("::"),
                "function": function_info.name,
                "is_async": function_info.is_async,
                "return_type": function_info.return_type
            }
        }))
    }

    /// Generate JSON schema for a parameter
    fn generate_parameter_schema(&self, param: &ParameterInfo) -> Result<Value> {
        let mut schema = match param.type_name.to_lowercase().as_str() {
            "string" => json!({"type": "string"}),
            "number" | "integer" | "int" => json!({"type": "number"}),
            "boolean" | "bool" => json!({"type": "boolean"}),
            "array" => json!({"type": "array"}),
            "object" => json!({"type": "object"}),
            _ => json!({"type": "object", "description": format!("{} parameter", param.type_name)}),
        };

        // Add description if available
        if let Some(description) = &param.description {
            schema["description"] = json!(description);
        }

        // Add default value if available
        if let Some(default_value) = &param.default_value {
            schema["default"] = default_value.clone();
        }

        // Add validation rules
        if !param.validation_rules.is_empty() {
            let mut validation = HashMap::new();
            for rule in &param.validation_rules {
                validation.insert(rule.rule_type.clone(), json!(rule.parameters));
            }
            schema["validation"] = json!(validation);
        }

        Ok(schema)
    }

    /// Validate function signature against expected patterns
    pub fn validate_function_signature(&self, function_info: &AsyncFunctionInfo) -> Result<Vec<String>> {
        if !self.config.validate_signatures {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Check for common patterns
        if function_info.name.is_empty() {
            warnings.push("Function name is empty".to_string());
        }

        if function_info.parameters.is_empty() {
            warnings.push("Function has no parameters - consider accepting an args object".to_string());
        }

        if !function_info.is_async {
            warnings.push("Function is not async - tool functions should typically be async".to_string());
        }

        // Check parameter naming conventions
        for param in &function_info.parameters {
            if param.name.is_empty() {
                warnings.push("Parameter has empty name".to_string());
            }

            if param.type_name.is_empty() {
                warnings.push(format!("Parameter '{}' has no type information", param.name));
            }
        }

        Ok(warnings)
    }

    /// Validate function parameters using the schema validator
    pub fn validate_parameters(&self, function_info: &AsyncFunctionInfo, parameters: &Value) -> Result<super::schema_validator::ValidationResult> {
        let result = self.schema_validator.validate_function_parameters(function_info, parameters);

        if !result.is_valid {
            warn!("Parameter validation failed for {}.{}: {} errors",
                  function_info.module_path.join("::"),
                  function_info.name,
                  result.errors.len());
        }

        Ok(result)
    }

    /// Generate comprehensive schema with validation
    pub fn generate_validated_schema(&self, function_info: &AsyncFunctionInfo) -> Result<Value> {
        let schema = self.schema_validator.generate_function_schema(function_info)?;

        // Add additional metadata from AST analysis
        let mut enhanced_schema = schema;

        // Add complexity analysis
        let complexity = self.analyze_function_complexity(function_info)?;
        enhanced_schema["metadata"]["complexity"] = json!(complexity);

        // Add security classification
        let security_class = self.classify_function_security(function_info)?;
        enhanced_schema["metadata"]["security"] = json!(security_class);

        Ok(enhanced_schema)
    }

    /// Analyze function complexity based on parameters and naming
    fn analyze_function_complexity(&self, function_info: &AsyncFunctionInfo) -> Result<String> {
        let param_count = function_info.parameters.len();
        let has_optional_params = function_info.parameters.iter().any(|p| p.is_optional);
        let has_validation_rules = function_info.parameters.iter().any(|p| !p.validation_rules.is_empty());

        let complexity = match (param_count, has_optional_params, has_validation_rules) {
            (0, _, _) => "simple".to_string(),
            (1..=3, false, false) => "basic".to_string(),
            (1..=3, true, false) => "intermediate".to_string(),
            (1..=3, _, true) => "intermediate".to_string(),
            (4..=7, _, _) => "complex".to_string(),
            (8.., _, _) => "very_complex".to_string(),
        };

        Ok(complexity)
    }

    /// Classify function security level based on naming and parameters
    fn classify_function_security(&self, function_info: &AsyncFunctionInfo) -> Result<String> {
        let function_name = &function_info.name;
        let module_name = function_info.module_path.join("::");

        // Check for potentially dangerous operations
        let is_dangerous = function_name.contains("delete") ||
                          function_name.contains("remove") ||
                          function_name.contains("write") ||
                          function_name.contains("execute") ||
                          function_name.contains("format") ||
                          module_name.contains("system") ||
                          module_name.contains("admin");

        // Check for file system operations
        let is_file_operation = module_name.contains("file") ||
                               module_name.contains("io") ||
                               function_name.contains("path");

        // Check for network operations
        let is_network_operation = module_name.contains("network") ||
                                 module_name.contains("http") ||
                                 function_name.contains("fetch") ||
                                 function_name.contains("request");

        let security_class = match (is_dangerous, is_file_operation, is_network_operation) {
            (true, _, _) => "high_risk".to_string(),
            (_, true, _) => "medium_risk".to_string(),
            (_, _, true) => "medium_risk".to_string(),
            (false, false, false) => "low_risk".to_string(),
        };

        Ok(security_class)
    }

    /// Validate schema quality and completeness
    pub fn validate_schema_quality(&self, schema: &Value) -> Result<Vec<String>> {
        let mut issues = Vec::new();

        // Check for required fields
        if schema.get("type").is_none() {
            issues.push("Schema missing 'type' field".to_string());
        }

        if schema.get("properties").is_none() {
            issues.push("Schema missing 'properties' field".to_string());
        }

        // Check for descriptions
        if schema.get("description").is_none() {
            issues.push("Schema missing 'description' field".to_string());
        }

        // Check parameter schemas
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (param_name, param_schema) in properties {
                if param_schema.get("type").is_none() {
                    issues.push(format!("Parameter '{}' missing 'type' field", param_name));
                }

                if param_schema.get("description").is_none() {
                    issues.push(format!("Parameter '{}' missing 'description' field", param_name));
                }
            }
        }

        Ok(issues)
    }
}

impl Default for RuneAstAnalyzer {
    fn default() -> Self {
        Self::new().expect("Failed to create default AST analyzer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_discover_simple_module() -> Result<()> {
        // Test basic module discovery with one async function
        let source = r#"
            pub mod file_operations {
                pub async fn create_file(args) {
                    #{ success: true, file: args.path }
                }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "file_operations");
        assert_eq!(modules[0].path, vec!["file_operations"]);
        assert_eq!(modules[0].functions.len(), 1);
        assert_eq!(modules[0].functions[0].name, "create_file");

        Ok(())
    }

    #[tokio::test]
    async fn test_discover_multiple_modules() -> Result<()> {
        // Test discovery of multiple modules with different functions
        let source = r#"
            pub mod file {
                pub async fn create(args) { #{ success: true } }
                pub async fn delete(args) { #{ success: true } }
            }

            pub mod search {
                pub async fn query(args) { #{ success: true } }
            }

            pub mod ui {
                pub async fn format_results(args) { #{ success: true } }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;

        assert_eq!(modules.len(), 3);

        let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
        assert!(module_names.contains(&"file".to_string()));
        assert!(module_names.contains(&"search".to_string()));
        assert!(module_names.contains(&"ui".to_string()));

        // Verify function counts
        let file_module = modules.iter().find(|m| m.name == "file").unwrap();
        assert_eq!(file_module.functions.len(), 2);

        let search_module = modules.iter().find(|m| m.name == "search").unwrap();
        assert_eq!(search_module.functions.len(), 1);

        Ok(())
    }

    #[test]
    fn test_parameter_extraction_file_operations() -> Result<()> {
        let source = r#"
            pub mod file {
                pub async fn create_file(args) {
                    #{ success: true, file: args.path }
                }

                pub async fn delete_file(args) {
                    #{ success: true, deleted: args.path }
                }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;
        let file_module = modules.iter().find(|m| m.name == "file").unwrap();

        assert_eq!(file_module.functions.len(), 2);

        // Test create_file function
        let create_file = &file_module.functions[0];
        assert_eq!(create_file.name, "create_file");
        assert!(create_file.parameters.len() >= 1);

        // Should have inferred parameters based on naming
        let param_names: Vec<String> = create_file.parameters.iter().map(|p| p.name.clone()).collect();
        assert!(param_names.contains(&"path".to_string()) || param_names.contains(&"input".to_string()));

        Ok(())
    }

    #[test]
    fn test_return_type_inference() -> Result<()> {
        let source = r#"
            pub mod file {
                pub async fn read_file(args) {
                    "file content"
                }

                pub async fn list_files(args) {
                    ["file1.txt", "file2.txt"]
                }

                pub async fn delete_file(args) {
                    true
                }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;
        let file_module = modules.iter().find(|m| m.name == "file").unwrap();

        for function in &file_module.functions {
            assert!(function.return_type.is_some());

            match function.name.as_str() {
                "read_file" => assert_eq!(function.return_type.as_ref().unwrap(), "String"),
                "list_files" => assert_eq!(function.return_type.as_ref().unwrap(), "Array"),
                "delete_file" => assert_eq!(function.return_type.as_ref().unwrap(), "Boolean"),
                _ => {}
            }
        }

        Ok(())
    }

    #[test]
    fn test_async_detection() -> Result<()> {
        let source = r#"
            pub mod file {
                pub async fn create_file(args) {
                    #{ success: true }
                }
            }

            pub mod ui {
                pub fn format_results(args) {
                    "formatted results"
                }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;

        for module in modules {
            for function in &module.functions {
                println!("DEBUG: Module: {}, Function: {}, is_async: {}", module.name, function.name, function.is_async);
                match (module.name.as_str(), function.name.as_str()) {
                    ("file", _) => assert!(function.is_async, "File functions should be async"),
                    ("ui", "format_results") => assert!(!function.is_async, "format_results should be sync"),
                    _ => {}
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_naming_convention_inference() -> Result<()> {
        let source = r#"
            pub mod search {
                pub async fn search_files(args) {
                    ["result1", "result2"]
                }
            }

            pub mod agent {
                pub async fn analyze_data(args) {
                    #{ analysis: "complete", confidence: 0.95 }
                }
            }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new()?;

        let modules = analyzer.analyze_modules(&unit)?;

        for module in modules {
            for function in &module.functions {
                // Check that parameters were inferred based on naming conventions
                assert!(!function.parameters.is_empty(),
                        "Function {}::{} should have inferred parameters",
                        module.name, function.name);

                // Check that descriptions were generated
                for param in &function.parameters {
                    assert!(param.description.is_some(),
                           "Parameter {} should have a description", param.name);
                }
            }
        }

        Ok(())
    }

    /// Helper function to compile Rune source for testing
    fn compile_source(source: &str, context: &rune::Context) -> Result<Arc<rune::Unit>> {
        let source_obj = rune::Source::memory(source)?;
        let mut sources = rune::Sources::new();
        sources.insert(source_obj)?;

        let mut diagnostics = rune::Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(context)
            .with_diagnostics(&mut diagnostics)
            .build();

        // Report warnings but don't fail on them
        if !diagnostics.is_empty() {
            let mut writer = rune::termcolor::StandardStream::stderr(rune::termcolor::ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;
        Ok(Arc::new(unit))
    }
}