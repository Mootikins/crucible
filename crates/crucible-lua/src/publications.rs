//! Data a plugin publishes for clients to render.
//!
//! `cru.plugin.publish("<key>", value)` is the generic contribution channel: a
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

use crate::host_hook::HostHook;
use mlua::{Lua, LuaSerdeExt, Result as LuaResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Told, after a publication lands, which plugin published under which key.
///
/// Named rather than written inline at each use: the same signature appears on
/// the field, the setter and the daemon's closure, and three copies of a
/// four-part `dyn` bound is what `clippy::type_complexity` is for.
pub type PublicationChangeHook = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Publications by key, then by publishing plugin.
///
/// Keyed that way round because clients ask by key ("who offers isolation?"),
/// and more than one plugin may answer. Attribution is kept so a client can
/// tell two answers apart and a stale entry can be traced home.
#[derive(Clone, Default)]
pub struct PublicationRegistry {
    entries: Arc<Mutex<HashMap<String, HashMap<String, serde_json::Value>>>>,
    /// Called after a publication lands, with `(plugin, key)`.
    ///
    /// A `dyn` here on purpose, and it is the crate-dependency-firewall case:
    /// notifying clients means emitting a daemon event, and this crate must
    /// not know what a `SessionEventMessage` is. The daemon installs the
    /// closure at boot; a registry with none set simply stores, which is what
    /// every test and the `cru plugin check` path want.
    ///
    /// Install-once, and shared across clones. See [`HostHook`].
    on_change: HostHook<PublicationChangeHook>,
}

impl std::fmt::Debug for PublicationRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.entries.lock().map(|g| g.len()).unwrap_or(0);
        write!(f, "PublicationRegistry({n} keys)")
    }
}

