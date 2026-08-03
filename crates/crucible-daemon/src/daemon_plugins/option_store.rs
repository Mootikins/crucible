//! Durable values for settings changed through `crucible.options`.
//!
//! `plugin.option_set` calls the plugin's own Lua setter, which writes wherever
//! that plugin keeps its state — in memory. Nothing outlived the daemon, so a
//! settings pane silently forgot every change on restart, which is worse than
//! having no pane: the user watches the value take effect and has no reason to
//! doubt it stuck.
//!
//! Values are replayed **through the plugin's own setter** at boot rather than
//! merged into its config section. Where a value lives is the plugin's business
//! — the options path is a path through the *settings tree*, and only the
//! plugin knows whether that matches its config layout. Replaying reproduces
//! the state by construction; guessing a config key would be right for `oci`
//! and wrong for the first plugin whose tree does not mirror its TOML.
//!
//! Deliberately a separate file from the user's `crucible.toml`: writing that
//! back would have to preserve comments and formatting to be acceptable, and
//! losing someone's config comments to a settings toggle is not a trade worth
//! making.

use crucible_lua::OptionsRegistry;
use std::path::{Path, PathBuf};

const FILE: &str = "plugin-options.json";

/// One stored value: the settings-tree path, and what it was set to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredOption {
    path: Vec<String>,
    value: serde_json::Value,
}

/// `plugin -> [ { path, value } ]`.
type Store = std::collections::BTreeMap<String, Vec<StoredOption>>;

