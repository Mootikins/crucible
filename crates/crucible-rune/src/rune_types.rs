//! Rune module registration for Crucible types
//!
//! This module creates Rune bindings for our Rust types so scripts can
//! work with typed data instead of raw JSON.

use crate::events::{EnrichedRecipe, RecipeEnrichment, RecipeParameter};
use rune::{Any, ContextError, Module};

/// Create the Crucible module for Rune
///
/// This registers our types and functions so Rune scripts can use them:
/// ```rune
/// use crucible::{Recipe, RecipeEnrichment};
///
/// pub fn on_recipe_discovered(recipe) {
///     let enrichment = RecipeEnrichment::new();
///     enrichment.category = "testing";
///     enrichment
/// }
/// ```
pub fn crucible_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("crucible")?;

    // Register RecipeEnrichment
    module.ty::<RuneRecipeEnrichment>()?;
    module.function_meta(RuneRecipeEnrichment::new)?;
    module.function_meta(RuneRecipeEnrichment::with_category)?;
    module.function_meta(RuneRecipeEnrichment::with_tag)?;
    module.function_meta(RuneRecipeEnrichment::with_priority)?;
    module.function_meta(RuneRecipeEnrichment::with_hidden)?;
    module.function_meta(RuneRecipeEnrichment::set_extra)?;

    // Accessor methods
    module.function_meta(RuneRecipeEnrichment::get_category)?;
    module.function_meta(RuneRecipeEnrichment::get_tags)?;
    module.function_meta(RuneRecipeEnrichment::get_priority)?;

    // Register Recipe (read-only view for scripts)
    module.ty::<RuneRecipe>()?;
    module.function_meta(RuneRecipe::name)?;
    module.function_meta(RuneRecipe::doc)?;
    module.function_meta(RuneRecipe::is_private)?;
    module.function_meta(RuneRecipe::parameters)?;

    // Helper functions
    module.function_meta(categorize_by_name)?;

    Ok(module)
}

/// Recipe enrichment type for Rune
///
/// Wraps RecipeEnrichment with Rune-friendly methods.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible)]
pub struct RuneRecipeEnrichment {
    inner: RecipeEnrichment,
}

impl RuneRecipeEnrichment {
    // === Implementation methods (for Rust tests) ===

    /// Create a new empty enrichment (impl)
    pub fn new_impl() -> Self {
        Self {
            inner: RecipeEnrichment::default(),
        }
    }

    /// Set the category (impl)
    pub fn with_category_impl(mut self, category: String) -> Self {
        self.inner.category = Some(category);
        self
    }

    /// Add a tag (impl)
    pub fn with_tag_impl(mut self, tag: String) -> Self {
        self.inner.tags.push(tag);
        self
    }

    /// Set priority (impl)
    pub fn with_priority_impl(mut self, priority: i64) -> Self {
        self.inner.priority = Some(priority as i32);
        self
    }

    /// Convert to the Rust type
    pub fn into_inner(self) -> RecipeEnrichment {
        self.inner
    }

    // === Rune bindings ===

    /// Create a new empty enrichment
    #[rune::function(path = Self::new)]
    pub fn new() -> Self {
        Self::new_impl()
    }

    /// Set the category (builder pattern)
    #[rune::function(path = Self::with_category)]
    pub fn with_category(self, category: String) -> Self {
        self.with_category_impl(category)
    }

    /// Add a tag (builder pattern)
    #[rune::function(path = Self::with_tag)]
    pub fn with_tag(self, tag: String) -> Self {
        self.with_tag_impl(tag)
    }

    /// Set priority (builder pattern)
    #[rune::function(path = Self::with_priority)]
    pub fn with_priority(self, priority: i64) -> Self {
        self.with_priority_impl(priority)
    }

    /// Set hidden flag (builder pattern)
    #[rune::function(path = Self::with_hidden)]
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.inner.hidden = Some(hidden);
        self
    }

    /// Set arbitrary extra field
    #[rune::function(path = Self::set_extra)]
    pub fn set_extra(mut self, key: String, value: String) -> Self {
        self.inner
            .extra
            .insert(key, serde_json::Value::String(value));
        self
    }

    /// Get category
    #[rune::function(path = Self::get_category)]
    pub fn get_category(&self) -> Option<String> {
        self.inner.category.clone()
    }

    /// Get tags
    #[rune::function(path = Self::get_tags)]
    pub fn get_tags(&self) -> Vec<String> {
        self.inner.tags.clone()
    }

    /// Get priority
    #[rune::function(path = Self::get_priority)]
    pub fn get_priority(&self) -> Option<i64> {
        self.inner.priority.map(|p| p as i64)
    }
}

