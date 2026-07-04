use crucible_core::events::SessionEvent;
use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

/// Convert SessionEvent to Lua table
///
/// Creates a Lua table representation of the event suitable for script processing.
pub(super) fn session_event_to_lua(lua: &Lua, event: &SessionEvent) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("type", event.type_name())?;
    table.set("event_type", event.event_type())?;
    table.set("summary", event.summary(200))?;

    // Flatten serialized event fields into the table for Lua access
    match serde_json::to_value(event) {
        Ok(json) => {
            if let JsonValue::Object(map) = json {
                for (key, value) in map {
                    if key != "type" {
                        let lua_val = lua.to_value(&value)?;
                        table.set(key, lua_val)?;
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to serialize event to JSON: {}", e);
        }
    }

    Ok(table)
}

/// Convert Lua table to JSON value
pub(super) fn lua_table_to_json(table: &Table) -> LuaResult<JsonValue> {
    let mut map = serde_json::Map::new();

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;

        let key_str = match key {
            Value::String(s) => s.to_str()?.to_string(),
            Value::Integer(i) => i.to_string(),
            _ => continue, // Skip non-string, non-integer keys
        };

        let json_val = serde_json::to_value(&value).map_err(mlua::Error::external)?;
        map.insert(key_str, json_val);
    }

    Ok(JsonValue::Object(map))
}
