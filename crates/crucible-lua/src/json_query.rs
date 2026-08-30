//! Object Query (oq) module for Lua scripts
//!
//! Provides multi-format parsing (JSON, YAML, TOML, TOON) and jq-style querying.
//!
//! ## Usage in Lua
//!
//! ```lua
//! local oq = require("oq")
//!
//! -- Parse any format (auto-detected)
//! local obj = oq.parse('{"name": "Alice"}')        -- JSON
//! local obj = oq.parse('name: Alice\nage: 30')     -- YAML or TOON
//! local obj = oq.parse('name = "Alice"')           -- TOML
//!
//! -- Explicit format parsing
//! local obj = oq.parse_as('name: Alice', 'yaml')
//! local obj = oq.parse_as('name = "Alice"', 'toml')
//!
//! -- Encode to different formats (JSON encoding lives on cru.json.encode)
//! local yaml_str = oq.yaml(obj)           -- name: Alice\nage: 30
//! local toml_str = oq.toml(obj)           -- name = "Alice"
//! local toon_str = oq.toon(obj)           -- name: Alice\nage: 30
//!
//! -- Format conversion
//! local yaml = oq.convert(obj, 'yaml')
//! local toml = oq.convert(obj, 'toml')
//!
//! -- jq-style queries
//! local names = oq.query(users, ".[] | .name")
//! local first = oq.query(users, ".[0]")
//! local filtered = oq.query(users, ".[] | select(.age > 21)")
//!
//! -- Smart formatting (extracts long content into blocks)
//! local formatted = oq.format(response)
//! local formatted = oq.format(response, { tool = "read_file" })
//! ```
//!
//! ## Legacy JSON API (backwards compatible)
//!
//! ```lua
//! local json = require("json")
//! local obj = json.parse('{"x": 1}')
//! local str = json.encode(obj)
//! local pretty = json.pretty(obj)
//! ```

use crate::error::LuaError;
use mlua::{Lua, LuaSerdeExt, Value};
use oq::{compile_filter, format_tool_response, format_tool_response_with, run_filter, ToolType};
use serde_json::Value as JsonValue;

/// Supported data formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Toon,
}

impl Format {
    /// Parse format from string name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            "toml" => Some(Format::Toml),
            "toon" => Some(Format::Toon),
            _ => None,
        }
    }

    /// Get format name as string
    pub fn name(&self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Toon => "toon",
        }
    }
}

/// Detect the format of a string based on content heuristics
pub fn detect_format(s: &str) -> Format {
    let trimmed = s.trim();
    let lines: Vec<&str> = trimmed.lines().collect();

    // Check for TOML section headers [section] or [[table]] first
    // These look like JSON arrays but aren't
    let first_line = lines.first().map(|l| l.trim()).unwrap_or("");
    let is_toml_section = first_line.starts_with('[')
        && first_line.ends_with(']')
        && !first_line.contains(',')
        && !first_line.contains(':');

    if is_toml_section {
        return Format::Toml;
    }

    // JSON: starts with { or [
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Format::Json;
    }

    // TOML detection heuristics:
    // - Has [section] headers
    // - Has key = "value" patterns (equals with quoted string)
    // - Has key = value patterns with TOML-style values

    // Check for TOML section headers [section] anywhere in file
    let has_toml_sections = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with('[')
            && l.ends_with(']')
            && !l.starts_with("[[")
            && !l.contains(',')
            && !l.contains(':')
    });

    // Check for TOML array of tables [[section]]
    let has_toml_array_tables = lines
        .iter()
        .any(|line| line.trim().starts_with("[[") && line.trim().ends_with("]]"));

    // Check for TOML-style assignments: key = "value" or key = 123
    let has_toml_assignments = lines.iter().any(|line| {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') {
            return false;
        }
        // Must have = with spaces around it (TOML style)
        if let Some(eq_pos) = l.find(" = ") {
            let key = &l[..eq_pos];
            // TOML keys are bare words or quoted
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '"')
        } else {
            false
        }
    });

    if has_toml_sections || has_toml_array_tables || has_toml_assignments {
        return Format::Toml;
    }

    // YAML detection heuristics:
    // - Starts with --- (YAML document marker)
    // - Has key: value (colon-space) with YAML-style values
    // - Has - item (dash-space for lists)
    // - Multi-line strings with | or >
    if trimmed.starts_with("---") {
        return Format::Yaml;
    }

    // Check for YAML-specific patterns
    let has_yaml_list = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with("- ") || l == "-"
    });

    let has_yaml_multiline = lines.iter().any(|line| {
        let l = line.trim();
        l.ends_with(": |") || l.ends_with(": >") || l.ends_with(": |+") || l.ends_with(": |-")
    });

    // YAML typically has nested indentation with spaces
    let has_yaml_nesting = lines
        .iter()
        .any(|line| line.starts_with("  ") && line.trim().contains(": "));

    if has_yaml_list || has_yaml_multiline || has_yaml_nesting {
        return Format::Yaml;
    }

    // TOON: simple key: value without YAML complexity
    // TOON uses colon-space but is flat (no nesting, no lists)
    let has_colon_space = lines.iter().any(|line| {
        let l = line.trim();
        !l.is_empty() && !l.starts_with('#') && l.contains(": ")
    });

    if has_colon_space {
        // Distinguish TOON from YAML:
        // TOON is flat, YAML has nesting/lists
        // If no YAML indicators were found, it's likely TOON
        return Format::Toon;
    }

    // Default to JSON (let parser determine)
    Format::Json
}

