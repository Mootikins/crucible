//! Popup module for Steel scripts
//!
//! Provides popup entry and request creation for Steel scripts.
//!
//! ## Steel Usage
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

use crucible_core::interaction::{PopupRequest, PopupResponse};
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
}
