use crucible_core::events::SessionEvent;
use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

/// Keys the event envelope owns. Payload keys never override these, and they
/// are stripped back out when rebuilding an event from a handler's return.
/// Reserved: a payload key with one of these names is overwritten on the way
/// in (with a warning) and unrecoverable on the way back.
const ENVELOPE_KEYS: [&str; 3] = ["type", "event_type", "summary"];

/// Stamp the envelope onto a flat map, envelope winning over payload keys.
///
/// The warning is the M2 tradeoff made visible: flattening cannot carry a
/// payload key named `summary` (or `type`, `event_type`) — the envelope owns
/// those names, and the payload value is dropped on the round-trip. Warning at
/// the collision site beats silently returning a different event than the
/// plugin sent.
fn stamp_envelope(
    map: &mut serde_json::Map<String, JsonValue>,
    type_value: &str,
    event: &SessionEvent,
) {
    for key in ENVELOPE_KEYS {
        if map.contains_key(key) {
            warn!(
                key,
                event = type_value,
                "event payload key collides with a reserved envelope key; \
                 the envelope value wins and the payload value is dropped"
            );
        }
    }
    map.insert("type".into(), JsonValue::String(type_value.to_string()));
    map.insert(
        "event_type".into(),
        JsonValue::String(event.event_type().to_string()),
    );
    map.insert("summary".into(), JsonValue::String(event.summary(200)));
}

/// Project a `SessionEvent` to the flat shape handlers see.
///
/// `SessionEvent::Custom { name, payload }` is an internal Rust shape, not a
/// designed handler API — serde turned it into `event.payload.tool` and made
/// `event.name` mean the event type, while the docs and every plugin used
/// `event.tool` / `event.args`. Project the payload to the top level and let
/// `type` carry the event name (or the variant name for built-in events), so
/// a handler sees one flat table. Pinned by `handlers::tests::conversion`.
///
/// This is the single projection: [`session_event_to_lua`] (script handlers
/// via `crucible.on`) is built from this JSON, so the two live handler paths
/// cannot drift apart — they used to, presenting `"ToolCalled"` to one and
/// `"tool_called"` to the other for the same event.
pub(crate) fn session_event_to_flat_json(event: &SessionEvent) -> JsonValue {
    let mut flat = match event {
        SessionEvent::Custom { payload, .. } => match payload {
            JsonValue::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        },
        _ => match serde_json::to_value(event) {
            Ok(JsonValue::Object(mut map)) => {
                // The serde tag is snake_case ("tool_called"); handlers see
                // `type_name()` ("ToolCalled"). Dropped here, restored from
                // the original event in the inverse.
                map.remove("type");
                map
            }
            Ok(_) => serde_json::Map::new(),
            // Preserve the error signal — `core_handler` checks for Null.
            Err(e) => {
                warn!("Failed to serialize event to JSON: {}", e);
                return JsonValue::Null;
            }
        },
    };

    let type_value = match event {
        SessionEvent::Custom { name, .. } => name.clone(),
        _ => event.type_name().to_string(),
    };
    stamp_envelope(&mut flat, &type_value, event);
    JsonValue::Object(flat)
}

/// Convert SessionEvent to Lua table.
///
/// Built from [`session_event_to_flat_json`] so script handlers and file
/// handlers see the same shape by construction.
pub(crate) fn session_event_to_lua(lua: &Lua, event: &SessionEvent) -> LuaResult<Table> {
    let table = lua.create_table()?;
    match session_event_to_flat_json(event) {
        JsonValue::Object(map) => {
            for (key, value) in map {
                table.set(key.as_str(), lua.to_value(&value)?)?;
            }
        }
        _ => {
            // Serialization failed; give handlers the envelope rather than
            // an empty table.
            table.set("type", event.type_name())?;
            table.set("event_type", event.event_type())?;
            table.set("summary", event.summary(200))?;
        }
    }
    Ok(table)
}

/// Inverse of [`session_event_to_flat_json`].
///
/// A handler returns the same flat table it was given, so rebuild the shape
/// `serde_json::from_value::<SessionEvent>` expects: the `Custom` envelope for
/// custom events, the snake_case serde tag for built-in variants.
pub(crate) fn flat_json_to_session_event_json(
    flat: JsonValue,
    original: &SessionEvent,
) -> JsonValue {
    let JsonValue::Object(mut map) = flat else {
        return flat;
    };
    for key in ENVELOPE_KEYS {
        map.remove(key);
    }

    if let SessionEvent::Custom { name, .. } = original {
        // Lowercase `custom` is the serde tag; `SessionEvent::type_name()`
        // returns the capitalised variant name and is not interchangeable.
        return serde_json::json!({
            "type": "custom",
            "name": name,
            "payload": JsonValue::Object(map),
        });
    }

    // Built-in variant: restore the serde tag the flat shape replaced with
    // `type_name()`. Taken from the original event rather than derived, so a
    // rename in either scheme cannot desynchronise them.
    if let Ok(JsonValue::Object(orig)) = serde_json::to_value(original) {
        if let Some(tag) = orig.get("type") {
            map.insert("type".into(), tag.clone());
        }
    }
    JsonValue::Object(map)
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
