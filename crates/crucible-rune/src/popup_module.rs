//! Popup module for Rune
//!
//! Provides popup entry creation and popup request handling for Rune scripts.
//!
//! # Example
//!
//! ```rune
//! use crucible::popup::{PopupEntry, PopupRequest, entry, request};
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
//!
//! // Create a popup request for user selection
//! let req = request("Select a note", [
//!     entry("Daily Note", Some("Today's journal")),
//!     entry("Todo List", Some("Tasks for the week")),
//! ]);
//! ```

use crucible_core::interaction::PopupRequest;
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

    // Register the RunePopupRequest wrapper type
    module.ty::<RunePopupRequest>()?;

    // PopupRequest constructor methods
    module.function_meta(RunePopupRequest::new)?;
    module.function_meta(RunePopupRequest::entry)?;
    module.function_meta(RunePopupRequest::allow_other)?;

    // PopupRequest accessor methods
    module.function_meta(RunePopupRequest::title)?;
    module.function_meta(RunePopupRequest::entries)?;
    module.function_meta(RunePopupRequest::entry_count)?;

    // Convenience function: request(title, entries) -> PopupRequest
    module.function_meta(request)?;

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

// =============================================================================
// RunePopupRequest - Wrapper for PopupRequest
// =============================================================================

/// PopupRequest wrapper for Rune
///
/// Wraps crucible_core::interaction::PopupRequest with Rune-friendly methods.
/// Use this to create popup requests that can trigger user interactions.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::popup, name = PopupRequest)]
pub struct RunePopupRequest {
    inner: PopupRequest,
}

impl RunePopupRequest {
    // === Implementation methods (for Rust use) ===

    /// Create from a core PopupRequest
    pub fn from_core(request: PopupRequest) -> Self {
        Self { inner: request }
    }

    /// Convert to core PopupRequest
    pub fn into_core(self) -> PopupRequest {
        self.inner
    }

    /// Get a reference to the inner PopupRequest
    pub fn as_core(&self) -> &PopupRequest {
        &self.inner
    }

    /// Create a new popup request with title (impl)
    pub fn new_impl(title: String) -> Self {
        Self {
            inner: PopupRequest::new(title),
        }
    }

    /// Add an entry (impl)
    pub fn entry_impl(mut self, entry: RunePopupEntry) -> Self {
        self.inner = self.inner.entry(entry.into_core());
        self
    }

    /// Set allow_other (impl)
    pub fn allow_other_impl(mut self) -> Self {
        self.inner = self.inner.allow_other();
        self
    }

    /// Get the title (impl)
    pub fn title_impl(&self) -> String {
        self.inner.title.clone()
    }

    /// Get the entry count (impl)
    pub fn entry_count_impl(&self) -> usize {
        self.inner.entries.len()
    }

    // === Rune bindings ===

    /// Create a new popup request with a title
    #[rune::function(path = Self::new)]
    pub fn new(title: String) -> Self {
        Self::new_impl(title)
    }

    /// Add an entry (builder pattern)
    #[rune::function(path = Self::entry)]
    pub fn entry(self, entry: RunePopupEntry) -> Self {
        self.entry_impl(entry)
    }

    /// Allow free-text input (builder pattern)
    #[rune::function(path = Self::allow_other)]
    pub fn allow_other(self) -> Self {
        self.allow_other_impl()
    }

    /// Get the popup title
    #[rune::function(path = Self::title)]
    pub fn title(&self) -> String {
        self.title_impl()
    }

    /// Get all entries as a vector
    #[rune::function(path = Self::entries)]
    pub fn entries(&self) -> Vec<RunePopupEntry> {
        self.inner
            .entries
            .iter()
            .map(|e| RunePopupEntry::from_core(e.clone()))
            .collect()
    }

    /// Get the number of entries
    #[rune::function(path = Self::entry_count)]
    pub fn entry_count(&self) -> usize {
        self.entry_count_impl()
    }
}

/// Create a popup request (implementation for Rust use)
pub fn request_impl(title: String, entries: Vec<RunePopupEntry>) -> RunePopupRequest {
    let core_entries: Vec<PopupEntry> = entries.into_iter().map(|e| e.into_core()).collect();
    RunePopupRequest {
        inner: PopupRequest::new(title).entries(core_entries),
    }
}

/// Convenience function to create a popup request
///
/// # Arguments
/// * `title` - The popup title/prompt
/// * `entries` - Array of popup entries
///
/// # Example
/// ```rune
/// use crucible::popup::{entry, request};
///
/// let req = request("Select an option", [
///     entry("First", Some("The first option")),
///     entry("Second", None),
/// ]);
/// ```
#[rune::function]
fn request(title: String, entries: Vec<RunePopupEntry>) -> RunePopupRequest {
    request_impl(title, entries)
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

    // =========================================================================
    // PopupRequest tests
    // =========================================================================

    #[test]
    fn test_rune_popup_request_new() {
        let request = RunePopupRequest::new_impl("Select a note".to_string());
        assert_eq!(request.title_impl(), "Select a note");
        assert_eq!(request.entry_count_impl(), 0);
    }

    #[test]
    fn test_rune_popup_request_with_entries() {
        let entry1 = RunePopupEntry::new_impl("Note 1".to_string());
        let entry2 =
            RunePopupEntry::new_impl("Note 2".to_string()).with_description_impl("A note".to_string());

        let request = RunePopupRequest::new_impl("Select".to_string())
            .entry_impl(entry1)
            .entry_impl(entry2);

        assert_eq!(request.entry_count_impl(), 2);
    }

    #[test]
    fn test_request_convenience_function() {
        let entries = vec![
            entry_impl("First".to_string(), None),
            entry_impl("Second".to_string(), Some("Description".to_string())),
        ];

        let request = request_impl("Choose one".to_string(), entries);

        assert_eq!(request.title_impl(), "Choose one");
        assert_eq!(request.entry_count_impl(), 2);
    }

    #[test]
    fn test_popup_request_into_core() {
        let entry = entry_impl("Test".to_string(), Some("Desc".to_string()));
        let request =
            RunePopupRequest::new_impl("Title".to_string()).entry_impl(entry);

        let core = request.into_core();

        assert_eq!(core.title, "Title");
        assert_eq!(core.entries.len(), 1);
        assert_eq!(core.entries[0].label, "Test");
        assert_eq!(core.entries[0].description, Some("Desc".to_string()));
    }

    /// Test PopupRequest from Rune script
    #[test]
    fn test_popup_request_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(popup_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use crucible::popup::{PopupRequest, PopupEntry, entry, request};

            pub fn main() {
                // Test using convenience function
                let req = request("Select a note", [
                    entry("Daily", Some("Daily journal")),
                    entry("Todo", None),
                ]);
                req.entry_count()
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

        let unit = result.expect("Should compile script with popup request");
        let unit = Arc::new(unit);

        let mut vm = Vm::new(runtime, unit);
        let output = vm.call(rune::Hash::type_hash(["main"]), ()).unwrap();
        let count: usize = rune::from_value(output).unwrap();

        assert_eq!(count, 2, "Should have 2 entries");
    }

    /// Test PopupRequest builder pattern from Rune
    #[test]
    fn test_popup_request_builder_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(popup_module().unwrap()).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            use crucible::popup::{PopupRequest, PopupEntry};

            pub fn main() {
                let req = PopupRequest::new("Choose action")
                    .entry(PopupEntry::new("Save").with_description("Save file"))
                    .entry(PopupEntry::new("Discard"))
                    .allow_other();
                req.title()
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
        let title: String = rune::from_value(output).unwrap();

        assert_eq!(title, "Choose action");
    }
}
