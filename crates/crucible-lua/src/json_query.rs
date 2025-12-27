//! Data query module for Lua scripts
//!
//! Provides multi-format parsing (JSON, TOON) and jq-style querying via the tq crate.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local data = require("data")
//!
//! -- Parse JSON or TOON (auto-detected)
//! local obj = data.parse('{"name": "Alice", "age": 30}')
//! local obj2 = data.parse('name: Alice\nage: 30')  -- TOON format
//!
//! -- Encode to different formats
//! local json_str = data.json(obj)           -- {"name":"Alice","age":30}
//! local toon_str = data.toon(obj)           -- name: Alice\nage: 30
//! local pretty = data.json_pretty(obj)      -- Pretty-printed JSON
//!
//! -- jq-style queries
//! local names = data.query(users, ".[] | .name")
//! local first = data.query(users, ".[0]")
//! local filtered = data.query(users, ".[] | select(.age > 21)")
//!
//! -- Smart formatting (extracts long content into blocks)
//! local formatted = data.format(response)
//! local formatted = data.format(response, { tool = "read_file" })
//! ```
//!
//! ## Legacy JSON API (backwards compatible)
//!
//! ```lua
//! local json = require("json")
//! local data = json.parse('{"x": 1}')
//! local str = json.encode(data)
//! local pretty = json.pretty(data)
//! ```

use crate::error::LuaError;
use mlua::{Lua, Value};
use serde_json::Value as JsonValue;
use tq::{compile_filter, format_tool_response, format_tool_response_with, run_filter, ToolType};

/// Register the data module with a Lua state
pub fn register_data_module(lua: &Lua) -> Result<(), LuaError> {
    let data = lua.create_table()?;

    // data.parse(str) -> table (auto-detects JSON or TOON)
    let parse_fn = lua.create_function(|lua, s: String| {
        let value = tq::parse_auto(&s).map_err(|e| mlua::Error::external(e))?;
        json_to_lua(lua, value)
    })?;
    data.set("parse", parse_fn)?;

    // data.json(table) -> string (compact JSON)
    let json_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string(&json).map_err(|e| mlua::Error::external(e))
    })?;
    data.set("json", json_fn)?;

    // data.json_pretty(table) -> string (pretty-printed JSON)
    let json_pretty_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string_pretty(&json).map_err(|e| mlua::Error::external(e))
    })?;
    data.set("json_pretty", json_pretty_fn)?;

    // data.toon(table) -> string (TOON format)
    let toon_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;
        tq::json_to_toon(json).map_err(|e| mlua::Error::external(e))
    })?;
    data.set("toon", toon_fn)?;

    // data.query(table, filter) -> table/value (jq-style query)
    let query_fn = lua.create_function(|lua, (value, filter_str): (Value, String)| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;

        let filter =
            compile_filter(&filter_str).map_err(|e| mlua::Error::external(e.to_string()))?;

        let results = run_filter(&filter, json).map_err(|e| mlua::Error::external(e.to_string()))?;

        // Return single value or array of results
        if results.len() == 1 {
            json_to_lua(lua, results.into_iter().next().unwrap())
        } else {
            let arr = JsonValue::Array(results);
            json_to_lua(lua, arr)
        }
    })?;
    data.set("query", query_fn)?;

    // data.format(table, options?) -> string (smart TOON formatting)
    let format_fn = lua.create_function(|lua, (value, options): (Value, Option<mlua::Table>)| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;

        let result = if let Some(opts) = options {
            // Check for tool type hint
            if let Ok(tool_name) = opts.get::<String>("tool") {
                let tool_type = ToolType::from_name(&tool_name);
                format_tool_response_with(&json, tool_type)
            } else {
                format_tool_response(&json)
            }
        } else {
            format_tool_response(&json)
        };

        Ok(result)
    })?;
    data.set("format", format_fn)?;

    // data.null constant
    let null = lua.create_userdata(DataNull)?;
    data.set("null", null)?;

    // Register data module globally
    lua.globals().set("data", data)?;

    Ok(())
}

