//! What must be true of every plugin in `runtime/plugins/`.
//!
//! The bundled set is the one thing here that is read off disk rather
//! than listed, so a plugin added to the tree joins these assertions
//! without anyone remembering to add it.
use super::super::*;

/// Directory holding the plugins that ship with the repo.
fn shipped_plugins_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/plugins"
    ))
}

/// Names of every shipped plugin directory, read from disk so a newly
/// added plugin is covered without editing this list.
fn shipped_plugin_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(shipped_plugins_dir())
        .expect("runtime/plugins must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

/// Discovery alone proves nothing about a plugin's health — `oci` was
/// discovered `Active` for months while dying on its first `require`,
/// because `execute_plugin` errors were downgraded to a `warn!` on a
/// stdout auto-spawn points at /dev/null. This is the Phase-6 smoke:
/// every shipped plugin must load through the REAL loader and *execute* —
/// state `Active`, no `last_error`, and a spec extracted (proof its
/// `init.lua` ran to completion and returned its table).
#[tokio::test]
async fn every_shipped_plugin_executes() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    let info = loader.loaded_plugin_info();
    for name in shipped_plugin_names() {
        let entry = info
            .iter()
            .find(|p| p["name"].as_str() == Some(name.as_str()))
            .unwrap_or_else(|| panic!("shipped plugin '{name}' missing from plugin info"));

        assert_eq!(
            entry["state"].as_str(),
            Some("Active"),
            "shipped plugin '{name}' did not reach Active: {entry:#}"
        );
        let last_error = entry["last_error"].as_str().unwrap_or("");
        assert!(
            last_error.is_empty(),
            "shipped plugin '{name}' recorded an error: {last_error}"
        );
    }
}

/// The kill switch: `[plugins.<name>] enabled = false` in config.toml must
/// keep a bundled plugin from ever executing.
///
/// Editing the extracted `plugin.yaml` does not work — the runtime tree is
/// re-stamped from the binary whenever `version + blake3(tree)` changes, which
/// silently restores `enabled: true`. Config is the only durable lever, and
/// `oci` is the plugin that most needs it (it shells out to a container
/// runtime). Paired with `every_shipped_plugin_executes` above, which proves
/// `oci` DOES load when config says nothing.
#[tokio::test]
async fn a_plugin_disabled_in_config_never_executes() {
    let plugin_config = HashMap::from([(
        "oci".to_string(),
        serde_json::json!({ "enabled": false, "runtime": "podman" }),
    )]);
    let mut loader = DaemonPluginLoader::new(plugin_config).expect("loader");
    loader
        .load_plugins(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    assert!(
        !loader.loaded_plugin_names().contains(&"oci".to_string()),
        "oci is disabled in config but loaded anyway: {:?}",
        loader.loaded_plugin_names()
    );
    // Other bundled plugins are untouched — the switch is per-plugin.
    assert!(
        loader
            .loaded_plugin_names()
            .contains(&"reflection".to_string()),
        "disabling oci must not disable anything else: {:?}",
        loader.loaded_plugin_names()
    );
}

/// A shipped plugin whose manifest doesn't parse is not merely broken —
/// it never enters `PluginManager::plugins` at all, so it is absent from
/// `plugin.list` with no error anywhere but the daemon log.
///
/// `reflection` shipped that way: `plugin.yaml` declared the capabilities
/// `session` and `fs`, neither a `Capability` variant.
#[test]
fn every_shipped_plugin_is_discovered() {
    let mut manager = PluginManager::new();
    manager.add_search_path_with_source(shipped_plugins_dir(), PluginSource::Runtime);

    let mut discovered = manager.discover().expect("discovery");
    discovered.sort();

    assert_eq!(
        discovered,
        shipped_plugin_names(),
        "a shipped plugin failed discovery — it will be invisible in `plugin.list`"
    );
}

/// Every shipped manifest carries the same block of identifying fields.
///
/// `oci` was eight lines with no `author`, no `license` and no `main` while the
/// other six carried all three — an arbitrary difference nobody would notice
/// until they were generating an index of what ships and half the rows were
/// blank. One shape, asserted, so it stays one shape.
#[test]
fn every_shipped_manifest_declares_the_same_identifying_fields() {
    const REQUIRED: &[&str] = &[
        "name",
        "version",
        "description",
        "author",
        "license",
        "main",
    ];

    let mut missing: Vec<String> = Vec::new();
    for name in shipped_plugin_names() {
        let manifest_path = shipped_plugins_dir().join(&name).join("plugin.yaml");
        let body = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
        let manifest: serde_yaml::Value = serde_yaml::from_str(&body)
            .unwrap_or_else(|e| panic!("{name}/plugin.yaml does not parse: {e}"));

        for field in REQUIRED {
            if manifest.get(field).is_none() {
                missing.push(format!("{name}: {field}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "shipped manifests are missing identifying fields: {missing:#?}"
    );
}

/// A Fennel plugin executes in the daemon, not merely in discovery.
///
/// It never had. `crucible-lua`'s discovery pass compiles a `.fnl` main before
/// running it in its throwaway sandbox VM, so a Fennel plugin was discovered,
/// its spec extracted, and its tools counted — and then `execute_plugin` in
/// the real VM read the same file and handed the raw Fennel to `lua.load`,
/// dying on the `;;;` header with "syntax error near '-'". Nothing caught it
/// because the repo's only Fennel plugin lived under `docs/plugins/` and was
/// exercised only by `PluginManager`, which is the half that worked.
#[tokio::test]
async fn a_fennel_plugin_executes_in_the_daemon_vm() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let plugin_dir = tmp.path().join("fennel-smoke");
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: fennel-smoke\nversion: \"0.1.0\"\ndescription: smoke\nmain: init.fnl\n",
    )
    .expect("write manifest");
    // The `;;;` comment matters: it is what a raw-Lua load chokes on.
    std::fs::write(
        plugin_dir.join("init.fnl"),
        ";;; fennel-smoke — a Fennel main\n\
         (fn ping [_args] {:pong true})\n\
         {:name \"fennel-smoke\"\n\
          :version \"0.1.0\"\n\
          :tools {:ping {:desc \"ping\" :params [] :fn ping}}}\n",
    )
    .expect("write init.fnl");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load fennel plugin");

    let info = loader.loaded_plugin_info();
    let entry = info
        .iter()
        .find(|p| p["name"].as_str() == Some("fennel-smoke"))
        .unwrap_or_else(|| panic!("fennel plugin missing from plugin info: {info:#?}"));

    assert_eq!(
        entry["state"].as_str(),
        Some("Active"),
        "a Fennel plugin must execute, not just be discovered: {entry:#}"
    );
    assert_eq!(entry["last_error"].as_str().unwrap_or(""), "");
    assert_eq!(
        entry["tools"].as_u64(),
        Some(1),
        "its spec table must come back from the real VM: {entry:#}"
    );
}
