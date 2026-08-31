use crucible_lua::{manifest::PluginState, LuaExecutor};
use crucible_lua::{stubs::StubGenerator, PluginManager};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const TEMPLATE_PLUGIN_YAML: &str =
    include_str!("../../crucible-cli/src/commands/plugin/templates/plugin.yaml");
const TEMPLATE_INIT_LUA: &str =
    include_str!("../../crucible-cli/src/commands/plugin/templates/init.luau");
const TEMPLATE_HEALTH_LUA: &str =
    include_str!("../../crucible-cli/src/commands/plugin/templates/health.luau");
const TEMPLATE_LUARC_JSON: &str =
    include_str!("../../crucible-cli/src/commands/plugin/templates/.luarc.json");
const TEMPLATE_INIT_TEST_LUA: &str =
    include_str!("../../crucible-cli/src/commands/plugin/templates/tests/init_test.luau");

fn write_scaffold_plugin(root: &Path, name: &str) {
    let plugin_dir = root.join(name);
    fs::create_dir_all(plugin_dir.join("tests")).unwrap();
    fs::write(
        plugin_dir.join("plugin.yaml"),
        TEMPLATE_PLUGIN_YAML.replace("{{name}}", name),
    )
    .unwrap();
    fs::write(
        plugin_dir.join("init.lua"),
        TEMPLATE_INIT_LUA.replace("{{name}}", name),
    )
    .unwrap();
    fs::write(
        plugin_dir.join("health.lua"),
        TEMPLATE_HEALTH_LUA.replace("{{name}}", name),
    )
    .unwrap();
    fs::write(plugin_dir.join(".luarc.json"), TEMPLATE_LUARC_JSON).unwrap();
    fs::write(
        plugin_dir.join("tests").join("init_test.lua"),
        TEMPLATE_INIT_TEST_LUA.replace("{{name}}", name),
    )
    .unwrap();
}

/// Give the executor the roots the RUNTIME gives a plugin, and no others:
/// the plugin's parent (so `require("<plugin-name>")` works) plus the
/// plugin's own `lua/` directory, which the returned guard scopes.
///
/// The harness used to add `<plugin_dir>/?.lua` on top, which made a require
/// pass here that failed in the daemon. `shipped_plugin_lua_suite_passes` is
/// the gate that mirrors the loader; this must not disagree with it.
fn configure_modules(executor: &LuaExecutor, plugin_dir: &Path) -> crucible_lua::PrivateRootGuard {
    let parent = plugin_dir.parent().unwrap_or(plugin_dir).to_path_buf();
    executor
        .configure_module_roots(vec![parent])
        .expect("module roots");
    executor
        .enter_plugin_root(plugin_dir)
        .expect("plugin module root")
}

fn create_reload_plugin_files(root: &Path, name: &str, module_value: &str) {
    let plugin_dir = root.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("plugin.yaml"),
        format!(
            "name: {name}\nversion: \"1.0.0\"\nmain: init.lua\nexports:\n  auto_discover: true\n"
        ),
    )
    .unwrap();

    let init_source = format!(
        r#"
local core = require("{name}.core")
return {{
    name = "{name}",
    version = "1.0.0",
    tools = {{
        current_value = {{
            desc = "Read current module value",
            fn = function()
                return core.value
            end,
        }},
    }},
}}
"#
    );
    fs::write(plugin_dir.join("init.lua"), init_source).unwrap();
    fs::write(
        plugin_dir.join("core.lua"),
        format!("return {{ value = '{module_value}' }}\n"),
    )
    .unwrap();
}

#[tokio::test]
async fn test_plugin_test_roundtrip() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "roundtrip-plugin";
    write_scaffold_plugin(temp.path(), plugin_name);

    let plugin_dir = temp.path().join(plugin_name);
    let tests_path = plugin_dir.join("tests").join("init_test.lua");
    let tests_source = fs::read_to_string(tests_path).unwrap();

    let executor = LuaExecutor::new().unwrap();
    executor.install_test_harness().unwrap();
    let lua = executor.lua();

    let _module_scope = configure_modules(&executor, &plugin_dir);

    lua.load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
        .unwrap();

    lua.load(&tests_source)
        .set_name("tests/init_test.lua")
        .exec()
        .unwrap();

    let results: mlua::Table = lua.load("return run_tests()").eval().unwrap();
    let passed: usize = results.get("passed").unwrap();
    let failed: usize = results.get("failed").unwrap();
    let pending: usize = results.get("pending").unwrap();

    assert_eq!(passed, 3);
    assert_eq!(failed, 0);
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn test_plugin_test_with_mocks() {
    let executor = LuaExecutor::new().unwrap();
    executor.install_test_harness().unwrap();

    let source = r#"
function handler(args)
    test_mocks.setup({
        kiln = {
            notes = {
                { title = "Lua Basics", path = "lua.md", content = "lua plugin testing" },
                { title = "Rust Notes", path = "rust.md", content = "ownership model" },
            },
        },
    })

    local results = cru.kiln.search("lua", { limit = 5 })
    local calls = test_mocks.get_calls("kiln", "search")

    return {
        count = #results,
        first_path = results[1] and results[1].path or nil,
        call_count = #calls,
        first_query = calls[1] and calls[1][1] or nil,
    }
end
"#;

    let result = executor
        .execute_source(source, serde_json::json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["count"], 1);
    assert_eq!(content["first_path"], "lua.md");
    assert_eq!(content["call_count"], 1);
    assert_eq!(content["first_query"], "lua");
}