/// Register the legacy json module with a Lua state (backwards compatible)
pub fn register_json_module(lua: &Lua) -> Result<(), LuaError> {
    let json = lua.create_table()?;

    // json.parse(str) -> table
    let parse_fn = lua.create_function(|lua, s: String| {
        let value: JsonValue =
            serde_json::from_str(&s).map_err(|e| mlua::Error::external(e))?;
        json_to_lua(lua, value)
    })?;
    json.set("parse", parse_fn)?;

    // json.encode(table) -> string
    let encode_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string(&json).map_err(|e| mlua::Error::external(e))
    })?;
    json.set("encode", encode_fn)?;

    // json.pretty(table) -> string (pretty-printed)
    let pretty_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value).map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string_pretty(&json).map_err(|e| mlua::Error::external(e))
    })?;
    json.set("pretty", pretty_fn)?;

    // json.null constant
    let null = lua.create_userdata(JsonNull)?;
    json.set("null", null)?;

    // Register json module globally
    lua.globals().set("json", json)?;

    Ok(())
}

/// Marker type for data null
#[derive(Clone, Copy)]
struct DataNull;

impl mlua::UserData for DataNull {}

/// Marker type for JSON null (legacy)
#[derive(Clone, Copy)]
struct JsonNull;

impl mlua::UserData for JsonNull {}

