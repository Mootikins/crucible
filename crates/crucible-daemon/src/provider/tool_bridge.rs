//! Bridge between Crucible tool definitions and genai's Tool type.

use crucible_core::traits::tools::ToolDefinition;
use genai::chat::Tool;

pub fn llm_tool_to_genai(tool: &ToolDefinition) -> Tool {
    let mut converted = Tool::new(tool.name.clone()).with_description(tool.description.clone());
    if let Some(mut schema) = tool.parameters.clone() {
        sanitize_tool_schema(&mut schema);
        converted = converted.with_schema(schema);
    }
    converted
}

/// Sanitize tool schema for OpenAI-spec compliance.
/// Ensures empty object schemas have a `properties` key and removes metadata fields.
pub(crate) fn sanitize_tool_schema(schema: &mut serde_json::Value) {
    if let Some(obj) = schema.as_object_mut() {
        // If this is an object type without properties, add empty properties
        if obj.get("type").and_then(|v| v.as_str()) == Some("object")
            && !obj.contains_key("properties")
        {
            obj.insert(
                "properties".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
        }

        // Remove metadata fields that llama.cpp doesn't like
        obj.remove("$schema");
        obj.remove("title");

        // Recursively sanitize nested schemas in properties
        if let Some(properties) = obj.get_mut("properties") {
            if let Some(props_obj) = properties.as_object_mut() {
                for (_, prop_schema) in props_obj.iter_mut() {
                    // Remove redundant "default": null from Option<T> fields
                    if let Some(prop_obj) = prop_schema.as_object_mut() {
                        if prop_obj.get("default").and_then(|v| v.as_null()).is_some() {
                            prop_obj.remove("default");
                        }
                    }
                    sanitize_tool_schema(prop_schema);
                }
            }
        }

        // Recursively sanitize array item schemas
        if let Some(items) = obj.get_mut("items") {
            sanitize_tool_schema(items);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn llm_tool_to_genai_carries_name_description_and_schema() {
        let tool =
            ToolDefinition::new("read_file", "Read contents of a file").with_parameters(json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }));

        let converted = llm_tool_to_genai(&tool);

        assert_eq!(converted.name, "read_file".into());
        assert_eq!(
            converted.description.as_deref(),
            Some("Read contents of a file")
        );
        assert!(converted.schema.is_some());
    }

    #[test]
    fn test_sanitize_empty_object_schema() {
        let mut schema = json!({"type": "object"});
        sanitize_tool_schema(&mut schema);
        assert!(schema.get("properties").is_some());
        assert_eq!(schema["properties"], json!({}));
    }

    #[test]
    fn test_sanitize_strips_meta_fields() {
        let mut schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Foo",
            "type": "object",
            "properties": {}
        });
        sanitize_tool_schema(&mut schema);
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
        assert!(schema.get("type").is_some());
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_sanitize_recursive_nested_schema() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object"
                }
            }
        });
        sanitize_tool_schema(&mut schema);
        assert!(schema["properties"]["nested"].get("properties").is_some());
        assert_eq!(schema["properties"]["nested"]["properties"], json!({}));
    }
}
