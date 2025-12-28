//! Graph traversal module for Rune scripts
//!
//! Provides functions for traversing note graphs (outlinks, inlinks).
//!
//! # Example
//!
//! ```rune
//! use graph::{find, outlinks, inlinks};
//!
//! // Build a graph from notes
//! let g = #{
//!     notes: [
//!         #{ title: "Index", path: "Index.md", links: ["Project A", "Project B"] },
//!         #{ title: "Project A", path: "projects/a.md", links: ["Index"] },
//!         #{ title: "Project B", path: "projects/b.md", links: [] },
//!     ]
//! };
//!
//! // Get notes linked FROM a note (outlinks)
//! let out = graph::outlinks(g, "Index")?;  // returns [Project A, Project B]
//!
//! // Get notes linking TO a note (inlinks/backlinks)
//! let back = graph::inlinks(g, "Index")?;  // returns [Project A]
//!
//! // Find a note by title
//! let note = graph::find(g, "Index")?;
//! ```

use crate::mcp_types::{json_to_rune, rune_to_json};
use rune::runtime::{ToValue, VmResult};
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Error type for graph operations (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::graph, name = GraphError)]
pub struct RuneGraphError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RuneGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RuneGraphError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

// =============================================================================
// Internal Helper Functions (work with JSON values)
// =============================================================================

/// Extract notes array from graph JSON object
fn get_notes_json(graph: &JsonValue) -> Result<&Vec<JsonValue>, String> {
    graph
        .get("notes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Graph must have 'notes' array field".to_string())
}

/// Get title from a note JSON object
fn get_title_json(note: &JsonValue) -> Result<&str, String> {
    note.get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Note must have 'title' string field".to_string())
}

/// Get links array from a note JSON object
fn get_links_json(note: &JsonValue) -> Vec<&str> {
    note.get("links")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Internal find implementation (JSON-based)
fn find_impl_json(graph: &JsonValue, title: &str) -> Result<Option<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    for note in notes {
        if get_title_json(note)? == title {
            return Ok(Some(note.clone()));
        }
    }

    Ok(None)
}

/// Internal outlinks implementation (JSON-based)
fn outlinks_impl_json(graph: &JsonValue, title: &str) -> Result<Vec<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    // Find the source note and get its links
    let mut source_links: Vec<&str> = Vec::new();
    for note in notes {
        if get_title_json(note)? == title {
            source_links = get_links_json(note);
            break;
        }
    }

    // Find notes that match the links
    let mut result: Vec<JsonValue> = Vec::new();
    for note in notes {
        let note_title = get_title_json(note)?;
        if source_links.contains(&note_title) {
            result.push(note.clone());
        }
    }

    Ok(result)
}

/// Internal inlinks implementation (JSON-based)
fn inlinks_impl_json(graph: &JsonValue, title: &str) -> Result<Vec<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    let mut result: Vec<JsonValue> = Vec::new();
    for note in notes {
        let links = get_links_json(note);
        if links.contains(&title) {
            result.push(note.clone());
        }
    }

    Ok(result)
}

// =============================================================================
// Rune Functions
// =============================================================================

/// Find a note by title
///
/// Returns the note object if found, or unit if not found.
#[rune::function]
fn find(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    match find_impl_json(&graph_json, &title)
        .map_err(|e| RuneGraphError::new(e))?
    {
        Some(note_json) => {
            // Convert back to Rune
            match json_to_rune(&note_json) {
                VmResult::Ok(v) => Ok(v),
                VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
            }
        }
        None => Ok(Value::empty()),
    }
}

/// Get outlinks (notes linked FROM the given note)
///
/// Returns an array of note objects that the source note links to.
#[rune::function]
fn outlinks(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    let result_json = outlinks_impl_json(&graph_json, &title)
        .map_err(|e| RuneGraphError::new(e))?;

    // Convert back to Rune
    let result_array = JsonValue::Array(result_json);
    match json_to_rune(&result_array) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
    }
}

/// Get inlinks/backlinks (notes linking TO the given note)
///
/// Returns an array of note objects that link to the target note.
#[rune::function]
fn inlinks(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    let result_json = inlinks_impl_json(&graph_json, &title)
        .map_err(|e| RuneGraphError::new(e))?;

    // Convert back to Rune
    let result_array = JsonValue::Array(result_json);
    match json_to_rune(&result_array) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
    }
}