fn file(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

fn load(dir: &Path) -> Store {
    std::fs::read_to_string(file(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remember `value` for `plugin`'s option at `path`, replacing any previous
/// value for the same path.
///
/// Best-effort: a failed write is logged, never propagated. The set itself
/// already succeeded, and turning "this will not survive a restart" into
/// "your change was rejected" would be a worse answer to a full disk.
pub fn record(dir: &Path, plugin: &str, path: &[String], value: serde_json::Value) {
    let mut store = load(dir);
    let entries = store.entry(plugin.to_string()).or_default();
    entries.retain(|e| e.path != path);
    entries.push(StoredOption {
        path: path.to_vec(),
        value,
    });

    let write = std::fs::create_dir_all(dir).and_then(|()| {
        let json = serde_json::to_string_pretty(&store).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(file(dir), json)
    });
    if let Err(e) = write {
        tracing::warn!(
            plugin = %plugin,
            path = %path.join("."),
            error = %e,
            "could not persist a plugin option; it applies until the daemon restarts"
        );
    }
}

/// Replay stored values through each plugin's own setter.
///
/// Runs after plugins load *and* after the user's `init.lua`, so an explicit
/// change in the settings pane outranks both the TOML the plugin was handed and
/// what init.lua set — it is the most recent thing the user actually did.
///
/// A value whose option no longer exists (the plugin dropped or renamed it) is
/// logged and skipped, not treated as an error: an upgrade must not fail to
/// boot because last month's setting no longer means anything.
pub fn restore(dir: &Path, registry: &OptionsRegistry) {
    for (plugin, entries) in load(dir) {
        replay(registry, &plugin, entries);
    }
}

/// Replay one plugin's stored values, for a reload.
///
/// A reload re-runs the plugin's `setup(cfg)` against the config it was
/// originally handed, so without this a value set in the settings pane survives
/// a daemon restart but not the Reload button sitting beside it.
pub fn restore_plugin(dir: &Path, registry: &OptionsRegistry, plugin: &str) {
    if let Some(entries) = load(dir).remove(plugin) {
        replay(registry, plugin, entries);
    }
}

fn replay(registry: &OptionsRegistry, plugin: &str, entries: Vec<StoredOption>) {
    for entry in entries {
        if let Err(e) = registry.set(plugin, &entry.path, entry.value.clone(), "restore") {
            tracing::warn!(
                plugin = %plugin,
                path = %entry.path.join("."),
                error = %e,
                "stored plugin option could not be restored; skipping it"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    /// A registry whose `oci` tree writes into a Lua table we can read back,
    /// mirroring how a real plugin keeps its config.
    fn registry() -> (Lua, OptionsRegistry) {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        let reg = OptionsRegistry::new();
        crucible_lua::register_options_module(&lua, &crucible, reg.clone(), "oci".to_string())
            .unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua.load(
            r#"
            state = { image = "alpine" }
            crucible.options{
              type = "group",
              get = function(info) return state[info.option] end,
              set = function(info, v) state[info.option] = v end,
              args = { image = { type = "input", name = "Image" } },
            }
            "#,
        )
        .exec()
        .unwrap();
        (lua, reg)
    }

    /// The point of the file: a value set through the pane is still set after
    /// the daemon restarts.
    #[test]
    fn a_recorded_value_is_replayed_through_the_plugins_own_setter() {
        let dir = tempfile::tempdir().unwrap();
        record(
            dir.path(),
            "oci",
            &["image".to_string()],
            serde_json::json!("debian"),
        );

        // A fresh registry, as after a restart: the plugin loaded with its
        // own default and has never heard of the stored value.
        let (lua, reg) = registry();
        assert_eq!(
            reg.get("oci", &["image".to_string()], "web").unwrap(),
            "alpine"
        );

        restore(dir.path(), &reg);

        assert_eq!(
            reg.get("oci", &["image".to_string()], "web").unwrap(),
            "debian"
        );
        // ...and it went where the PLUGIN keeps it, not into a shape this
        // module guessed at.
        let state: mlua::Table = lua.globals().get("state").unwrap();
        assert_eq!(state.get::<String>("image").unwrap(), "debian");
    }

    #[test]
    fn the_last_value_for_a_path_wins_rather_than_accumulating() {
        let dir = tempfile::tempdir().unwrap();
        let path = vec!["image".to_string()];
        record(dir.path(), "oci", &path, serde_json::json!("debian"));
        record(dir.path(), "oci", &path, serde_json::json!("fedora"));

        let (_lua, reg) = registry();
        restore(dir.path(), &reg);
        assert_eq!(reg.get("oci", &path, "web").unwrap(), "fedora");
        assert_eq!(load(dir.path())["oci"].len(), 1);
    }

    /// An option that no longer exists must not stop the daemon booting, and
    /// must not stop the options beside it from being restored.
    #[test]
    fn a_stored_option_the_plugin_dropped_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        record(
            dir.path(),
            "oci",
            &["removed_last_year".to_string()],
            serde_json::json!(true),
        );
        record(
            dir.path(),
            "oci",
            &["image".to_string()],
            serde_json::json!("debian"),
        );

        let (_lua, reg) = registry();
        restore(dir.path(), &reg);
        assert_eq!(
            reg.get("oci", &["image".to_string()], "web").unwrap(),
            "debian",
            "a stale entry must not take the entries beside it down"
        );
    }

    #[test]
    fn no_store_file_restores_nothing_and_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (_lua, reg) = registry();
        restore(dir.path(), &reg);
        assert_eq!(
            reg.get("oci", &["image".to_string()], "web").unwrap(),
            "alpine"
        );
    }
}

/// The store is only as good as the paths it is reached through: a reload that
/// skips the replay, or a write that lands outside the daemon's data root, both
/// look like the feature works until the user reloads or a test pollutes a real
/// home.
#[cfg(test)]
mod wiring_tests {
    use super::*;
    use crate::daemon_plugins::DaemonPluginLoader;
    use crate::protocol::{Request, RequestId};
    use crucible_lua::PluginSource;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    const PLUGIN: &str = "optplug";

    /// A plugin whose `image` option is re-initialized every time its
    /// `init.lua` runs — exactly what a reload does to a plugin that takes its
    /// defaults from the TOML it was handed.
    fn write_option_plugin(root: &Path) {
        let dir = root.join(PLUGIN);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.yaml"),
            format!("name: {PLUGIN}\nversion: \"0.1.0\"\nmain: init.lua\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join("init.lua"),
            r#"
            _G.optplug_state = { image = "alpine" }
            crucible.options{
              type = "group",
              get = function(info) return _G.optplug_state[info.option] end,
              set = function(info, v) _G.optplug_state[info.option] = v end,
              args = { image = { type = "input", name = "Image" } },
            }
            return { name = "optplug" }
            "#,
        )
        .unwrap();
    }

    async fn loader_with_store(plugins: &Path, store: &Path) -> DaemonPluginLoader {
        let mut loader = DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("loader")
            .with_option_store(store.to_path_buf());
        loader
            .load_plugins(&[(plugins.to_path_buf(), PluginSource::Runtime)])
            .await
            .expect("load plugins");
        loader
    }

    fn image() -> Vec<String> {
        vec!["image".to_string()]
    }

    /// Reload sits beside the settings in the plugin panel: pressing it must
    /// not undo the setting next to it.
    #[tokio::test]
    async fn a_reload_replays_stored_values_over_the_plugins_defaults() {
        let plugins = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        write_option_plugin(plugins.path());

        let mut loader = loader_with_store(plugins.path(), store.path()).await;

        // What the settings pane does: set through the plugin, then persist.
        loader
            .options()
            .set(PLUGIN, &image(), serde_json::json!("debian"), "web")
            .expect("set");
        record(store.path(), PLUGIN, &image(), serde_json::json!("debian"));

        loader.reload_plugin(PLUGIN).await.expect("reload");

        assert_eq!(
            loader.options().get(PLUGIN, &image(), "web").unwrap(),
            "debian",
            "a reload re-ran the plugin's defaults and dropped the stored value"
        );
    }

    /// The daemon's resolved data root, not `crucible_home()` — otherwise an
    /// in-process test with an injected root writes into the developer's real
    /// `~/.crucible`.
    #[tokio::test]
    async fn a_value_set_over_rpc_is_persisted_under_the_bound_data_root() {
        let plugins = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        write_option_plugin(plugins.path());

        let loader = Arc::new(Mutex::new(Some(
            loader_with_store(plugins.path(), store.path()).await,
        )));

        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "plugin.option_set".to_string(),
            params: serde_json::json!({
                "plugin": PLUGIN,
                "path": ["image"],
                "value": "debian",
            }),
        };
        let resp = crate::server::plugins::handle_plugin_option_call(
            req,
            &loader,
            crate::server::plugins::OptionAction::Set,
        )
        .await;
        assert!(resp.error.is_none(), "option_set failed: {:?}", resp.error);

        let stored = load(store.path());
        let entries = stored.get(PLUGIN).unwrap_or_else(|| {
            panic!(
                "nothing persisted under the bound data root {:?}",
                store.path()
            )
        });
        assert_eq!(entries[0].value, serde_json::json!("debian"));
    }
}
