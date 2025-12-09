//! Event handler system - discovers and executes Rune scripts for events
//!
//! Scripts are discovered from:
//! - `~/.crucible/runes/events/<event_name>/`
//! - `{kiln}/runes/events/<event_name>/`

use crate::events::{CrucibleEvent, EnrichedRecipe};
use crate::regex_module::regex_module;
use crate::rune_types::{crucible_module, RuneRecipeEnrichment};
use crate::RuneError;
use rune::runtime::RuntimeContext;
use rune::{Context, Diagnostics, Source, Sources, Vm};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

/// Configuration for event handler discovery
#[derive(Debug, Clone)]
pub struct EventHandlerConfig {
    /// Base directories to search for `runes/events/` folders
    pub base_directories: Vec<PathBuf>,
}

impl EventHandlerConfig {
    /// Create config with default directories
    pub fn with_defaults(kiln_path: Option<&Path>) -> Self {
        let mut dirs = vec![];

        // Global runes directory
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".crucible"));
        }

        // Kiln-specific runes directory
        if let Some(kiln) = kiln_path {
            dirs.push(kiln.to_path_buf());
        }

        Self {
            base_directories: dirs,
        }
    }

    /// Get all event directories for a given event name
    fn event_directories(&self, event_name: &str) -> Vec<PathBuf> {
        self.base_directories
            .iter()
            .map(|base| base.join("runes").join("events").join(event_name))
            .filter(|p| p.is_dir())
            .collect()
    }
}

/// Handles events by discovering and running Rune scripts
pub struct EventHandler {
    config: EventHandlerConfig,
    context: Arc<Context>,
    runtime: Arc<RuntimeContext>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(config: EventHandlerConfig) -> Result<Self, RuneError> {
        let mut context =
            Context::with_default_modules().map_err(|e| RuneError::Context(e.to_string()))?;

        // Install standard modules
        context
            .install(rune_modules::json::module(false)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Install regex module
        context
            .install(regex_module()?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Install our crucible module with types
        context
            .install(crucible_module()?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        let runtime = Arc::new(
            context
                .runtime()
                .map_err(|e| RuneError::Context(e.to_string()))?,
        );

        Ok(Self {
            config,
            context: Arc::new(context),
            runtime,
        })
    }

    /// Ensure event directories exist (creates empty folders)
    pub fn ensure_event_directories(&self, event_names: &[&str]) -> Result<(), RuneError> {
        for base in &self.config.base_directories {
            for event_name in event_names {
                let event_dir = base.join("runes").join("events").join(event_name);
                if !event_dir.exists() {
                    std::fs::create_dir_all(&event_dir)
                        .map_err(|e| RuneError::Io(format!("Failed to create {:?}: {}", event_dir, e)))?;
                    debug!("Created event directory: {:?}", event_dir);
                }
            }
        }
        Ok(())
    }

    /// Discover handler scripts for an event
    fn discover_handlers(&self, event_name: &str) -> Vec<PathBuf> {
        let mut handlers = vec![];

        for dir in self.config.event_directories(event_name) {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "rn").unwrap_or(false) {
                        handlers.push(path);
                    }
                }
            }
        }

        // Sort for deterministic ordering
        handlers.sort();
        handlers
    }

    /// Process an event through all handlers
    ///
    /// Each handler receives the current state and returns enrichment.
    /// Enrichments are applied in order.
    pub async fn process_event<E>(&self, mut event: E) -> Result<E, RuneError>
    where
        E: CrucibleEvent,
    {
        let handlers = self.discover_handlers(E::NAME);

        if handlers.is_empty() {
            debug!("No handlers found for event: {}", E::NAME);
            return Ok(event);
        }

        debug!(
            "Found {} handlers for event: {}",
            handlers.len(),
            E::NAME
        );

        for handler_path in handlers {
            debug!("Running handler: {:?}", handler_path);

            match self
                .run_handler::<E>(&handler_path, &event)
                .await
            {
                Ok(Some(enrichment)) => {
                    event.apply_enrichment(enrichment);
                    debug!("Applied enrichment from {:?}", handler_path);
                }
                Ok(None) => {
                    debug!("Handler {:?} returned no enrichment", handler_path);
                }
                Err(e) => {
                    warn!("Handler {:?} failed: {}", handler_path, e);
                    // Continue with other handlers
                }
            }
        }

        Ok(event)
    }

