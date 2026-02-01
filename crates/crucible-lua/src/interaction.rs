//! Unified interaction module for Lua scripts
//!
//! Provides `cru.interaction.*` bindings for all interaction types:
//! - `ask` - Questions with choices
//! - `popup` - Popup with rich entries
//! - `panel` - Interactive panel with filtering
//! - `permission` - Permission requests
//!
//! ## Lua Usage
//!
//! ```lua
//! local interaction = cru.interaction
//!
//! -- Ask a question with choices
//! local ask_req = interaction.ask({
//!     question = "Which database?",
//!     choices = {"PostgreSQL", "SQLite", "MongoDB"},
//!     multi_select = false,
//!     allow_other = true
//! })
//!
//! -- Create a popup with entries
//! local popup_req = interaction.popup({
//!     title = "Select a note",
//!     items = {
//!         { label = "Daily Note", description = "Today's journal" },
//!         { label = "Todo List" }
//!     },
//!     allow_other = false
//! })
//!
//! -- Create an interactive panel
//! local panel_req = interaction.panel({
//!     header = "Select features",
//!     items = {
//!         { label = "Auth", description = "Authentication" },
//!         { label = "Logging" }
//!     },
//!     filterable = true,
//!     multi_select = true
//! })
//!
//! -- Create a permission request
//! local perm_req = interaction.permission({
//!     action = "bash",
//!     tokens = {"npm", "install", "lodash"}
//! })
//! ```
//!
//! ## Fennel Usage
//!
//! ```fennel
//! (local interaction cru.interaction)
//!
//! ;; Ask a question
//! (local ask-req (interaction.ask {:question "Which DB?"
//!                                   :choices ["Postgres" "SQLite"]}))
//!
//! ;; Create a panel
//! (local panel-req (interaction.panel {:header "Select"
//!                                       :items [{:label "A"} {:label "B"}]
//!                                       :filterable true}))
//! ```

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_core::interaction::{AskRequest, PanelItem, PermRequest};
use crucible_core::types::PopupEntry;
use mlua::{Lua, Result as LuaResult, Table, Value};

/// Register the interaction module with a Lua state
///
/// This creates `cru.interaction` and `crucible.interaction` namespaces
/// with unified access to all interaction types.
pub fn register_interaction_module(lua: &Lua) -> Result<(), LuaError> {
    let interaction = lua.create_table()?;

    // interaction.ask(opts) -> table (AskRequest)
    let ask_fn = lua.create_function(|lua, opts: Table| create_ask_request(lua, &opts))?;
    interaction.set("ask", ask_fn)?;

    // interaction.popup(opts) -> table (PopupRequest)
    let popup_fn = lua.create_function(|lua, opts: Table| create_popup_request(lua, &opts))?;
    interaction.set("popup", popup_fn)?;

    // interaction.panel(opts) -> table (InteractivePanel)
    let panel_fn = lua.create_function(|lua, opts: Table| create_panel_request(lua, &opts))?;
    interaction.set("panel", panel_fn)?;

    // interaction.permission(opts) -> table (PermRequest)
    let permission_fn =
        lua.create_function(|lua, opts: Table| create_permission_request(lua, &opts))?;
    interaction.set("permission", permission_fn)?;

    // Register in both cru and crucible namespaces
    register_in_namespaces(lua, "interaction", interaction)?;

    Ok(())
}

