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
        for entry in entries {
            if let Err(e) = registry.set(&plugin, &entry.path, entry.value.clone(), "restore") {
                tracing::warn!(
                    plugin = %plugin,
                    path = %entry.path.join("."),
                    error = %e,
                    "stored plugin option could not be restored; skipping it"
                );
            }
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
