//! One definition of a leaf, and the nested merge the settings file needs.
//!
//! **The store is flat.** `cru.config.set { chat = { model = "x" } }` is
//! authoring sugar for one write at one path, `chat.model`. [`flatten_leaves`]
//! is the door every config write goes through, so the rank gate, the pin
//! walk and `config.save` all ask about the same paths and cannot disagree
//! about what a leaf is. Neovim core makes the same choice: options are flat,
//! `set opt=x` replaces, and a deep merge is a library call a plugin makes for
//! itself.
//!
//! A nested table therefore *loses nothing*: writing `chat.model` leaves every
//! other `chat` key alone because the write never names them. What it cannot
//! say is "this key is gone" — that verb is `config.unset`.
//!
//! [`deep_merge`] survives for one caller: the `settings.json` rewrite. That
//! file stays nested JSON on disk, because a settings UI round-trips it and a
//! person may open it.
//!
//! [`flatten_leaves`]: crate::config::flatten_leaves
//! [`deep_merge`]: crate::config::deep_merge

use serde_json::{Map, Value};

/// Deep-merge `overlay` into `base`: objects merge key by key, recursively;
/// arrays and scalars replace wholesale. `null` is a value, not a deletion.
///
/// **This is the `settings.json` rewrite, not the config store.** The store
/// flattens ([`flatten_leaves`]) and writes one leaf at a time. The file is
/// merged rather than replaced so that a hand edit survives a save that does
/// not restate it.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match overlay {
        Value::Object(map) => {
            let Value::Object(base_map) = base else {
                // Type change: an object over an array or a scalar replaces.
                *base = Value::Object(map);
                return;
            };
            for (key, value) in map {
                match base_map.get_mut(&key) {
                    Some(slot) => deep_merge(slot, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        other => *base = other,
    }
}

/// Every terminal value of `overlay`, as `(dot-joined path, value)` pairs, in
/// the order the overlay wrote them.
///
/// **What counts as terminal.** A scalar is terminal. An array is terminal:
/// arrays replace wholesale, so no element has an identity to record — this is
/// what makes `runtimepath` one leaf and `cru.rtp.append` the way to add to it.
/// A non-empty object is not terminal; it is a path segment.
///
/// **An empty table sets nothing.** `{ chat = {} }` names no terminal value,
/// so it writes no leaf, records no provenance row, and takes no rank. The
/// alternative — treating it as a value — made the BRANCH a leaf, which is
/// what let one `config.save` remove an unrelated subtree from the layer under
/// it. A caller that means "remove what is there" says `config.unset`.
///
/// **A key that already holds a dot is a path.** `{ ["myplugin.debug"] = true }`
/// and `{ myplugin = { debug = true } }` name ONE leaf, `myplugin.debug`, so
/// `:set myplugin.debug=1` writes where a nested table writes. Two spellings
/// of one path in one overlay collapse to one entry, and the last wins.
///
/// A `overlay` that is not an object yields nothing: no path names the root,
/// and the store's value is always an object.
pub fn flatten_leaves(overlay: Value) -> Vec<(String, Value)> {
    let mut leaves = Vec::new();
    if let Value::Object(map) = overlay {
        push_leaves(&mut String::new(), map, &mut leaves);
    }
    leaves
}

/// The recursive half of [`flatten_leaves`].
fn push_leaves(path: &mut String, map: Map<String, Value>, leaves: &mut Vec<(String, Value)>) {
    for (key, value) in map {
        let saved = path.len();
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(&key);
        match value {
            Value::Object(child) if !child.is_empty() => push_leaves(path, child, leaves),
            // An empty object falls here and is dropped: `is_empty` sends it
            // to this arm, and the arm records nothing for it.
            Value::Object(_) => {}
            terminal => leaves.push((path.clone(), terminal)),
        }
        path.truncate(saved);
    }
}

/// Write one terminal value at a dot-joined path, building the objects on the
/// way.
///
/// A path segment that holds a non-object is replaced by the object the rest
/// of the path needs: the store's value is a projection of its leaves, and a
/// leaf that exists has to be reachable.
pub fn set_leaf(root: &mut Value, path: &str, value: Value) {
    let Some((head, rest)) = path.split_once('.') else {
        if let Value::Object(map) = root {
            map.insert(path.to_string(), value);
        }
        return;
    };
    let Value::Object(map) = root else {
        return;
    };
    let slot = map
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    set_leaf(slot, rest, value);
}

/// The value at a dot-joined path, if the tree holds one.
///
/// The read half of [`set_leaf`], and the ONE resolver both config read doors
/// use — `cru.config.get` and the `config.get` RPC — so `:set myplugin.debug=1`
/// and `:set myplugin.debug?` name the same value. It descends segment by
/// segment and never tries the literal name first: the store holds no key with
/// a dot in it, because [`flatten_leaves`] turns a dotted key into a path.
pub fn leaf_at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cursor = root;
    for segment in path.split('.') {
        cursor = cursor.get(segment)?;
    }
    Some(cursor)
}

/// The nested object that holds exactly `leaves`.
///
/// The inverse of [`flatten_leaves`], for the two places a nested shape is
/// what the caller needs: `settings.json` on disk, and the accepted delta a
/// `config.save` answers with.
pub fn nest_leaves(leaves: impl IntoIterator<Item = (String, Value)>) -> Value {
    let mut root = Value::Object(Map::new());
    for (path, value) in leaves {
        set_leaf(&mut root, &path, value);
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn merged(mut base: Value, overlay: Value) -> Value {
        deep_merge(&mut base, overlay);
        base
    }

    fn paths(overlay: Value) -> Vec<String> {
        flatten_leaves(overlay)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }

    // ── `deep_merge`: the `settings.json` rewrite ────────────────────────

    /// A save that restates one `chat` key must not drop the rest of the file.
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
        let result = merged(
            json!({"session_kiln": "notes"}),
            json!({"session_kiln": null}),
        );
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

    // ── `flatten_leaves`: the store's one definition of a leaf ───────────

    #[test]
    fn a_nested_table_flattens_to_one_leaf_per_terminal_value() {
        assert_eq!(
            flatten_leaves(json!({"chat": {"model": "x", "show_thinking": true}})),
            vec![
                ("chat.model".to_string(), json!("x")),
                ("chat.show_thinking".to_string(), json!(true)),
            ]
        );
    }

    /// An array replaces wholesale, so it has no per-element identity and is
    /// one leaf.
    #[test]
    fn an_array_is_one_leaf() {
        assert_eq!(
            flatten_leaves(json!({"runtimepath": ["/a", "/b"]})),
            vec![("runtimepath".to_string(), json!(["/a", "/b"]))]
        );
    }

    /// `null` is a value a config can mean (`session_kiln = nil` is not
    /// expressible in Lua, but JSON and TOML seeds carry it), so it is a leaf.
    #[test]
    fn null_is_a_leaf() {
        assert_eq!(
            flatten_leaves(json!({"session_kiln": null})),
            vec![("session_kiln".to_string(), json!(null))]
        );
    }

    #[test]
    fn an_empty_table_names_no_leaf_at_any_depth() {
        assert_eq!(paths(json!({"chat": {}})), Vec::<String>::new());
        assert_eq!(
            paths(json!({"llm": {"providers": {}}})),
            Vec::<String>::new()
        );
        assert_eq!(paths(json!({})), Vec::<String>::new());
    }

    /// The dotted spelling and the nested one are the same leaf, which is what
    /// makes `:set myplugin.debug=1` write where a config file writes.
    #[test]
    fn a_dotted_key_and_a_nested_table_name_one_path() {
        assert_eq!(
            flatten_leaves(json!({"myplugin.debug": true})),
            flatten_leaves(json!({"myplugin": {"debug": true}}))
        );
        assert_eq!(
            paths(json!({"myplugin.debug": true})),
            vec!["myplugin.debug"]
        );
    }

    /// Round-tripping a flatten through [`nest_leaves`] is the identity on
    /// everything a store can hold, which is what lets `settings.json` stay
    /// nested while the store stays flat.
    #[test]
    fn nesting_the_leaves_again_rebuilds_the_table() {
        let overlay = json!({
            "chat": {"model": "x", "show_thinking": true},
            "runtimepath": ["/a"],
            "myplugin.debug": true,
        });
        assert_eq!(
            nest_leaves(flatten_leaves(overlay)),
            json!({
                "chat": {"model": "x", "show_thinking": true},
                "runtimepath": ["/a"],
                "myplugin": {"debug": true},
            })
        );
    }

    /// A non-object overlay names no path, so it contributes nothing. The
    /// store's value is always an object.
    #[test]
    fn a_scalar_overlay_names_no_leaf() {
        assert_eq!(flatten_leaves(json!("scalar")), Vec::new());
        assert_eq!(flatten_leaves(json!([1, 2])), Vec::new());
    }
}
