use crucible_core::events::SessionEvent;
use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

/// Convert SessionEvent to Lua table
///
/// Creates a Lua table representation of the event suitable for script processing.
pub(crate) fn session_event_to_lua(lua: &Lua, event: &SessionEvent) -> LuaResult<Table> {
    let table = lua.create_table()?;

    // `SessionEvent::Custom { name, payload }` is an internal Rust shape, not a
    // designed handler API — serde turned it into `event.payload.tool` and made
    // `event.name` mean the event type, while the docs and every plugin used
    // `event.tool` / `event.args`. Project the payload to the top level and let
    // `type` carry the event name, so a handler sees one flat table.
    // Pinned by `handlers::tests::conversion`.
    if let SessionEvent::Custom { name, payload } = event {
        if let JsonValue::Object(map) = payload {
            for (key, value) in map {
                table.set(key.as_str(), lua.to_value(value)?)?;
            }
        }
        // Envelope last: `event.type` stays trustworthy even if a payload
        // carries a colliding key.
        table.set("type", name.as_str())?;
        table.set("event_type", event.event_type())?;
        table.set("summary", event.summary(200))?;
        return Ok(table);
    }

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

/// Keys the event envelope owns. Payload keys never override these, and they
/// are stripped back out when rebuilding an event from a handler's return.
const ENVELOPE_KEYS: [&str; 3] = ["type", "event_type", "summary"];

/// Project a `SessionEvent` to the flat JSON shape handlers see.
///
/// The JSON twin of [`session_event_to_lua`], for the file-handler path in
/// `core_handler`, which round-trips through JSON in a separate Lua state
/// rather than building a `Table` directly. Both paths are live — script
/// handlers via `crucible.on`, file handlers via `to_core_handlers` — so they
/// have to agree on the shape or plugin authors face two contracts.
pub(crate) fn session_event_to_flat_json(event: &SessionEvent) -> JsonValue {
    let mut json = serde_json::to_value(event).unwrap_or(JsonValue::Null);
    let SessionEvent::Custom { name, payload } = event else {
        return json;
    };

    let mut flat = match payload {
        JsonValue::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    flat.insert("type".into(), JsonValue::String(name.clone()));
    flat.insert(
        "event_type".into(),
        JsonValue::String(event.event_type().to_string()),
    );
    flat.insert("summary".into(), JsonValue::String(event.summary(200)));
    json = JsonValue::Object(flat);
    json
}

/// Inverse of [`session_event_to_flat_json`].
///
/// A handler returns the same flat table it was given, so rebuild the
/// `SessionEvent::Custom` envelope around it before deserializing. Only
/// applies when the event *was* `Custom`; every other variant already
/// round-trips through serde unchanged.
pub(crate) fn flat_json_to_session_event_json(
    flat: JsonValue,
    original: &SessionEvent,
) -> JsonValue {
    let SessionEvent::Custom { name, .. } = original else {
        return flat;
    };
    let JsonValue::Object(mut map) = flat else {
        return flat;
    };
    for key in ENVELOPE_KEYS {
        map.remove(key);
    }
    // Lowercase `custom` is the serde tag; `SessionEvent::type_name()` returns
    // the capitalised variant name and is not interchangeable here.
    serde_json::json!({
        "type": "custom",
        "name": name,
        "payload": JsonValue::Object(map),
    })
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
