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
/// the loader and the manifest record is gone.
#[tokio::test]
async fn install_then_remove_acts_on_the_running_loader_and_the_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let manifest_path = tmp.path().join(plugin_ops::INSTALLED_PLUGINS_FILE);
    let plugins_dir = tmp.path().join("plugins");
    // Pre-created clone: `bootstrap_plugin_entry` short-circuits to
    // AlreadyPresent, so the test exercises name derivation, bootstrap
    // outcome, and the manifest write without relaxing the git-URL allowlist.
    write_plugin(&plugins_dir, "fresh");

    // Step 1 of handle_plugin_install: clone + record.
    let installed = plugin_ops::install_at(
        crucible_core::config::PluginEntry {
            url: "user/fresh".to_string(),
            branch: None,
            pin: None,
            enabled: true,
        },
        &manifest_path,
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

    // The remove flow, in handler order: installed-precondition, deactivate +
    // forget, then the manifest commit (purge only after success).
    assert!(
        plugin_ops::installed_at(&manifest_path, "fresh").expect("installed check"),
        "an installed plugin is recorded"
    );
    loader
        .deactivate_and_forget_plugin("fresh")
        .await
        .expect("deactivate");
    let removed =
        plugin_ops::remove_at("fresh", true, &manifest_path, &plugins_dir).expect("remove");
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
        !plugin_ops::installed_at(&manifest_path, "fresh").expect("installed check"),
        "the manifest record must be gone"
    );
}

/// A plugin can be recorded in the manifest yet unknown to the running
/// daemon — its clone was deleted by hand, or bootstrap failed at boot (no
/// network, repo gone). Removal must still work: `handle_plugin_remove`'s
/// installed-precondition already guards against typos, and "nothing to
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
/// into the user plugins dir and loaded at boot, now being recorded in the
/// manifest — must report loaded, not failure. `load_all` skips Active
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

    let report = crate::server::plugin_install::install_load_report(
        &loader,
        "veteran",
        &tmp.path().join("veteran"),
    );
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

    let report = crate::server::plugin_install::install_load_report(&loader, "brokentool", &dir);
    assert!(!report.loaded);
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|e| e.contains("no api for you")),
        "the plugin's own error must surface: {:?}",
        report.error
    );

    let report = crate::server::plugin_install::install_load_report(
        &loader,
        "neverseen",
        &tmp.path().join("neverseen"),
    );
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

/// A repo named `crucible-greeter` whose spec table says `name = "greeter"`
/// is a thoroughly conventional layout, and it puts two naming authorities in
/// play: the installed manifest and the clone dir go by the URL name, the
/// plugin manager by the declared name. Resolution must go through the
/// clone DIRECTORY, or install reports a healthy plugin as broken and remove
/// silently no-ops (unload's NotFound swallowed, manifest record gone,
/// plugin still running and now unremovable).
#[tokio::test]
async fn a_declared_name_differing_from_the_repo_name_still_installs_and_removes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let manifest_path = tmp.path().join(plugin_ops::INSTALLED_PLUGINS_FILE);
    let plugins_dir = tmp.path().join("plugins");
    let clone_dir = plugins_dir.join("crucible-greeter");
    std::fs::create_dir_all(&clone_dir).unwrap();
    std::fs::write(
        clone_dir.join("init.lua"),
        r#"return {
            name = "greeter",
            version = "0.1.0",
            tools = { greeter_probe = { description = "x", fn = function() return "t" end } },
        }"#,
    )
    .unwrap();

    plugin_ops::install_at(
        crucible_core::config::PluginEntry {
            url: "user/crucible-greeter".to_string(),
            branch: None,
            pin: None,
            enabled: true,
        },
        &manifest_path,
        &plugins_dir,
    )
    .await
    .expect("install");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(plugins_dir.clone(), PluginSource::User)])
        .await
        .expect("activation load");

    // Install must report the plugin loaded, resolving through the dir.
    let report =
        crate::server::plugin_install::install_load_report(&loader, "crucible-greeter", &clone_dir);
    assert!(
        report.loaded,
        "a healthy plugin must not be reported broken because the name it \
         declares differs from its directory: {:?}",
        report.error
    );

    // Remove must reach the actual plugin, resolving through the dir.
    let resolved = loader
        .plugin_name_for_dir(&clone_dir)
        .expect("the clone dir maps to the manager key");
    // Identity is the DIRECTORY name: the only name knowable without running
    // Lua, and therefore the only one discovery can key on. The name the
    // plugin declares is honoured for `[plugins.<name>]` lookup instead — see
    // `PluginManifest::declared_name`.
    assert_eq!(resolved, "crucible-greeter");
    loader
        .deactivate_and_forget_plugin(&resolved)
        .await
        .expect("deactivate");
    assert!(
        !loader
            .plugin_registry()
            .tool_names()
            .contains("greeter_probe"),
        "remove must deactivate the ACTUAL plugin, not no-op on the URL name"
    );
}