// =============================================================================
// Module Registration
// =============================================================================

/// Create the graph module for Rune
pub fn graph_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("graph")?;

    // Register the error type
    module.ty::<RuneGraphError>()?;

    // Register functions
    module.function_meta(find)?;
    module.function_meta(outlinks)?;
    module.function_meta(inlinks)?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // =========================================================================
    // Module creation test
    // =========================================================================

    #[test]
    fn test_graph_module_creation() {
        let module = graph_module();
        assert!(module.is_ok(), "Should create graph module");
    }

    // =========================================================================
    // JSON-based tests (test internal implementations)
    // =========================================================================

    #[test]
    fn test_find_existing_note() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": [] },
                { "title": "Project A", "path": "a.md", "links": [] }
            ]
        });

        let result = find_impl_json(&graph, "Index").unwrap();
        assert!(result.is_some(), "Should find note");
        assert_eq!(result.unwrap()["title"], "Index");
    }

    #[test]
    fn test_find_missing_note_returns_none() {
        let graph = json!({
            "notes": [{ "title": "Index", "path": "Index.md", "links": [] }]
        });

        let result = find_impl_json(&graph, "NonExistent").unwrap();
        assert!(result.is_none(), "Should return None for missing note");
    }

    #[test]
    fn test_outlinks_returns_linked_notes() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A", "Project B"] },
                { "title": "Project A", "path": "a.md", "links": ["Index"] },
                { "title": "Project B", "path": "b.md", "links": [] },
                { "title": "Orphan", "path": "orphan.md", "links": [] }
            ]
        });

        let result = outlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(result.len(), 2, "Should return 2 notes");

        let mut titles: Vec<&str> = result
            .iter()
            .filter_map(|n| n["title"].as_str())
            .collect();
        titles.sort();

        assert_eq!(titles, vec!["Project A", "Project B"]);
    }

    #[test]
    fn test_outlinks_empty_when_no_links() {
        let graph = json!({
            "notes": [{ "title": "Orphan", "path": "orphan.md", "links": [] }]
        });

        let result = outlinks_impl_json(&graph, "Orphan").unwrap();
        assert_eq!(result.len(), 0, "Should return empty vec");
    }

    #[test]
    fn test_inlinks_returns_notes_linking_to_target() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A", "Project B"] },
                { "title": "Project A", "path": "a.md", "links": ["Index"] },
                { "title": "Project B", "path": "b.md", "links": [] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(result.len(), 1, "Only Project A links to Index");
        assert_eq!(result[0]["title"], "Project A");
    }

    #[test]
    fn test_inlinks_empty_when_no_backlinks() {
        let graph = json!({
            "notes": [
                { "title": "Orphan", "path": "orphan.md", "links": [] },
                { "title": "Another", "path": "another.md", "links": [] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Orphan").unwrap();
        assert_eq!(result.len(), 0, "Should return empty vec");
    }

    #[test]
    fn test_inlinks_multiple_backlinks() {
        let graph = json!({
            "notes": [
                { "title": "Hub", "path": "hub.md", "links": [] },
                { "title": "A", "path": "a.md", "links": ["Hub"] },
                { "title": "B", "path": "b.md", "links": ["Hub"] },
                { "title": "C", "path": "c.md", "links": ["Hub"] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Hub").unwrap();
        assert_eq!(result.len(), 3, "A, B, and C all link to Hub");
    }

    #[test]
    fn test_chained_traversal() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A"] },
                { "title": "Project A", "path": "a.md", "links": ["Sub Page"] },
                { "title": "Sub Page", "path": "sub.md", "links": [] }
            ]
        });

        // Get outlinks from Index
        let first_hop = outlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(first_hop.len(), 1);
        assert_eq!(first_hop[0]["title"], "Project A");

        // Get outlinks from Project A
        let second_hop = outlinks_impl_json(&graph, "Project A").unwrap();
        assert_eq!(second_hop.len(), 1);
        assert_eq!(second_hop[0]["title"], "Sub Page");
    }
}
