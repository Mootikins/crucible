//! The TOML → Lua emitter behind `cru config migrate`.
//!
//! Takes the JSON projection of a config's own keys (the oracle's parse of
//! `config.toml`, or the remainder after the migrate split) and renders one
//! `cru.config.set({...})` call. The T5.4 compatibility gate proves the
//! round trip: for every fixture TOML, evaluating the emitted Lua through
//! the one-VM boot path extracts a config equal to the oracle's.

use serde_json::Value;

/// Render `value`'s keys as one `cru.config.set({...})` call.
///
/// Top-level keys are emitted in schema order — the field order of
/// [`crate::config::CliAppConfig`], read off its serialized default
/// (`serde_json` preserves insertion order) — with unknown keys after, in
/// their input order. Nested tables keep their input order.
///
/// `null` never occurs (TOML cannot express it) and is skipped defensively.
/// An EMPTY table is also skipped: `[section]` with no keys configures
/// nothing the defaults layer does not already provide, and an empty Lua
/// table would round-trip as an array.
pub fn emit_lua_config(value: &Value) -> String {
    let Value::Object(map) = value else {
        return "cru.config.set({})\n".to_string();
    };

    let schema_order: Vec<String> = serde_json::to_value(crate::config::CliAppConfig::default())
        .ok()
        .and_then(|default| {
            default
                .as_object()
                .map(|object| object.keys().cloned().collect())
        })
        .unwrap_or_default();

    let mut ordered: Vec<(&String, &Value)> = Vec::new();
    for key in &schema_order {
        if let Some(entry) = map.get_key_value(key) {
            ordered.push(entry);
        }
    }
    for entry in map {
        if !schema_order.contains(entry.0) {
            ordered.push(entry);
        }
    }

    let mut out = String::from("cru.config.set({\n");
    for (key, value) in ordered {
        if skip_value(value) {
            continue;
        }
        out.push_str("    ");
        out.push_str(&emit_key(key));
        out.push_str(" = ");
        emit_value(&mut out, value, 1);
        out.push_str(",\n");
    }
    out.push_str("})\n");
    out
}

/// Whether a value has no Lua rendering that survives the round trip.
fn skip_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

/// A table key: bare when it is a Lua identifier, bracket-quoted otherwise.
fn emit_key(key: &str) -> String {
    let bare = !key.is_empty()
        && !key.chars().next().unwrap().is_ascii_digit()
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !is_lua_keyword(key);
    if bare {
        key.to_string()
    } else {
        format!("[{}]", quote(key))
    }
}

fn is_lua_keyword(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "goto"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

/// A Lua string literal for `text`, double-quoted with escapes.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\{}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn emit_value(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => out.push_str("nil"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&quote(s)),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for item in items {
                indent(out, depth + 1);
                emit_value(out, item, depth + 1);
                out.push_str(",\n");
            }
            indent(out, depth);
            out.push('}');
        }
        Value::Object(map) => {
            out.push_str("{\n");
            for (key, value) in map {
                if skip_value(value) {
                    continue;
                }
                indent(out, depth + 1);
                out.push_str(&emit_key(key));
                out.push_str(" = ");
                emit_value(out, value, depth + 1);
                out.push_str(",\n");
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_that_are_not_identifiers_are_bracket_quoted() {
        let lua = emit_lua_config(&json!({
            "kilns": { "my-notes": "/k", "plain": "/p" }
        }));
        assert!(lua.contains(r#"["my-notes"] = "/k""#), "{lua}");
        assert!(lua.contains(r#"plain = "/p""#), "{lua}");
    }

    #[test]
    fn strings_with_backslashes_and_quotes_survive_quoting() {
        let lua = emit_lua_config(&json!({ "default_kiln": "a\\b\"c" }));
        assert!(lua.contains(r#"default_kiln = "a\\b\"c""#), "{lua}");
    }

    #[test]
    fn schema_keys_come_before_free_form_keys() {
        let lua = emit_lua_config(&json!({
            "zz_custom": { "x": 1 },
            "kiln_path": "/kiln"
        }));
        let kiln = lua.find("kiln_path").expect("kiln_path emitted");
        let custom = lua.find("zz_custom").expect("free-form key emitted");
        assert!(kiln < custom, "schema order first:\n{lua}");
    }

    #[test]
    fn an_empty_section_is_skipped() {
        let lua = emit_lua_config(&json!({ "mcp": {}, "default_kiln": "notes" }));
        assert!(!lua.contains("mcp"), "{lua}");
        assert!(lua.contains("default_kiln"), "{lua}");
    }
}