/// Create an AskRequest from a Lua table
///
/// Expected table format:
/// ```lua
/// {
///     question = "Which option?",
///     choices = {"A", "B", "C"},  -- optional
///     multi_select = false,       -- optional, default false
///     allow_other = false         -- optional, default false
/// }
/// ```
fn create_ask_request(lua: &Lua, opts: &Table) -> LuaResult<Table> {
    let question: String = opts
        .get("question")
        .map_err(|_| mlua::Error::runtime("interaction.ask requires 'question' field (string)"))?;

    let multi_select: bool = opts.get("multi_select").unwrap_or(false);
    let allow_other: bool = opts.get("allow_other").unwrap_or(false);

    // Parse choices if provided
    let mut choices: Option<Vec<String>> = None;
    if let Ok(choices_table) = opts.get::<Table>("choices") {
        let mut choice_vec = Vec::new();
        for pair in choices_table.pairs::<i64, String>() {
            let (_, choice) = pair?;
            choice_vec.push(choice);
        }
        if !choice_vec.is_empty() {
            choices = Some(choice_vec);
        }
    }

    // Build the result table
    let result = lua.create_table()?;
    result.set("question", question)?;
    if let Some(ref c) = choices {
        let choices_lua = lua.create_table()?;
        for (i, choice) in c.iter().enumerate() {
            choices_lua.set(i + 1, choice.clone())?;
        }
        result.set("choices", choices_lua)?;
    }
    result.set("multi_select", multi_select)?;
    result.set("allow_other", allow_other)?;

    Ok(result)
}

/// Create a PopupRequest from a Lua table
///
/// Expected table format:
/// ```lua
/// {
///     title = "Select a note",
///     items = {
///         { label = "Daily Note", description = "Today's journal" },
///         { label = "Todo List" }
///     },
///     allow_other = false  -- optional, default false
/// }
/// ```
fn create_popup_request(lua: &Lua, opts: &Table) -> LuaResult<Table> {
    let title: String = opts
        .get("title")
        .map_err(|_| mlua::Error::runtime("interaction.popup requires 'title' field (string)"))?;

    let allow_other: bool = opts.get("allow_other").unwrap_or(false);

    // Parse items
    let items_table: Table = opts
        .get("items")
        .map_err(|_| mlua::Error::runtime("interaction.popup requires 'items' field (table)"))?;

    let mut entries = Vec::new();
    for pair in items_table.pairs::<i64, Table>() {
        let (_, item_table) = pair?;
        let label: String = item_table.get("label").map_err(|_| {
            mlua::Error::runtime("Each item in interaction.popup requires 'label' field")
        })?;
        let description: Option<String> = item_table.get("description").ok();

        let mut entry = PopupEntry::new(label);
        if let Some(desc) = description {
            entry = entry.with_description(desc);
        }
        entries.push(entry);
    }

    // Build the result table (compatible with lua_request_to_core)
    let result = lua.create_table()?;
    result.set("title", title)?;

    let entries_lua = lua.create_table()?;
    for (i, entry) in entries.iter().enumerate() {
        let entry_table = lua.create_table()?;
        entry_table.set("label", entry.label.clone())?;
        if let Some(ref desc) = entry.description {
            entry_table.set("description", desc.clone())?;
        }
        entries_lua.set(i + 1, entry_table)?;
    }
    result.set("entries", entries_lua)?;
    result.set("allow_other", allow_other)?;

    Ok(result)
}

/// Create an InteractivePanel from a Lua table
///
/// Expected table format:
/// ```lua
/// {
///     header = "Select features",
///     items = {
///         { label = "Auth", description = "Authentication" },
///         { label = "Logging" }
///     },
///     filterable = true,      -- optional, default false
///     multi_select = false,   -- optional, default false
///     allow_other = false     -- optional, default false
/// }
/// ```
fn create_panel_request(lua: &Lua, opts: &Table) -> LuaResult<Table> {
    let header: String = opts
        .get("header")
        .map_err(|_| mlua::Error::runtime("interaction.panel requires 'header' field (string)"))?;

    let filterable: bool = opts.get("filterable").unwrap_or(false);
    let multi_select: bool = opts.get("multi_select").unwrap_or(false);
    let allow_other: bool = opts.get("allow_other").unwrap_or(false);

    // Parse items
    let items_table: Table = opts
        .get("items")
        .map_err(|_| mlua::Error::runtime("interaction.panel requires 'items' field (table)"))?;

    let mut items = Vec::new();
    for pair in items_table.pairs::<i64, Table>() {
        let (_, item_table) = pair?;
        let label: String = item_table.get("label").map_err(|_| {
            mlua::Error::runtime("Each item in interaction.panel requires 'label' field")
        })?;
        let description: Option<String> = item_table.get("description").ok();

        let mut item = PanelItem::new(label);
        if let Some(desc) = description {
            item = item.with_description(desc);
        }
        items.push(item);
    }

    // Build the result table (compatible with lua_panel_to_core)
    let result = lua.create_table()?;
    result.set("header", header)?;

    let items_lua = lua.create_table()?;
    for (i, item) in items.iter().enumerate() {
        let item_table = lua.create_table()?;
        item_table.set("label", item.label.clone())?;
        if let Some(ref desc) = item.description {
            item_table.set("description", desc.clone())?;
        }
        items_lua.set(i + 1, item_table)?;
    }
    result.set("items", items_lua)?;

    // Build hints table
    let hints_table = lua.create_table()?;
    hints_table.set("filterable", filterable)?;
    hints_table.set("multi_select", multi_select)?;
    hints_table.set("allow_other", allow_other)?;
    result.set("hints", hints_table)?;

    Ok(result)
}

