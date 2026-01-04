//! Panel module for Rune
//!
//! Provides the InteractivePanel primitive for building interactive UI flows.
//!
//! # Example
//!
//! ```rune
//! use crucible::panel::{Panel, PanelItem, PanelHints, item, panel};
//!
//! // Create a simple selection panel
//! let p = panel("Select database", [
//!     item("PostgreSQL", Some("Full-featured RDBMS")),
//!     item("SQLite", Some("Embedded, single-file")),
//! ]);
//!
//! // Create a panel with hints
//! let hints = PanelHints::new()
//!     .filterable()
//!     .allow_other();
//!
//! let p = Panel::new("Search notes")
//!     .item(item("Daily Note", None))
//!     .item(item("Todo List", None))
//!     .hints(hints);
//!
//! // Convenience functions
//! let confirmed = confirm("Delete this file?");
//! let choice = select("Pick one", ["A", "B", "C"]);
//! ```

use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem, PanelResult};
use rune::runtime::VmResult;
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;

/// Create the panel module for Rune under crucible::panel namespace
pub fn panel_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("crucible", ["panel"])?;

    // Register PanelItem wrapper
    module.ty::<RunePanelItem>()?;
    module.function_meta(RunePanelItem::new)?;
    module.function_meta(RunePanelItem::with_description)?;
    module.function_meta(RunePanelItem::with_data)?;
    module.function_meta(RunePanelItem::label)?;
    module.function_meta(RunePanelItem::description)?;

    // Convenience function: item(label, description) -> PanelItem
    module.function_meta(item)?;

    // Register PanelHints wrapper
    module.ty::<RunePanelHints>()?;
    module.function_meta(RunePanelHints::new)?;
    module.function_meta(RunePanelHints::filterable)?;
    module.function_meta(RunePanelHints::multi_select)?;
    module.function_meta(RunePanelHints::allow_other)?;
    module.function_meta(RunePanelHints::initial_selection)?;
    module.function_meta(RunePanelHints::initial_filter)?;

    // Register Panel (InteractivePanel) wrapper
    module.ty::<RunePanel>()?;
    module.function_meta(RunePanel::new)?;
    module.function_meta(RunePanel::item)?;
    module.function_meta(RunePanel::hints)?;
    module.function_meta(RunePanel::header)?;
    module.function_meta(RunePanel::items)?;

    // Convenience function: panel(header, items) -> Panel
    module.function_meta(panel)?;

    // Register PanelResult wrapper
    module.ty::<RunePanelResult>()?;
    module.function_meta(RunePanelResult::selected)?;
    module.function_meta(RunePanelResult::cancelled)?;
    module.function_meta(RunePanelResult::other)?;
    module.function_meta(RunePanelResult::is_cancelled)?;
    module.function_meta(RunePanelResult::selected_indices)?;
    module.function_meta(RunePanelResult::other_text)?;

    // Convenience functions
    module.function_meta(confirm)?;
    module.function_meta(select)?;
    module.function_meta(multi_select)?;

    Ok(module)
}

// =============================================================================
// PanelItem
// =============================================================================

/// PanelItem wrapper for Rune
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::panel, name = PanelItem)]
pub struct RunePanelItem {
    inner: PanelItem,
}

impl RunePanelItem {
    /// Create from a core PanelItem
    pub fn from_core(item: PanelItem) -> Self {
        Self { inner: item }
    }

    /// Convert to core PanelItem
    pub fn into_core(self) -> PanelItem {
        self.inner
    }

    /// Implementation: create new item
    pub fn new_impl(label: String) -> Self {
        Self {
            inner: PanelItem::new(label),
        }
    }

    /// Implementation: add description
    pub fn with_description_impl(mut self, description: String) -> Self {
        self.inner = self.inner.with_description(description);
        self
    }

    /// Implementation: add data
    pub fn with_data_impl(mut self, data: JsonValue) -> Self {
        self.inner = self.inner.with_data(data);
        self
    }

    // Rune-exposed methods

    /// Create a new panel item
    #[rune::function(path = Self::new)]
    fn new(label: &str) -> Self {
        Self::new_impl(label.to_string())
    }

    /// Add a description
    #[rune::function(instance)]
    fn with_description(self, description: &str) -> Self {
        self.with_description_impl(description.to_string())
    }

    /// Add data
    #[rune::function(instance)]
    fn with_data(mut self, data: Value) -> VmResult<Self> {
        if let Ok(json) = rune_value_to_json(&data) {
            self.inner = self.inner.with_data(json);
        }
        VmResult::Ok(self)
    }

    /// Get the label
    #[rune::function(instance)]
    fn label(&self) -> String {
        self.inner.label.clone()
    }

    /// Get the description
    #[rune::function(instance)]
    fn description(&self) -> Option<String> {
        self.inner.description.clone()
    }
}

/// Convenience function to create a panel item
#[rune::function]
pub fn item(label: String, description: Option<String>) -> RunePanelItem {
    let mut item = RunePanelItem::new_impl(label);
    if let Some(desc) = description {
        item = item.with_description_impl(desc);
    }
    item
}

