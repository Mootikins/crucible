//! JSON query module for Lua scripts
//!
//! Provides JSON parsing and basic querying capabilities.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local json = require("json")
//!
//! -- Parse JSON string to Lua table
//! local data = json.parse('{"name": "Alice", "age": 30}')
//! print(data.name)  -- Alice
//!
//! -- Encode Lua table to JSON string
//! local str = json.encode({ name = "Bob", scores = {1, 2, 3} })
//!
//! -- Pretty print JSON
//! local pretty = json.pretty(data)
//! ```

use crate::error::LuaError;
use mlua::{Lua, Value};
use serde_json::Value as JsonValue;

/// Register the json module with a Lua state
pub fn register_json_module(lua: &Lua) -> Result<(), LuaError> {
    let json = lua.create_table()?;

    // json.parse(str) -> table
    let parse_fn = lua.create_function(|lua, s: String| {
        let value: JsonValue = serde_json::from_str(&s)
            .map_err(|e| mlua::Error::external(e))?;
        json_to_lua(lua, value)
    })?;
    json.set("parse", parse_fn)?;

    // json.encode(table) -> string
    let encode_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value)
            .map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string(&json)
            .map_err(|e| mlua::Error::external(e))
    })?;
    json.set("encode", encode_fn)?;

    // json.pretty(table) -> string (pretty-printed)
    let pretty_fn = lua.create_function(|lua, value: Value| {
        let json = lua_to_json(lua, value)
            .map_err(|e| mlua::Error::external(e))?;
        serde_json::to_string_pretty(&json)
            .map_err(|e| mlua::Error::external(e))
    })?;
    json.set("pretty", pretty_fn)?;

    // json.null constant
    let null = lua.create_userdata(JsonNull)?;
    json.set("null", null)?;

    // Register json module globally
    lua.globals().set("json", json)?;

    Ok(())
}

/// Marker type for JSON null
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
            // Check if it's our JsonNull marker
            if ud.is::<JsonNull>() {
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
        lua
    }

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
            .load(r#"
                local original = { name = "Test", values = { 1, 2, 3 }, nested = { x = 10 } }
                local encoded = json.encode(original)
                local decoded = json.parse(encoded)
                return decoded.name == "Test" and decoded.nested.x == 10
            "#)
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
}
