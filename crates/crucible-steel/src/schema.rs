//! Schema extraction from Steel contracts
//!
//! Converts Steel contracts and annotations to JSON schemas for
//! MCP tool registration and validation.
//!
//! ## Steel Contracts
//!
//! ```scheme
//! ;; Steel uses contracts with predicates
//! (define/contract (search query limit)
//!   (->/c string? positive? list?)
//!   (kb-search query limit))
//! ```
//!
//! Extracts to:
//! ```json
//! {
//!   "type": "object",
//!   "properties": {
//!     "query": { "type": "string" },
//!     "limit": { "type": "integer", "minimum": 1 }
//!   },
//!   "required": ["query", "limit"]
//! }
//! ```

use crate::types::{SteelTool, ToolParam};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Steel contract type representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ContractType {
    /// Primitive type predicate: string?, number?, boolean?, etc.
    Primitive { predicate: String },
    /// Optional: or/c predicate #f
    Optional { inner: Box<ContractType> },
    /// List of elements
    List { element: Option<Box<ContractType>> },
    /// Hash/object
    Hash { fields: Option<Vec<(String, ContractType)>> },
    /// Union: or/c
    Union { types: Vec<ContractType> },
    /// Any value
    Any,
    /// Custom predicate
    Custom { predicate: String },
}

impl ContractType {
    /// Parse from a Steel predicate name
    pub fn from_predicate(pred: &str) -> Self {
        match pred.trim() {
            // Primitives
            "string?" => ContractType::Primitive {
                predicate: "string".to_string(),
            },
            "number?" | "integer?" | "int?" => ContractType::Primitive {
                predicate: "number".to_string(),
            },
            "positive?" => ContractType::Primitive {
                predicate: "positive".to_string(),
            },
            "negative?" => ContractType::Primitive {
                predicate: "negative".to_string(),
            },
            "boolean?" | "bool?" => ContractType::Primitive {
                predicate: "boolean".to_string(),
            },
            "null?" | "void?" => ContractType::Primitive {
                predicate: "null".to_string(),
            },

            // Collections
            "list?" => ContractType::List { element: None },
            "pair?" => ContractType::List { element: None },
            "vector?" => ContractType::List { element: None },
            "hash?" | "hashmap?" => ContractType::Hash { fields: None },

            // Any
            "any/c" | "any?" => ContractType::Any,

            // Custom predicate
            other => ContractType::Custom {
                predicate: other.to_string(),
            },
        }
    }

    /// Convert to JSON Schema representation
    pub fn to_json_schema(&self) -> JsonValue {
        match self {
            ContractType::Primitive { predicate } => match predicate.as_str() {
                "string" => serde_json::json!({ "type": "string" }),
                "number" => serde_json::json!({ "type": "number" }),
                "positive" => serde_json::json!({ "type": "integer", "minimum": 1 }),
                "negative" => serde_json::json!({ "type": "integer", "maximum": -1 }),
                "boolean" => serde_json::json!({ "type": "boolean" }),
                "null" => serde_json::json!({ "type": "null" }),
                _ => serde_json::json!({ "type": "string" }),
            },
            ContractType::Optional { inner } => {
                // For optional, just return the inner schema
                // The "required" array handles optionality
                inner.to_json_schema()
            }
            ContractType::List { element } => {
                if let Some(el) = element {
                    serde_json::json!({
                        "type": "array",
                        "items": el.to_json_schema()
                    })
                } else {
                    serde_json::json!({ "type": "array" })
                }
            }
            ContractType::Hash { fields } => {
                if let Some(f) = fields {
                    let properties: serde_json::Map<String, JsonValue> = f
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_json_schema()))
                        .collect();
                    serde_json::json!({
                        "type": "object",
                        "properties": properties
                    })
                } else {
                    serde_json::json!({ "type": "object" })
                }
            }
            ContractType::Union { types } => {
                serde_json::json!({
                    "oneOf": types.iter().map(|t| t.to_json_schema()).collect::<Vec<_>>()
                })
            }
            ContractType::Any => serde_json::json!({}),
            ContractType::Custom { predicate } => {
                // Custom predicates can't be easily mapped
                serde_json::json!({
                    "description": format!("Satisfies predicate: {}", predicate)
                })
            }
        }
    }
}

