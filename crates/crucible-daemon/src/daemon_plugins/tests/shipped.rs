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

/// The kill switch: `plugins.<name>.enabled = false` in the config store must
/// keep a bundled plugin from ever executing. A user writes it in `init.lua`;
/// the web writes it to `settings.json`. Either way the loader sees the same
/// `plugin_config` map this test hands it.
///
/// Editing the extracted entry file does not work — the runtime tree is
/// re-stamped from the binary whenever `version + blake3(tree)` changes, which
/// silently restores the shipped copy. Config is the only durable lever, and
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
/// `oci` was eight lines with no `author` and no `license` while the
/// other six carried all three — an arbitrary difference nobody would notice
/// until they were generating an index of what ships and half the rows were
/// blank. One shape, asserted, so it stays one shape.
#[test]
fn every_shipped_plugin_declares_the_same_identifying_fields() {
    const REQUIRED: &[&str] = &["name", "version", "description", "author", "license"];

    let mut missing: Vec<String> = Vec::new();
    for name in shipped_plugin_names() {
        // The spec table in the entry file, which is where this metadata
        // lives now that `plugin.yaml` is gone. Read as text rather than
        // executed: this asserts the field is DECLARED, and executing a
        // plugin to find out would be the defect `discover_only` exists to
        // avoid.
        let entry = crucible_lua::source_files::init_file(&shipped_plugins_dir().join(&name))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name} ships no entry file"));
        let body = std::fs::read_to_string(&entry)
            .unwrap_or_else(|e| panic!("read {}: {e}", entry.display()));

        for field in REQUIRED {
            if !body.contains(&format!("{field} = ")) {
                missing.push(format!("{name}: {field}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "shipped plugins are missing identifying fields: {missing:#?}"
    );
}

/// Every control the shipped plugins declare is one the closed set knows.
///
/// Derived from a REAL loader over `runtime/plugins/`, not from a grep of
/// source text. That distinction is the whole point: the previous generation of
/// gates in this repo read their own sources and were satisfiable without the
/// entry they were meant to require. Here the expectation comes from the tree
/// the daemon actually registered, so a plugin whose options never registered
/// cannot pass by having the right words in its file.
///
/// It is also the safety net under `validate_tree`. That refusal fires inside
/// `setup()`, so a shipped plugin declaring an unknown control would end
/// `Error` and inert — which `every_shipped_plugin_executes` above already
/// catches. This test says the *stronger* thing: the trees that DID register
/// contain only kinds the frontends can draw.
#[tokio::test]
async fn shipped_plugin_trees_declare_only_known_controls() {
    use crucible_lua::options::Control;

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    let options = loader.options();
    let plugins = options.plugins();
    assert!(
        !plugins.is_empty(),
        "no shipped plugin registered an options tree — this gate would pass vacuously",
    );

    /// Walk a described tree, collecting every `type` it carries.
    fn collect(node: &serde_json::Value, into: &mut Vec<(String, String)>, path: &str) {
        if let Some(ty) = node["type"].as_str() {
            into.push((path.to_string(), ty.to_string()));
        }
        for child in node["args"].as_array().into_iter().flatten() {
            let key = child["key"].as_str().unwrap_or("?");
            let child_path = if path.is_empty() {
                key.to_string()
            } else {
                format!("{path}.{key}")
            };
            collect(child, into, &child_path);
        }
    }

    let mut checked = 0usize;
    for plugin in &plugins {
        let tree = options
            .describe(plugin, "web")
            .unwrap_or_else(|| panic!("'{plugin}' has a tree but would not describe itself"));
        let mut found = Vec::new();
        collect(&tree, &mut found, "");
        for (path, declared) in found {
            assert!(
                Control::parse(&declared).is_some(),
                "plugin '{plugin}' declares unknown control '{declared}' at '{path}'",
            );
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "walked no option nodes — the gate proved nothing"
    );
}

/// Run the shipped defaults on `lua` as the boot does: under
/// `LuaSource::Builtin`, so every `cru.plugin.setup` entry in the file lands
/// at rank Builtin. `BUILTIN_INIT_LUA` is the same bytes the runtimepath
/// copy holds, and it reads no environment.
fn run_shipped_defaults(lua: &mlua::Lua) {
    let previous = crucible_lua::set_source(lua, crucible_lua::LuaSource::Builtin);
    lua.load(crucible_lua::BUILTIN_INIT_LUA)
        .set_name("shipped defaults")
        .exec()
        .expect("the shipped defaults must load");
    crucible_lua::set_source(lua, previous);
}

/// The shipped set is a Builtin fragment in `runtime/defaults/init.luau`.
///
/// The expectation comes from the filesystem and the running VM, not from
/// the text of the defaults file: the set of names the fragment wrote must
/// equal the set of directories under `runtime/plugins/`, and every entry
/// must sit at rank Builtin so an operator entry for the same name wins.
#[test]
fn the_builtin_fragment_names_every_shipped_plugin_directory() {
    use crucible_core::config::SpecRank;

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let lua = loader.executor().lua();
    run_shipped_defaults(lua);

    let spec = crucible_lua::spec_of(lua);
    let mut named: Vec<String> = spec.iter().map(|e| e.name.clone()).collect();
    named.sort();
    assert_eq!(
        named,
        shipped_plugin_names(),
        "the Builtin fragment and runtime/plugins/ disagree"
    );
    for name in &named {
        assert_eq!(
            spec.rank_of(name),
            Some(SpecRank::Builtin),
            "'{name}' must come from the shipped defaults at rank Builtin"
        );
    }
}

/// A loader booted on an empty config home: the shipped defaults ran, the
/// operator wrote nothing, and the shipped runtimepath is the only one.
///
/// Child-scoped values only, see AGENTS.md "Hermeticity": the import root is
/// a value under `home`, and no environment variable is read or set.
async fn boot_loader_with_home(home: &std::path::Path) -> DaemonPluginLoader {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    crucible_lua::set_import_root(loader.executor().lua(), home.join("lua"));
    run_shipped_defaults(loader.executor().lua());
    // Task 7 replaces this call with the spec-driven activation. Until then
    // `load_plugins` activates every discovered directory and reads no spec.
    loader
        .load_plugins(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");
    loader
}

/// Boot with an empty config home and the shipped runtimepath. Every
/// directory under `runtime/plugins/` must come out Active, because the
/// Builtin fragment names it and nothing disables it.
///
/// Today `load_plugins` reads no spec, so this holds by the old rule. Once
/// activation is spec-driven, the Builtin fragment is the only thing that
/// keeps the shipped set active, and this test is what proves the fragment
/// is complete. It stays enabled: the ignore-reason gate admits only a
/// prerequisite token, and a pending task is not one.
#[tokio::test]
async fn a_fresh_boot_activates_every_shipped_plugin() {
    let home = tempfile::TempDir::new().unwrap();
    let loader = boot_loader_with_home(home.path()).await;
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
    }
}
