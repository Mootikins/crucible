//! Runtime install/remove: the `plugin_ops` `_at` cores composed with loader
//! activation, exactly as `handle_plugin_install`/`handle_plugin_remove`
//! sequence them — hermetic (injected temp paths, never
//! `daemon_plugin_paths()`, no git, no network).
use super::super::*;
use crate::plugin_ops;

fn write_plugin(plugins_dir: &std::path::Path, name: &str) {
    let dir = plugins_dir.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.yaml"),
        format!("name: {name}\nversion: \"0.1.0\"\nmain: init.lua\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("init.lua"),
        format!(
            r#"return {{
                name = "{name}",
                version = "0.1.0",
                tools = {{ {name}_probe = {{ description = "x", fn = function() return "t" end }} }},
            }}"#
        ),
    )
    .unwrap();
}

/// The full install → activate → remove round trip that the RPC handlers
/// perform: after install + a second `load_plugins` pass the plugin's tools
/// are registered and it is listed; after the remove flow nothing remains in
/// the loader and the TOML declaration is gone.
#[tokio::test]
async fn install_then_remove_acts_on_the_running_loader_and_the_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toml_path = tmp.path().join("plugins.toml");
    let plugins_dir = tmp.path().join("plugins");
    // Pre-created clone: `bootstrap_plugin_entry` short-circuits to
    // AlreadyPresent, so the test exercises name derivation, bootstrap
    // outcome, and the TOML write without relaxing the git-URL allowlist.
    write_plugin(&plugins_dir, "fresh");

    // Step 1 of handle_plugin_install: clone + declare.
    let installed = plugin_ops::install_at(
        crucible_core::config::PluginEntry {
            url: "user/fresh".to_string(),
            branch: None,
            pin: None,
            enabled: true,
        },
        &toml_path,
        &plugins_dir,
    )
    .await
    .expect("install");
    assert_eq!(installed.name, "fresh");
    assert!(matches!(
        installed.outcome,
        crate::BootstrapOutcome::AlreadyPresent
    ));

    // Step 2: activate on the running loader.
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(plugins_dir.clone(), PluginSource::User)])
        .await
        .expect("activation load");
    assert!(
        loader.plugin_registry().tool_names().contains("fresh_probe"),
        "an installed plugin's tools must be registered without restart"
    );
    let info = loader.loaded_plugin_info();
    let entry = info
        .iter()
        .find(|p| p["name"] == "fresh")
        .expect("installed plugin listed");
    assert_eq!(entry["state"], "Active", "got: {entry}");

    // The remove flow, in handler order: declared-precondition, deactivate +
    // forget, then the TOML commit (purge only after success).
    assert!(
        plugin_ops::declared_at(&toml_path, "fresh").expect("declared check"),
        "an installed plugin is declared"
    );
    loader
        .deactivate_and_forget_plugin("fresh")
        .await
        .expect("deactivate");
    let removed =
        plugin_ops::remove_at("fresh", true, &toml_path, &plugins_dir).expect("remove");
    assert_eq!(removed.purged_dir, Some(plugins_dir.join("fresh")));

    assert!(
        !loader.plugin_registry().tool_names().contains("fresh_probe"),
        "a removed plugin's tools must be unregistered"
    );
    assert!(
        !loader
            .loaded_plugin_info()
            .iter()
            .any(|p| p["name"] == "fresh"),
        "a removed plugin must leave plugin.list"
    );
    assert!(
        !plugin_ops::declared_at(&toml_path, "fresh").expect("declared check"),
        "the TOML declaration must be gone"
    );
}
