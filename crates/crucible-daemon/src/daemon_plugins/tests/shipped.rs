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
/// because activation errors were downgraded to a `warn!` on a
/// stdout auto-spawn points at /dev/null. This is the Phase-6 smoke:
/// every shipped plugin must load through the REAL loader and *execute* —
/// state `Active`, no `last_error`, and a spec extracted (proof its
/// `init.lua` ran to completion and returned its table).
#[tokio::test]
async fn every_shipped_plugin_executes() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .activate_discovered(&[(shipped_plugins_dir(), PluginSource::Runtime)])
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
        .activate_discovered(&[(shipped_plugins_dir(), PluginSource::Runtime)])
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

    let mut discovered = manager.discover(&mlua::Lua::new()).expect("discovery");
    discovered.sort();

    assert_eq!(
        discovered,
        shipped_plugin_names(),
        "a shipped plugin failed discovery — it will be invisible in `plugin.list`"
    );
}

/// Discovery reads the intercept grant from the fragment (`spec.luau`) and
/// never from the entry file. `oci` declared `intercepts_tools = true` in
/// its `init.luau` return table, which granted nothing: the host refused
/// every `handled = true` it returned, and the container was a no-op.
///
/// The expectation comes from a real `discover` over `runtime/plugins/`,
/// not from a grep of the fragment text.
#[test]
fn every_shipped_plugin_with_an_intercept_grant_declares_it_in_its_fragment() {
    let mut manager = PluginManager::new();
    manager.add_search_path_with_source(shipped_plugins_dir(), PluginSource::Runtime);
    manager.discover(&mlua::Lua::new()).expect("discovery");

    let mut granted: Vec<&str> = manager
        .list()
        .filter(|plugin| plugin.manifest.intercepts_tools)
        .map(|plugin| plugin.manifest.name.as_str())
        .collect();
    granted.sort();

    assert_eq!(
        granted,
        ["oci"],
        "the plugins whose fragment grants interception must be exactly the ones that take tool calls over"
    );
}

/// Every shipped plugin describes itself in a fragment, `spec.luau`.
///
/// Discovery reads the fragment and runs no plugin code, so a plugin with
/// no fragment has no version and no declared name at any point, and
/// `plugin.list` shows a blank row for it. The expectation comes from a
/// real `discover` over `runtime/plugins/`, not from the text of a file.
/// This replaced a gate that read each `init.luau` for `name = ` and four
/// more substrings, which `author = "agent"` in a tool body satisfied.
#[test]
fn every_shipped_plugin_has_a_fragment_with_a_name() {
    let mut manager = PluginManager::new();
    manager.add_search_path_with_source(shipped_plugins_dir(), PluginSource::Runtime);
    manager.discover(&mlua::Lua::new()).expect("discovery");

    let mut missing: Vec<String> = Vec::new();
    for name in shipped_plugin_names() {
        let manifest = &manager
            .get(&name)
            .unwrap_or_else(|| panic!("shipped plugin '{name}' was not discovered"))
            .manifest;
        if manifest.declared_name.as_deref() != Some(name.as_str()) {
            missing.push(format!(
                "{name}: the fragment declares name {:?}",
                manifest.declared_name
            ));
        }
        if manifest.version.is_none() {
            missing.push(format!("{name}: version"));
        }
        if manifest.description.is_empty() {
            missing.push(format!("{name}: description"));
        }
    }

    assert!(
        missing.is_empty(),
        "shipped plugins whose fragment does not describe them: {missing:#?}"
    );
}

/// The keys a fragment holds, and the module table a plugin returns from
/// `init.luau` does not.
const FRAGMENT_METADATA: [&str; 6] = [
    "name",
    "version",
    "description",
    "author",
    "license",
    "intercepts_tools",
];

/// A plugin's metadata lives in its fragment and nowhere else.
///
/// The loader reads declarations only from the module table, so a metadata
/// key there is a second source of truth that nothing checks. `oci` carried
/// `intercepts_tools = true` in its module table for months after the
/// fragment became the one place the grant counts, and the two could drift
/// apart without a test noticing.
///
/// Each module table is the one activation seeded into `package.loaded` on
/// the loader VM, so the check reads what a `require` would answer.
#[tokio::test]
async fn no_shipped_module_table_carries_fragment_metadata() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .activate_discovered(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    let loaded: mlua::Table = loader
        .lua()
        .globals()
        .get::<mlua::Table>("package")
        .and_then(|package| package.get("loaded"))
        .expect("package.loaded");

    let names = shipped_plugin_names();
    let mut carried: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for name in &names {
        let module: mlua::Table = loaded
            .get(name.as_str())
            .unwrap_or_else(|e| panic!("{name}: activation seeded no module table: {e}"));
        for key in FRAGMENT_METADATA {
            if module.contains_key(key).expect("key lookup") {
                carried.push(format!("{name}: {key}"));
            }
        }
        checked += 1;
    }

    assert_eq!(checked, names.len(), "the gate checked fewer plugins than ship");
    assert!(
        carried.is_empty(),
        "module tables that carry fragment metadata: {carried:#?}"
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
        .activate_discovered(&[(shipped_plugins_dir(), PluginSource::Runtime)])
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
    loader
        .add_plugin_paths(&[(shipped_plugins_dir(), PluginSource::Runtime)])
        .expect("search path");
    loader
        .load_plugins_from_spec()
        .await
        .expect("activate the spec");
    loader
}

/// Boot with an empty config home and the shipped runtimepath. Every
/// directory under `runtime/plugins/` must come out Active, because the
/// Builtin fragment names it and nothing disables it.
///
/// Activation is spec-driven, so the Builtin fragment is the only thing
/// that keeps the shipped set active, and this test is what proves the
/// fragment is complete: remove one name from
/// `runtime/defaults/init.luau` and this fails.
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
        let last_error = entry["last_error"].as_str().unwrap_or("");
        assert!(
            last_error.is_empty(),
            "shipped plugin '{name}' recorded an error: {last_error}"
        );
    }
}
