//! Data a plugin publishes for clients to render.
//!
//! `crucible.publish("<key>", value)` is the generic contribution channel: a
//! plugin states something about itself once, the daemon stores it verbatim,
//! and every client — TUI, web, anything later — reads the same answer.
//!
//! It exists because the alternative was clients reading plugin config. The web
//! learned which isolation profiles a box offered by reaching into raw
//! `[plugins.*]` TOML and matching on a `profiles` table — so `crucible-web`
//! encoded the `oci` plugin's config schema, in the rendering layer, for a
//! plugin whose entire design goal is that no Rust knows what a container is.
//! Every new option meant editing Rust, and a second isolating plugin with a
//! different config shape would simply not appear.
//!
//! Publications invert that. The plugin already has its own config — `setup()`
//! is handed it — so it is the only thing that should be deciding what its
//! config *means*. Values are opaque JSON here: this module never inspects
//! them, and a key it has never heard of round-trips unchanged.
//!
//! Unlike [`crate::plugin_status`] these are not session-scoped. A status slot
//! describes one live session; a publication describes the plugin.

use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Publications by key, then by publishing plugin.
///
/// Keyed that way round because clients ask by key ("who offers isolation?"),
/// and more than one plugin may answer. Attribution is kept so a client can
/// tell two answers apart and a stale entry can be traced home.
#[derive(Debug, Clone, Default)]
pub struct PublicationRegistry {
    entries: Arc<Mutex<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl PublicationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, plugin: &str, key: &str, value: serde_json::Value) {
        if let Ok(mut g) = self.entries.lock() {
            g.entry(key.to_string())
                .or_default()
                .insert(plugin.to_string(), value);
        }
    }

    /// Every plugin's answer for `key`, sorted by plugin name.
    ///
    /// Sorted rather than hash order so a client rendering two answers gets a
    /// stable order across reads.
    pub fn get(&self, key: &str) -> Vec<(String, serde_json::Value)> {
        let Ok(g) = self.entries.lock() else {
            return Vec::new();
        };
        let Some(by_plugin) = g.get(key) else {
            return Vec::new();
        };
        let mut out: Vec<_> = by_plugin
            .iter()
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Everything published, as `key -> plugin -> value`.
    pub fn all(&self) -> HashMap<String, HashMap<String, serde_json::Value>> {
        self.entries.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Drop everything a plugin published.
    ///
    /// Called when a plugin is reloaded: its previous answers described the
    /// previous version's config, and a publication that outlives the plugin
    /// that made it is indistinguishable from a current one.
    pub fn release_plugin(&self, plugin: &str) {
        if let Ok(mut g) = self.entries.lock() {
            for by_plugin in g.values_mut() {
                by_plugin.remove(plugin);
            }
            g.retain(|_, by_plugin| !by_plugin.is_empty());
        }
    }
}

/// Register `crucible.publish`.
///
/// ```lua
/// crucible.publish("isolation", {
///   available = true,
///   profiles  = { "rust", "throwaway" },
/// })
/// ```
///
/// `plugin` is supplied by the loader rather than the caller: a plugin naming
/// someone else as the author of its data would make attribution worthless.
pub fn register_publish_module(
    lua: &Lua,
    crucible: &Table,
    registry: PublicationRegistry,
    plugin: String,
) -> LuaResult<()> {
    let publish = lua.create_function(move |lua, (key, value): (String, mlua::Value)| {
        if key.is_empty() {
            return Err(mlua::Error::runtime(
                "crucible.publish: a key is required, e.g. crucible.publish(\"isolation\", {…})",
            ));
        }
        let json: serde_json::Value = lua.from_value(value).map_err(|e| {
            mlua::Error::runtime(format!(
                "crucible.publish: '{key}' must be JSON-encodable data, not a function or \
                 userdata: {e}"
            ))
        })?;
        registry.set(&plugin, &key, json);
        Ok(())
    })?;
    crucible.set("publish", publish)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lua_with_publish(registry: PublicationRegistry, plugin: &str) -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        register_publish_module(&lua, &crucible, registry, plugin.to_string()).unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    #[test]
    fn a_plugin_publishes_a_value_clients_can_read_back() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        lua.load(r#"crucible.publish("isolation", { available = true, profiles = { "rust" } })"#)
            .exec()
            .unwrap();

        let answers = reg.get("isolation");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].0, "oci");
        assert_eq!(answers[0].1["available"], json!(true));
        assert_eq!(answers[0].1["profiles"], json!(["rust"]));
    }

    /// The point of the channel: the registry never inspects what it stores, so
    /// a key added by a plugin the daemon has never heard of still round-trips.
    #[test]
    fn an_unknown_key_round_trips_unchanged() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "somebody");
        lua.load(r#"crucible.publish("weather", { sky = "blue", temp = 21 })"#)
            .exec()
            .unwrap();

        let answers = reg.get("weather");
        assert_eq!(answers[0].1["sky"], json!("blue"));
        assert_eq!(answers[0].1["temp"], json!(21));
    }

    #[test]
    fn two_plugins_answering_one_key_are_both_kept_and_attributed() {
        let reg = PublicationRegistry::new();
        lua_with_publish(reg.clone(), "oci")
            .load(r#"crucible.publish("isolation", { available = true })"#)
            .exec()
            .unwrap();
        lua_with_publish(reg.clone(), "firecracker")
            .load(r#"crucible.publish("isolation", { available = false })"#)
            .exec()
            .unwrap();

        let answers = reg.get("isolation");
        assert_eq!(answers.len(), 2, "one plugin's answer overwrote another's");
        assert_eq!(answers[0].0, "firecracker", "answers must be sorted");
        assert_eq!(answers[1].0, "oci");
    }

    #[test]
    fn republishing_replaces_that_plugins_previous_answer() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        lua.load(r#"crucible.publish("isolation", { available = false })"#)
            .exec()
            .unwrap();
        lua.load(r#"crucible.publish("isolation", { available = true })"#)
            .exec()
            .unwrap();

        let answers = reg.get("isolation");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].1["available"], json!(true));
    }

    /// A reloaded plugin's old answers described its old config.
    #[test]
    fn releasing_a_plugin_drops_only_its_own_publications() {
        let reg = PublicationRegistry::new();
        reg.set("oci", "isolation", json!({ "available": true }));
        reg.set("other", "isolation", json!({ "available": true }));
        reg.set("oci", "weather", json!("rain"));

        reg.release_plugin("oci");

        assert_eq!(reg.get("isolation").len(), 1);
        assert_eq!(reg.get("isolation")[0].0, "other");
        assert!(
            reg.get("weather").is_empty(),
            "an emptied key must not linger as an empty map"
        );
    }

    #[test]
    fn an_empty_key_is_refused_rather_than_stored_unfindable() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        assert!(lua
            .load(r#"crucible.publish("", { a = 1 })"#)
            .exec()
            .is_err());
    }

    #[test]
    fn a_value_that_cannot_be_json_is_refused_naming_the_key() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        let err = lua
            .load(r#"crucible.publish("isolation", function() end)"#)
            .exec()
            .unwrap_err()
            .to_string();
        assert!(err.contains("isolation"), "got: {err}");
    }
}