    /// Run a single handler script
    async fn run_handler<E>(
        &self,
        script_path: &Path,
        event: &E,
    ) -> Result<Option<E::Enrichment>, RuneError>
    where
        E: CrucibleEvent,
    {
        // Read script
        let source_code =
            std::fs::read_to_string(script_path).map_err(|e| RuneError::Io(e.to_string()))?;

        // Compile
        let mut sources = Sources::new();
        let script_name = script_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("handler");

        sources
            .insert(
                Source::new(script_name, &source_code)
                    .map_err(|e| RuneError::Compile(e.to_string()))?,
            )
            .map_err(|e| RuneError::Compile(e.to_string()))?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            for diag in diagnostics.diagnostics() {
                warn!("Rune diagnostic: {:?}", diag);
            }
        }

        let unit = result.map_err(|e| RuneError::Compile(e.to_string()))?;
        let unit = Arc::new(unit);

        // Create VM
        let mut vm = Vm::new(self.runtime.clone(), unit);

        // Convert event to JSON for passing to script
        let event_json = event.to_json().map_err(|e| RuneError::Conversion(e.to_string()))?;

        // Look for handler function: on_<event_name>
        let handler_name = format!("on_{}", E::NAME);
        let hash = rune::Hash::type_hash([handler_name.as_str()]);

        // Convert JSON to Rune value
        let event_value = json_to_rune_value(event_json)?;

        // Call the handler
        let output = vm
            .call(hash, (event_value,))
            .map_err(|e| RuneError::Execution(e.to_string()))?;

        // Convert output back to JSON
        let output_json = rune_value_to_json(output)?;

        // If null/empty, no enrichment
        if output_json.is_null() {
            return Ok(None);
        }

        // Parse as enrichment
        let enrichment: E::Enrichment = serde_json::from_value(output_json)
            .map_err(|e| RuneError::Conversion(e.to_string()))?;

        Ok(Some(enrichment))
    }

    /// Process multiple events (convenience method)
    pub async fn process_recipes(
        &self,
        recipes: Vec<EnrichedRecipe>,
    ) -> Result<Vec<EnrichedRecipe>, RuneError> {
        let mut results = Vec::with_capacity(recipes.len());
        for recipe in recipes {
            results.push(self.process_event(recipe).await?);
        }
        Ok(results)
    }
}

/// Convert JSON value to Rune value
fn json_to_rune_value(value: JsonValue) -> Result<rune::Value, RuneError> {
    use rune::runtime::ToValue;

    match value {
        JsonValue::Null => Ok(rune::Value::empty()),
        JsonValue::Bool(b) => b.to_value().map_err(|e| RuneError::Conversion(e.to_string())),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_value()
                    .map_err(|e| RuneError::Conversion(e.to_string()))
            } else if let Some(f) = n.as_f64() {
                f.to_value()
                    .map_err(|e| RuneError::Conversion(e.to_string()))
            } else {
                Err(RuneError::Conversion("Invalid number".to_string()))
            }
        }
        JsonValue::String(s) => s.to_value().map_err(|e| RuneError::Conversion(e.to_string())),
        JsonValue::Array(arr) => {
            let values: Vec<rune::Value> = arr
                .into_iter()
                .map(json_to_rune_value)
                .collect::<Result<Vec<_>, _>>()?;
            values
                .to_value()
                .map_err(|e| RuneError::Conversion(e.to_string()))
        }
        JsonValue::Object(map) => {
            let obj: std::collections::HashMap<String, rune::Value> = map
                .into_iter()
                .map(|(k, v)| Ok((k, json_to_rune_value(v)?)))
                .collect::<Result<_, RuneError>>()?;
            obj.to_value()
                .map_err(|e| RuneError::Conversion(e.to_string()))
        }
    }
}