// =============================================================================
// PanelHints
// =============================================================================

/// PanelHints wrapper for Rune
#[derive(Debug, Clone, Any, Default)]
#[rune(item = ::crucible::panel, name = PanelHints)]
pub struct RunePanelHints {
    inner: PanelHints,
}

impl RunePanelHints {
    /// Create from core PanelHints
    pub fn from_core(hints: PanelHints) -> Self {
        Self { inner: hints }
    }

    /// Convert to core PanelHints
    pub fn into_core(self) -> PanelHints {
        self.inner
    }

    // Rune-exposed methods

    /// Create new hints with defaults
    #[rune::function(path = Self::new)]
    fn new() -> Self {
        Self::default()
    }

    /// Enable filtering
    #[rune::function(instance)]
    fn filterable(mut self) -> Self {
        self.inner = self.inner.filterable();
        self
    }

    /// Enable multi-select
    #[rune::function(instance)]
    fn multi_select(mut self) -> Self {
        self.inner = self.inner.multi_select();
        self
    }

    /// Allow "other" text input
    #[rune::function(instance)]
    fn allow_other(mut self) -> Self {
        self.inner = self.inner.allow_other();
        self
    }

    /// Set initial selection indices
    #[rune::function(instance)]
    fn initial_selection(mut self, indices: Vec<usize>) -> Self {
        self.inner = self.inner.initial_selection(indices);
        self
    }

    /// Set initial filter text
    #[rune::function(instance)]
    fn initial_filter(mut self, filter: String) -> Self {
        self.inner.initial_filter = Some(filter);
        self
    }
}

// =============================================================================
// Panel (InteractivePanel)
// =============================================================================

/// InteractivePanel wrapper for Rune
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::panel, name = Panel)]
pub struct RunePanel {
    inner: InteractivePanel,
}

impl RunePanel {
    /// Create from core InteractivePanel
    pub fn from_core(panel: InteractivePanel) -> Self {
        Self { inner: panel }
    }

    /// Convert to core InteractivePanel
    pub fn into_core(self) -> InteractivePanel {
        self.inner
    }

    /// Implementation: create new panel
    pub fn new_impl(header: String) -> Self {
        Self {
            inner: InteractivePanel::new(header),
        }
    }

    // Rune-exposed methods

    /// Create a new panel
    #[rune::function(path = Self::new)]
    fn new(header: &str) -> Self {
        Self::new_impl(header.to_string())
    }

    /// Add an item to the panel
    #[rune::function(instance)]
    fn item(mut self, item: RunePanelItem) -> Self {
        self.inner.items.push(item.into_core());
        self
    }

    /// Set hints
    #[rune::function(instance)]
    fn hints(mut self, hints: RunePanelHints) -> Self {
        self.inner = self.inner.hints(hints.into_core());
        self
    }

    /// Get the header
    #[rune::function(instance)]
    fn header(&self) -> String {
        self.inner.header.clone()
    }

    /// Get the items
    #[rune::function(instance)]
    fn items(&self) -> Vec<RunePanelItem> {
        self.inner
            .items
            .iter()
            .cloned()
            .map(RunePanelItem::from_core)
            .collect()
    }
}

/// Convenience function to create a panel
#[rune::function]
pub fn panel(header: &str, items: Vec<RunePanelItem>) -> RunePanel {
    let core_items: Vec<PanelItem> = items.into_iter().map(|i| i.into_core()).collect();
    RunePanel {
        inner: InteractivePanel::new(header).items(core_items),
    }
}

// =============================================================================
// PanelResult
// =============================================================================

/// PanelResult wrapper for Rune
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible::panel, name = PanelResult)]
pub struct RunePanelResult {
    inner: PanelResult,
}

impl RunePanelResult {
    /// Create from core PanelResult
    pub fn from_core(result: PanelResult) -> Self {
        Self { inner: result }
    }

    /// Convert to core PanelResult
    pub fn into_core(self) -> PanelResult {
        self.inner
    }

    // Rune-exposed methods

    /// Create a result with selected indices
    #[rune::function(path = Self::selected)]
    fn selected(indices: Vec<usize>) -> Self {
        Self {
            inner: PanelResult::selected(indices),
        }
    }

    /// Create a cancelled result
    #[rune::function(path = Self::cancelled)]
    fn cancelled() -> Self {
        Self {
            inner: PanelResult::cancelled(),
        }
    }

    /// Create a result with "other" text
    #[rune::function(path = Self::other)]
    fn other(text: &str) -> Self {
        Self {
            inner: PanelResult::other(text),
        }
    }

    /// Check if cancelled
    #[rune::function(instance)]
    fn is_cancelled(&self) -> bool {
        self.inner.cancelled
    }

    /// Get selected indices
    #[rune::function(instance)]
    fn selected_indices(&self) -> Vec<usize> {
        self.inner.selected.clone()
    }

