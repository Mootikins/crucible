use super::create_test_plugin;
use crate::lifecycle::PluginManager;
use tempfile::TempDir;

#[test]
fn test_discover_plugins() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "plugin-a", "1.0.0");
    create_test_plugin(temp.path(), "plugin-b", "2.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);

    let discovered = manager.discover().unwrap();
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
    let discovered = manager.discover().unwrap();

    assert_eq!(discovered.len(), 1);
    assert!(discovered.contains(&"my-plugin".to_string()));

    let plugin = manager.get("my-plugin").unwrap();
    // Discovery reads the directory; it never runs the plugin's Lua. The
    // version lives in the spec table, so at this point NO version is known.
    // The manifest used to hold a "0.0.0" placeholder here, which the RPC
    // and the web UI then showed as if a release had said so.
    assert_eq!(plugin.version(), None);
    let wire = serde_json::to_value(&plugin.manifest).unwrap();
    assert_eq!(
        wire["version"],
        serde_json::Value::Null,
        "a discovered-but-unloaded plugin must report no version: {wire}"
    );
}

#[test]
fn test_discover_manifestless_with_spec_override() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    // Plugin returns a spec with custom name/version
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"return { name = "custom-name", version = "1.2.0" }"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let plugin = manager.get("my-plugin").unwrap();
    // Identity stays the directory name; the declared one is recorded for
    // `[plugins.<name>]` lookup. Version does come from the spec — it names
    // nothing the host has to resolve before running Lua.
    assert_eq!(plugin.manifest.name, "my-plugin");
    assert_eq!(
        plugin.manifest.declared_name.as_deref(),
        Some("custom-name")
    );
    assert_eq!(plugin.version(), Some("1.2.0"));
}

/// The spec table supplies a plugin's version and description.
///
/// There is no manifest to take precedence over it any more. This replaces
/// `test_manifest_takes_precedence_over_lua_table`, whose premise was that a
/// `plugin.yaml` could out-declare the Lua — a file that no longer exists.
#[test]
fn the_spec_table_supplies_the_plugins_identity() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"return { name = "my-plugin", version = "2.0.0" }"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let plugin = manager.get("my-plugin").unwrap();
    assert_eq!(plugin.manifest.name, "my-plugin");
    assert_eq!(plugin.version(), Some("2.0.0"));
}

/// Pure-Lua vendoring is THE supported dependency mechanism: native rocks
/// cannot load into the statically vendored interpreter. A vendored module
/// sits in the plugin's own directory and is required under the plugin's
/// name (`my-plugin/vendored.lua`, required as `"my-plugin.vendored"`), so
/// two plugins vendoring the same library cannot collide. The manager and
/// the daemon resolve it identically, through the plugin root the plugin
/// directory sits in — the parity the two loaders used to lack. The
/// assertion pins the VALUE the vendored module returned, read back out of
/// the VM, so a require that silently resolved elsewhere fails.
#[test]
fn a_vendored_module_resolves_under_the_plugin_namespace() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("vendored.lua"),
        r#"return { version = "3.1.4-vendored" }"#,
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"
local dep = require("my-plugin.vendored")
rawset(_G, "vendored_probe", dep.version)
return { name = "my-plugin", version = "1.0.0" }
"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let probe: String = manager.eval_runtime("return vendored_probe").unwrap();
    assert_eq!(probe, "3.1.4-vendored");
}

#[test]
fn test_empty_directory_not_discovered() {
    let temp = TempDir::new().unwrap();
    let empty_dir = temp.path().join("empty-dir");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert!(discovered.is_empty());
}

/// `plugins.declare` in the config holds plugin declarations, so a plugin
/// actually named `declare` could never be configured through the store
/// form. Discovery must refuse it by name — a silent discovery would drop
/// that plugin's configuration with no visible reason.
#[test]
fn a_plugin_named_declare_is_a_named_discovery_error_not_a_plugin() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join(crucible_core::config::PLUGINS_DECLARE_KEY);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "return { name = 'declare' }").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert!(
        discovered.is_empty(),
        "the reserved name must not be discovered: {discovered:?}"
    );
    let errors = manager.discovery_errors();
    assert_eq!(errors.len(), 1, "one named refusal: {errors:?}");
    assert_eq!(errors[0].path, plugin_dir, "the error names the path");
    assert!(
        errors[0].error.contains("reserved")
            && errors[0]
                .error
                .contains(crucible_core::config::PLUGINS_DECLARE_KEY),
        "the error names the reserved name and why: {}",
        errors[0].error
    );
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
    let discovered = manager.discover().unwrap();

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
/// Load order is the tie-break between two handlers of equal priority, so it
/// must give the same answer on every machine.
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
    let discovered = manager.discover().unwrap();

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
/// roots in the order it receives them, so a plugin from a higher root loads
/// first even when its name sorts last.
#[test]
fn a_higher_search_path_loads_before_a_lower_one() {
    let high = TempDir::new().unwrap();
    let low = TempDir::new().unwrap();
    create_test_plugin(high.path(), "zulu", "1.0.0");
    create_test_plugin(low.path(), "alpha", "1.0.0");

    let mut manager = PluginManager::new()
        .with_search_paths(vec![high.path().to_path_buf(), low.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert_eq!(
        discovered,
        vec!["zulu", "alpha"],
        "the search path rank must outrank the name order"
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
    let discovered = manager.discover().unwrap();

    assert_eq!(
        discovered,
        vec!["alpha", "mike", "zulu"],
        "one sort must cover directory and single-file plugins"
    );
}
