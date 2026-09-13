use super::create_test_plugin;
use crate::lifecycle::PluginManager;
use crate::manifest::PluginState;
use mlua::Lua;
use tempfile::TempDir;

#[test]
fn test_discover_plugins() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "plugin-a", "1.0.0");
    create_test_plugin(temp.path(), "plugin-b", "2.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);

    let discovered = manager.discover(&Lua::new()).unwrap();
    assert_eq!(discovered.len(), 2);
    assert!(discovered.contains(&"plugin-a".to_string()));
    assert!(discovered.contains(&"plugin-b".to_string()));
}

#[test]
fn test_discover_directory_without_manifest() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "-- code").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(discovered.len(), 1);
    assert!(discovered.contains(&"my-plugin".to_string()));

    let plugin = manager.get("my-plugin").unwrap();
    // Discovery reads the directory and the fragment; it never runs the
    // plugin's Lua. This plugin has no fragment, so NO version is known. The
    // manifest used to hold a "0.0.0" placeholder here, which the RPC and
    // the web UI then showed as if a release had said so.
    assert_eq!(plugin.version(), None);
    let wire = serde_json::to_value(&plugin.manifest).unwrap();
    assert_eq!(
        wire["version"],
        serde_json::Value::Null,
        "a discovered-but-unloaded plugin must report no version: {wire}"
    );
}

/// The fragment's `name` is recorded as the declared name. Identity stays
/// the directory name: a repo cloned as `my-plugin` whose fragment declares
/// `name = "custom-name"` is still `my-plugin` to the runtimepath, and
/// `[plugins.custom-name]` still reaches it.
#[test]
fn a_fragment_name_is_recorded_as_the_declared_name() {
    let root = root_with_plugin(
        "my-plugin",
        Some(r#"return { name = "custom-name", version = "1.2.0" }"#),
        "return {}",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![root.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    let plugin = manager.get("my-plugin").unwrap();
    assert_eq!(plugin.manifest.name, "my-plugin");
    assert_eq!(
        plugin.manifest.declared_name.as_deref(),
        Some("custom-name")
    );
    assert_eq!(plugin.version(), Some("1.2.0"));
}

#[test]
fn test_empty_directory_not_discovered() {
    let temp = TempDir::new().unwrap();
    let empty_dir = temp.path().join("empty-dir");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert!(discovered.is_empty());
}

/// The file name Crucible removed. It appears here, and in no other fixture
/// in the tree, because this test is the one place whose subject is the
/// removal itself.
const REMOVED_MANIFEST_FILE: &str = "plugin.yaml";

/// A directory that holds only the removed manifest file is not a plugin.
///
/// `test_empty_directory_not_discovered` does not cover this: an empty
/// directory fails every possible rule, so it stays green under a
/// reintroduced manifest branch. Fixtures across two crates kept writing a
/// manifest beside their entry file for months after the loader stopped
/// reading one, and no test could fail, so the dead format went on teaching
/// itself. This states the rule the loader actually applies — the entry file
/// alone identifies a plugin.
#[test]
fn a_directory_holding_only_the_removed_manifest_is_not_a_plugin() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("manifest-only");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join(REMOVED_MANIFEST_FILE),
        "name: manifest-only\nversion: \"1.0.0\"\nmain: init.lua\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert!(
        discovered.is_empty(),
        "a manifest identifies no plugin, but discovery returned {discovered:?}"
    );
    assert!(
        manager.get("manifest-only").is_none(),
        "a manifest-only directory reached the plugin table"
    );
}

/// Discovery reads one directory in ascending order of name.
///
/// `std::fs::read_dir` specifies no order, and the order changes with the file
/// system, so without the sort this assertion depends on the disk.
///
/// This sort does not decide the activation order. The daemon's activation
/// pass sorts every discovered name, so that order is one alphabetical sort
/// across every directory whatever `read_dir` answers. What the discovery
/// sort decides is which entry wins a duplicated name, and it makes a
/// discovery log reproducible.
#[test]
fn discovery_reads_one_search_path_in_name_order() {
    let temp = TempDir::new().unwrap();
    // The creation order is neither alphabetical nor its reverse. A file
    // system that returns entries in creation order fails this test without
    // the sort, and so does one that returns the reverse. tmpfs on Linux
    // returns the reverse, so a reverse alphabetical fixture passed here even
    // with the sort removed.
    for name in ["mike", "alpha", "zulu", "delta", "yankee"] {
        create_test_plugin(temp.path(), name, "1.0.0");
    }

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(
        discovered,
        vec!["alpha", "delta", "mike", "yankee", "zulu"],
        "discovery must sort one directory by name"
    );
}

/// The rank between search paths outranks the name order inside one of them.
///
/// `crucible_core::runtime_path::entry::Origin` declares the rank between
/// roots, highest first, and `search_paths` preserves it. Discovery takes the
/// roots in the order it receives them, so a plugin from a higher root is
/// DISCOVERED first even when its name sorts last.
///
/// It also ACTIVATES first. The daemon's activation pass keeps discovery
/// order: the search-path rank first, then the file name inside one path.
/// An earlier version of this comment said that the pass sorted every name
/// again, and it does not. The daemon test
/// `activation_follows_discovery_order_rank_then_file_name` pins that order.
#[test]
fn a_higher_search_path_is_discovered_before_a_lower_one() {
    let high = TempDir::new().unwrap();
    let low = TempDir::new().unwrap();
    create_test_plugin(high.path(), "zulu", "1.0.0");
    create_test_plugin(low.path(), "alpha", "1.0.0");

    let mut manager = PluginManager::new()
        .with_search_paths(vec![high.path().to_path_buf(), low.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(
        discovered,
        vec!["zulu", "alpha"],
        "the search path rank must outrank the name order"
    );
}

/// A duplicated name resolves to the same entry on every file system.
///
/// This is what the discovery sort buys. A directory `omega` and a file
/// `omega.lua` in ONE search path both claim the name `omega`, because a
/// plugin's name is its file stem. Discovery keeps the first of the two and
/// logs the second as shadowed, so without the sort the winner is whatever
/// `read_dir` answered. The sort by file name puts `omega` before
/// `omega.lua`, so the directory wins on every machine.
///
/// `dir` is the assertion because it is the only thing that separates the two:
/// a directory plugin records the plugin directory, a single-file plugin
/// records the search path.
#[test]
fn a_directory_wins_a_name_a_single_file_plugin_also_claims() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "omega", "1.0.0");
    std::fs::write(temp.path().join("omega.lua"), "return {}").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(
        discovered,
        vec!["omega"],
        "a duplicated name must be discovered once"
    );
    assert_eq!(
        manager.get("omega").expect("omega was discovered").dir,
        temp.path().join("omega"),
        "the directory must win the name, not the single file beside it"
    );
}

/// A single-file plugin sorts with the directories, by file name.
///
/// Both kinds live in one directory, so one sort must cover both. The file
/// name carries the extension, which keeps the order total.
#[test]
fn discovery_sorts_single_file_plugins_with_directories() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "mike", "1.0.0");
    std::fs::write(temp.path().join("zulu.lua"), "return {}").unwrap();
    std::fs::write(temp.path().join("alpha.lua"), "return {}").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(
        discovered,
        vec!["alpha", "mike", "zulu"],
        "one sort must cover directory and single-file plugins"
    );
}

