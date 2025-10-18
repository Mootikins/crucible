/// JSON Schema generation from tool macro metadata
///
/// This module converts tool metadata (extracted from Rune macros) into
/// MCP-compliant JSON Schema format for tool parameter validation.

use serde_json::{json, Map, Value};

// Import types from tool_metadata_storage
use super::tool_metadata_storage::{ParameterMetadata, ToolMacroMetadata, TypeSpec};

// ============================================================================
// SCHEMA GENERATION
// ============================================================================

/// Generate MCP-compliant JSON schema from macro metadata
///
/// Produces a JSON Schema object in the format:
/// ```json
/// {
///   "type": "object",
///   "properties": {
///     "param1": { "type": "string" },
///     "param2": { "type": "number" }
///   },
///   "required": ["param1"]
/// }
/// ```
///
/// # Arguments
/// * `metadata` - Tool metadata containing parameter definitions
///
/// # Returns
/// A JSON Value representing the complete schema object
///
/// # Example
/// ```
/// use crucible_mcp::rune_tools::schema_generator::{
///     generate_schema, ToolMacroMetadata, ParameterMetadata, TypeSpec
/// };
///
/// let metadata = ToolMacroMetadata {
///     name: "create_note".to_string(),
///     description: "Creates a new note".to_string(),
///     parameters: vec![
///         ParameterMetadata {
///             name: "title".to_string(),
///             type_spec: TypeSpec::String,
///             is_optional: false,
///         },
///         ParameterMetadata {
///             name: "content".to_string(),
///             type_spec: TypeSpec::String,
///             is_optional: false,
///         },
///         ParameterMetadata {
///             name: "folder".to_string(),
///             type_spec: TypeSpec::String,
///             is_optional: true,
///         },
///     ],
/// };
///
/// let schema = generate_schema(&metadata);
/// // schema["required"] will be ["title", "content"]
/// // schema["properties"]["folder"] will exist but not be required
/// ```
pub fn generate_schema(metadata: &ToolMacroMetadata) -> Value {
    let properties = generate_properties(&metadata.parameters);
    let required = generate_required(&metadata.parameters);

    json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

/// Convert TypeSpec to JSON Schema type object
///
/// Maps Rune type specifications to their JSON Schema equivalents:
/// - TypeSpec::String → {"type": "string"}
/// - TypeSpec::Number → {"type": "number"}
/// - TypeSpec::Boolean → {"type": "boolean"}
/// - TypeSpec::Array(inner) → {"type": "array", "items": <inner_schema>}
/// - TypeSpec::Object → {"type": "object"}
///
/// # Arguments
/// * `spec` - The type specification to convert
///
/// # Returns
/// A JSON Value representing the type in JSON Schema format
fn type_spec_to_json_schema(spec: &TypeSpec) -> Value {
    match spec {
        TypeSpec::String => json!({"type": "string"}),
        TypeSpec::Number => json!({"type": "number"}),
        TypeSpec::Boolean => json!({"type": "boolean"}),
        TypeSpec::Array(inner) => {
            json!({
                "type": "array",
                "items": type_spec_to_json_schema(inner)
            })
        }
        TypeSpec::Object => json!({"type": "object"}),
    }
}

/// Generate properties object for schema
///
/// Creates a mapping of parameter names to their JSON Schema type definitions.
///
/// # Arguments
/// * `params` - List of parameter metadata
///
/// # Returns
/// A Map where keys are parameter names and values are JSON Schema type objects
fn generate_properties(params: &[ParameterMetadata]) -> Map<String, Value> {
    params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                type_spec_to_json_schema(&param.type_spec),
            )
        })
        .collect()
}