/// Parse string with explicit format
pub fn parse_with_format(s: &str, format: Format) -> Result<JsonValue, String> {
    match format {
        Format::Json => serde_json::from_str(s).map_err(|e| e.to_string()),
        Format::Yaml => serde_yaml::from_str(s).map_err(|e| e.to_string()),
        Format::Toml => {
            let toml_value: toml::Value = toml::from_str(s).map_err(|e| e.to_string())?;
            toml_to_json(toml_value)
        }
        Format::Toon => oq::parse_auto(s).map_err(|e| e.to_string()),
    }
}

/// Parse string with auto-detection
pub fn parse_auto(s: &str) -> Result<JsonValue, String> {
    let format = detect_format(s);
    parse_with_format(s, format)
}

/// Convert TOML value to JSON value
fn toml_to_json(value: toml::Value) -> Result<JsonValue, String> {
    match value {
        toml::Value::String(s) => Ok(JsonValue::String(s)),
        toml::Value::Integer(i) => Ok(JsonValue::Number(i.into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or_else(|| "Invalid float value".to_string()),
        toml::Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        toml::Value::Datetime(dt) => Ok(JsonValue::String(dt.to_string())),
        toml::Value::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.into_iter().map(toml_to_json).collect();
            Ok(JsonValue::Array(json_arr?))
        }
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (k, v) in table {
                map.insert(k, toml_to_json(v)?);
            }
            Ok(JsonValue::Object(map))
        }
    }
}

