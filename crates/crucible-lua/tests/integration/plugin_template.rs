//! Plugin template validation tests.

#[test]
fn test_plugin_template_yaml_is_valid() {
    let template_yaml =
        include_str!("../../../crucible-cli/src/commands/plugin/templates/plugin.yaml");
    let substituted = template_yaml.replace("{{name}}", "test-plugin");

    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(&substituted);
    assert!(parsed.is_ok(), "plugin.yaml template should be valid YAML");

    let manifest = parsed.unwrap();
    assert!(manifest["name"].is_string(), "name field should be present");
    assert!(
        manifest["version"].is_string(),
        "version field should be present"
    );
    assert!(manifest["main"].is_string(), "main field should be present");
    assert_eq!(manifest["main"].as_str().unwrap(), "init.lua");
}

#[test]
fn test_plugin_template_init_lua_is_syntactically_valid() {
    let template_lua = include_str!("../../../crucible-cli/src/commands/plugin/templates/init.lua");
    let substituted = template_lua.replace("{{name}}", "test-plugin");

    let lua = mlua::Lua::new();
    // The template registers its lifecycle hook by CALLING
    // `crucible.on_session_start` at load, which is the only registration the
    // loader honours — so evaluating it needs that global. A bare `Lua::new()`
    // has no `crucible` table; the daemon's executor always does.
    let crucible = lua.create_table().expect("crucible table");
    crucible
        .set(
            "on_session_start",
            lua.create_function(|_, _: mlua::Function| Ok(()))
                .expect("stub"),
        )
        .expect("stub on_session_start");
    lua.globals().set("crucible", crucible).expect("set global");

    let result = lua.load(&substituted).eval::<mlua::Value>();

    assert!(
        result.is_ok(),
        "init.lua template should be syntactically valid Lua: {:?}",
        result.err()
    );
}

/// The template must show only constructs the loader actually reads.
///
/// It used to carry `-- @tool name="greet"` / `-- @param` doc comments and a
/// `hooks = { on_session_start = ... }` spec field. Neither does anything:
/// `parse_tools`, `parse_commands` and `parse_views` have no callers on any
/// live path (only `parse_handlers` does, from `handlers/registry.rs`), and a
/// plugin that returns a spec table has its exports registered from that table
/// and nothing else. `hooks` is not among the recognised spec fields at all.
/// A starter template that leads with three no-ops teaches them.
#[test]
fn test_plugin_template_uses_only_live_constructs() {
    let template_lua = include_str!("../../../crucible-cli/src/commands/plugin/templates/init.lua");

    // Anchored to a comment at the start of a line: the template names these
    // constructs in prose precisely to say that they do nothing.
    for dead in ["@tool", "@param"] {
        assert!(
            !template_lua.lines().any(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("-- ") && trimmed[3..].trim_start().starts_with(dead)
            }),
            "`{dead}` is never parsed; it must not look load-bearing in the template"
        );
    }
    assert!(
        template_lua.contains("crucible.on_session_start("),
        "the hook must be registered by calling the api, not by a `hooks` field"
    );
    assert!(
        template_lua.contains("return {"),
        "Should return a plugin spec"
    );
    assert!(
        template_lua.contains("tools = {"),
        "the spec table is where tools are declared"
    );
}

#[test]
fn test_plugin_template_health_lua_is_syntactically_valid() {
    let template_lua =
        include_str!("../../../crucible-cli/src/commands/plugin/templates/health.lua");
    let substituted = template_lua.replace("{{name}}", "test-plugin");

    let lua = mlua::Lua::new();
    let result = lua.load(&substituted).eval::<mlua::Value>();

    assert!(
        result.is_ok(),
        "health.lua template should be syntactically valid Lua: {:?}",
        result.err()
    );
}