/// Generate required array for schema
///
/// Filters parameters to include only those that are not optional.
///
/// # Arguments
/// * `params` - List of parameter metadata
///
/// # Returns
/// A vector of parameter names that are required (is_optional == false)
fn generate_required(params: &[ParameterMetadata]) -> Vec<String> {
    params
        .iter()
        .filter(|param| !param.is_optional)
        .map(|param| param.name.clone())
        .collect()
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_spec_to_json_schema_string() {
        let spec = TypeSpec::String;
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(result, json!({"type": "string"}));
    }

    #[test]
    fn test_type_spec_to_json_schema_number() {
        let spec = TypeSpec::Number;
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(result, json!({"type": "number"}));
    }

    #[test]
    fn test_type_spec_to_json_schema_boolean() {
        let spec = TypeSpec::Boolean;
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(result, json!({"type": "boolean"}));
    }

    #[test]
    fn test_type_spec_to_json_schema_array() {
        let spec = TypeSpec::Array(Box::new(TypeSpec::String));
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(
            result,
            json!({
                "type": "array",
                "items": {"type": "string"}
            })
        );
    }

    #[test]
    fn test_type_spec_to_json_schema_nested_array() {
        let spec = TypeSpec::Array(Box::new(TypeSpec::Array(Box::new(TypeSpec::Number))));
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(
            result,
            json!({
                "type": "array",
                "items": {
                    "type": "array",
                    "items": {"type": "number"}
                }
            })
        );
    }

    #[test]
    fn test_type_spec_to_json_schema_object() {
        let spec = TypeSpec::Object;
        let result = type_spec_to_json_schema(&spec);
        assert_eq!(result, json!({"type": "object"}));
    }

    #[test]
    fn test_generate_properties_empty() {
        let params: Vec<ParameterMetadata> = vec![];
        let result = generate_properties(&params);
        assert!(result.is_empty());
    }

    #[test]
    fn test_generate_properties_basic() {
        let params = vec![
            ParameterMetadata {
                name: "title".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "count".to_string(),
                type_spec: TypeSpec::Number,
                is_optional: false,
            },
        ];
        let result = generate_properties(&params);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("title"), Some(&json!({"type": "string"})));
        assert_eq!(result.get("count"), Some(&json!({"type": "number"})));
    }

    #[test]
    fn test_generate_required_empty() {
        let params: Vec<ParameterMetadata> = vec![];
        let result = generate_required(&params);
        assert!(result.is_empty());
    }

    #[test]
    fn test_generate_required_all_required() {
        let params = vec![
            ParameterMetadata {
                name: "title".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "content".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
        ];
        let result = generate_required(&params);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"title".to_string()));
        assert!(result.contains(&"content".to_string()));
    }

    #[test]
    fn test_generate_required_mixed() {
        let params = vec![
            ParameterMetadata {
                name: "title".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "folder".to_string(),
                type_spec: TypeSpec::String,
                is_optional: true,
            },
        ];
        let result = generate_required(&params);

        assert_eq!(result.len(), 1);
        assert!(result.contains(&"title".to_string()));
        assert!(!result.contains(&"folder".to_string()));
    }

    #[test]
    fn test_generate_required_all_optional() {
        let params = vec![
            ParameterMetadata {
                name: "folder".to_string(),
                type_spec: TypeSpec::String,
                is_optional: true,
            },
            ParameterMetadata {
                name: "tags".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::String)),
                is_optional: true,
            },
        ];
        let result = generate_required(&params);
        assert!(result.is_empty());
    }

    #[test]
    fn test_generate_schema_empty() {
        let metadata = ToolMacroMetadata {
            name: "empty_tool".to_string(),
            description: "A tool with no parameters".to_string(),
            parameters: vec![],
        };

        let schema = generate_schema(&metadata);

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"], json!({}));
        assert_eq!(schema["required"], json!([]));
    }

    #[test]
    fn test_generate_schema_with_optional() {
        let metadata = ToolMacroMetadata {
            name: "create_note".to_string(),
            description: "Creates a new note".to_string(),
            parameters: vec![
                ParameterMetadata {
                    name: "title".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: false,
                },
                ParameterMetadata {
                    name: "content".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: false,
                },
                ParameterMetadata {
                    name: "folder".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: true,
                },
            ],
        };

        let schema = generate_schema(&metadata);

        // Verify structure
        assert_eq!(schema["type"], "object");

        // Verify properties includes all parameters
        let properties = schema["properties"].as_object().unwrap();
        assert_eq!(properties.len(), 3);
        assert!(properties.contains_key("title"));
        assert!(properties.contains_key("content"));
        assert!(properties.contains_key("folder"));

        // Verify required excludes optional parameters
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("title")));
        assert!(required.contains(&json!("content")));
        assert!(!required.contains(&json!("folder")));
    }

    #[test]
    fn test_generate_schema_complex_types() {
        let metadata = ToolMacroMetadata {
            name: "complex_tool".to_string(),
            description: "A tool with complex parameter types".to_string(),
            parameters: vec![
                ParameterMetadata {
                    name: "tags".to_string(),
                    type_spec: TypeSpec::Array(Box::new(TypeSpec::String)),
                    is_optional: false,
                },
                ParameterMetadata {
                    name: "metadata".to_string(),
                    type_spec: TypeSpec::Object,
                    is_optional: true,
                },
                ParameterMetadata {
                    name: "active".to_string(),
                    type_spec: TypeSpec::Boolean,
                    is_optional: false,
                },
            ],
        };

        let schema = generate_schema(&metadata);

        // Verify complex type mapping
        let properties = schema["properties"].as_object().unwrap();
        assert_eq!(
            properties.get("tags"),
            Some(&json!({
                "type": "array",
                "items": {"type": "string"}
            }))
        );
        assert_eq!(properties.get("metadata"), Some(&json!({"type": "object"})));
        assert_eq!(properties.get("active"), Some(&json!({"type": "boolean"})));

        // Verify required array
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("tags")));
        assert!(required.contains(&json!("active")));
        assert!(!required.contains(&json!("metadata")));
    }

    #[test]
    fn test_generate_schema_matches_mcp_format() {
        let metadata = ToolMacroMetadata {
            name: "create_note".to_string(),
            description: "Creates a note".to_string(),
            parameters: vec![
                ParameterMetadata {
                    name: "title".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: false,
                },
                ParameterMetadata {
                    name: "content".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: false,
                },
                ParameterMetadata {
                    name: "folder".to_string(),
                    type_spec: TypeSpec::String,
                    is_optional: true,
                },
            ],
        };

        let schema = generate_schema(&metadata);

        // This is the exact MCP format from the task specification
        let expected = json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "content": {"type": "string"},
                "folder": {"type": "string"}
            },
            "required": ["title", "content"]
        });

        assert_eq!(schema, expected);
    }
}