/// The bootstrap union: a declared plugin bootstraps with nothing in the
/// manifest, an installed one with nothing declared, and on a name shared
/// by both the DECLARATION wins — with the shadowed name reported, never
/// applied silently.
#[test]
fn the_bootstrap_union_prefers_the_declaration_and_names_the_shadow() {
    let entry = |url: &str, pin: Option<&str>| crucible_core::config::PluginEntry {
        url: url.to_string(),
        branch: None,
        pin: pin.map(str::to_string),
        enabled: true,
    };

    // Declared alone: bootstraps with an empty manifest.
    let (entries, shadows) = crate::daemon_plugins::union_plugin_entries(
        vec![("greeter".into(), entry("user/greeter", None))],
        Vec::new(),
    );
    assert_eq!(entries.len(), 1);
    assert!(shadows.is_empty());

    // Installed alone: bootstraps with nothing declared.
    let (entries, shadows) = crate::daemon_plugins::union_plugin_entries(
        Vec::new(),
        vec![("tool".into(), entry("other/tool", None))],
    );
    assert_eq!(entries.len(), 1);
    assert!(shadows.is_empty());

    // Both name the same plugin: the declaration's entry survives, the
    // installed one is shadowed BY NAME in the returned report.
    let (entries, shadows) = crate::daemon_plugins::union_plugin_entries(
        vec![("greeter".into(), entry("user/greeter", Some("v2")))],
        vec![
            ("greeter".into(), entry("user/greeter", Some("v1"))),
            ("tool".into(), entry("other/tool", None)),
        ],
    );
    assert_eq!(entries.len(), 2);
    let greeter = entries.iter().find(|e| e.url == "user/greeter").unwrap();
    assert_eq!(
        greeter.pin.as_deref(),
        Some("v2"),
        "the declared entry must win the union"
    );
    assert_eq!(shadows, vec!["greeter".to_string()]);
}

/// `plugins.declare` is a reserved subkey holding declarations; the option
/// splitter must never hand it to a plugin's `setup(cfg)`.
#[test]
fn split_plugins_config_never_hands_declarations_to_setup() {
    let raw = std::collections::BTreeMap::from([
        (
            crucible_core::config::PLUGINS_DECLARE_KEY.to_string(),
            serde_json::json!({ "greeter": "user/greeter" }),
        ),
        (
            "reflection".to_string(),
            serde_json::json!({ "model": "llama3.2" }),
        ),
        ("watch".to_string(), serde_json::json!(true)),
    ]);

    let (sections, watch) = crate::daemon_plugins::split_plugins_config(&raw);
    assert!(watch);
    assert!(sections.contains_key("reflection"));
    assert!(
        !sections.contains_key(crucible_core::config::PLUGINS_DECLARE_KEY),
        "the declaration table must not reach any setup(cfg): {sections:?}"
    );
}