/// Create a PermRequest from a Lua table
///
/// Expected table format:
/// ```lua
/// -- Bash permission
/// { action = "bash", tokens = {"npm", "install", "lodash"} }
///
/// -- Read permission
/// { action = "read", segments = {"home", "user", "project"} }
///
/// -- Write permission
/// { action = "write", segments = {"home", "user", "output.txt"} }
///
/// -- Tool permission
/// { action = "tool", name = "search", args = { query = "test" } }
/// ```
fn create_permission_request(lua: &Lua, opts: &Table) -> LuaResult<Table> {
    let action: String = opts.get("action").map_err(|_| {
        mlua::Error::runtime(
            "interaction.permission requires 'action' field (bash|read|write|tool)",
        )
    })?;

    let result = lua.create_table()?;
    result.set("action", action.clone())?;

    match action.as_str() {
        "bash" => {
            let tokens_table: Table = opts.get("tokens").map_err(|_| {
                mlua::Error::runtime("interaction.permission with action='bash' requires 'tokens' field (table of strings)")
            })?;

            let mut tokens = Vec::new();
            for pair in tokens_table.pairs::<i64, String>() {
                let (_, token) = pair?;
                tokens.push(token);
            }

            let tokens_lua = lua.create_table()?;
            for (i, token) in tokens.iter().enumerate() {
                tokens_lua.set(i + 1, token.clone())?;
            }
            result.set("tokens", tokens_lua)?;
        }
        "read" | "write" => {
            let segments_table: Table = opts.get("segments").map_err(|_| {
                mlua::Error::runtime(format!(
                    "interaction.permission with action='{}' requires 'segments' field (table of strings)",
                    action
                ))
            })?;

            let mut segments = Vec::new();
            for pair in segments_table.pairs::<i64, String>() {
                let (_, segment) = pair?;
                segments.push(segment);
            }

            let segments_lua = lua.create_table()?;
            for (i, segment) in segments.iter().enumerate() {
                segments_lua.set(i + 1, segment.clone())?;
            }
            result.set("segments", segments_lua)?;
        }
        "tool" => {
            let name: String = opts.get("name").map_err(|_| {
                mlua::Error::runtime(
                    "interaction.permission with action='tool' requires 'name' field (string)",
                )
            })?;
            result.set("name", name)?;

            // Args can be any table, pass through as-is
            if let Ok(args) = opts.get::<Value>("args") {
                result.set("args", args)?;
            }
        }
        _ => {
            return Err(mlua::Error::runtime(format!(
                "interaction.permission: unknown action '{}'. Expected: bash, read, write, tool",
                action
            )));
        }
    }

    Ok(result)
}

