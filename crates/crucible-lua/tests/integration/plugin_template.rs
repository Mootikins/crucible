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
    let result = lua.load(&substituted).eval::<mlua::Value>();

    assert!(
        result.is_ok(),
        "init.lua template should be syntactically valid Lua: {:?}",
        result.err()
    );
}

#[test]
fn test_plugin_template_tool_annotation_format() {
    let template_lua = include_str!("../../../crucible-cli/src/commands/plugin/templates/init.lua");

    assert!(
        template_lua.contains("@tool name=\"greet\""),
        "Should have @tool annotation"
    );
    assert!(
        template_lua.contains("@param name string"),
        "Should have @param annotation"
    );
    assert!(
        template_lua.contains("on_session_start"),
        "Should have on_session_start hook"
    );
    assert!(
        template_lua.contains("return {"),
        "Should return a plugin spec"
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
