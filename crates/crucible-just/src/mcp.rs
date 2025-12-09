//! Generate MCP tool definitions from justfile recipes

use crate::{Justfile, Recipe};
use serde_json::{json, Value};

/// MCP Tool definition (matches rmcp::model::Tool structure)
///
/// Includes optional enrichment fields that can be populated
/// by Rune event handlers.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,

    // Enrichment fields (populated by event handlers)
    /// Category for grouping (e.g., "testing", "build", "deploy")
    pub category: Option<String>,
    /// Tags for filtering (e.g., ["ci", "quick"])
    pub tags: Vec<String>,
    /// Priority for ordering (lower = higher priority)
    pub priority: Option<i32>,
}

impl Justfile {
    /// Generate MCP tools from all public recipes
    pub fn to_mcp_tools(&self) -> Vec<McpTool> {
        self.public_recipes()
            .into_iter()
            .map(|r| r.to_mcp_tool())
            .collect()
    }
}

impl Recipe {
    /// Convert recipe to MCP tool definition
    ///
    /// Enrichment fields (category, tags, priority) are initially empty
    /// and can be populated later by Rune event handlers.
    pub fn to_mcp_tool(&self) -> McpTool {
        let description = self
            .doc
            .clone()
            .unwrap_or_else(|| format!("Run just recipe: {}", self.name));

        let input_schema = self.to_json_schema();

        McpTool {
            name: format!("just_{}", self.name.replace('-', "_")),
            description,
            input_schema,
            // Enrichment fields start empty
            category: None,
            tags: vec![],
            priority: None,
        }
    }

    /// Generate JSON Schema for recipe parameters
    fn to_json_schema(&self) -> Value {
        if self.parameters.is_empty() {
            return json!({
                "type": "object",
                "properties": {},
                "required": []
            });
        }

        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            let prop = json!({
                "type": "string",
                "description": format!("Parameter: {}", param.name)
            });
            properties.insert(param.name.to_lowercase(), prop);

            // Required if no default and not variadic
            if param.default.is_none() && param.kind == "singular" {
                required.push(param.name.to_lowercase());
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parameter;

    #[test]
    fn test_recipe_to_mcp_tool() {
        let recipe = Recipe {
            attributes: vec![],
            body: vec![],
            dependencies: vec![],
            doc: Some("Run tests".to_string()),
            name: "test".to_string(),
            namepath: "test".to_string(),
            parameters: vec![],
            priors: 0,
            private: false,
            quiet: false,
            shebang: false,
        };

        let tool = recipe.to_mcp_tool();
        assert_eq!(tool.name, "just_test");
        assert_eq!(tool.description, "Run tests");
    }

    #[test]
    fn test_recipe_with_params_to_mcp_tool() {
        let recipe = Recipe {
            attributes: vec![],
            body: vec![],
            dependencies: vec![],
            doc: Some("Test a crate".to_string()),
            name: "test-crate".to_string(),
            namepath: "test-crate".to_string(),
            parameters: vec![Parameter {
                default: None,
                export: false,
                kind: "singular".to_string(),
                name: "CRATE".to_string(),
            }],
            priors: 0,
            private: false,
            quiet: false,
            shebang: false,
        };

        let tool = recipe.to_mcp_tool();
        assert_eq!(tool.name, "just_test_crate");

        let schema = &tool.input_schema;
        assert!(schema["properties"]["crate"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("crate")));
    }

    #[test]
    fn test_optional_param_schema() {
        // Test recipe with optional FULL parameter (has default)
        let recipe = Recipe {
            attributes: vec![],
            body: vec![],
            dependencies: vec![],
            doc: Some("Run tests".to_string()),
            name: "test".to_string(),
            namepath: "test".to_string(),
            parameters: vec![Parameter {
                default: Some(json!("")), // Empty default = optional
                export: false,
                kind: "singular".to_string(),
                name: "FULL".to_string(),
            }],
            priors: 0,
            private: false,
            quiet: false,
            shebang: false,
        };

        let tool = recipe.to_mcp_tool();
        let schema = &tool.input_schema;

        // FULL should be in properties
        assert!(schema["properties"]["full"].is_object());

        // FULL should NOT be required (has default)
        let required = schema["required"].as_array().unwrap();
        assert!(!required.contains(&json!("full")));

        println!("Schema: {}", serde_json::to_string_pretty(&schema).unwrap());
    }

    #[test]
    fn test_mixed_params_schema() {
        // Test recipe with required CRATE and optional FULL
        let recipe = Recipe {
            attributes: vec![],
            body: vec![],
            dependencies: vec![],
            doc: Some("Test a crate".to_string()),
            name: "test-crate".to_string(),
            namepath: "test-crate".to_string(),
            parameters: vec![
                Parameter {
                    default: None, // Required
                    export: false,
                    kind: "singular".to_string(),
                    name: "CRATE".to_string(),
                },
                Parameter {
                    default: Some(json!("")), // Optional
                    export: false,
                    kind: "singular".to_string(),
                    name: "FULL".to_string(),
                },
            ],
            priors: 0,
            private: false,
            quiet: false,
            shebang: false,
        };

        let tool = recipe.to_mcp_tool();
        let schema = &tool.input_schema;

        // Both should be in properties
        assert!(schema["properties"]["crate"].is_object());
        assert!(schema["properties"]["full"].is_object());

        // Only CRATE should be required
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("crate")));
        assert!(!required.contains(&json!("full")));

        println!("Schema: {}", serde_json::to_string_pretty(&schema).unwrap());
    }
}