/// Convert a Lua permission table to a crucible_core PermRequest
pub fn lua_permission_to_core(table: &Table) -> LuaResult<PermRequest> {
    let action: String = table.get("action")?;

    match action.as_str() {
        "bash" => {
            let tokens_table: Table = table.get("tokens")?;
            let mut tokens = Vec::new();
            for pair in tokens_table.pairs::<i64, String>() {
                let (_, token) = pair?;
                tokens.push(token);
            }
            Ok(PermRequest::bash(tokens))
        }
        "read" => {
            let segments_table: Table = table.get("segments")?;
            let mut segments = Vec::new();
            for pair in segments_table.pairs::<i64, String>() {
                let (_, segment) = pair?;
                segments.push(segment);
            }
            Ok(PermRequest::read(segments))
        }
        "write" => {
            let segments_table: Table = table.get("segments")?;
            let mut segments = Vec::new();
            for pair in segments_table.pairs::<i64, String>() {
                let (_, segment) = pair?;
                segments.push(segment);
            }
            Ok(PermRequest::write(segments))
        }
        "tool" => {
            let name: String = table.get("name")?;
            let args = if let Ok(args_value) = table.get::<Value>("args") {
                serde_json::to_value(&args_value).map_err(mlua::Error::external)?
            } else {
                serde_json::Value::Null
            };
            Ok(PermRequest::tool(name, args))
        }
        _ => Err(mlua::Error::runtime(format!(
            "Unknown permission action: {}",
            action
        ))),
    }
}

