//! Schema extraction from Luau type annotations
//!
//! Uses full_moon to parse Luau source and extract function signatures
//! to generate tool schemas for MCP/agent consumption.
//!
//! ## Example
//!
//! ```lua
//! -- Input: Luau with type annotations
//! function search(query: string, limit: number?): {SearchResult}
//!     return kb_search(query, limit or 10)
//! end
//! ```
//!
//! Extracts:
//! ```json
//! {
//!   "name": "search",
//!   "params": [
//!     { "name": "query", "type": "string", "required": true },
//!     { "name": "limit", "type": "number", "required": false }
//!   ],
//!   "returns": { "type": "array", "items": "SearchResult" }
//! }
//! ```

use crate::error::LuaError;
use crate::types::{LuaTool, ToolParam};
use full_moon::ast::{self, luau};
use full_moon::visitors::Visitor;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Extracted function signature with types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Function name
    pub name: String,
    /// Parameters with types
    pub params: Vec<TypedParam>,
    /// Return type (if annotated)
    pub return_type: Option<LuauType>,
    /// Doc comment if present
    pub description: Option<String>,
}

/// A parameter with type information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedParam {
    /// Parameter name
    pub name: String,
    /// Luau type
    pub type_info: LuauType,
    /// Whether optional (T? syntax)
    pub optional: bool,
}

/// Luau type representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LuauType {
    /// Primitive: string, number, boolean, nil
    Primitive { name: String },
    /// Optional: T?
    Optional { inner: Box<LuauType> },
    /// Array: {T}
    Array { element: Box<LuauType> },
    /// Table/Object: { key: Type, ... }
    Table { fields: Vec<(String, LuauType)> },
    /// Union: T | U
    Union { types: Vec<LuauType> },
    /// Named type reference
    Named { name: String },
    /// Function type
    Function {
        params: Vec<LuauType>,
        returns: Box<LuauType>,
    },
    /// Unknown/any
    Any,
}

impl LuauType {
    /// Convert to JSON Schema representation
    pub fn to_json_schema(&self) -> JsonValue {
        match self {
            LuauType::Primitive { name } => match name.as_str() {
                "string" => serde_json::json!({ "type": "string" }),
                "number" => serde_json::json!({ "type": "number" }),
                "boolean" => serde_json::json!({ "type": "boolean" }),
                "nil" => serde_json::json!({ "type": "null" }),
                _ => serde_json::json!({ "type": "string" }),
            },
            LuauType::Optional { inner } => {
                // Optional types pass through to the inner type's schema
                // The optionality is handled at the parameter level via "required"
                inner.to_json_schema()
            }
            LuauType::Array { element } => {
                serde_json::json!({
                    "type": "array",
                    "items": element.to_json_schema()
                })
            }
            LuauType::Table { fields } => {
                let properties: serde_json::Map<String, JsonValue> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json_schema()))
                    .collect();
                serde_json::json!({
                    "type": "object",
                    "properties": properties
                })
            }
            LuauType::Union { types } => {
                serde_json::json!({
                    "oneOf": types.iter().map(|t| t.to_json_schema()).collect::<Vec<_>>()
                })
            }
            LuauType::Named { name } => {
                serde_json::json!({ "$ref": format!("#/definitions/{}", name) })
            }
            LuauType::Function { .. } => {
                serde_json::json!({ "type": "function" })
            }
            LuauType::Any => {
                serde_json::json!({})
            }
        }
    }

    /// Check if this type is optional
    pub fn is_optional(&self) -> bool {
        matches!(self, LuauType::Optional { .. })
    }
}

/// Extract function signatures from Luau source
pub fn extract_signatures(source: &str) -> Result<Vec<FunctionSignature>, LuaError> {
    let ast = full_moon::parse(source)
        .map_err(|e| LuaError::InvalidTool(format!("Parse error: {:?}", e)))?;

    let mut extractor = SignatureExtractor::new(source);
    extractor.visit_ast(&ast);

    Ok(extractor.signatures)
}

