//! Event system for Rune script handlers
//!
//! Events flow through Rune scripts in `events/<event_name>/` directories.
//! Scripts receive typed payloads and return enrichment data.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Trait for events that can be processed by Rune scripts
///
/// Each event type defines:
/// - A name (for folder discovery)
/// - An enrichment type (what scripts return)
/// - How to apply enrichment to the payload
pub trait CrucibleEvent: Sized + Serialize {
    /// Event name used for folder discovery (e.g., "recipe_discovered")
    const NAME: &'static str;

    /// The enrichment type that scripts return
    type Enrichment: Default + Serialize + for<'de> Deserialize<'de>;

    /// Apply enrichment data to this event payload
    fn apply_enrichment(&mut self, enrichment: Self::Enrichment);

    /// Convert self to JSON for passing to Rune
    fn to_json(&self) -> Result<JsonValue, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Enrichment data for recipe events
///
/// Scripts return this to add metadata to recipes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeEnrichment {
    /// Category for grouping (e.g., "testing", "build", "deploy")
    #[serde(default)]
    pub category: Option<String>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Priority for ordering (lower = higher priority)
    #[serde(default)]
    pub priority: Option<i32>,

    /// Whether to hide from default listings
    #[serde(default)]
    pub hidden: Option<bool>,

    /// Arbitrary additional metadata
    #[serde(default, flatten)]
    pub extra: HashMap<String, JsonValue>,
}

/// A recipe with enrichment applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedRecipe {
    // Core recipe fields (from crucible-just::Recipe)
    pub name: String,
    pub doc: Option<String>,
    pub parameters: Vec<RecipeParameter>,
    pub private: bool,

    // Enrichment fields
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub extra: HashMap<String, JsonValue>,
}

/// Simplified recipe parameter for Rune
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeParameter {
    pub name: String,
    pub kind: String,
    pub default: Option<JsonValue>,
}

impl EnrichedRecipe {
    /// Create from a basic recipe (before enrichment)
    pub fn from_recipe(
        name: String,
        doc: Option<String>,
        parameters: Vec<RecipeParameter>,
        private: bool,
    ) -> Self {
        Self {
            name,
            doc,
            parameters,
            private,
            category: None,
            tags: vec![],
            priority: None,
            hidden: false,
            extra: HashMap::new(),
        }
    }
}

impl CrucibleEvent for EnrichedRecipe {
    const NAME: &'static str = "recipe_discovered";
    type Enrichment = RecipeEnrichment;

    fn apply_enrichment(&mut self, e: Self::Enrichment) {
        if e.category.is_some() {
            self.category = e.category;
        }
        if !e.tags.is_empty() {
            self.tags.extend(e.tags);
        }
        if e.priority.is_some() {
            self.priority = e.priority;
        }
        if let Some(hidden) = e.hidden {
            self.hidden = hidden;
        }
        self.extra.extend(e.extra);
    }
}

/// MCP tool execution result event
///
/// Represents the result of executing an MCP tool, used for
/// filtering and transforming tool output through Rune plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultEvent {
    /// Name of the tool that was executed (e.g., "just_test")
    pub tool_name: String,
    /// Arguments passed to the tool
    pub arguments: JsonValue,
    /// Whether the tool execution resulted in an error
    pub is_error: bool,
    /// Content blocks returned by the tool
    pub content: Vec<ContentBlock>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Content block types matching MCP specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64 encoded)
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String> },
}

impl ToolResultEvent {
    /// Get all text content concatenated with newlines
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replace all content with a single text block
    pub fn with_text_content(mut self, text: String) -> Self {
        self.content = vec![ContentBlock::Text { text }];
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipe_enrichment_default() {
        let e = RecipeEnrichment::default();
        assert!(e.category.is_none());
        assert!(e.tags.is_empty());
        assert!(e.priority.is_none());
    }

    #[test]
    fn test_recipe_enrichment_deserialize() {
        let json = r#"{"category": "testing", "tags": ["ci", "fast"], "priority": 1}"#;
        let e: RecipeEnrichment = serde_json::from_str(json).unwrap();
        assert_eq!(e.category, Some("testing".to_string()));
        assert_eq!(e.tags, vec!["ci", "fast"]);
        assert_eq!(e.priority, Some(1));
    }

    #[test]
    fn test_recipe_enrichment_extra_fields() {
        let json = r#"{"category": "testing", "custom_field": "custom_value"}"#;
        let e: RecipeEnrichment = serde_json::from_str(json).unwrap();
        assert_eq!(e.category, Some("testing".to_string()));
        assert_eq!(
            e.extra.get("custom_field"),
            Some(&JsonValue::String("custom_value".to_string()))
        );
    }

    #[test]
    fn test_apply_enrichment() {
        let mut recipe = EnrichedRecipe::from_recipe(
            "test".to_string(),
            Some("Run tests".to_string()),
            vec![],
            false,
        );

        let enrichment = RecipeEnrichment {
            category: Some("testing".to_string()),
            tags: vec!["ci".to_string()],
            priority: Some(1),
            hidden: Some(false),
            extra: HashMap::new(),
        };

        recipe.apply_enrichment(enrichment);

        assert_eq!(recipe.category, Some("testing".to_string()));
        assert_eq!(recipe.tags, vec!["ci"]);
        assert_eq!(recipe.priority, Some(1));
    }

    #[test]
    fn test_enriched_recipe_serializes() {
        let recipe = EnrichedRecipe::from_recipe(
            "build".to_string(),
            Some("Build the project".to_string()),
            vec![RecipeParameter {
                name: "target".to_string(),
                kind: "singular".to_string(),
                default: None,
            }],
            false,
        );

        let json = serde_json::to_value(&recipe).unwrap();
        assert_eq!(json["name"], "build");
        assert_eq!(json["doc"], "Build the project");
    }

    #[test]
    fn test_tool_result_event_serialize_deserialize() {
        let event = ToolResultEvent {
            tool_name: "just_test".to_string(),
            arguments: serde_json::json!({"crate": "crucible-rune"}),
            is_error: false,
            content: vec![ContentBlock::Text {
                text: "test output".to_string(),
            }],
            duration_ms: 1234,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: ToolResultEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tool_name, "just_test");
        assert_eq!(parsed.duration_ms, 1234);
    }

    #[test]
    fn test_content_block_text_variant() {
        let block = ContentBlock::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn test_content_block_image_variant() {
        let block = ContentBlock::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["mime_type"], "image/png");
    }

    #[test]
    fn test_text_content_extracts_text_blocks() {
        let event = ToolResultEvent {
            tool_name: "test".to_string(),
            arguments: serde_json::json!(null),
            is_error: false,
            content: vec![
                ContentBlock::Text {
                    text: "line1".to_string(),
                },
                ContentBlock::Image {
                    data: "x".to_string(),
                    mime_type: "image/png".to_string(),
                },
                ContentBlock::Text {
                    text: "line2".to_string(),
                },
            ],
            duration_ms: 0,
        };

        assert_eq!(event.text_content(), "line1\nline2");
    }

    #[test]
    fn test_with_text_content_replaces_all() {
        let event = ToolResultEvent {
            tool_name: "test".to_string(),
            arguments: serde_json::json!(null),
            is_error: false,
            content: vec![
                ContentBlock::Text {
                    text: "old1".to_string(),
                },
                ContentBlock::Text {
                    text: "old2".to_string(),
                },
            ],
            duration_ms: 0,
        };

        let new_event = event.with_text_content("new content".to_string());
        assert_eq!(new_event.content.len(), 1);
        assert_eq!(new_event.text_content(), "new content");
    }
}
