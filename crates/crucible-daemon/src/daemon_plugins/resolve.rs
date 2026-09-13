//! Where a plugin's `enabled` flag and its `opts` come from.
//!
//! Two pure functions, and the only readers of the two resolution rules the
//! activation plan states. Both take the merged [`Spec`] and the config
//! store's `plugins.<name>` section; `resolve_opts` also takes the plugin's
//! fragment, through its manifest. `docs/Meta/CONTEXT.md` defines the words.
//!
//! The Builtin fragment must not write `enabled` through the config store: a
//! `cru.config.set` under `LuaSource::Builtin` lands at rank Lua, which
//! outranks the web's `settings.json`, so the web could never disable a
//! shipped plugin. The spec lives in its own store, and `enabled` resolves
//! across the two stores here.

use crucible_core::config::{Spec, SpecRank};
use serde_json::{Map, Value};

/// Whether `name` is enabled. First answer wins:
///
/// 1. the operator's own entry (`SpecRank::Operator`);
/// 2. the config leaf `plugins.<name>.enabled`, at any layer, so the web's
///    toggle in `settings.json` still works;
/// 3. the merged entry below the operator: the Builtin fragment, then the
///    plugin's own fragment, which `Spec::merge` already ordered;
/// 4. `true`.
pub(crate) fn resolve_enabled(spec: &Spec, name: &str, config_leaf: Option<bool>) -> bool {
    let operator = spec.at(name, SpecRank::Operator).and_then(|e| e.enabled);
    let fragments = spec.get(name).and_then(|e| e.enabled);
    operator.or(config_leaf).or(fragments).unwrap_or(true)
}

/// The `opts` table `setup(opts)` and `config(module, opts)` receive.
///
/// A shallow object merge, lowest first: the plugin fragment's `opts`
/// (`manifest_opts`, which discovery read), the Builtin fragment's, the
/// config leaves under `plugins.<name>`, then the operator's entry, so the
/// operator's own line beats a saved setting.
///
/// The fragment reaches this function through the manifest only. Discovery
/// reads `spec.luau` into `PluginManifest::opts` and writes nothing to the
/// spec store, so a `SpecRank::PluginFragment` layer would read the same
/// table a second time, or read nothing.
///
/// `enabled` is not an opt: it is stripped from the config section before
/// the merge. The result is always an object, so a plugin can index it.
pub(crate) fn resolve_opts(
    spec: &Spec,
    name: &str,
    manifest_opts: &Value,
    config_section: &Value,
) -> Value {
    let mut out = Map::new();
    lay(&mut out, manifest_opts);
    lay_rank(&mut out, spec, name, SpecRank::Builtin);
    let mut section = config_section.as_object().cloned().unwrap_or_default();
    section.remove("enabled");
    out.extend(section);
    lay_rank(&mut out, spec, name, SpecRank::Operator);
    Value::Object(out)
}

/// Copy `over`'s keys onto `base`. A non-object contributes nothing.
fn lay(base: &mut Map<String, Value>, over: &Value) {
    if let Some(object) = over.as_object() {
        base.extend(object.clone());
    }
}

/// Copy the `opts` one rank wrote onto `base`, when that rank wrote any.
fn lay_rank(base: &mut Map<String, Value>, spec: &Spec, name: &str, rank: SpecRank) {
    if let Some(entry) = spec.at(name, rank) {
        lay(base, &entry.opts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::SpecEntry;
    use serde_json::json;

    #[test]
    fn a_settings_leaf_disables_a_shipped_plugin() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry::from_positional("reflection").unwrap(),
            SpecRank::Builtin,
        );
        assert!(!resolve_enabled(&spec, "reflection", Some(false)));
    }

    #[test]
    fn an_operator_entry_outranks_the_settings_leaf() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: Some(true),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        assert!(resolve_enabled(&spec, "x", Some(false)));
    }

    #[test]
    fn opts_merge_fragment_then_builtin_then_settings_then_operator() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                opts: json!({ "a": "builtin", "b": "builtin" }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Builtin,
        );
        spec.merge(
            SpecEntry {
                opts: json!({ "a": "operator" }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        let out = resolve_opts(
            &spec,
            "x",
            &json!({ "a": "fragment", "c": "fragment" }),
            &json!({ "b": "settings", "enabled": false }),
        );
        assert_eq!(
            out,
            json!({ "a": "operator", "b": "settings", "c": "fragment" })
        );
    }

    /// An operator entry that says nothing about `enabled` leaves the
    /// answer to the settings leaf, and a Builtin `enabled = false` below
    /// it still counts when no leaf speaks.
    #[test]
    fn an_operator_entry_that_says_nothing_defers_to_the_leaf_then_the_fragment() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: Some(false),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Builtin,
        );
        spec.merge(
            SpecEntry {
                opts: json!({ "n": 1 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        assert!(resolve_enabled(&spec, "x", Some(true)));
        assert!(!resolve_enabled(&spec, "x", None));
        assert!(resolve_enabled(&Spec::default(), "y", None));
    }

    /// A plugin with no entry and no section still gets a table.
    #[test]
    fn opts_are_always_an_object() {
        let out = resolve_opts(&Spec::default(), "x", &Value::Null, &Value::Null);
        assert_eq!(out, json!({}));
    }
}