/// Convert a Lua ask table to a crucible_core AskRequest
pub fn lua_ask_to_core(table: &Table) -> LuaResult<AskRequest> {
    let question: String = table.get("question")?;
    let multi_select: bool = table.get("multi_select").unwrap_or(false);
    let allow_other: bool = table.get("allow_other").unwrap_or(false);

    let mut request = AskRequest::new(question);

    if let Ok(choices_table) = table.get::<Table>("choices") {
        let mut choices = Vec::new();
        for pair in choices_table.pairs::<i64, String>() {
            let (_, choice) = pair?;
            choices.push(choice);
        }
        if !choices.is_empty() {
            request = request.choices(choices);
        }
    }

    if multi_select {
        request = request.multi_select();
    }
    if allow_other {
        request = request.allow_other();
    }

    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::lua_panel_to_core;
    use crate::popup::lua_request_to_core;
    use crucible_core::interaction::PermAction;

    #[test]
    fn test_register_interaction_module() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register interaction module");

        // Verify cru.interaction exists
        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let interaction: Table = cru.get("interaction").expect("interaction should exist");
        assert!(interaction.contains_key("ask").unwrap());
        assert!(interaction.contains_key("popup").unwrap());
        assert!(interaction.contains_key("panel").unwrap());
        assert!(interaction.contains_key("permission").unwrap());

        // Verify crucible.interaction also exists
        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let interaction2: Table = crucible
            .get("interaction")
            .expect("interaction should exist");
        assert!(interaction2.contains_key("ask").unwrap());
    }

    #[test]
    fn test_interaction_ask() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.ask({
                question = "Which database?",
                choices = {"PostgreSQL", "SQLite", "MongoDB"},
                multi_select = false,
                allow_other = true
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("question").unwrap(), "Which database?");
        assert!(!result.get::<bool>("multi_select").unwrap());
        assert!(result.get::<bool>("allow_other").unwrap());

        let choices: Table = result.get("choices").expect("choices should exist");
        assert_eq!(choices.get::<String>(1).unwrap(), "PostgreSQL");
        assert_eq!(choices.get::<String>(2).unwrap(), "SQLite");
        assert_eq!(choices.get::<String>(3).unwrap(), "MongoDB");
    }

    #[test]
    fn test_interaction_ask_to_core() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.ask({
                question = "Test?",
                choices = {"A", "B"},
                multi_select = true
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let ask_request = lua_ask_to_core(&result).expect("Should convert");

        assert_eq!(ask_request.question, "Test?");
        assert_eq!(
            ask_request.choices,
            Some(vec!["A".to_string(), "B".to_string()])
        );
        assert!(ask_request.multi_select);
        assert!(!ask_request.allow_other);
    }

    #[test]
    fn test_interaction_popup() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.popup({
                title = "Select a note",
                items = {
                    { label = "Daily Note", description = "Today's journal" },
                    { label = "Todo List" }
                },
                allow_other = true
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("title").unwrap(), "Select a note");
        assert!(result.get::<bool>("allow_other").unwrap());

        let entries: Table = result.get("entries").expect("entries should exist");
        let entry1: Table = entries.get(1).expect("entry1 should exist");
        assert_eq!(entry1.get::<String>("label").unwrap(), "Daily Note");
        assert_eq!(
            entry1.get::<String>("description").unwrap(),
            "Today's journal"
        );

        // Can convert to core type
        let popup_request = lua_request_to_core(&result).expect("Should convert");
        assert_eq!(popup_request.title, "Select a note");
        assert_eq!(popup_request.entries.len(), 2);
    }

    #[test]
    fn test_interaction_panel() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.panel({
                header = "Select features",
                items = {
                    { label = "Auth", description = "Authentication" },
                    { label = "Logging" }
                },
                filterable = true,
                multi_select = true
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("header").unwrap(), "Select features");

        let hints: Table = result.get("hints").expect("hints should exist");
        assert!(hints.get::<bool>("filterable").unwrap());
        assert!(hints.get::<bool>("multi_select").unwrap());

        // Can convert to core type
        let panel = lua_panel_to_core(&result).expect("Should convert");
        assert_eq!(panel.header, "Select features");
        assert_eq!(panel.items.len(), 2);
        assert!(panel.hints.filterable);
        assert!(panel.hints.multi_select);
    }

    #[test]
    fn test_interaction_permission_bash() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.permission({
                action = "bash",
                tokens = {"npm", "install", "lodash"}
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("action").unwrap(), "bash");

        let tokens: Table = result.get("tokens").expect("tokens should exist");
        assert_eq!(tokens.get::<String>(1).unwrap(), "npm");
        assert_eq!(tokens.get::<String>(2).unwrap(), "install");
        assert_eq!(tokens.get::<String>(3).unwrap(), "lodash");

        // Can convert to core type
        let perm = lua_permission_to_core(&result).expect("Should convert");
        assert_eq!(perm.tokens(), &["npm", "install", "lodash"]);
        assert_eq!(perm.pattern_at(2), "npm install *");
    }

    #[test]
    fn test_interaction_permission_read() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.permission({
                action = "read",
                segments = {"home", "user", "project"}
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let perm = lua_permission_to_core(&result).expect("Should convert");
        assert_eq!(perm.tokens(), &["home", "user", "project"]);
    }

    #[test]
    fn test_interaction_permission_write() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.permission({
                action = "write",
                segments = {"tmp", "output.txt"}
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let perm = lua_permission_to_core(&result).expect("Should convert");
        assert_eq!(perm.tokens(), &["tmp", "output.txt"]);
    }

    #[test]
    fn test_interaction_permission_tool() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.permission({
                action = "tool",
                name = "search",
                args = { query = "test", limit = 10 }
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("action").unwrap(), "tool");
        assert_eq!(result.get::<String>("name").unwrap(), "search");

        let perm = lua_permission_to_core(&result).expect("Should convert");
        match &perm.action {
            PermAction::Tool { name, args } => {
                assert_eq!(name, "search");
                assert_eq!(args["query"], "test");
                assert_eq!(args["limit"], 10);
            }
            _ => panic!("Expected Tool action"),
        }
    }

    #[test]
    fn test_interaction_ask_missing_question() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.ask({ choices = {"A", "B"} })
        "#;

        let result = lua.load(script).eval::<Table>();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("question"));
    }

    #[test]
    fn test_interaction_permission_invalid_action() {
        let lua = Lua::new();
        register_interaction_module(&lua).expect("Should register");

        let script = r#"
            return cru.interaction.permission({ action = "invalid" })
        "#;

        let result = lua.load(script).eval::<Table>();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown action"));
    }
}