#[test]
fn test_plugin_test_failure_reporting() {
    let executor = LuaExecutor::new().unwrap();
    executor.install_test_harness().unwrap();
    let lua = executor.lua();

    lua.load(
        r#"
describe("failure-suite", function()
    it("should report assertion details", function()
        expect.equal(1, 2)
    end)
end)
"#,
    )
    .set_name("failing_test.lua")
    .exec()
    .unwrap();

    let results: mlua::Table = lua.load("return run_tests()").eval().unwrap();
    let failed: usize = results.get("failed").unwrap();
    let errors: mlua::Table = results.get("errors").unwrap();
    let first_error: mlua::Table = errors.get(1).unwrap();
    let test_name: String = first_error.get("name").unwrap();
    let error: String = first_error.get("error").unwrap();

    assert_eq!(failed, 1);
    assert_eq!(test_name, "should report assertion details");
    assert!(error.contains("Expected: 1"));
    assert!(error.contains("Actual: 2"));
}

#[test]
fn test_plugin_health_check() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("health-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("health.lua"),
        r#"
return {
    check = function()
        cru.health.start("health-plugin")
        cru.health.ok("loaded")
    end,
}
"#,
    )
    .unwrap();

    let health_source = fs::read_to_string(plugin_dir.join("health.lua")).unwrap();
    let executor = LuaExecutor::new().unwrap();
    executor.install_test_harness().unwrap();
    let lua = executor.lua();

    let health_module: mlua::Table = lua.load(&health_source).eval().unwrap();
    let check: mlua::Function = health_module.get("check").unwrap();
    check.call::<()>(()).unwrap();

    let results: mlua::Table = lua.load("return cru.health.get_results()").eval().unwrap();
    let healthy: bool = results.get("healthy").unwrap();
    let checks: mlua::Table = results.get("checks").unwrap();
    let first_check: mlua::Table = checks.get(1).unwrap();
    let level: String = first_check.get("level").unwrap();
    let msg: String = first_check.get("msg").unwrap();

    assert!(healthy);
    assert_eq!(level, "ok");
    assert_eq!(msg, "loaded");
}

#[test]
fn test_plugin_reload_picks_up_changes() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "reload_pipeline";
    create_reload_plugin_files(temp.path(), plugin_name, "v1");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load(plugin_name).unwrap();

    let before: String = manager
        .eval_runtime("local mod = require('reload_pipeline.core'); return mod.value")
        .unwrap();
    assert_eq!(before, "v1");

    fs::write(
        temp.path().join(plugin_name).join("core.lua"),
        "return { value = 'v2' }\n",
    )
    .unwrap();

    manager.reload_plugin(plugin_name).unwrap();

    let after: String = manager
        .eval_runtime("local mod = require('reload_pipeline.core'); return mod.value")
        .unwrap();
    let state = manager.get(plugin_name).unwrap().state;

    assert_eq!(state, PluginState::Active);
    assert_eq!(after, "v2");
}

#[test]
fn test_scaffold_template_validity() {
    let plugin_name = "template-check";

    let yaml_source = TEMPLATE_PLUGIN_YAML.replace("{{name}}", plugin_name);
    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_source).unwrap();
    assert_eq!(yaml["name"], plugin_name);
    // `.luau` is what `cru plugin new` writes and what the manifest names:
    // the extension Luau's own editor tooling recognises.
    assert_eq!(yaml["main"], "init.luau");

    // LuaLS has no Luau dialect. 5.1 is the closest it models — Luau derives
    // from it — so a scaffolded plugin is not told about 5.4-only syntax
    // (integer division, `goto`, `<close>`) that the runtime would refuse.
    let json: Value = serde_json::from_str(TEMPLATE_LUARC_JSON).unwrap();
    assert_eq!(json["runtime"]["version"], "Lua 5.1");

    let temp = TempDir::new().unwrap();
    write_scaffold_plugin(temp.path(), plugin_name);
    let plugin_dir = temp.path().join(plugin_name);

    let executor = LuaExecutor::new().unwrap();
    executor.install_test_harness().unwrap();
    let lua = executor.lua();
    let _module_scope = configure_modules(&executor, &plugin_dir);
    lua.load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
        .unwrap();
    lua.load(
        "expect.is_not_nil = function(val) if val == nil then error('Expected non-nil value', 2) end end",
    )
    .exec()
    .unwrap();

    let init_module: mlua::Table = lua
        .load(TEMPLATE_INIT_LUA.replace("{{name}}", plugin_name))
        .set_name("init.lua")
        .eval()
        .unwrap();
    let plugin_spec_name: String = init_module.get("name").unwrap();
    assert_eq!(plugin_spec_name, plugin_name);

    let health_module: mlua::Table = lua
        .load(TEMPLATE_HEALTH_LUA.replace("{{name}}", plugin_name))
        .set_name("health.lua")
        .eval()
        .unwrap();
    let health_fn: mlua::Function = health_module.get("check").unwrap();
    health_fn.call::<()>(()).unwrap();

    lua.load(TEMPLATE_INIT_TEST_LUA.replace("{{name}}", plugin_name))
        .set_name("tests/init_test.lua")
        .exec()
        .unwrap();
}

#[test]
fn test_stub_generation_and_verification() {
    let dir = TempDir::new().unwrap();
    StubGenerator::generate(dir.path()).unwrap();

    let cru_lua = dir.path().join("cru.lua");
    assert!(cru_lua.exists());

    let stubs = fs::read_to_string(&cru_lua).unwrap();
    assert!(stubs.contains("---@class cru.kiln"));
    assert!(stubs.contains("---@class cru.http"));
    assert!(stubs.contains("---@class cru.fs"));

    let verified = StubGenerator::verify(&cru_lua).unwrap();
    assert!(verified);
}
