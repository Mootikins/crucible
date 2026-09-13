//! The plugin context: who is running, and what authority they hold.
//!
//! Both markers used to be ordinary writable Lua globals. `cru._current_plugin`
//! scoped every `cru.storage` call and was never stamped in THIS VM at all, so
//! daemon-side storage errored unless a plugin forged the global — and a forged
//! one reached any plugin's namespace. They ride one Rust-side context now.
use super::*;
use crate::storage::sqlite::{SqliteConfig, SqliteNoteStore, SqlitePool};
use crucible_core::storage::PropertyStore;

/// Write the `init.lua` for one plugin under `tmp`, which is the search path
/// the loader discovers it in.
fn plugin_dir(tmp: &std::path::Path, name: &str, body: &str) {
    let dir = tmp.join(name);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(
        dir.join("init.lua"),
        format!("{body}\nreturn {{ name = \"{name}\", version = \"0.1.0\" }}\n"),
    )
    .expect("init.lua");
}

fn property_store() -> Arc<SqliteNoteStore> {
    let pool = SqlitePool::new(SqliteConfig::memory()).expect("pool");
    Arc::new(SqliteNoteStore::new(pool))
}

/// A daemon-loaded plugin can use `cru.storage` at all.
///
/// The namespace came from `cru._current_plugin`, which only the standalone
/// `PluginManager` VM ever stamped. In the daemon's VM — the one that actually
/// runs plugins — the global was never set, so every `cru.storage` call from a
/// shipped plugin raised "requires a plugin context".
#[tokio::test]
async fn a_daemon_loaded_plugin_can_use_storage() {
    let tmp = tempfile::TempDir::new().unwrap();
    plugin_dir(
        tmp.path(),
        "keeper",
        r#"cru.storage.set("e1", "k", "kept")"#,
    );

    let store = property_store();
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .upgrade_with_property_store(Arc::clone(&store) as Arc<dyn PropertyStore>)
        .expect("storage upgrade");
    loader
        .activate_discovered(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");

    assert_eq!(
        store
            .property_get("e1", "plugin:keeper", "k")
            .await
            .expect("read"),
        Some("kept".to_string()),
        "a daemon-loaded plugin's storage write did not reach its namespace"
    );
}

/// One plugin cannot write into another plugin's storage namespace.
///
/// Assert the namespace, not an error: assigning `cru._current_plugin` is
/// inert now, so the write lands where it belongs and nothing raises.
#[tokio::test]
async fn a_daemon_loaded_plugin_cannot_forge_another_plugins_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    plugin_dir(
        tmp.path(),
        "alpha",
        r#"
        cru._current_plugin = "beta"
        cru.storage.set("e1", "k", "written-by-alpha")
        "#,
    );
    plugin_dir(tmp.path(), "beta", "");

    let store = property_store();
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .upgrade_with_property_store(Arc::clone(&store) as Arc<dyn PropertyStore>)
        .expect("storage upgrade");
    loader
        .activate_discovered(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");

    assert_eq!(
        store
            .property_get("e1", "plugin:alpha", "k")
            .await
            .expect("read alpha"),
        Some("written-by-alpha".to_string()),
        "the write must land in the running plugin's namespace"
    );
    assert_eq!(
        store
            .property_get("e1", "plugin:beta", "k")
            .await
            .expect("read beta"),
        None,
        "a forged `cru._current_plugin` reached another plugin's storage"
    );
}
