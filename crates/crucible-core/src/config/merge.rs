//! The deep merge that layers config sources.
//!
//! The rule, decided in `thoughts/2026-08-24-lua-versus-toml-config.md` and
//! specified in `thoughts/2026-08-24-lua-config-milestone-1.md` §4: objects
//! merge key by key, recursively; arrays and scalars replace wholesale. The
//! escape is a data spelling, [`REPLACE_MARKER`], because the rule governs
//! every layer — a Lua-only function could not make a TOML seed replace a
//! table.
//!
//! The motivating bug is sibling loss: the old top-level insert made
//! `cru.config.set { chat = { show_thinking = true } }` drop every other
//! `chat` key back to its default, silently.

use serde_json::Value;

/// The key that makes a table replace instead of merge.
///
/// `__replace = true` is valid bare syntax in TOML, JSON keys and a Lua table
/// literal. Its presence triggers replacement; its value is never inspected.
/// The marker is consumed wherever it appears — it can never leak into the
/// extracted config or a `cru.config.get` read.
pub const REPLACE_MARKER: &str = "__replace";

/// Deep-merge `overlay` into `base`. Tables merge key by key.
/// Arrays and scalars replace. [`REPLACE_MARKER`] inside a table
/// makes that table replace instead of merge. The marker key is
/// always consumed and never appears in the result.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    deep_merge_traced(base, overlay, &mut String::new(), &mut |_, _| {});
}

/// [`deep_merge`], reporting each landing point to `observer`.
///
/// The observer runs once per wholesale write — an inserted key, a replaced
/// value — with the dot-joined path and the (marker-stripped) value that
/// landed there. The config store uses it to record provenance without a
/// second copy of the merge walk; per-key recursion into a merged object
/// reports the leaves it writes, never the object itself.
pub fn deep_merge_traced(
    base: &mut Value,
    overlay: Value,
    path: &mut String,
    observer: &mut dyn FnMut(&str, &Value),
) {
    match overlay {
        Value::Object(map) if !map.contains_key(REPLACE_MARKER) => {
            if let Value::Object(base_map) = base {
                for (key, value) in map {
                    let saved = path.len();
                    if !path.is_empty() {
                        path.push('.');
                    }
                    path.push_str(&key);
                    match base_map.get_mut(&key) {
                        Some(slot) => deep_merge_traced(slot, value, path, observer),
                        None => {
                            let value = strip_replace_markers(value);
                            observer(path, &value);
                            base_map.insert(key, value);
                        }
                    }
                    path.truncate(saved);
                }
            } else {
                // Type change: object over array or scalar replaces.
                let value = strip_replace_markers(Value::Object(map));
                observer(path, &value);
                *base = value;
            }
        }
        // A marked table, an array, or a scalar: replacement wins. `null` is
        // a value, not a deletion marker.
        other => {
            let value = strip_replace_markers(other);
            observer(path, &value);
            *base = value;
        }
    }
}

/// Remove [`REPLACE_MARKER`] at every depth of `value`.
///
/// Applied to everything that lands by insertion or replacement, so a nested
/// marker inside a fresh subtree is consumed exactly like one on a merge path.
pub fn strip_replace_markers(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| key != REPLACE_MARKER)
                .map(|(key, value)| (key, strip_replace_markers(value)))
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(strip_replace_markers).collect())
        }
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn merged(mut base: Value, overlay: Value) -> Value {
        deep_merge(&mut base, overlay);
        base
    }

    /// The motivating bug: a partial `chat` table must not drop its siblings.
    #[test]
    fn a_partial_table_keeps_the_siblings_it_does_not_name() {
        let result = merged(
            json!({"chat": {"show_thinking": false, "agent_preference": "a", "show_diffs": true}}),
            json!({"chat": {"show_thinking": true}}),
        );
        assert_eq!(
            result,
            json!({"chat": {"show_thinking": true, "agent_preference": "a", "show_diffs": true}})
        );
    }

    #[test]
    fn the_replace_marker_removes_what_the_overlay_omits() {
        let result = merged(
            json!({"llm": {"providers": {"a": {"endpoint": "x"}, "b": {"endpoint": "y"}}}}),
            json!({"llm": {"providers": {"__replace": true, "a": {"endpoint": "x"}}}}),
        );
        assert_eq!(result, json!({"llm": {"providers": {"a": {"endpoint": "x"}}}}));
    }

    #[test]
    fn arrays_replace_and_never_merge_element_wise() {
        let result = merged(
            json!({"runtimepath": ["/a", "/b"]}),
            json!({"runtimepath": ["/c"]}),
        );
        assert_eq!(result, json!({"runtimepath": ["/c"]}));
    }

    #[test]
    fn a_type_change_replaces_in_both_directions() {
        assert_eq!(
            merged(json!({"k": "scalar"}), json!({"k": {"a": 1}})),
            json!({"k": {"a": 1}})
        );
        assert_eq!(
            merged(json!({"k": {"a": 1}}), json!({"k": "scalar"})),
            json!({"k": "scalar"})
        );
    }

    #[test]
    fn null_is_a_value_not_a_deletion_marker() {
        let result = merged(json!({"session_kiln": "notes"}), json!({"session_kiln": null}));
        assert_eq!(result, json!({"session_kiln": null}));
    }

    #[test]
    fn a_three_level_overlay_keeps_the_deep_siblings() {
        let result = merged(
            json!({"llm": {"providers": {"local": {"default_model": "m", "endpoint": "old"}}}}),
            json!({"llm": {"providers": {"local": {"endpoint": "http://x"}}}}),
        );
        assert_eq!(
            result,
            json!({"llm": {"providers": {"local": {"default_model": "m", "endpoint": "http://x"}}}})
        );
    }

    #[test]
    fn a_bare_marker_replaces_with_an_empty_table() {
        let result = merged(
            json!({"providers": {"a": 1}}),
            json!({"providers": {"__replace": true}}),
        );
        assert_eq!(result, json!({"providers": {}}));
    }

    #[test]
    fn a_marker_nested_in_an_inserted_subtree_is_stripped() {
        let result = merged(
            json!({}),
            json!({"fresh": {"inner": {"__replace": true, "kept": 1}}}),
        );
        assert_eq!(result, json!({"fresh": {"inner": {"kept": 1}}}));
    }
}
