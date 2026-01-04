//! Popup and Panel modules for Steel scripts
//!
//! Provides popup entry, request, and interactive panel creation for Steel scripts.
//!
//! ## Popup Usage
//!
//! ```scheme
//! ;; Create a popup entry
//! (popup-entry "Option 1" "Description of option 1")
//!
//! ;; Create a popup request with entries
//! (popup-request "Select a note"
//!   (list
//!     (popup-entry "Daily Note" "Today's journal")
//!     (popup-entry "Todo List" #f)))
//!
//! ;; Create a request that allows free-text input
//! (popup-request-with-other "Search or select"
//!   (list (popup-entry "Recent" "Recently viewed")))
//! ```
//!
//! ## Panel Usage
//!
//! ```scheme
//! ;; Create a panel item
//! (panel-item "PostgreSQL" "Full-featured RDBMS")
//!
//! ;; Create an interactive panel
//! (panel "Select database"
//!   (list
//!     (panel-item "PostgreSQL" "Full-featured RDBMS")
//!     (panel-item "SQLite" "Embedded, single-file")))
//!
//! ;; Create a confirmation panel
//! (confirm "Delete this file?")
//!
//! ;; Create a selection panel from strings
//! (select "Pick one" (list "A" "B" "C"))
//! ```

use crucible_core::interaction::{
    InteractivePanel, PanelHints, PanelItem, PanelResult, PopupRequest, PopupResponse,
};
use crucible_core::types::PopupEntry;
use serde_json::Value as JsonValue;

/// PopupModule provides popup-related functions for Steel
pub struct PopupModule;

impl PopupModule {
    /// Create a new popup module
    pub fn new() -> Self {
        Self
    }

    /// Create a popup entry
    pub fn entry(label: &str, description: Option<&str>) -> PopupEntry {
        let mut entry = PopupEntry::new(label);
        if let Some(desc) = description {
            entry = entry.with_description(desc);
        }
        entry
    }

    /// Create a popup entry with data
    pub fn entry_with_data(label: &str, description: Option<&str>, data: JsonValue) -> PopupEntry {
        let mut entry = PopupEntry::new(label);
        if let Some(desc) = description {
            entry = entry.with_description(desc);
        }
        entry.with_data(data)
    }

    /// Create a popup request
    pub fn request(title: &str, entries: Vec<PopupEntry>) -> PopupRequest {
        PopupRequest::new(title).entries(entries)
    }

    /// Create a popup request that allows free-text input
    pub fn request_with_other(title: &str, entries: Vec<PopupEntry>) -> PopupRequest {
        PopupRequest::new(title).entries(entries).allow_other()
    }

    /// Create a popup response for a selection
    pub fn response_selected(index: usize, entry: PopupEntry) -> PopupResponse {
        PopupResponse::selected(index, entry)
    }

    /// Create a popup response with free-text
    pub fn response_other(text: &str) -> PopupResponse {
        PopupResponse::other(text)
    }

    /// Create an empty popup response (dismissed)
    pub fn response_none() -> PopupResponse {
        PopupResponse::none()
    }