/// Extract a single tool definition from source
pub fn extract_tool(source: &str, source_path: &str) -> Result<Option<LuaTool>, LuaError> {
    let signatures = extract_signatures(source)?;

    // Look for a function named "handler" or "main", or the first exported function
    let sig = signatures
        .iter()
        .find(|s| s.name == "handler" || s.name == "main")
        .or_else(|| signatures.first());

    let sig = match sig {
        Some(s) => s,
        None => return Ok(None),
    };

    // Convert to LuaTool
    let params = sig
        .params
        .iter()
        .map(|p| ToolParam {
            name: p.name.clone(),
            param_type: type_to_string(&p.type_info),
            description: String::new(), // Could extract from doc comments
            required: !p.optional,
            default: None,
        })
        .collect();

    Ok(Some(LuaTool {
        name: sig.name.clone(),
        description: sig.description.clone().unwrap_or_default(),
        params,
        source_path: source_path.to_string(),
        is_fennel: false,
    }))
}

/// Convert LuauType to simple string representation
pub fn type_to_string(ty: &LuauType) -> String {
    match ty {
        LuauType::Primitive { name } => name.clone(),
        LuauType::Optional { inner } => format!("{}?", type_to_string(inner)),
        LuauType::Array { element } => format!("array<{}>", type_to_string(element)),
        LuauType::Table { .. } => "object".to_string(),
        LuauType::Union { types } => types
            .iter()
            .map(type_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        LuauType::Named { name } => name.clone(),
        LuauType::Function { .. } => "function".to_string(),
        LuauType::Any => "any".to_string(),
    }
}

/// AST visitor that extracts function signatures
struct SignatureExtractor<'a> {
    #[allow(dead_code)]
    source: &'a str,
    signatures: Vec<FunctionSignature>,
    /// Pending doc comment for next function
    pending_doc: Option<String>,
}

