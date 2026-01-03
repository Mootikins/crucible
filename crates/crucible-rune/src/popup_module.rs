//! Popup module for Rune
//!
//! Provides popup entry creation for Rune scripts.
//!
//! # Example
//!
//! ```rune
//! use crucible::popup::{PopupEntry, entry};
//!
//! // Create a simple entry
//! let item = entry("help", None);
//!
//! // Create an entry with description
//! let item = entry("search", Some("Search notes"));
//!
//! // Create using the type constructor
//! let item = PopupEntry::new("quit")
//!     .with_description("Exit the application");
//! ```

use crucible_core::types::PopupEntry;
use rune::runtime::VmResult;
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;

/// Create the popup module for Rune under crucible::popup namespace
pub fn popup_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("crucible", ["popup"])?;

    // Register the RunePopupEntry wrapper type
    module.ty::<RunePopupEntry>()?;

    // Constructor methods
    module.function_meta(RunePopupEntry::new)?;
    module.function_meta(RunePopupEntry::with_description)?;
    module.function_meta(RunePopupEntry::with_data)?;

    // Accessor methods
    module.function_meta(RunePopupEntry::label)?;
    module.function_meta(RunePopupEntry::description)?;
    module.function_meta(RunePopupEntry::has_data)?;

    // Convenience function: entry(label, description) -> PopupEntry
    module.function_meta(entry)?;

    Ok(module)
}

/// PopupEntry wrapper for Rune
///
/// Wraps crucible_core::types::PopupEntry with Rune-friendly methods.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::popup, name = PopupEntry)]
pub struct RunePopupEntry {
    inner: PopupEntry,
}

impl RunePopupEntry {
    // === Implementation methods (for Rust use) ===

    /// Create from a core PopupEntry
    pub fn from_core(entry: PopupEntry) -> Self {
        Self { inner: entry }
    }

    /// Convert to core PopupEntry
    pub fn into_core(self) -> PopupEntry {
        self.inner
    }

    /// Get a reference to the inner PopupEntry
    pub fn as_core(&self) -> &PopupEntry {
        &self.inner
    }

    /// Create a new popup entry with just a label (impl)
    pub fn new_impl(label: String) -> Self {
        Self {
            inner: PopupEntry::new(label),
        }
    }

    /// Add a description (impl)
    pub fn with_description_impl(mut self, description: String) -> Self {
        self.inner = self.inner.with_description(description);
        self
    }

    /// Add JSON data (impl)
    pub fn with_data_impl(mut self, data: JsonValue) -> Self {
        self.inner = self.inner.with_data(data);
        self
    }

    /// Get the label (impl)
    pub fn label_impl(&self) -> String {
        self.inner.label.clone()
    }

    /// Get the description (impl)
    pub fn description_impl(&self) -> Option<String> {
        self.inner.description.clone()
    }

    /// Check if entry has data (impl)
    pub fn has_data_impl(&self) -> bool {
        self.inner.data.is_some()
    }

    // === Rune bindings ===

    /// Create a new popup entry with just a label
    #[rune::function(path = Self::new)]
    pub fn new(label: String) -> Self {
        Self::new_impl(label)
    }

    /// Add a description (builder pattern)
    #[rune::function(path = Self::with_description)]
    pub fn with_description(self, description: String) -> Self {
        self.with_description_impl(description)
    }

    /// Add arbitrary JSON data (builder pattern)
    ///
    /// Accepts a Rune value and converts it to JSON for storage.
    #[rune::function(path = Self::with_data)]
    pub fn with_data(mut self, data: Value) -> VmResult<Self> {
        // Convert Rune value to JSON
        match rune_value_to_json(&data) {
            Ok(json) => {
                self.inner = self.inner.with_data(json);
                VmResult::Ok(self)
            }
            Err(e) => VmResult::panic(format!("Failed to convert data to JSON: {}", e)),
        }
    }

    /// Get the label
    #[rune::function(path = Self::label)]
    pub fn label(&self) -> String {
        self.label_impl()
    }

    /// Get the description (if any)
    #[rune::function(path = Self::description)]
    pub fn description(&self) -> Option<String> {
        self.description_impl()
    }

    /// Check if entry has data
    #[rune::function(path = Self::has_data)]
    pub fn has_data(&self) -> bool {
        self.has_data_impl()
    }
}

/// Create a popup entry (implementation for Rust use)
pub fn entry_impl(label: String, description: Option<String>) -> RunePopupEntry {
    let mut popup_entry = PopupEntry::new(label);
    if let Some(desc) = description {
        popup_entry = popup_entry.with_description(desc);
    }
    RunePopupEntry { inner: popup_entry }
}

/// Convenience function to create a popup entry
///
/// # Arguments
/// * `label` - The display text (required)
/// * `description` - Optional description text
///
/// # Example
/// ```rune
/// use crucible::popup::entry;
///
/// let item = entry("help", Some("Show help"));
/// let simple = entry("quit", None);
/// ```
#[rune::function]
fn entry(label: String, description: Option<String>) -> RunePopupEntry {
    entry_impl(label, description)
}

