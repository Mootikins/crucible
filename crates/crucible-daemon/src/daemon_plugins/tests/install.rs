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
        loader
            .plugin_registry()
            .tool_names()
            .contains("fresh_probe"),
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
    let removed = plugin_ops::remove_at("fresh", true, &toml_path, &plugins_dir).expect("remove");
    assert_eq!(removed.purged_dir, Some(plugins_dir.join("fresh")));

    assert!(
        !loader
            .plugin_registry()
            .tool_names()
            .contains("fresh_probe"),
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

/// A plugin can be declared in plugins.toml yet unknown to the running
/// daemon — its clone was deleted by hand, or bootstrap failed at boot (no
/// network, repo gone). Removal must still work: `handle_plugin_remove`'s
/// declared-precondition already guards against typos, and "nothing to
/// deactivate" is not a refusal. Erroring on `unload`'s NotFound left the
/// stale declaration permanently unremovable while the daemon ran.
#[tokio::test]
async fn removing_a_declared_plugin_the_daemon_never_discovered_still_works() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .deactivate_and_forget_plugin("ghost")
        .await
        .expect("a plugin the daemon never discovered has nothing to deactivate");
}

/// `plugin.install` of a plugin that is ALREADY Active — manually cloned
/// into the user plugins dir and loaded at boot, now being declared in
/// plugins.toml — must report loaded, not failure. `load_all` skips Active
/// plugins (`AlreadyLoaded`), so the activation pass's return value does not
/// contain them; judging by that value alone reported `loaded: false` with a
/// fabricated error for a healthy plugin and failed `cru plugin add`'s exit
/// code.
#[tokio::test]
async fn installing_an_already_active_plugin_reports_loaded_not_failure() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugins_dir = tmp.path().to_path_buf();
    write_plugin(&plugins_dir, "veteran");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    // Boot: the manually cloned plugin loads and is Active.
    loader
        .load_plugins(&[(plugins_dir.clone(), PluginSource::User)])
        .await
        .expect("boot load");

    // The install flow's activation pass over the same dir.
    loader
        .load_plugins(&[(plugins_dir, PluginSource::User)])
        .await
        .expect("activation load");

    let report = crate::server::plugins::install_load_report(&loader, "veteran");
    assert!(
        report.loaded,
        "an already-Active plugin is loaded, not broken: {:?}",
        report.error
    );
    assert_eq!(report.tools, 1, "counts come from the loader's state");
    assert!(report.error.is_none(), "got: {:?}", report.error);
}

/// The inverse cases: a plugin whose execution failed reports its
/// `last_error`, and a plugin the loader never saw reports a pointer at
/// `plugin.list` — neither claims `loaded`.
#[tokio::test]
async fn install_load_report_surfaces_failure_and_absence() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("brokentool");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("init.lua"), r#"error("no api for you")"#).unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::User)])
        .await
        .expect("load_plugins is fail-open per plugin");

    let report = crate::server::plugins::install_load_report(&loader, "brokentool");
    assert!(!report.loaded);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|e| e.contains("no api for you")),
        "the plugin's own error must surface: {:?}",
        report.error
    );

    let report = crate::server::plugins::install_load_report(&loader, "neverseen");
    assert!(!report.loaded);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|e| e.contains("plugin.list")),
        "absence points at the diagnostic surface: {:?}",
        report.error
    );
}