impl<'a> SignatureExtractor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            signatures: Vec::new(),
            pending_doc: None,
        }
    }

    /// Parse a type annotation from full_moon AST
    fn parse_type_info(&self, type_info: &luau::TypeInfo) -> LuauType {
        use luau::TypeInfo;

        match type_info {
            TypeInfo::Basic(token) => {
                let name = token.to_string().trim().to_string();
                match name.as_str() {
                    "string" | "number" | "boolean" | "nil" => LuauType::Primitive { name },
                    "any" => LuauType::Any,
                    _ => LuauType::Named { name },
                }
            }
            TypeInfo::Optional { base, .. } => LuauType::Optional {
                inner: Box::new(self.parse_type_info(base)),
            },
            TypeInfo::Array { type_info, .. } => LuauType::Array {
                element: Box::new(self.parse_type_info(type_info)),
            },
            TypeInfo::Table { fields, .. } => {
                let parsed_fields: Vec<(String, LuauType)> = fields
                    .iter()
                    .filter_map(|field| {
                        if let luau::TypeFieldKey::Name(name) = field.key() {
                            Some((
                                name.to_string().trim().to_string(),
                                self.parse_type_info(field.value()),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                LuauType::Table {
                    fields: parsed_fields,
                }
            }
            TypeInfo::Union(type_union) => {
                let types: Vec<LuauType> = type_union
                    .types()
                    .iter()
                    .map(|t| self.parse_type_info(t))
                    .collect();
                LuauType::Union { types }
            }
            TypeInfo::Callback {
                arguments,
                return_type,
                ..
            } => {
                let params: Vec<LuauType> = arguments
                    .iter()
                    .map(|arg| self.parse_type_info(arg.type_info()))
                    .collect();
                let returns = Box::new(self.parse_type_info(return_type));
                LuauType::Function { params, returns }
            }
            _ => LuauType::Any,
        }
    }
}

impl<'a> Visitor for SignatureExtractor<'a> {
    fn visit_function_declaration(&mut self, node: &ast::FunctionDeclaration) {
        let name = node.name().to_string();
        let params = self.extract_params_with_types(node.body());

        // Extract return type if present
        let return_type = node
            .body()
            .return_type()
            .map(|rt| self.parse_type_info(rt.type_info()));

        self.signatures.push(FunctionSignature {
            name,
            params,
            return_type,
            description: self.pending_doc.take(),
        });
    }

    fn visit_local_function(&mut self, node: &ast::LocalFunction) {
        let name = node.name().to_string();
        let params = self.extract_params_with_types(node.body());

        let return_type = node
            .body()
            .return_type()
            .map(|rt| self.parse_type_info(rt.type_info()));

        self.signatures.push(FunctionSignature {
            name,
            params,
            return_type,
            description: self.pending_doc.take(),
        });
    }
}

impl<'a> SignatureExtractor<'a> {
    /// Extract parameters with their type annotations from a function body
    fn extract_params_with_types(&self, body: &ast::FunctionBody) -> Vec<TypedParam> {
        let params = body.parameters();
        let type_specifiers: Vec<_> = body.type_specifiers().collect();

        params
            .iter()
            .enumerate()
            .filter_map(|(i, param)| {
                match param {
                    ast::Parameter::Name(name_token) => {
                        let param_name = name_token.to_string().trim().to_string();

                        // Get the type specifier at this position (if any)
                        let (type_info, optional) = type_specifiers
                            .get(i)
                            .and_then(|opt_spec| opt_spec.as_ref())
                            .map(|spec| {
                                let ty = self.parse_type_info(spec.type_info());
                                let is_optional = ty.is_optional();
                                (ty, is_optional)
                            })
                            .unwrap_or((LuauType::Any, false));

                        Some(TypedParam {
                            name: param_name,
                            type_info,
                            optional,
                        })
                    }
                    ast::Parameter::Ellipsis(_) => None,
                    _ => None,
                }
            })
            .collect()
    }
}

/// Generate a JSON Schema for a tool's input parameters
pub fn generate_input_schema(tool: &LuaTool) -> JsonValue {
    let properties: serde_json::Map<String, JsonValue> = tool
        .params
        .iter()
        .map(|p| {
            let schema = match p.param_type.as_str() {
                "string" => serde_json::json!({ "type": "string", "description": p.description }),
                "number" => serde_json::json!({ "type": "number", "description": p.description }),
                "boolean" => serde_json::json!({ "type": "boolean", "description": p.description }),
                _ => serde_json::json!({ "type": "string", "description": p.description }),
            };
            (p.name.clone(), schema)
        })
        .collect();

    let required: Vec<String> = tool
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.clone())
        .collect();

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let source = r#"
            function handler(query, limit)
                return {}
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].name, "handler");
        assert_eq!(signatures[0].params.len(), 2);
    }

    #[test]
    fn test_extract_local_function() {
        let source = r#"
            local function process(data)
                return data
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].name, "process");
    }

    // ========== TDD: Failing tests for typed parameter extraction ==========

    #[test]
    fn test_extract_typed_string_param() {
        let source = r#"
            function search(query: string)
                return {}
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].params.len(), 1);

        let query_param = &signatures[0].params[0];
        assert_eq!(query_param.name, "query");
        // This should be Primitive { name: "string" }, not Any
        assert!(
            matches!(&query_param.type_info, LuauType::Primitive { name } if name == "string"),
            "Expected Primitive {{ name: \"string\" }}, got {:?}",
            query_param.type_info
        );
        assert!(!query_param.optional);
    }

    #[test]
    fn test_extract_typed_number_param() {
        let source = r#"
            function count(limit: number)
                return limit
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        let param = &signatures[0].params[0];

        assert_eq!(param.name, "limit");
        assert!(
            matches!(&param.type_info, LuauType::Primitive { name } if name == "number"),
            "Expected Primitive {{ name: \"number\" }}, got {:?}",
            param.type_info
        );
    }

    #[test]
    fn test_extract_optional_param() {
        let source = r#"
            function search(query: string, limit: number?)
                return {}
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        assert_eq!(signatures[0].params.len(), 2);

        // First param should be required string
        let query = &signatures[0].params[0];
        assert_eq!(query.name, "query");
        assert!(!query.optional);
        assert!(
            matches!(&query.type_info, LuauType::Primitive { name } if name == "string"),
            "Expected string primitive, got {:?}",
            query.type_info
        );

        // Second param should be optional number
        let limit = &signatures[0].params[1];
        assert_eq!(limit.name, "limit");
        assert!(limit.optional, "limit should be optional");
        // The inner type should be number
        match &limit.type_info {
            LuauType::Optional { inner } => {
                assert!(
                    matches!(inner.as_ref(), LuauType::Primitive { name } if name == "number"),
                    "Expected Optional<number>, got Optional<{:?}>",
                    inner
                );
            }
            _ => panic!("Expected Optional type, got {:?}", limit.type_info),
        }
    }

    #[test]
    fn test_extract_multiple_typed_params() {
        let source = r#"
            function process(name: string, age: number, active: boolean)
                return {}
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        let params = &signatures[0].params;

        assert_eq!(params.len(), 3);
        assert!(matches!(&params[0].type_info, LuauType::Primitive { name } if name == "string"));
        assert!(matches!(&params[1].type_info, LuauType::Primitive { name } if name == "number"));
        assert!(matches!(&params[2].type_info, LuauType::Primitive { name } if name == "boolean"));
    }

    // ========== TDD: Tests for return type extraction ==========

    #[test]
    fn test_extract_return_type_primitive() {
        let source = r#"
            function getName(): string
                return "hello"
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        let return_type = signatures[0].return_type.as_ref();

        assert!(return_type.is_some(), "Expected return type annotation");
        assert!(
            matches!(return_type.unwrap(), LuauType::Primitive { name } if name == "string"),
            "Expected Primitive {{ name: \"string\" }}, got {:?}",
            return_type
        );
    }

    #[test]
    fn test_extract_return_type_array() {
        let source = r#"
            function getNumbers(): {number}
                return {1, 2, 3}
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        let return_type = signatures[0].return_type.as_ref();

        assert!(return_type.is_some());
        match return_type.unwrap() {
            LuauType::Array { element } => {
                assert!(
                    matches!(element.as_ref(), LuauType::Primitive { name } if name == "number"),
                    "Expected Array<number>, got Array<{:?}>",
                    element
                );
            }
            other => panic!("Expected Array type, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_return_type_optional() {
        let source = r#"
            function findUser(): User?
                return nil
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        let return_type = signatures[0].return_type.as_ref();

        assert!(return_type.is_some());
        match return_type.unwrap() {
            LuauType::Optional { inner } => {
                assert!(
                    matches!(inner.as_ref(), LuauType::Named { name } if name == "User"),
                    "Expected Optional<User>, got Optional<{:?}>",
                    inner
                );
            }
            other => panic!("Expected Optional type, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_no_return_type() {
        let source = r#"
            function doSomething()
                print("hello")
            end
        "#;

        let signatures = extract_signatures(source).unwrap();
        assert!(
            signatures[0].return_type.is_none(),
            "Expected no return type, got {:?}",
            signatures[0].return_type
        );
    }

    // ========== End TDD tests ==========

    #[test]
    fn test_generate_input_schema() {
        let tool = LuaTool {
            name: "search".to_string(),
            description: "Search the knowledge base".to_string(),
            params: vec![
                ToolParam {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search query".to_string(),
                    required: true,
                    default: None,
                },
                ToolParam {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "Max results".to_string(),
                    required: false,
                    default: None,
                },
            ],
            source_path: "tools/search.lua".to_string(),
            is_fennel: false,
        };

        let schema = generate_input_schema(&tool);

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["limit"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["query"]));
    }
}
