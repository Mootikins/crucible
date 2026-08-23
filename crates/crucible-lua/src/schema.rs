//! JSON Schema generation for Lua tools
//!
//! Type information comes from a plugin's spec-table declarations (see
//! `discovered.rs`), not inline
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

use crate::discovered::DiscoveredParam;
use crate::types::LuaTool;
#[cfg(test)]
use crate::types::ToolParam;
use serde_json::Value as JsonValue;

/// Build an object JSON Schema from `(name, ldoc_type, description, required)` tuples.
fn object_schema<'a>(
    params: impl Iterator<Item = (&'a str, &'a str, &'a str, bool)> + Clone,
) -> JsonValue {
    let properties: serde_json::Map<String, JsonValue> = params
        .clone()
        .map(|(name, ty, desc, _)| {
            let schema = match ty {
                "string" => serde_json::json!({ "type": "string", "description": desc }),
                "number" => serde_json::json!({ "type": "number", "description": desc }),
                "boolean" => serde_json::json!({ "type": "boolean", "description": desc }),
                _ => serde_json::json!({ "type": "string", "description": desc }),
            };
            (name.to_string(), schema)
        })
        .collect();

    let required: Vec<String> = params
        .filter(|(_, _, _, required)| *required)
        .map(|(name, _, _, _)| name.to_string())
        .collect();

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

/// Generate a JSON Schema for a tool's input parameters
pub fn generate_input_schema(tool: &LuaTool) -> JsonValue {
    object_schema(tool.params.iter().map(|p| {
        (
            p.name.as_str(),
            p.param_type.as_str(),
            p.description.as_str(),
            p.required,
        )
    }))
}

/// Generate a JSON Schema from spec-declared params.
///
/// [`DiscoveredParam`] flags `optional` where [`crate::types::ToolParam`] flags
/// `required` — the two declaration syntaxes are inverses, so they cannot share
/// one struct without lying about one of them.
pub fn discovered_params_to_json_schema(params: &[DiscoveredParam]) -> JsonValue {
    object_schema(params.iter().map(|p| {
        (
            p.name.as_str(),
            p.param_type.as_str(),
            p.description.as_str(),
            !p.optional,
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