    /// Get "other" text if present
    #[rune::function(instance)]
    fn other_text(&self) -> Option<String> {
        self.inner.other.clone()
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Create a confirmation panel (Yes/No)
#[rune::function]
pub fn confirm(message: &str) -> RunePanel {
    RunePanel {
        inner: InteractivePanel::new(message).items([PanelItem::new("Yes"), PanelItem::new("No")]),
    }
}

/// Create a single-select panel from string choices
#[rune::function]
pub fn select(header: &str, choices: Vec<String>) -> RunePanel {
    let items: Vec<PanelItem> = choices.into_iter().map(PanelItem::new).collect();
    RunePanel {
        inner: InteractivePanel::new(header).items(items),
    }
}

/// Create a multi-select panel from string choices
#[rune::function]
pub fn multi_select(header: &str, choices: Vec<String>) -> RunePanel {
    let items: Vec<PanelItem> = choices.into_iter().map(PanelItem::new).collect();
    RunePanel {
        inner: InteractivePanel::new(header)
            .items(items)
            .hints(PanelHints::default().multi_select()),
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Convert a Rune Value to serde_json::Value
fn rune_value_to_json(value: &Value) -> Result<JsonValue, ()> {
    let type_info = value.type_info();
    let type_name = format!("{}", type_info);

    if type_name.contains("String") {
        let s: String = rune::from_value(value.clone()).map_err(|_| ())?;
        Ok(JsonValue::String(s))
    } else if type_name.contains("i64") {
        let i: i64 = rune::from_value(value.clone()).map_err(|_| ())?;
        Ok(JsonValue::Number(i.into()))
    } else if type_name.contains("f64") {
        let f: f64 = rune::from_value(value.clone()).map_err(|_| ())?;
        serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or(())
    } else if type_name.contains("bool") {
        let b: bool = rune::from_value(value.clone()).map_err(|_| ())?;
        Ok(JsonValue::Bool(b))
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_item_creation() {
        let item = RunePanelItem::new_impl("Test".to_string());
        assert_eq!(item.inner.label, "Test");
        assert!(item.inner.description.is_none());
    }

    #[test]
    fn test_panel_item_with_description() {
        let item = RunePanelItem::new_impl("Test".to_string())
            .with_description_impl("A description".to_string());
        assert_eq!(item.inner.description, Some("A description".to_string()));
    }

    #[test]
    fn test_panel_hints_chaining() {
        let mut hints = PanelHints::default();
        hints = hints.filterable();
        assert!(hints.filterable);

        hints = hints.multi_select();
        assert!(hints.multi_select);

        hints = hints.allow_other();
        assert!(hints.allow_other);

        // Verify wrapper works
        let rune_hints = RunePanelHints::from_core(hints);
        assert!(rune_hints.inner.filterable);
        assert!(rune_hints.inner.multi_select);
        assert!(rune_hints.inner.allow_other);
    }

    #[test]
    fn test_panel_creation() {
        let panel = RunePanel::new_impl("Select".to_string());
        assert_eq!(panel.inner.header, "Select");
        assert!(panel.inner.items.is_empty());
    }

    #[test]
    fn test_panel_with_items() {
        let items = vec![PanelItem::new("A"), PanelItem::new("B")];

        let panel = RunePanel::from_core(InteractivePanel::new("Choose").items(items));

        assert_eq!(panel.inner.items.len(), 2);
        assert_eq!(panel.inner.items[0].label, "A");
        assert_eq!(panel.inner.items[1].label, "B");
    }

    #[test]
    fn test_panel_result_selected() {
        let result = RunePanelResult::from_core(PanelResult::selected([0, 2]));
        assert!(!result.inner.cancelled);
        assert_eq!(result.inner.selected, vec![0, 2]);
    }

    #[test]
    fn test_panel_result_cancelled() {
        let result = RunePanelResult::from_core(PanelResult::cancelled());
        assert!(result.inner.cancelled);
    }

    #[test]
    fn test_panel_result_other() {
        let result = RunePanelResult::from_core(PanelResult::other("custom"));
        assert_eq!(result.inner.other, Some("custom".to_string()));
    }

    #[test]
    fn test_convenience_item() {
        let i = item_impl("Label".to_string(), Some("Desc".to_string()));
        assert_eq!(i.inner.label, "Label");
        assert_eq!(i.inner.description, Some("Desc".to_string()));
    }

    #[test]
    fn test_convenience_panel() {
        let items = vec![
            RunePanelItem::new_impl("A".to_string()),
            RunePanelItem::new_impl("B".to_string()),
        ];
        let p = panel_impl("Header", items);
        assert_eq!(p.inner.header, "Header");
        assert_eq!(p.inner.items.len(), 2);
    }

    /// Helper for tests - same as item but without Rune attribute
    fn item_impl(label: String, description: Option<String>) -> RunePanelItem {
        let mut item = RunePanelItem::new_impl(label);
        if let Some(desc) = description {
            item = item.with_description_impl(desc);
        }
        item
    }

    /// Helper for tests - same as panel but without Rune attribute
    fn panel_impl(header: &str, items: Vec<RunePanelItem>) -> RunePanel {
        let core_items: Vec<PanelItem> = items.into_iter().map(|i| i.into_core()).collect();
        RunePanel {
            inner: InteractivePanel::new(header).items(core_items),
        }
    }
}