/// Map parameter type string to JSON Schema
fn param_type_to_schema(param_type: &str, description: &str) -> JsonValue {
    let base = match param_type.to_lowercase().as_str() {
        "string" | "str" => serde_json::json!({ "type": "string" }),
        "number" | "num" | "integer" | "int" => serde_json::json!({ "type": "number" }),
        "boolean" | "bool" => serde_json::json!({ "type": "boolean" }),
        "array" | "list" => serde_json::json!({ "type": "array" }),
        "object" | "hash" => serde_json::json!({ "type": "object" }),
        "any" => serde_json::json!({}),
        other => {
            // Check if it's a predicate (ends with ?)
            if other.ends_with('?') {
                ContractType::from_predicate(other).to_json_schema()
            } else {
                serde_json::json!({ "type": "string" })
            }
        }
    };

    // Add description if not empty
    if !description.is_empty() {
        let mut schema = base;
        if let Some(obj) = schema.as_object_mut() {
            obj.insert(
                "description".to_string(),
                JsonValue::String(description.to_string()),
            );
        }
        schema
    } else {
        base
    }
}

/// Generate a JSON Schema for a tool's input parameters
pub fn generate_input_schema(tool: &SteelTool) -> JsonValue {
    let properties: serde_json::Map<String, JsonValue> = tool
        .params
        .iter()
        .map(|p| {
            let schema = param_type_to_schema(&p.param_type, &p.description);
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

/// Extracted contract signature from Steel source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSignature {
    /// Function name
    pub name: String,
    /// Input parameter contracts
    pub input_contracts: Vec<ContractType>,
    /// Output contract (return type)
    pub output_contract: Option<ContractType>,
}

/// Extract contract signatures from Steel source
///
/// Parses `define/contract` forms to extract contract specifications.
pub fn extract_contracts(source: &str) -> Vec<ContractSignature> {
    let mut signatures = Vec::new();

    // Pattern for define/contract with ->/c
    // (define/contract (func-name params...) (->/c contracts...) body)
    let contract_pattern = Regex::new(
        r"(?s)\(define/contract\s*\((\S+)([^)]*)\)\s*\(->/c([^)]+)\)",
    ).unwrap();

    for cap in contract_pattern.captures_iter(source) {
        let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let contracts_str = cap.get(3).map(|m| m.as_str()).unwrap_or("");

        // Parse individual contracts
        let contracts: Vec<ContractType> = contracts_str
            .split_whitespace()
            .map(|pred| ContractType::from_predicate(pred))
            .collect();

        // Last contract is the return type, others are inputs
        let (input_contracts, output_contract) = if contracts.len() > 1 {
            let inputs = contracts[..contracts.len() - 1].to_vec();
            let output = contracts.last().cloned();
            (inputs, output)
        } else if contracts.len() == 1 {
            (Vec::new(), contracts.into_iter().next())
        } else {
            (Vec::new(), None)
        };

        signatures.push(ContractSignature {
            name,
            input_contracts,
            output_contract,
        });
    }

    signatures
}

/// Convert a contract signature to tool parameters
pub fn contract_to_params(
    sig: &ContractSignature,
    param_names: &[String],
) -> Vec<ToolParam> {
    sig.input_contracts
        .iter()
        .enumerate()
        .map(|(i, contract)| {
            let name = param_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("arg{}", i));

            let param_type = match contract {
                ContractType::Primitive { predicate } => predicate.clone(),
                ContractType::List { .. } => "array".to_string(),
                ContractType::Hash { .. } => "object".to_string(),
                ContractType::Any => "any".to_string(),
                ContractType::Optional { inner } => {
                    format!("{}?", match inner.as_ref() {
                        ContractType::Primitive { predicate } => predicate.clone(),
                        _ => "any".to_string(),
                    })
                }
                ContractType::Union { .. } => "any".to_string(),
                ContractType::Custom { predicate } => predicate.clone(),
            };

            let required = !matches!(contract, ContractType::Optional { .. });

            ToolParam {
                name,
                param_type,
                description: String::new(),
                required,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ContractType tests
    // =========================================================================

    #[test]
    fn test_predicate_string() {
        let ct = ContractType::from_predicate("string?");
        assert!(matches!(ct, ContractType::Primitive { predicate } if predicate == "string"));
    }

    #[test]
    fn test_predicate_number() {
        let ct = ContractType::from_predicate("number?");
        assert!(matches!(ct, ContractType::Primitive { predicate } if predicate == "number"));
    }

    #[test]
    fn test_predicate_positive() {
        let ct = ContractType::from_predicate("positive?");
        assert!(matches!(ct, ContractType::Primitive { predicate } if predicate == "positive"));
    }

    #[test]
    fn test_predicate_list() {
        let ct = ContractType::from_predicate("list?");
        assert!(matches!(ct, ContractType::List { .. }));
    }

    #[test]
    fn test_predicate_hash() {
        let ct = ContractType::from_predicate("hash?");
        assert!(matches!(ct, ContractType::Hash { .. }));
    }

    #[test]
    fn test_predicate_custom() {
        let ct = ContractType::from_predicate("my-validator?");
        assert!(
            matches!(ct, ContractType::Custom { predicate } if predicate == "my-validator?")
        );
    }

    // =========================================================================
    // JSON Schema generation tests
    // =========================================================================

    #[test]
    fn test_schema_string() {
        let ct = ContractType::Primitive {
            predicate: "string".to_string(),
        };
        let schema = ct.to_json_schema();
        assert_eq!(schema["type"], "string");
    }

    #[test]
    fn test_schema_number() {
        let ct = ContractType::Primitive {
            predicate: "number".to_string(),
        };
        let schema = ct.to_json_schema();
        assert_eq!(schema["type"], "number");
    }

    #[test]
    fn test_schema_positive() {
        let ct = ContractType::Primitive {
            predicate: "positive".to_string(),
        };
        let schema = ct.to_json_schema();
        assert_eq!(schema["type"], "integer");
        assert_eq!(schema["minimum"], 1);
    }

    #[test]
    fn test_schema_array() {
        let ct = ContractType::List {
            element: Some(Box::new(ContractType::Primitive {
                predicate: "string".to_string(),
            })),
        };
        let schema = ct.to_json_schema();
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");
    }

    #[test]
    fn test_schema_object() {
        let ct = ContractType::Hash { fields: None };
        let schema = ct.to_json_schema();
        assert_eq!(schema["type"], "object");
    }

    // =========================================================================
    // Tool schema generation tests
    // =========================================================================

    #[test]
    fn test_generate_input_schema() {
        let tool = SteelTool {
            name: "search".to_string(),
            description: "Search notes".to_string(),
            params: vec![
                ToolParam {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search query".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "Max results".to_string(),
                    required: false,
                },
            ],
            source_path: "tools/search.scm".to_string(),
        };

        let schema = generate_input_schema(&tool);

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["type"], "number");
        assert_eq!(schema["required"], serde_json::json!(["query"]));
    }

    #[test]
    fn test_generate_schema_with_predicates() {
        let tool = SteelTool {
            name: "count".to_string(),
            description: "Count items".to_string(),
            params: vec![ToolParam {
                name: "n".to_string(),
                param_type: "positive?".to_string(),
                description: "Positive count".to_string(),
                required: true,
            }],
            source_path: "tools/count.scm".to_string(),
        };

        let schema = generate_input_schema(&tool);

        assert_eq!(schema["properties"]["n"]["type"], "integer");
        assert_eq!(schema["properties"]["n"]["minimum"], 1);
    }

    // =========================================================================
    // Contract extraction tests
    // =========================================================================

    #[test]
    fn test_extract_simple_contract() {
        let source = r#"
(define/contract (add x y)
  (->/c number? number? number?)
  (+ x y))
"#;

        let contracts = extract_contracts(source);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].name, "add");
        assert_eq!(contracts[0].input_contracts.len(), 2);
        assert!(contracts[0].output_contract.is_some());
    }

    #[test]
    fn test_extract_string_contract() {
        let source = r#"
(define/contract (greet name)
  (->/c string? string?)
  (string-append "Hello, " name))
"#;

        let contracts = extract_contracts(source);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].input_contracts.len(), 1);

        let input = &contracts[0].input_contracts[0];
        assert!(matches!(input, ContractType::Primitive { predicate } if predicate == "string"));
    }

    #[test]
    fn test_extract_hash_contract() {
        let source = r#"
(define/contract (process-args args)
  (->/c hash? list?)
  (hash->list args))
"#;

        let contracts = extract_contracts(source);
        assert_eq!(contracts.len(), 1);

        let input = &contracts[0].input_contracts[0];
        assert!(matches!(input, ContractType::Hash { .. }));

        let output = contracts[0].output_contract.as_ref().unwrap();
        assert!(matches!(output, ContractType::List { .. }));
    }

    #[test]
    fn test_contract_to_params() {
        let sig = ContractSignature {
            name: "search".to_string(),
            input_contracts: vec![
                ContractType::Primitive {
                    predicate: "string".to_string(),
                },
                ContractType::Primitive {
                    predicate: "number".to_string(),
                },
            ],
            output_contract: Some(ContractType::List { element: None }),
        };

        let params = contract_to_params(&sig, &["query".to_string(), "limit".to_string()]);

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "query");
        assert_eq!(params[0].param_type, "string");
        assert_eq!(params[1].name, "limit");
        assert_eq!(params[1].param_type, "number");
    }

    #[test]
    fn test_no_contracts_in_source() {
        let source = r#"
(define (add x y)
  (+ x y))
"#;

        let contracts = extract_contracts(source);
        assert!(contracts.is_empty());
    }
}
