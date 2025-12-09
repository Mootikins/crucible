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
}