/// Convert JSON value to TOML value
fn json_to_toml(value: &JsonValue) -> Result<toml::Value, String> {
    match value {
        JsonValue::Null => Ok(toml::Value::String("null".to_string())),
        JsonValue::Bool(b) => Ok(toml::Value::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err("Invalid number".to_string())
            }
        }
        JsonValue::String(s) => Ok(toml::Value::String(s.clone())),
        JsonValue::Array(arr) => {
            let toml_arr: Result<Vec<_>, _> = arr.iter().map(json_to_toml).collect();
            Ok(toml::Value::Array(toml_arr?))
        }
        JsonValue::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                table.insert(k.clone(), json_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}

/// Encode JSON value to specified format
pub fn encode_to_format(value: &JsonValue, format: Format) -> Result<String, String> {
    match format {
        Format::Json => serde_json::to_string(value).map_err(|e| e.to_string()),
        Format::Yaml => serde_yaml::to_string(value).map_err(|e| e.to_string()),
        Format::Toml => {
            let toml_value = json_to_toml(value)?;
            toml::to_string(&toml_value).map_err(|e| e.to_string())
        }
        Format::Toon => oq::json_to_toon(value.clone()).map_err(|e| e.to_string()),
    }
}

/// Register the oq module with a Lua state
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`].
///
/// Three declarations say `any` where the older comments said `table`, and
/// `any` is the one that is true: `parse`, `parse_as` and `query` all end in
/// `json_to_lua`, which answers with whatever the document holds. A scalar
/// document (`oq.parse("42")`) answers a number, and a `query` that selects a
/// name answers a string.
///
/// A bad format name and a syntax error both RAISE. None of these functions
/// answers `nil` on failure, so no return type is optional.
pub fn register_oq_module(lua: &Lua) -> Result<(), LuaError> {
    let mut oq_ns = crate::host_registry::Ns::new(lua, "cru.oq")?;

    // Auto-detects JSON, YAML, TOML, or TOON.
    oq_ns.func("parse", "(text: string) -> any", |lua, text: String| {
        let value = parse_auto(&text).map_err(mlua::Error::external)?;
        json_to_lua(lua, value)
    })?;

    // The format name is one of `json`, `yaml`/`yml`, `toml`, `toon`
    // (`Format::from_name`); any other name raises.
    oq_ns.func(
        "parse_as",
        "(text: string, format: string) -> any",
        |lua, (text, format_name): (String, String)| {
            let format = Format::from_name(&format_name)
                .ok_or_else(|| mlua::Error::external(format!("Unknown format: {}", format_name)))?;
            let value = parse_with_format(&text, format).map_err(mlua::Error::external)?;
            json_to_lua(lua, value)
        },
    )?;

    oq_ns.func("toon", "(value: any) -> string", |lua, value: Value| {
        let json = lua_to_json(lua, value)?;
        oq::json_to_toon(json).map_err(mlua::Error::external)
    })?;

    oq_ns.func("yaml", "(value: any) -> string", |lua, value: Value| {
        let json = lua_to_json(lua, value)?;
        serde_yaml::to_string(&json).map_err(mlua::Error::external)
    })?;

    oq_ns.func("toml", "(value: any) -> string", |lua, value: Value| {
        let json = lua_to_json(lua, value)?;
        let toml_value = json_to_toml(&json).map_err(mlua::Error::external)?;
        toml::to_string(&toml_value).map_err(mlua::Error::external)
    })?;

    oq_ns.func(
        "convert",
        "(value: any, format: string) -> string",
        |lua, (value, format_name): (Value, String)| {
            let format = Format::from_name(&format_name)
                .ok_or_else(|| mlua::Error::external(format!("Unknown format: {}", format_name)))?;
            let json = lua_to_json(lua, value)?;
            encode_to_format(&json, format).map_err(mlua::Error::external)
        },
    )?;

    // Answers one of `json`, `yaml`, `toml`, `toon` — never nil, because
    // `detect_format` falls back to JSON.
    oq_ns.func("detect", "(text: string) -> string", |_, text: String| {
        let format = detect_format(&text);
        Ok(format.name().to_string())
    })?;

    // jq-style query.
    oq_ns.func(
        "query",
        "(value: any, filter: string) -> any",
        |lua, (value, filter_str): (Value, String)| {
            let json = lua_to_json(lua, value)?;

            let filter =
                compile_filter(&filter_str).map_err(|e| mlua::Error::external(e.to_string()))?;

            let results =
                run_filter(&filter, json).map_err(|e| mlua::Error::external(e.to_string()))?;

            // Return single value or array of results
            if results.len() == 1 {
                json_to_lua(lua, results.into_iter().next().unwrap())
            } else {
                let arr = JsonValue::Array(results);
                json_to_lua(lua, arr)
            }
        },
    )?;

    // Smart TOON formatting. `tool` is the only key the body reads, so the
    // options table is declared with that one field.
    oq_ns.func(
        "format",
        "(value: any, options: { tool: string? }?) -> string",
        |lua, (value, options): (Value, Option<mlua::Table>)| {
            let json = lua_to_json(lua, value)?;

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
        },
    )?;

    // `oq.null` is a userdata constant, not a function, so it goes on the
    // table directly. The VM walk sees it and the stub generator renders it
    // as a field; `Ns` describes only what you call.
    let null = lua.create_ser_userdata(OqNull)?;
    oq_ns.table().set("null", null)?;

    // Register oq module globally
    let oq = oq_ns.publish()?;
    lua.globals().set("oq", oq)?;

    Ok(())
}

/// Marker type for oq null
#[derive(Clone, Copy)]
struct OqNull;

impl mlua::UserData for OqNull {}

impl serde::Serialize for OqNull {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_none()
    }
}

/// Convert JSON value to Lua value
pub fn json_to_lua(lua: &Lua, value: JsonValue) -> mlua::Result<Value> {
    lua.to_value(&value)
}

/// Convert Lua value to JSON value
pub fn lua_to_json(_lua: &Lua, value: Value) -> Result<JsonValue, LuaError> {
    Ok(serde_json::to_value(&value)?)
}

/// Mark `table` as a JSON array, and return it.
///
/// Lua cannot tell an empty list from an empty map — both are `{}` — and
/// mlua resolves the ambiguity as a *map* (`encode_empty_tables_as_array`
/// defaults to false). So a tool that returns a list of results emits
/// `"results":[…]` when it found something and `"results":{}` when it did
/// not: the JSON *type* of the field changes with the data, which is the
/// worst thing a tool result can do to a model reading it.
///
/// Marking is the only fix that survives an empty list, because it is
/// carried on the table rather than inferred from its contents.
pub fn mark_json_array(lua: &Lua, table: mlua::Table) -> mlua::Result<mlua::Table> {
    use mlua::LuaSerdeExt;
    table.set_metatable(Some(lua.array_metatable()))?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;
    use mlua::Table;

    /// An empty list must stay a list on the wire.
    ///
    /// A plugin tool returning `{ results = {}, degraded = {} }` handed the
    /// model `"results":{}` — an object — while a populated one gave an
    /// array. The field's type tracked the data.
    #[test]
    fn an_empty_marked_table_encodes_as_an_array_not_an_object() {
        let lua = TestLuaBuilder::new().build();

        let empty = mark_json_array(&lua, lua.create_table().unwrap()).unwrap();
        let json = lua_to_json(&lua, mlua::Value::Table(empty)).unwrap();
        assert_eq!(json.to_string(), "[]", "an empty marked table must be []");

        let unmarked = lua.create_table().unwrap();
        let json = lua_to_json(&lua, mlua::Value::Table(unmarked)).unwrap();
        assert_eq!(
            json.to_string(),
            "{}",
            "unmarked stays an object; this is the trap"
        );
    }

    /// Marking must not disturb a populated list.
    #[test]
    fn a_populated_marked_table_still_encodes_as_an_array() {
        let lua = TestLuaBuilder::new().build();
        let t = lua.create_table().unwrap();
        t.push("brave").unwrap();
        t.push("startpage").unwrap();
        let marked = mark_json_array(&lua, t).unwrap();
        let json = lua_to_json(&lua, mlua::Value::Table(marked)).unwrap();
        assert_eq!(json.to_string(), r#"["brave","startpage"]"#);
    }

    // =========================================================================
    // oq module tests
    // =========================================================================

    #[test]
    fn test_oq_parse_json() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(r#"return oq.parse('{"name": "Alice", "age": 30}')"#)
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_oq_parse_array() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua.load(r#"return oq.parse('[1, 2, 3]')"#).eval().unwrap();

        assert_eq!(result.get::<i64>(1).unwrap(), 1);
        assert_eq!(result.get::<i64>(2).unwrap(), 2);
        assert_eq!(result.get::<i64>(3).unwrap(), 3);
    }

    #[test]
    fn test_oq_roundtrip() {
        // `oq.json`/`oq.json_pretty` are removed (cru.json.encode owns JSON
        // encoding); the encode->parse roundtrip lives on through yaml.
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: bool = lua
            .load(
                r#"
                local original = { name = "Test", values = { 1, 2, 3 }, nested = { x = 10 } }
                local encoded = oq.yaml(original)
                local decoded = oq.parse_as(encoded, "yaml")
                return decoded.name == "Test" and decoded.nested.x == 10
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }

    #[test]
    fn test_oq_parse_toon() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse([[
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
    fn test_oq_toon_encoding() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(r#"return oq.toon({ name = "Bob", age = 25 })"#)
            .eval()
            .unwrap();

        assert!(result.contains("name: Bob") || result.contains("name:Bob"));
        assert!(result.contains("age: 25") || result.contains("age:25"));
    }

    #[test]
    fn test_oq_query_single() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Alice", age = 30 }
                return oq.query(obj, ".name")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, "Alice");
    }

    #[test]
    fn test_oq_query_array_access() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: i64 = lua
            .load(
                r#"
                local arr = { 10, 20, 30 }
                return oq.query(arr, ".[1]")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 20);
    }

    #[test]
    fn test_oq_query_iterate() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"
                local users = {
                    { name = "Alice", age = 30 },
                    { name = "Bob", age = 25 }
                }
                return oq.query(users, ".[] | .name")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>(1).unwrap(), "Alice");
        assert_eq!(result.get::<String>(2).unwrap(), "Bob");
    }

    #[test]
    fn test_oq_query_nested() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: i64 = lua
            .load(
                r#"
                local obj = { user = { profile = { score = 100 } } }
                return oq.query(obj, ".user.profile.score")
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 100);
    }

    #[test]
    fn test_oq_format() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Test", value = 42 }
                return oq.format(obj)
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("name") && result.contains("Test"));
        assert!(result.contains("value") && result.contains("42"));
    }

    #[test]
    fn test_oq_format_with_tool_type() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(
                r#"
                local response = { path = "file.lua", content = "print('hello')" }
                return oq.format(response, { tool = "read_file" })
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("file.lua") || result.contains("path"));
    }

    #[test]
    fn test_oq_null() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(r#"return oq.yaml({ value = oq.null })"#)
            .eval()
            .unwrap();

        assert!(
            result.contains("null"),
            "the null marker must encode as null: {result}"
        );
    }

    // =========================================================================
    // YAML format tests
    // =========================================================================

    #[test]
    fn test_oq_parse_yaml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse_as([[
name: Alice
age: 30
]], 'yaml')"#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_oq_parse_yaml_with_list() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse_as([[
items:
  - first
  - second
  - third
]], 'yaml')"#,
            )
            .eval()
            .unwrap();

        let items: Table = result.get("items").unwrap();
        assert_eq!(items.get::<String>(1).unwrap(), "first");
        assert_eq!(items.get::<String>(2).unwrap(), "second");
        assert_eq!(items.get::<String>(3).unwrap(), "third");
    }

    #[test]
    fn test_oq_yaml_encoding() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(r#"return oq.yaml({ name = "Bob", age = 25 })"#)
            .eval()
            .unwrap();

        assert!(result.contains("name:") || result.contains("name :"));
        assert!(result.contains("Bob"));
        assert!(result.contains("age:") || result.contains("age :"));
        assert!(result.contains("25"));
    }

    #[test]
    fn test_oq_yaml_nested() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse_as([[
user:
  name: Alice
  profile:
    score: 100
]], 'yaml')"#,
            )
            .eval()
            .unwrap();

        let user: Table = result.get("user").unwrap();
        assert_eq!(user.get::<String>("name").unwrap(), "Alice");
        let profile: Table = user.get("profile").unwrap();
        assert_eq!(profile.get::<i64>("score").unwrap(), 100);
    }

    // =========================================================================
    // TOML format tests
    // =========================================================================

    #[test]
    fn test_oq_parse_toml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse_as([[
name = "Alice"
age = 30
]], 'toml')"#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("name").unwrap(), "Alice");
        assert_eq!(result.get::<i64>("age").unwrap(), 30);
    }

    #[test]
    fn test_oq_parse_toml_with_section() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: Table = lua
            .load(
                r#"return oq.parse_as([[
[package]
name = "my-app"
version = "1.0.0"

[dependencies]
serde = "1.0"
]], 'toml')"#,
            )
            .eval()
            .unwrap();

        let package: Table = result.get("package").unwrap();
        assert_eq!(package.get::<String>("name").unwrap(), "my-app");
        assert_eq!(package.get::<String>("version").unwrap(), "1.0.0");

        let deps: Table = result.get("dependencies").unwrap();
        assert_eq!(deps.get::<String>("serde").unwrap(), "1.0");
    }

    #[test]
    fn test_oq_toml_encoding() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(r#"return oq.toml({ name = "Bob", age = 25 })"#)
            .eval()
            .unwrap();

        assert!(result.contains("name = "));
        assert!(result.contains("\"Bob\""));
        assert!(result.contains("age = "));
        assert!(result.contains("25"));
    }

    // =========================================================================
    // Format detection tests
    // =========================================================================

    #[test]
    fn test_detect_json() {
        assert_eq!(detect_format(r#"{"name": "Alice"}"#), Format::Json);
        assert_eq!(detect_format(r#"[1, 2, 3]"#), Format::Json);
    }

    #[test]
    fn test_detect_yaml() {
        assert_eq!(detect_format("---\nname: Alice"), Format::Yaml);
        assert_eq!(detect_format("- item1\n- item2"), Format::Yaml);
        assert_eq!(detect_format("user:\n  name: Alice"), Format::Yaml);
    }

    #[test]
    fn test_detect_toml() {
        assert_eq!(detect_format("[package]\nname = \"test\""), Format::Toml);
        assert_eq!(detect_format("name = \"Alice\"\nage = 30"), Format::Toml);
        assert_eq!(detect_format("name = \"Alice\""), Format::Toml);
    }

    #[test]
    fn test_detect_toon() {
        assert_eq!(detect_format("name: Alice\nage: 30"), Format::Toon);
    }

    #[test]
    fn test_oq_detect_function() {
        let lua = TestLuaBuilder::new().with_json_query().build();

        let json: String = lua.load(r#"return oq.detect('{"x": 1}')"#).eval().unwrap();
        assert_eq!(json, "json");

        let toml: String = lua
            .load(
                r#"return oq.detect([[
[package]
name = "test"
]])"#,
            )
            .eval()
            .unwrap();
        assert_eq!(toml, "toml");
    }

    // =========================================================================
    // Format conversion tests
    // =========================================================================

    #[test]
    fn test_oq_convert_to_yaml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Alice", age = 30 }
                return oq.convert(obj, 'yaml')
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("name"));
        assert!(result.contains("Alice"));
    }

    #[test]
    fn test_oq_convert_to_toml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: String = lua
            .load(
                r#"
                local obj = { name = "Alice", age = 30 }
                return oq.convert(obj, 'toml')
            "#,
            )
            .eval()
            .unwrap();

        assert!(result.contains("name = "));
        assert!(result.contains("age = "));
    }

    #[test]
    fn test_oq_roundtrip_yaml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: bool = lua
            .load(
                r#"
                local original = { name = "Test", values = { "a", "b", "c" } }
                local yaml_str = oq.yaml(original)
                local parsed = oq.parse_as(yaml_str, 'yaml')
                return parsed.name == "Test" and parsed.values[1] == "a"
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }

    #[test]
    fn test_oq_roundtrip_toml() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: bool = lua
            .load(
                r#"
                local original = { name = "Test", version = "1.0.0" }
                local toml_str = oq.toml(original)
                local parsed = oq.parse_as(toml_str, 'toml')
                return parsed.name == "Test" and parsed.version == "1.0.0"
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }

    #[test]
    fn test_oq_cross_format_conversion() {
        let lua = TestLuaBuilder::new().with_json_query().build();
        let result: bool = lua
            .load(
                r#"
                local obj = oq.parse('{"name": "Alice", "active": true}')
                local yaml_str = oq.yaml(obj)
                local from_yaml = oq.parse_as(yaml_str, 'yaml')
                local toml_str = oq.toml(from_yaml)
                local from_toml = oq.parse_as(toml_str, 'toml')
                return from_toml.name == "Alice" and from_toml.active == true
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }
}