/// A search root that holds one plugin directory: `init.luau`, and the
/// fragment when one is given. The caller adds the root as a search path.
fn root_with_plugin(name: &str, fragment: Option<&str>, init: &str) -> TempDir {
    let root = TempDir::new().unwrap();
    let plugin_dir = root.path().join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.luau"), init).unwrap();
    if let Some(fragment) = fragment {
        std::fs::write(plugin_dir.join(crate::lifecycle::FRAGMENT_FILE), fragment).unwrap();
    }
    root
}

/// Discovery reads `spec.luau` and never `init.luau`. The init file raises,
/// so a discovery that ran it would fail; the fragment's fields reach the
/// manifest, and the plugin stays `Discovered`.
#[test]
fn discovery_reads_a_fragment_without_running_init() {
    let root = root_with_plugin(
        "probe",
        Some(r#"return { version = "0.3.0", intercepts_tools = true, opts = { depth = 2 } }"#),
        r#"error("init ran at discovery")"#,
    );
    let lua = Lua::new();
    let mut manager = PluginManager::new().with_search_paths(vec![root.path().to_path_buf()]);
    manager.discover(&lua).unwrap();

    let plugin = manager.get("probe").unwrap();
    assert_eq!(plugin.manifest.version.as_deref(), Some("0.3.0"));
    assert!(plugin.manifest.intercepts_tools);
    assert_eq!(plugin.manifest.opts, serde_json::json!({ "depth": 2 }));
    assert_eq!(plugin.state, PluginState::Discovered);
    assert!(manager.discovery_errors().is_empty());
}

#[test]
fn a_plugin_without_a_fragment_has_its_directory_name_and_no_grant() {
    let root = root_with_plugin("bare", None, "return {}");
    let lua = Lua::new();
    let mut manager = PluginManager::new().with_search_paths(vec![root.path().to_path_buf()]);
    manager.discover(&lua).unwrap();

    let plugin = manager.get("bare").unwrap();
    assert_eq!(plugin.manifest.name, "bare");
    assert!(plugin.manifest.declared_name.is_none());
    assert!(plugin.manifest.version.is_none());
    assert!(!plugin.manifest.intercepts_tools);
    assert_eq!(plugin.manifest.opts, serde_json::json!({}));
}

/// A fragment that does not evaluate is a discovery error that names the
/// file, and the directory is not a plugin. Not a panic, and not a silent
/// skip: a skipped directory looks like one that was never installed.
#[test]
fn a_broken_fragment_is_a_discovery_error_and_the_plugin_is_not_registered() {
    let root = root_with_plugin("broken", Some("return {"), "return {}");
    let lua = Lua::new();
    let mut manager = PluginManager::new().with_search_paths(vec![root.path().to_path_buf()]);
    let discovered = manager.discover(&lua).unwrap();

    assert!(discovered.is_empty(), "{discovered:?}");
    assert!(manager.get("broken").is_none());
    let errors = manager.discovery_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].path, root.path().join("broken"));
    assert!(errors[0].error.contains("spec.luau"), "{}", errors[0].error);
}