/// Convert a Rune Value to serde_json::Value
///
/// This is a simplified conversion for popup data. For more complex
/// conversions, use the mcp_types module.
fn rune_value_to_json(value: &Value) -> Result<JsonValue, String> {
    let type_info = value.type_info();
    let type_name = format!("{}", type_info);

    if type_name.contains("String") {
        let s: String =
            rune::from_value(value.clone()).map_err(|e| format!("String conversion: {}", e))?;
        Ok(JsonValue::String(s))
    } else if type_name.contains("i64") {
        let i: i64 =
            rune::from_value(value.clone()).map_err(|e| format!("i64 conversion: {}", e))?;
        Ok(JsonValue::Number(i.into()))
    } else if type_name.contains("f64") {
        let f: f64 =
            rune::from_value(value.clone()).map_err(|e| format!("f64 conversion: {}", e))?;
        serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or_else(|| "Invalid float (NaN or infinity)".to_string())
    } else if type_name.contains("bool") {
        let b: bool =
            rune::from_value(value.clone()).map_err(|e| format!("bool conversion: {}", e))?;
        Ok(JsonValue::Bool(b))
    } else if type_name.contains("unit") || type_name == "()" {
        Ok(JsonValue::Null)
    } else if type_name.contains("Vec") {
        let vec: Vec<Value> =
            rune::from_value(value.clone()).map_err(|e| format!("Vec conversion: {}", e))?;
        let arr: Result<Vec<JsonValue>, String> = vec.iter().map(rune_value_to_json).collect();
        Ok(JsonValue::Array(arr?))
    } else if type_name.contains("Object") || type_name.contains("HashMap") {
        let map: std::collections::HashMap<String, Value> =
            rune::from_value(value.clone()).map_err(|e| format!("Object conversion: {}", e))?;
        let obj: Result<serde_json::Map<String, JsonValue>, String> = map
            .iter()
            .map(|(k, v)| rune_value_to_json(v).map(|jv| (k.clone(), jv)))
            .collect();
        Ok(JsonValue::Object(obj?))
    } else {
        // Fallback: use debug representation as string
        Ok(JsonValue::String(format!("{:?}", value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_module_creation() {
        let module = popup_module();
        assert!(module.is_ok(), "Should create popup module");
    }

    #[test]
    fn test_rune_popup_entry_new() {
        let entry = RunePopupEntry::new_impl("test".to_string());
        assert_eq!(entry.label_impl(), "test");
        assert!(entry.description_impl().is_none());
        assert!(!entry.has_data_impl());
    }

    #[test]
    fn test_rune_popup_entry_with_description() {
        let entry = RunePopupEntry::new_impl("help".to_string())
            .with_description_impl("Show help text".to_string());
        assert_eq!(entry.label_impl(), "help");
        assert_eq!(entry.description_impl(), Some("Show help text".to_string()));
    }

    #[test]
    fn test_entry_convenience_function() {
        let simple = entry_impl("quit".to_string(), None);
        assert_eq!(simple.label_impl(), "quit");
        assert!(simple.description_impl().is_none());

        let with_desc = entry_impl("help".to_string(), Some("Show help".to_string()));
        assert_eq!(with_desc.label_impl(), "help");
        assert_eq!(with_desc.description_impl(), Some("Show help".to_string()));
    }

    #[test]
    fn test_into_core_conversion() {
        let rune_entry =
            RunePopupEntry::new_impl("test".to_string()).with_description_impl("desc".to_string());
        let core_entry = rune_entry.into_core();

        assert_eq!(core_entry.label, "test");
        assert_eq!(core_entry.description, Some("desc".to_string()));
    }

    /// Test that popup module can be used from Rune script
    #[test]
    fn test_popup_entry_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        // Create context with popup module
        let mut context = Context::with_default_modules().unwrap();
        context.install(popup_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        // Rune script that creates popup entries
        let script = r#"
            use crucible::popup::{PopupEntry, entry};

            pub fn main() {
                // Test convenience function
                let item = entry("help", Some("Show help"));
                item.label()
            }
        "#;

        // Compile
        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile script with popup module");
        let unit = Arc::new(unit);

        // Execute
        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: String = rune::from_value(output).unwrap();

        assert_eq!(output, "help", "Should return the label");
    }

    /// Test PopupEntry::new() constructor from Rune
    #[test]
    fn test_popup_entry_constructor_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(popup_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use crucible::popup::PopupEntry;

            pub fn main() {
                let item = PopupEntry::new("quit")
                    .with_description("Exit application");
                item.description()
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        let unit = result.expect("Should compile");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let output: Option<String> = rune::from_value(output).unwrap();

        assert_eq!(output, Some("Exit application".to_string()));
    }
}