/// Convert Rune value to JSON
fn rune_value_to_json(value: rune::Value) -> Result<JsonValue, RuneError> {
    let type_info = value.type_info();
    let type_name = format!("{}", type_info);

    // Handle our custom types first
    if type_name.contains("RecipeEnrichment") {
        // Try to extract as our type
        if let Ok(enrichment) = rune::from_value::<RuneRecipeEnrichment>(value.clone()) {
            let inner = enrichment.into_inner();
            return serde_json::to_value(inner).map_err(|e| RuneError::Conversion(e.to_string()));
        }
    }

    // Standard type conversion
    if type_name.contains("String") {
        let s: String =
            rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        Ok(JsonValue::String(s))
    } else if type_name.contains("i64") {
        let i: i64 = rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        Ok(JsonValue::Number(i.into()))
    } else if type_name.contains("f64") {
        let f: f64 = rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        if let Some(n) = serde_json::Number::from_f64(f) {
            Ok(JsonValue::Number(n))
        } else {
            Ok(JsonValue::Null)
        }
    } else if type_name.contains("bool") {
        let b: bool = rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        Ok(JsonValue::Bool(b))
    } else if type_name == "unit" || type_name == "()" {
        Ok(JsonValue::Null)
    } else if type_name.contains("Vec") {
        let vec: Vec<rune::Value> =
            rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        let arr: Vec<JsonValue> = vec
            .into_iter()
            .map(rune_value_to_json)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JsonValue::Array(arr))
    } else if type_name.contains("Object") || type_name.contains("HashMap") {
        let map: std::collections::HashMap<String, rune::Value> =
            rune::from_value(value).map_err(|e| RuneError::Conversion(e.to_string()))?;
        let obj: serde_json::Map<String, JsonValue> = map
            .into_iter()
            .map(|(k, v)| Ok((k, rune_value_to_json(v)?)))
            .collect::<Result<_, RuneError>>()?;
        Ok(JsonValue::Object(obj))
    } else {
        // Unknown type - return debug representation
        debug!("Unknown Rune type: {}", type_name);
        Ok(JsonValue::String(format!("{:?}", value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_event_handler_config_defaults() {
        let config = EventHandlerConfig::with_defaults(None);
        // Should have at least the home directory
        assert!(!config.base_directories.is_empty() || dirs::home_dir().is_none());
    }

    #[test]
    fn test_event_handler_config_with_kiln() {
        let config = EventHandlerConfig::with_defaults(Some(Path::new("/tmp/test-kiln")));
        assert!(config
            .base_directories
            .iter()
            .any(|p| p == Path::new("/tmp/test-kiln")));
    }

    #[tokio::test]
    async fn test_event_handler_creation() {
        let config = EventHandlerConfig {
            base_directories: vec![],
        };
        let handler = EventHandler::new(config);
        assert!(handler.is_ok());
    }

    #[tokio::test]
    async fn test_process_event_no_handlers() {
        let config = EventHandlerConfig {
            base_directories: vec![],
        };
        let handler = EventHandler::new(config).unwrap();

        let recipe = EnrichedRecipe::from_recipe(
            "test".to_string(),
            Some("Run tests".to_string()),
            vec![],
            false,
        );

        let result = handler.process_event(recipe).await.unwrap();
        assert_eq!(result.name, "test");
        assert!(result.category.is_none()); // No enrichment applied
    }

    #[tokio::test]
    async fn test_process_event_with_handler() {
        let temp = TempDir::new().unwrap();
        let event_dir = temp.path().join("runes").join("events").join("recipe_discovered");
        std::fs::create_dir_all(&event_dir).unwrap();

        // Create a simple categorizer script
        let script = r#"
use crucible::categorize_by_name;

pub fn on_recipe_discovered(recipe) {
    let category = categorize_by_name(recipe["name"]);
    #{ category: category }
}
"#;
        std::fs::write(event_dir.join("categorizer.rn"), script).unwrap();

        let config = EventHandlerConfig {
            base_directories: vec![temp.path().to_path_buf()],
        };
        let handler = EventHandler::new(config).unwrap();

        let recipe = EnrichedRecipe::from_recipe(
            "test-unit".to_string(),
            Some("Run unit tests".to_string()),
            vec![],
            false,
        );

        let result = handler.process_event(recipe).await.unwrap();
        assert_eq!(result.name, "test-unit");
        assert_eq!(result.category, Some("testing".to_string()));
    }
}