impl PublicationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the change hook, once. The daemon calls this at boot. Answers
    /// `false` when one is already installed, which is a double boot rather
    /// than something to paper over.
    #[must_use]
    pub fn set_change_hook(&self, hook: PublicationChangeHook) -> bool {
        self.on_change.install(hook)
    }

    /// Store a publication and tell anyone listening.
    ///
    /// The hook runs **after** the lock is released. Holding the entries mutex
    /// across a callback that reaches the daemon's broadcast channel is how a
    /// publish from inside an event handler would deadlock against itself.
    pub fn set(&self, plugin: &str, key: &str, value: serde_json::Value) {
        if let Ok(mut g) = self.entries.lock() {
            g.entry(key.to_string())
                .or_default()
                .insert(plugin.to_string(), value);
        }
        if let Some(hook) = self.on_change.get() {
            hook(plugin, key);
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

/// Register `cru.plugin.publish`.
///
/// ```lua
/// cru.plugin.publish("isolation", {
///   available = true,
///   profiles  = { "rust", "throwaway" },
/// })
/// ```
///
/// `plugin` is supplied by the loader rather than the caller: a plugin naming
/// someone else as the author of its data would make attribution worthless.
///
/// ## Attribution is read at CALL time, not at bind time
///
/// The bound `plugin` is only a fallback. Every plugin shares one `cru` table,
/// so the closure captured here is replaced by the next plugin's registration
/// — and a publish that happens LATER than load (from a command, a hook, a
/// timer) would then be filed under whichever plugin was bound last. That is
/// not hypothetical: the kanban board published itself as `web-search`.
///
/// [`crate::plugin_context`] already exists for exactly this, and says so:
/// "a per-plugin rebind of the shared `cru.storage` table cannot do this work
/// … the last rebind would win for every late caller." `cru.storage` reads the
/// context; this now does too. The captured name still covers the one case the
/// context cannot: a publish from a plugin's own body during boot `require`,
/// before the loader has entered its context.
pub fn register_publish_module(
    lua: &Lua,
    registry: PublicationRegistry,
    plugin: String,
) -> LuaResult<()> {
    let publish = lua.create_function(move |lua, (key, value): (String, mlua::Value)| {
        if key.is_empty() {
            return Err(mlua::Error::runtime(
                "cru.plugin.publish: a key is required, e.g. cru.plugin.publish(\"isolation\", {…})",
            ));
        }
        let json: serde_json::Value = lua.from_value(value).map_err(|e| {
            mlua::Error::runtime(format!(
                "cru.plugin.publish: '{key}' must be JSON-encodable data, not a function or \
                 userdata: {e}"
            ))
        })?;
        let author =
            crate::plugin_context::current_plugin_name(lua).unwrap_or_else(|| plugin.clone());
        registry.set(&author, &key, json);
        Ok(())
    })?;
    crate::lua_util::get_or_create_module(lua, "plugin")?.set("publish", publish)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A publish that happens after load is filed under the plugin that is
    /// RUNNING, not the one that was bound last.
    ///
    /// Break `register_publish_module` back to the captured name and this
    /// fails with `web-search` — which is what production did: the kanban
    /// board's command published itself under the last plugin the loader
    /// happened to bind.
    #[test]
    fn a_late_publish_is_attributed_to_the_running_plugin() {
        let lua = Lua::new();
        let registry = PublicationRegistry::new();

        // Two plugins bind in turn, as the loader does. `web-search` wins the
        // shared `cru.plugin.publish` slot by going second.
        register_publish_module(&lua, registry.clone(), "kanban".to_string()).unwrap();
        register_publish_module(&lua, registry.clone(), "web-search".to_string()).unwrap();

        // Now kanban's command runs, under kanban's context.
        let restore = crate::plugin_context::enter_plugin(&lua, "kanban", false);
        lua.load(r#"cru.plugin.publish("kanban:board", { tickets = {} })"#)
            .exec()
            .unwrap();
        crate::plugin_context::set_owner(&lua, restore);

        let answers = registry.get("kanban:board");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].0, "kanban", "attribution follows execution");
    }

    /// With no context — a boot `require` running a plugin's body before the
    /// loader enters its context — the bound name is still right.
    #[test]
    fn a_publish_with_no_context_falls_back_to_the_bound_plugin() {
        let lua = Lua::new();
        let registry = PublicationRegistry::new();
        register_publish_module(&lua, registry.clone(), "oci".to_string()).unwrap();

        lua.load(r#"cru.plugin.publish("isolation", { available = true })"#)
            .exec()
            .unwrap();

        assert_eq!(registry.get("isolation")[0].0, "oci");
    }

    /// The hook is installed once at boot. A second install used to replace the
    /// first in silence, so a double boot looked exactly like a working one.
    #[test]
    fn the_change_hook_installs_once() {
        use std::sync::{Arc, Mutex};
        let registry = PublicationRegistry::new();
        let hits: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

        let sink = Arc::clone(&hits);
        assert!(registry.set_change_hook(Arc::new(move |_: &str, _: &str| {
            *sink.lock().unwrap() += 1;
        })));
        assert!(
            !registry.set_change_hook(Arc::new(|_: &str, _: &str| {
                panic!("the second hook must never fire")
            })),
            "the second install is refused"
        );

        registry.set("kanban", "kanban:board", json!({}));
        assert_eq!(*hits.lock().unwrap(), 1, "the first hook survives");
    }

    /// The hook fires with the plugin and key, after the value has landed.
    #[test]
    fn a_publication_notifies_after_it_is_stored() {
        use std::sync::{Arc, Mutex};
        let registry = PublicationRegistry::new();
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));

        let sink = seen.clone();
        let observed = registry.clone();
        assert!(
            registry.set_change_hook(Arc::new(move |plugin: &str, key: &str| {
                // Reading inside the hook proves the value is stored BEFORE the
                // notification: a client told to re-read must not race the write.
                assert_eq!(observed.get(key).len(), 1);
                sink.lock()
                    .unwrap()
                    .push((plugin.to_string(), key.to_string()));
            }))
        );

        registry.set("kanban", "kanban:board", json!({ "tickets": [] }));
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[("kanban".to_string(), "kanban:board".to_string())]
        );
    }

    fn lua_with_publish(registry: PublicationRegistry, plugin: &str) -> Lua {
        let lua = Lua::new();
        register_publish_module(&lua, registry, plugin.to_string()).unwrap();
        lua
    }

    #[test]
    fn a_plugin_publishes_a_value_clients_can_read_back() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        lua.load(r#"cru.plugin.publish("isolation", { available = true, profiles = { "rust" } })"#)
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
        lua.load(r#"cru.plugin.publish("weather", { sky = "blue", temp = 21 })"#)
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
            .load(r#"cru.plugin.publish("isolation", { available = true })"#)
            .exec()
            .unwrap();
        lua_with_publish(reg.clone(), "firecracker")
            .load(r#"cru.plugin.publish("isolation", { available = false })"#)
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
        lua.load(r#"cru.plugin.publish("isolation", { available = false })"#)
            .exec()
            .unwrap();
        lua.load(r#"cru.plugin.publish("isolation", { available = true })"#)
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
            .load(r#"cru.plugin.publish("", { a = 1 })"#)
            .exec()
            .is_err());
    }

    #[test]
    fn a_value_that_cannot_be_json_is_refused_naming_the_key() {
        let reg = PublicationRegistry::new();
        let lua = lua_with_publish(reg.clone(), "oci");
        let err = lua
            .load(r#"cru.plugin.publish("isolation", function() end)"#)
            .exec()
            .unwrap_err()
            .to_string();
        assert!(err.contains("isolation"), "got: {err}");
    }
}
