//! JSON Schema generation for Luau tools, over the one signature model.
//!
//! Type information comes from a plugin's spec-table declarations (see
//! `discovered.rs`). Those declarations are *text*, and this module used to
//! match four literals — `string`, `number`, `boolean`, and everything else
//! became `"string"`. A `string[]` parameter reached the agent as a string,
//! and an author's typo reached it the same way, silently.
//!
//! [`crate::signature`] reads the same text into a type, so `string[]`,
//! `string?`, `table<string, number>` and `{ name: string }` all mean here
//! what they mean in the Luau declaration the stub generator writes. A
//! declaration the model cannot read is reported by
//! [`crate::signature::LuaType::parse`] at load, where the author sees it.
//!
//! ## Example
//!
//! ```lua
//! return {
//!     tools = {
//!         search = {
//!             desc = "Search the knowledge base",
//!             params = {
//!                 { name = "query", type = "string", desc = "The search term" },
//!                 { name = "tags", type = "string[]", desc = "Filter", optional = true },
//!             },
//!             fn = search,
//!         },
//!     },
//! }
//! ```

use crate::discovered::DiscoveredParam;
use crate::signature::{LuaType, Param, Signature};
use serde_json::Value as JsonValue;

/// Read one declared type, falling back to `any` for text the model refuses.
///
/// The fallback is `any` and not `string`: a parameter whose declaration is
/// unreadable has no known shape, and saying "string" invents one. Load-time
/// validation is what turns the unreadable declaration into an error the
/// author sees; this keeps the tool usable in the meantime.
fn declared_type(text: &str) -> LuaType {
    LuaType::parse(text).unwrap_or(LuaType::Any)
}

/// Build the signature that a list of declared parameters describes.
fn signature_of(params: &[DiscoveredParam]) -> Signature {
    Signature {
        params: params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: declared_type(&p.param_type),
                description: Some(p.description.clone()),
                optional: p.optional,
            })
            .collect(),
        returns: Vec::new(),
    }
}

/// Generate a JSON Schema from spec-declared params.
pub fn discovered_params_to_json_schema(params: &[DiscoveredParam]) -> JsonValue {
    signature_of(params).to_input_schema()
}

/// The Luau signature a declared tool has, for the generated declarations.
pub fn tool_signature(params: &[DiscoveredParam], returns: Option<&str>) -> Signature {
    let mut signature = signature_of(params);
    if let Some(returns) = returns {
        signature.returns = vec![declared_type(returns)];
    }
    signature
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A required parameter goes in `required`. An optional one does not.
    #[test]
    fn only_a_required_parameter_is_required() {
        let param = |name: &str, param_type: &str, optional| DiscoveredParam {
            name: name.to_string(),
            param_type: param_type.to_string(),
            description: String::new(),
            optional,
        };
        let schema = discovered_params_to_json_schema(&[
            param("query", "string", false),
            param("limit", "number", true),
        ]);

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["limit"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["query"]));
    }

    /// The shapes the old four-label match rendered as `string`.
    #[test]
    fn a_compound_declaration_reaches_the_schema_intact() {
        let params = vec![
            DiscoveredParam {
                name: "tags".to_string(),
                param_type: "string[]".to_string(),
                description: "filter tags".to_string(),
                optional: true,
            },
            DiscoveredParam {
                name: "where".to_string(),
                param_type: "{ kiln: string, depth: number? }".to_string(),
                description: "scope".to_string(),
                optional: false,
            },
        ];

        let schema = discovered_params_to_json_schema(&params);
        assert_eq!(schema["properties"]["tags"]["type"], "array");
        assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
        assert_eq!(schema["properties"]["where"]["type"], "object");
        assert_eq!(
            schema["properties"]["where"]["properties"]["kiln"]["type"],
            "string"
        );
        assert_eq!(schema["required"], serde_json::json!(["where"]));
    }

    /// An unreadable declaration leaves the parameter unconstrained. It must
    /// NOT be quietly called a string — that is a shape nobody declared.
    #[test]
    fn an_unreadable_declaration_constrains_nothing() {
        let params = vec![DiscoveredParam {
            name: "mystery".to_string(),
            param_type: "array<".to_string(),
            description: String::new(),
            optional: false,
        }];
        let schema = discovered_params_to_json_schema(&params);
        assert!(
            schema["properties"]["mystery"].get("type").is_none(),
            "an unreadable type must not be rendered as a string: {schema:#}"
        );
    }
}