/// Convert JSON value to Lua value
pub fn json_to_lua(lua: &Lua, value: JsonValue) -> mlua::Result<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(s) => Ok(Value::String(lua.create_string(&s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.into_iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON value
pub fn lua_to_json(lua: &Lua, value: Value) -> Result<JsonValue, LuaError> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        Value::Integer(i) => Ok(JsonValue::Number(i.into())),
        Value::Number(n) => Ok(serde_json::Number::from_f64(n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Determine if it's an array or object
            // Arrays have consecutive integer keys starting at 1
            let len = t.raw_len();
            let is_array = len > 0 && (1..=len).all(|i| t.contains_key(i).unwrap_or(false));

            if is_array {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: Value = t.get(i)?;
                    arr.push(lua_to_json(lua, v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    let key = match k {
                        Value::String(s) => s.to_str()?.to_string(),
                        Value::Integer(i) => i.to_string(),
                        _ => continue,
                    };
                    map.insert(key, lua_to_json(lua, v)?);
                }
                Ok(JsonValue::Object(map))
            }
        }
        Value::UserData(ud) => {
            // Check if it's our null markers
            if ud.is::<JsonNull>() || ud.is::<DataNull>() {
                Ok(JsonValue::Null)
            } else {
                Ok(JsonValue::Null)
            }
        }
        // Functions, threads, etc. become null
        _ => Ok(JsonValue::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        register_json_module(&lua).unwrap();
        register_data_module(&lua).unwrap();
        lua
    }

    // =========================================================================
    // Legacy JSON module tests
    // =========================================================================

    #[test]
    fn test_json_parse_object() {
        let lua = setup_lua();
        let result: Table = lua
            .load(r#"return json.parse('{"name": "Alice", "age": 30}')"#)
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_json_parse_array() {
        let lua = setup_lua();
        let result: Table = lua
            .load(r#"return json.parse('[1, 2, 3]')"#)
            .eval()
            .unwrap();

        assert_eq!(result.get::<i64>(1).unwrap(), 1);
        assert_eq!(result.get::<i64>(2).unwrap(), 2);
        assert_eq!(result.get::<i64>(3).unwrap(), 3);
    }

    #[test]
    fn test_json_encode_object() {
        let lua = setup_lua();
        let result: String = lua
            .load(r#"return json.encode({ name = "Bob", age = 25 })"#)
            .eval()
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["name"], "Bob");
        assert_eq!(parsed["age"], 25);
    }

    #[test]
    fn test_json_encode_array() {
        let lua = setup_lua();
        let result: String = lua
            .load(r#"return json.encode({ 1, 2, 3 })"#)
            .eval()
            .unwrap();

        let parsed: Vec<i64> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, vec![1, 2, 3]);
    }

    #[test]
    fn test_json_roundtrip() {
        let lua = setup_lua();
        let result: bool = lua
            .load(
                r#"
                local original = { name = "Test", values = { 1, 2, 3 }, nested = { x = 10 } }
                local encoded = json.encode(original)
                local decoded = json.parse(encoded)
                return decoded.name == "Test" and decoded.nested.x == 10
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }

    #[test]
    fn test_json_pretty() {
        let lua = setup_lua();
        let result: String = lua
            .load(r#"return json.pretty({ name = "Alice" })"#)
            .eval()
            .unwrap();

        assert!(result.contains('\n'));
        assert!(result.contains("  ")); // indentation
    }

    #[test]
    fn test_json_null() {
        let lua = setup_lua();

        // json.null should encode as null
        let result: String = lua
            .load(r#"return json.encode({ value = json.null })"#)
            .eval()
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["value"].is_null());
    }

    // =========================================================================
    // New data module tests
    // =========================================================================

    #[test]
    fn test_data_parse_json() {
        let lua = setup_lua();
        let result: Table = lua
            .load(r#"return data.parse('{"name": "Alice", "age": 30}')"#)
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_data_parse_toon() {
        let lua = setup_lua();
        // TOON format: key: value pairs
        let result: Table = lua
            .load(
                r#"return data.parse([[
name: Alice
age: 30
]])"#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_data_json_encoding() {
        let lua = setup_lua();
        let result: String = lua
            .load(r#"return data.json({ name = "Bob" })"#)
            .eval()
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["name"], "Bob");
    }

    #[test]
    fn test_data_toon_encoding() {
        let lua = setup_lua();
        let result: String = lua
            .load(r#"return data.toon({ name = "Bob", age = 25 })"#)
            .eval()
            .unwrap();

        // TOON format should contain key: value pairs
        assert!(result.contains("name: Bob") || result.contains("name:Bob"));
        assert!(result.contains("age: 25") || result.contains("age:25"));
    }

    #[test]
    fn test_data_query_single() {
        let lua = setup_lua();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Alice", age = 30 }
                return data.query(obj, ".name")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, "Alice");
    }

    #[test]
    fn test_data_query_array_access() {
        let lua = setup_lua();
        let result: i64 = lua
            .load(
                r#"
                local arr = { 10, 20, 30 }
                return data.query(arr, ".[1]")
            "#,
            )
            .eval()
            .unwrap();

        // jq uses 0-based indexing
        assert_eq!(result, 20);
    }

    #[test]
    fn test_data_query_iterate() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local users = {
                    { name = "Alice", age = 30 },
                    { name = "Bob", age = 25 }
                }
                return data.query(users, ".[] | .name")
            "#,
            )
            .eval()
            .unwrap();

        // Should return array of names
        assert_eq!(result.get::<String>(1).unwrap(), "Alice");
        assert_eq!(result.get::<String>(2).unwrap(), "Bob");
    }

    #[test]
    fn test_data_query_nested() {
        let lua = setup_lua();
        let result: i64 = lua
            .load(
                r#"
                local obj = { user = { profile = { score = 100 } } }
                return data.query(obj, ".user.profile.score")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 100);
    }

    #[test]
    fn test_data_format() {
        let lua = setup_lua();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Test", value = 42 }
                return data.format(obj)
            "#,
            )
            .eval()
            .unwrap();

        // Should format as TOON
        assert!(result.contains("name") && result.contains("Test"));
        assert!(result.contains("value") && result.contains("42"));
    }

    #[test]
    fn test_data_format_with_tool_type() {
        let lua = setup_lua();
        let result: String = lua
            .load(
                r#"
                local response = { path = "file.lua", content = "print('hello')" }
                return data.format(response, { tool = "read_file" })
            "#,
            )
            .eval()
            .unwrap();

        // Should contain path info
        assert!(result.contains("file.lua") || result.contains("path"));
    }

    #[test]
    fn test_data_null() {
        let lua = setup_lua();

        // data.null should encode as null
        let result: String = lua
            .load(r#"return data.json({ value = data.null })"#)
            .eval()
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["value"].is_null());
    }
}
