//! Popup module for Lua scripts
//!
//! Provides popup entry and request creation for Lua and Fennel scripts.
//!
//! ## Lua Usage
//!
//! ```lua
//! local popup = require("popup")
//!
//! -- Create a popup entry
//! local entry = popup.entry("Option 1", "Description of option 1")
//!
//! -- Create a popup request with entries
//! local req = popup.request("Select a note", {
//!     popup.entry("Daily Note", "Today's journal"),
//!     popup.entry("Todo List"),  -- description is optional
//! })
//!
//! -- Create a request that allows free-text input
//! local req = popup.request_with_other("Search or select", {
//!     popup.entry("Recent", "Recently viewed"),
//! })
//! ```
//!
//! ## Fennel Usage
//!
//! ```fennel
//! (local popup (require :popup))
//!
//! ;; Create entries
//! (local entries [(popup.entry "Daily Note" "Today's journal")
//!                 (popup.entry "Todo List")])
//!
//! ;; Create popup request
//! (local req (popup.request "Select a note" entries))
//!
//! ;; With free-text option
//! (local req (popup.request_with_other "Search or select" entries))
//! ```

use crate::error::LuaError;
use mlua::{Lua, Result as LuaResult, Table, Value};

/// Register the popup module with a Lua state
pub fn register_popup_module(lua: &Lua) -> Result<(), LuaError> {
    let popup = lua.create_table()?;

    // popup.entry(label, description?) -> table
    let entry_fn = lua.create_function(|lua, (label, description): (String, Option<String>)| {
        let entry = lua.create_table()?;
        entry.set("label", label)?;
        if let Some(desc) = description {
            entry.set("description", desc)?;
        }
        Ok(entry)
    })?;
    popup.set("entry", entry_fn)?;

    // popup.entry_with_data(label, description, data) -> table
    let entry_with_data_fn = lua.create_function(
        |lua, (label, description, data): (String, Option<String>, Value)| {
            let entry = lua.create_table()?;
            entry.set("label", label)?;
            if let Some(desc) = description {
                entry.set("description", desc)?;
            }
            entry.set("data", data)?;
            Ok(entry)
        },
    )?;
    popup.set("entry_with_data", entry_with_data_fn)?;

    // popup.request(title, entries) -> table
    let request_fn = lua.create_function(|lua, (title, entries): (String, Table)| {
        let request = lua.create_table()?;
        request.set("title", title)?;
        request.set("entries", entries)?;
        request.set("allow_other", false)?;
        Ok(request)
    })?;
    popup.set("request", request_fn)?;

    // popup.request_with_other(title, entries) -> table
    let request_with_other_fn = lua.create_function(|lua, (title, entries): (String, Table)| {
        let request = lua.create_table()?;
        request.set("title", title)?;
        request.set("entries", entries)?;
        request.set("allow_other", true)?;
        Ok(request)
    })?;
    popup.set("request_with_other", request_with_other_fn)?;

    // popup.response_selected(index, entry) -> table
    let response_selected_fn = lua.create_function(|lua, (index, entry): (usize, Table)| {
        let response = lua.create_table()?;
        response.set("selected_index", index)?;
        response.set("selected_entry", entry)?;
        Ok(response)
    })?;
    popup.set("response_selected", response_selected_fn)?;

    // popup.response_other(text) -> table
    let response_other_fn = lua.create_function(|lua, text: String| {
        let response = lua.create_table()?;
        response.set("other", text)?;
        Ok(response)
    })?;
    popup.set("response_other", response_other_fn)?;

    // popup.response_none() -> table
    let response_none_fn = lua.create_function(|lua, ()| {
        let response = lua.create_table()?;
        // Empty response indicates dismissed
        Ok(response)
    })?;
    popup.set("response_none", response_none_fn)?;

    // Register as global module
    lua.globals().set("popup", popup)?;

    Ok(())
}

/// Convert a Lua popup entry table to a crucible_core PopupEntry
pub fn lua_entry_to_core(entry: &Table) -> LuaResult<crucible_core::types::PopupEntry> {
    let label: String = entry.get("label")?;
    let description: Option<String> = entry.get("description").ok();

    let mut popup_entry = crucible_core::types::PopupEntry::new(label);
    if let Some(desc) = description {
        popup_entry = popup_entry.with_description(desc);
    }

    // Handle data field if present
    if let Ok(data) = entry.get::<Value>("data") {
        if !matches!(data, Value::Nil) {
            if let Ok(json_data) = serde_json::to_value(&data) {
                popup_entry = popup_entry.with_data(json_data);
            }
        }
    }

    Ok(popup_entry)
}

/// Convert a Lua popup request table to a crucible_core PopupRequest
pub fn lua_request_to_core(request: &Table) -> LuaResult<crucible_core::interaction::PopupRequest> {
    let title: String = request.get("title")?;
    let entries_table: Table = request.get("entries")?;
    let allow_other: bool = request.get("allow_other").unwrap_or(false);

    let mut entries = Vec::new();
    for pair in entries_table.pairs::<i64, Table>() {
        let (_, entry_table) = pair?;
        entries.push(lua_entry_to_core(&entry_table)?);
    }

    let mut popup_request = crucible_core::interaction::PopupRequest::new(title).entries(entries);

    if allow_other {
        popup_request = popup_request.allow_other();
    }

    Ok(popup_request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_popup_module() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        // Verify popup table exists
        let popup: Table = lua.globals().get("popup").expect("popup should exist");
        assert!(popup.contains_key("entry").unwrap());
        assert!(popup.contains_key("request").unwrap());
        assert!(popup.contains_key("request_with_other").unwrap());
    }

    #[test]
    fn test_popup_entry_from_lua() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        let script = r#"
            return popup.entry("Test Label", "Test Description")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let entry = lua_entry_to_core(&result).expect("Should convert");

        assert_eq!(entry.label, "Test Label");
        assert_eq!(entry.description, Some("Test Description".to_string()));
    }

    #[test]
    fn test_popup_entry_without_description() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        let script = r#"
            return popup.entry("Simple")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let entry = lua_entry_to_core(&result).expect("Should convert");

        assert_eq!(entry.label, "Simple");
        assert!(entry.description.is_none());
    }

    #[test]
    fn test_popup_request_from_lua() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        let script = r#"
            return popup.request("Select", {
                popup.entry("Option 1", "First"),
                popup.entry("Option 2"),
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let request = lua_request_to_core(&result).expect("Should convert");

        assert_eq!(request.title, "Select");
        assert_eq!(request.entries.len(), 2);
        assert!(!request.allow_other);
    }

    #[test]
    fn test_popup_request_with_other() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        let script = r#"
            return popup.request_with_other("Choose", {
                popup.entry("Preset"),
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let request = lua_request_to_core(&result).expect("Should convert");

        assert!(request.allow_other);
    }

    #[test]
    fn test_popup_entry_with_data() {
        let lua = Lua::new();
        register_popup_module(&lua).expect("Should register popup module");

        let script = r#"
            return popup.entry_with_data("Item", "Desc", { id = 123, tags = {"a", "b"} })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let entry = lua_entry_to_core(&result).expect("Should convert");

        assert_eq!(entry.label, "Item");
        assert!(entry.data.is_some());
    }
}