    /// Generate Steel code that defines the popup functions
    ///
    /// These are stubs that demonstrate the API. In practice, these would
    /// be replaced by Rust-backed functions when registered with an executor.
    pub fn steel_stubs() -> &'static str {
        r#"
;; Popup module functions for creating popup entries and requests
;;
;; These allow Steel scripts to create popup requests for user interaction.

;; Create a popup entry with label and optional description
;; (popup-entry "Label" "Description") or (popup-entry "Label" #f)
(define (popup-entry label description)
  (hash
    'label label
    'description description))

;; Create a popup entry with additional data
(define (popup-entry-with-data label description data)
  (hash
    'label label
    'description description
    'data data))

;; Create a popup request from title and list of entries
(define (popup-request title entries)
  (hash
    'title title
    'entries entries
    'allow_other #f))

;; Create a popup request that allows free-text input
(define (popup-request-with-other title entries)
  (hash
    'title title
    'entries entries
    'allow_other #t))

;; Create a response indicating selection
(define (popup-response-selected index entry)
  (hash
    'selected_index index
    'selected_entry entry
    'other #f))

;; Create a response with free-text input
(define (popup-response-other text)
  (hash
    'selected_index #f
    'selected_entry #f
    'other text))

;; Create an empty response (popup dismissed)
(define (popup-response-none)
  (hash
    'selected_index #f
    'selected_entry #f
    'other #f))
"#
    }
}

impl Default for PopupModule {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// PanelModule - Interactive Panel for Steel
// =============================================================================

/// PanelModule provides interactive panel functions for Steel
pub struct PanelModule;

impl PanelModule {
    /// Create a new panel module
    pub fn new() -> Self {
        Self
    }

    /// Create a panel item
    pub fn item(label: &str, description: Option<&str>) -> PanelItem {
        let mut item = PanelItem::new(label);
        if let Some(desc) = description {
            item = item.with_description(desc);
        }
        item
    }

    /// Create a panel item with data
    pub fn item_with_data(label: &str, description: Option<&str>, data: JsonValue) -> PanelItem {
        let mut item = PanelItem::new(label);
        if let Some(desc) = description {
            item = item.with_description(desc);
        }
        item.with_data(data)
    }

    /// Create default panel hints
    pub fn hints() -> PanelHints {
        PanelHints::default()
    }

    /// Create panel hints with filtering enabled
    pub fn hints_filterable() -> PanelHints {
        PanelHints::default().filterable()
    }

    /// Create panel hints with multi-select enabled
    pub fn hints_multi_select() -> PanelHints {
        PanelHints::default().multi_select()
    }

    /// Create panel hints with "other" option enabled
    pub fn hints_allow_other() -> PanelHints {
        PanelHints::default().allow_other()
    }

    /// Create an interactive panel
    pub fn panel(header: &str, items: Vec<PanelItem>) -> InteractivePanel {
        InteractivePanel::new(header).items(items)
    }

    /// Create an interactive panel with hints
    pub fn panel_with_hints(
        header: &str,
        items: Vec<PanelItem>,
        hints: PanelHints,
    ) -> InteractivePanel {
        InteractivePanel::new(header).items(items).hints(hints)
    }

    /// Create a confirmation panel (Yes/No)
    pub fn confirm(message: &str) -> InteractivePanel {
        InteractivePanel::new(message).items([PanelItem::new("Yes"), PanelItem::new("No")])
    }

    /// Create a single-select panel from string choices
    pub fn select(header: &str, choices: Vec<&str>) -> InteractivePanel {
        let items: Vec<PanelItem> = choices.into_iter().map(PanelItem::new).collect();
        InteractivePanel::new(header).items(items)
    }

    /// Create a multi-select panel from string choices
    pub fn multi_select(header: &str, choices: Vec<&str>) -> InteractivePanel {
        let items: Vec<PanelItem> = choices.into_iter().map(PanelItem::new).collect();
        InteractivePanel::new(header)
            .items(items)
            .hints(PanelHints::default().multi_select())
    }

    /// Create a panel result with selected indices
    pub fn result_selected(indices: Vec<usize>) -> PanelResult {
        PanelResult::selected(indices)
    }

    /// Create a cancelled panel result
    pub fn result_cancelled() -> PanelResult {
        PanelResult::cancelled()
    }

    /// Create a panel result with "other" text
    pub fn result_other(text: &str) -> PanelResult {
        PanelResult::other(text)
    }

    /// Generate Steel code that defines the panel functions
    pub fn steel_stubs() -> &'static str {
        r#"
;; Panel module functions for creating interactive panels
;;
;; These allow Steel scripts to create interactive UI panels.

;; Create a panel item with label and optional description
;; (panel-item "Label" "Description") or (panel-item "Label" #f)
(define (panel-item label description)
  (hash
    'label label
    'description description))

;; Create a panel item with additional data
(define (panel-item-with-data label description data)
  (hash
    'label label
    'description description
    'data data))

;; Create default panel hints
(define (panel-hints)
  (hash
    'filterable #f
    'multi_select #f
    'allow_other #f))

;; Create panel hints with filtering
(define (panel-hints-filterable)
  (hash
    'filterable #t
    'multi_select #f
    'allow_other #f))

;; Create panel hints with multi-select
(define (panel-hints-multi-select)
  (hash
    'filterable #f
    'multi_select #t
    'allow_other #f))

;; Create an interactive panel from header and items
(define (panel header items)
  (hash
    'header header
    'items items
    'hints (panel-hints)))

;; Create an interactive panel with custom hints
(define (panel-with-hints header items hints)
  (hash
    'header header
    'items items
    'hints hints))

;; Create a confirmation panel (Yes/No)
(define (confirm message)
  (panel message
    (list (panel-item "Yes" #f) (panel-item "No" #f))))

;; Create a selection panel from string choices
(define (select header choices)
  (panel header
    (map (lambda (c) (panel-item c #f)) choices)))

;; Create a multi-select panel from string choices
(define (multi-select header choices)
  (panel-with-hints header
    (map (lambda (c) (panel-item c #f)) choices)
    (panel-hints-multi-select)))

;; Create a result with selected indices
(define (panel-result-selected indices)
  (hash
    'cancelled #f
    'selected indices
    'other #f))

;; Create a cancelled result
(define (panel-result-cancelled)
  (hash
    'cancelled #t
    'selected '()
    'other #f))

;; Create a result with "other" text
(define (panel-result-other text)
  (hash
    'cancelled #f
    'selected '()
    'other text))
"#
    }
}

impl Default for PanelModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_entry_creation() {
        let entry = PopupModule::entry("Test", Some("Description"));
        assert_eq!(entry.label, "Test");
        assert_eq!(entry.description, Some("Description".to_string()));
    }

    #[test]
    fn test_popup_entry_without_description() {
        let entry = PopupModule::entry("Simple", None);
        assert_eq!(entry.label, "Simple");
        assert!(entry.description.is_none());
    }

    #[test]
    fn test_popup_request_creation() {
        let entries = vec![
            PopupModule::entry("Option 1", Some("First option")),
            PopupModule::entry("Option 2", None),
        ];
        let request = PopupModule::request("Select", entries);

        assert_eq!(request.title, "Select");
        assert_eq!(request.entries.len(), 2);
        assert!(!request.allow_other);
    }

    #[test]
    fn test_popup_request_with_other() {
        let entries = vec![PopupModule::entry("Preset", None)];
        let request = PopupModule::request_with_other("Choose or type", entries);

        assert!(request.allow_other);
    }

    #[test]
    fn test_popup_response_selected() {
        let entry = PopupModule::entry("Selected", None);
        let response = PopupModule::response_selected(0, entry);

        assert_eq!(response.selected_index, Some(0));
        assert!(response.selected_entry.is_some());
    }

    #[test]
    fn test_popup_response_other() {
        let response = PopupModule::response_other("Custom text");

        assert!(response.selected_index.is_none());
        assert_eq!(response.other, Some("Custom text".to_string()));
    }

    #[test]
    fn test_steel_stubs_not_empty() {
        let stubs = PopupModule::steel_stubs();
        assert!(stubs.contains("popup-entry"));
        assert!(stubs.contains("popup-request"));
        assert!(stubs.contains("popup-request-with-other"));
    }

    // ==========================================================================
    // PanelModule Tests
    // ==========================================================================

    #[test]
    fn test_panel_item_creation() {
        let item = PanelModule::item("PostgreSQL", Some("Full-featured RDBMS"));
        assert_eq!(item.label, "PostgreSQL");
        assert_eq!(item.description, Some("Full-featured RDBMS".to_string()));
    }

    #[test]
    fn test_panel_item_without_description() {
        let item = PanelModule::item("SQLite", None);
        assert_eq!(item.label, "SQLite");
        assert!(item.description.is_none());
    }

    #[test]
    fn test_panel_creation() {
        let items = vec![
            PanelModule::item("Option 1", Some("First")),
            PanelModule::item("Option 2", None),
        ];
        let panel = PanelModule::panel("Select database", items);

        assert_eq!(panel.header, "Select database");
        assert_eq!(panel.items.len(), 2);
    }

    #[test]
    fn test_panel_with_hints() {
        let items = vec![PanelModule::item("Choice", None)];
        let hints = PanelModule::hints_multi_select();
        let panel = PanelModule::panel_with_hints("Pick many", items, hints);

        assert!(panel.hints.multi_select);
    }

    #[test]
    fn test_confirm_panel() {
        let panel = PanelModule::confirm("Delete this file?");

        assert_eq!(panel.header, "Delete this file?");
        assert_eq!(panel.items.len(), 2);
        assert_eq!(panel.items[0].label, "Yes");
        assert_eq!(panel.items[1].label, "No");
    }

    #[test]
    fn test_select_panel() {
        let panel = PanelModule::select("Pick one", vec!["A", "B", "C"]);

        assert_eq!(panel.header, "Pick one");
        assert_eq!(panel.items.len(), 3);
        assert!(!panel.hints.multi_select);
    }

    #[test]
    fn test_multi_select_panel() {
        let panel = PanelModule::multi_select("Pick many", vec!["X", "Y", "Z"]);

        assert_eq!(panel.items.len(), 3);
        assert!(panel.hints.multi_select);
    }

    #[test]
    fn test_panel_result_selected() {
        let result = PanelModule::result_selected(vec![0, 2]);

        assert!(!result.cancelled);
        assert_eq!(result.selected, vec![0, 2]);
    }

    #[test]
    fn test_panel_result_cancelled() {
        let result = PanelModule::result_cancelled();

        assert!(result.cancelled);
        assert!(result.selected.is_empty());
    }

    #[test]
    fn test_panel_result_other() {
        let result = PanelModule::result_other("Custom input");

        assert!(!result.cancelled);
        assert_eq!(result.other, Some("Custom input".to_string()));
    }

    #[test]
    fn test_panel_steel_stubs_not_empty() {
        let stubs = PanelModule::steel_stubs();
        assert!(stubs.contains("panel-item"));
        assert!(stubs.contains("panel"));
        assert!(stubs.contains("confirm"));
        assert!(stubs.contains("select"));
        assert!(stubs.contains("multi-select"));
    }
}
