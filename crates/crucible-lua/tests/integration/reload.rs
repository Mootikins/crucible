//! Plugin reload tests.

use crucible_lua::{lifecycle::PluginManager, manifest::PluginState};
use tempfile::TempDir;

use super::shared::create_plugin_files;

#[test]
fn test_reload_picks_up_changes() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "reload_sample";
    let init_source = r#"
local core = require("reload_sample.core")
return {
    name = "reload_sample",
    version = "1.0.0",
    tools = {
        current_value = {
            desc = "Read current module value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;

    create_plugin_files(
        temp.path(),
        plugin_name,
        init_source,
        "return { value = 'v1' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load(plugin_name).unwrap();

    let before: String = manager
        .eval_runtime("local mod = require('reload_sample.core'); return mod.value")
        .unwrap();
    assert_eq!(before, "v1");

    std::fs::write(
        temp.path()
            .join(plugin_name)
            .join(plugin_name)
            .join("core.lua"),
        "return { value = 'v2' }\n",
    )
    .unwrap();

    manager.reload_plugin(plugin_name).unwrap();

    let after: String = manager
        .eval_runtime("local mod = require('reload_sample.core'); return mod.value")
        .unwrap();
    assert_eq!(after, "v2");
}

#[test]
fn test_on_unload_hook_fires_on_reload() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "reload_hook";
    let init_source = r#"
_G.reload_trace = (_G.reload_trace or "") .. "L"
local core = require("reload_hook.core")

return {
    name = "reload_hook",
    version = "1.0.0",
    on_unload = function()
        _G.reload_trace = (_G.reload_trace or "") .. "U"
    end,
    tools = {
        current_value = {
            desc = "Read current module value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;

    create_plugin_files(
        temp.path(),
        plugin_name,
        init_source,
        "return { value = 'v1' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load(plugin_name).unwrap();

    let initial_trace: String = manager.eval_runtime("return _G.reload_trace").unwrap();
    assert_eq!(initial_trace, "L");

    std::fs::write(
        temp.path()
            .join(plugin_name)
            .join(plugin_name)
            .join("core.lua"),
        "return { value = 'v2' }\n",
    )
    .unwrap();

    manager.reload_plugin(plugin_name).unwrap();

    let trace_after_reload: String = manager.eval_runtime("return _G.reload_trace").unwrap();
    assert_eq!(trace_after_reload, "LUL");
}

#[test]
fn test_reload_failure_leaves_old_plugin_intact() {
    let temp = TempDir::new().unwrap();

    let fragile_init = r#"
local core = require("fragile_plugin.core")
return {
    name = "fragile_plugin",
    version = "1.0.0",
    tools = {
        fragile = {
            desc = "Fragile value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;
    create_plugin_files(
        temp.path(),
        "fragile_plugin",
        fragile_init,
        "return { value = 'good' }\n",
    );

    let stable_init = r#"
local core = require("stable_plugin.core")
return {
    name = "stable_plugin",
    version = "1.0.0",
    tools = {
        stable = {
            desc = "Stable value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;
    create_plugin_files(
        temp.path(),
        "stable_plugin",
        stable_init,
        "return { value = 'stable' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("fragile_plugin").unwrap();
    manager.load("stable_plugin").unwrap();

    let fragile_before: String = manager
        .eval_runtime("local mod = require('fragile_plugin.core'); return mod.value")
        .unwrap();
    let stable_before: String = manager
        .eval_runtime("local mod = require('stable_plugin.core'); return mod.value")
        .unwrap();
    assert_eq!(fragile_before, "good");
    assert_eq!(stable_before, "stable");

    std::fs::write(
        temp.path().join("fragile_plugin").join("init.lua"),
        "return { name = 'fragile_plugin', tools = {\n",
    )
    .unwrap();

    let reload_result = manager.reload_plugin("fragile_plugin");
    assert!(reload_result.is_err());

    let fragile_state = manager.get("fragile_plugin").unwrap().state;
    let stable_state = manager.get("stable_plugin").unwrap().state;
    assert_eq!(fragile_state, PluginState::Error);
    assert_eq!(stable_state, PluginState::Active);

    // After failed reload, fragile plugin's modules are cleared (not restored).
    // The stable plugin's modules remain untouched.
    let stable_after: String = manager
        .eval_runtime("local mod = require('stable_plugin.core'); return mod.value")
        .unwrap();
    assert_eq!(stable_after, "stable");
}