/// Recipe type for Rune (read-only view)
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible)]
pub struct RuneRecipe {
    name: String,
    doc: Option<String>,
    parameters: Vec<RecipeParameter>,
    private: bool,
}

impl RuneRecipe {
    /// Create from EnrichedRecipe
    pub fn from_enriched(recipe: &EnrichedRecipe) -> Self {
        Self {
            name: recipe.name.clone(),
            doc: recipe.doc.clone(),
            parameters: recipe.parameters.clone(),
            private: recipe.private,
        }
    }

    /// Get recipe name
    #[rune::function(path = Self::name)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Get recipe documentation
    #[rune::function(path = Self::doc)]
    pub fn doc(&self) -> Option<String> {
        self.doc.clone()
    }

    /// Check if recipe is private
    #[rune::function(path = Self::is_private)]
    pub fn is_private(&self) -> bool {
        self.private
    }

    /// Get parameter names
    #[rune::function(path = Self::parameters)]
    pub fn parameters(&self) -> Vec<String> {
        self.parameters.iter().map(|p| p.name.clone()).collect()
    }
}

/// Categorize recipe by name patterns (implementation)
///
/// Returns a category string based on common naming conventions.
pub fn categorize_by_name_impl(name: &str) -> &'static str {
    let lower = name.to_lowercase();

    if lower.starts_with("test") {
        "testing"
    } else if lower.starts_with("build") || lower.starts_with("release") {
        "build"
    } else if lower.starts_with("deploy") {
        "deploy"
    } else if lower.starts_with("clean") {
        "maintenance"
    } else if lower.starts_with("fmt")
        || lower.starts_with("lint")
        || lower.starts_with("check")
        || lower.starts_with("clippy")
    {
        "quality"
    } else if lower.starts_with("doc") {
        "documentation"
    } else if lower.starts_with("ci") {
        "ci"
    } else if lower.starts_with("web") || lower.starts_with("serve") {
        "web"
    } else if lower.starts_with("mcp") {
        "mcp"
    } else if lower.starts_with("bench") {
        "benchmarks"
    } else if lower == "default" {
        "default"
    } else {
        "other"
    }
}

/// Built-in helper: categorize recipe by name patterns (Rune binding)
#[rune::function]
fn categorize_by_name(name: String) -> String {
    categorize_by_name_impl(&name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_by_name() {
        assert_eq!(categorize_by_name_impl("test"), "testing");
        assert_eq!(categorize_by_name_impl("test-crate"), "testing");
        assert_eq!(categorize_by_name_impl("build"), "build");
        assert_eq!(categorize_by_name_impl("build-cli"), "build");
        assert_eq!(categorize_by_name_impl("release"), "build");
        assert_eq!(categorize_by_name_impl("deploy-prod"), "deploy");
        assert_eq!(categorize_by_name_impl("clean"), "maintenance");
        assert_eq!(categorize_by_name_impl("fmt"), "quality");
        assert_eq!(categorize_by_name_impl("clippy"), "quality");
        assert_eq!(categorize_by_name_impl("docs"), "documentation");
        assert_eq!(categorize_by_name_impl("ci"), "ci");
        assert_eq!(categorize_by_name_impl("web"), "web");
        assert_eq!(categorize_by_name_impl("mcp"), "mcp");
        assert_eq!(categorize_by_name_impl("default"), "default");
        assert_eq!(categorize_by_name_impl("random"), "other");
    }

    #[test]
    fn test_rune_recipe_enrichment_builder() {
        let enrichment = RuneRecipeEnrichment::new_impl()
            .with_category_impl("testing".to_string())
            .with_tag_impl("ci".to_string())
            .with_tag_impl("fast".to_string())
            .with_priority_impl(1);

        let inner = enrichment.into_inner();
        assert_eq!(inner.category, Some("testing".to_string()));
        assert_eq!(inner.tags, vec!["ci", "fast"]);
        assert_eq!(inner.priority, Some(1));
    }

    #[test]
    fn test_crucible_module_creation() {
        let module = crucible_module();
        assert!(module.is_ok(), "Should create crucible module");
    }
}
