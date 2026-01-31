//! Schema types and generation for Lua tools
//!
//! Provides type representations and JSON Schema generation for tool parameters.
//! Type information comes from LDoc annotations (see `annotations.rs`), not inline
//! type syntax (Lua 5.4 doesn't support type annotations in syntax).
//!
//! ## Example
//!
//! ```lua
//! --- Search the knowledge base
//! -- @tool
//! -- @param query string The search term
//! -- @param limit number? Maximum results
//! function search(query, limit)
//!     return kb_search(query, limit or 10)
//! end
//! ```
//!
//! Tool parameters are converted to schemas using this module's types.

use crate::types::LuaTool;
#[cfg(test)]
use crate::types::ToolParam;
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
    /// Type info
    pub type_info: LuauType,
    /// Whether optional (T? syntax)
    pub optional: bool,
}

/// Type representation for Lua tools
///
/// Named LuauType for historical reasons, but used for all Lua type representations.
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
    /// Create a primitive type
    pub fn primitive(name: &str) -> Self {
        LuauType::Primitive {
            name: name.to_string(),
        }
    }

    /// Create an optional type
    pub fn optional(inner: LuauType) -> Self {
        LuauType::Optional {
            inner: Box::new(inner),
        }
    }

    /// Parse a type string from LDoc annotation
    ///
    /// Supports: string, number, boolean, nil, any, Name?, array<T>
    pub fn from_ldoc(type_str: &str) -> Self {
        let type_str = type_str.trim();

        // Handle optional suffix
        if let Some(base) = type_str.strip_suffix('?') {
            return LuauType::Optional {
                inner: Box::new(LuauType::from_ldoc(base)),
            };
        }

        // Handle array syntax
        if let Some(inner) = type_str
            .strip_prefix("array<")
            .and_then(|s| s.strip_suffix('>'))
        {
            return LuauType::Array {
                element: Box::new(LuauType::from_ldoc(inner)),
            };
        }

        // Handle table syntax {T}
        if type_str.starts_with('{') && type_str.ends_with('}') {
            let inner = &type_str[1..type_str.len() - 1];
            return LuauType::Array {
                element: Box::new(LuauType::from_ldoc(inner)),
            };
        }

        // Primitives
        match type_str {
            "string" | "number" | "boolean" | "nil" => LuauType::Primitive {
                name: type_str.to_string(),
            },
            "any" => LuauType::Any,
            // Assume anything else is a named type
            _ => LuauType::Named {
                name: type_str.to_string(),
            },
        }
    }

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
    fn test_ldoc_type_parsing() {
        assert!(matches!(
            LuauType::from_ldoc("string"),
            LuauType::Primitive { name } if name == "string"
        ));

        assert!(matches!(
            LuauType::from_ldoc("number"),
            LuauType::Primitive { name } if name == "number"
        ));

        assert!(matches!(LuauType::from_ldoc("any"), LuauType::Any));
    }

    #[test]
    fn test_ldoc_optional_type() {
        let ty = LuauType::from_ldoc("string?");
        match ty {
            LuauType::Optional { inner } => {
                assert!(matches!(*inner, LuauType::Primitive { name } if name == "string"));
            }
            _ => panic!("Expected Optional type"),
        }
    }

    #[test]
    fn test_ldoc_array_type() {
        let ty = LuauType::from_ldoc("array<number>");
        match ty {
            LuauType::Array { element } => {
                assert!(matches!(*element, LuauType::Primitive { name } if name == "number"));
            }
            _ => panic!("Expected Array type"),
        }
    }

    #[test]
    fn test_ldoc_named_type() {
        let ty = LuauType::from_ldoc("SearchResult");
        assert!(matches!(ty, LuauType::Named { name } if name == "SearchResult"));
    }

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

    #[test]
    fn test_type_to_json_schema() {
        let string_ty = LuauType::primitive("string");
        assert_eq!(string_ty.to_json_schema()["type"], "string");

        let array_ty = LuauType::Array {
            element: Box::new(LuauType::primitive("number")),
        };
        assert_eq!(array_ty.to_json_schema()["type"], "array");
        assert_eq!(array_ty.to_json_schema()["items"]["type"], "number");
    }
}
