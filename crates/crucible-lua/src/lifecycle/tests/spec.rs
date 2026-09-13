use crate::command_effect::CommandEffect;
use crate::lifecycle::{
    spec_from_table, LifecycleError, LifecycleResult, PluginManager, PluginSpec, FRAGMENT_FILE,
};
use mlua::{Lua, Table};
use std::path::Path;
use tempfile::TempDir;

/// Evaluate `source` on a plain VM, then read the table it returns.
///
/// The VM is the test's own. Production reads the table from the daemon VM
/// after `init.luau` ran there, so no sandbox exists for a test to reach.
fn spec_of(source: &str) -> LifecycleResult<PluginSpec> {
    let lua = Lua::new();
    let table: Table = lua
        .load(source)
        .eval()
        .expect("the test source returns a table");
    spec_from_table(&table, Path::new("test/init.lua"))
}

#[test]
fn a_tool_declaration_is_read_with_its_source_path() {
    let source = r#"
return {
    tools = {
        my_tool = {
            desc = "Do something",
            params = {
                { name = "query", type = "string", desc = "Search query" },
            },
            fn = function(args) return { result = "ok" } end,
        },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.tools.len(), 1);
    assert_eq!(spec.tools[0].name, "my_tool");
    assert_eq!(spec.tools[0].description, "Do something");
    assert_eq!(spec.tools[0].source_path, "test/init.lua");
    assert_eq!(spec.tools[0].params.len(), 1);
    assert_eq!(spec.tools[0].params[0].name, "query");
    assert_eq!(spec.tools[0].params[0].param_type, "string");
    assert!(!spec.tools[0].params[0].optional);
}

#[test]
fn every_declaration_kind_is_read() {
    let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
function M.my_handler(ctx, event) return event end

return {
    tools = {
        my_tool = { desc = "A tool", fn = M.my_tool },
    },
    commands = {
        my_command = { desc = "A command", hint = "[args]", fn = M.my_command },
    },
    handlers = {
        { event = "note:created", priority = 150, name = "on_note_created", fn = M.my_handler },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.tools.len(), 1);
    assert_eq!(spec.commands.len(), 1);
    assert_eq!(spec.handlers.len(), 1);
}

#[test]
fn a_setup_function_is_recorded() {
    let source = r#"
return {
    setup = function(config)
        -- Called after load with plugin config
    end,
}
"#;
    let spec = spec_of(source).unwrap();

    assert!(spec.has_setup);
}

/// A table that declares nothing is an empty spec, not a refusal.
///
/// The old reader answered `None` for a table without a "spec field", and
/// the daemon then refused the plugin. Every table `init.luau` returns is
/// the plugin's module now, and a module may declare nothing.
#[test]
fn an_empty_table_is_an_empty_spec() {
    let spec = spec_of("return {}").unwrap();
    assert!(spec.tools.is_empty());
    assert!(spec.commands.is_empty());
    assert!(spec.handlers.is_empty());
    assert!(spec.services.is_empty());
    assert!(!spec.has_setup);
}

/// A module table that holds functions at its own keys declares nothing:
/// only the `tools`, `commands`, `services` and `handlers` tables do.
#[test]
fn a_module_table_of_functions_declares_nothing() {
    let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
return M
"#;
    let spec = spec_of(source).unwrap();
    assert!(spec.tools.is_empty());
    assert!(spec.commands.is_empty());
    assert!(!spec.has_setup);
}

#[test]
fn test_tool_params_required_and_optional() {
    let source = r#"
return {
    tools = {
        search = {
            desc = "Search",
            params = {
                { name = "query", type = "string", desc = "Search query" },
                { name = "limit", type = "number", desc = "Max results", optional = true },
            },
        },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    let tool = &spec.tools[0];
    assert_eq!(tool.params.len(), 2);
    assert!(!tool.params[0].optional);
    assert!(tool.params[1].optional);
    assert_eq!(tool.params[1].param_type, "number");
}

#[test]
fn test_handler_spec_fields() {
    let source = r#"
return {
    handlers = {
        { event = "note:created", priority = 50, pattern = "*.md", name = "on_md_created" },
        { event = "tool:after", name = "log_tool" },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.handlers.len(), 2);

    let h1 = &spec.handlers[0];
    assert_eq!(h1.event_type, "note:created");
    assert_eq!(h1.priority, 50);
    assert_eq!(h1.pattern, "*.md");
    assert_eq!(h1.name, "on_md_created");

    let h2 = &spec.handlers[1];
    assert_eq!(h2.event_type, "tool:after");
    assert_eq!(h2.priority, 100); // default
    assert_eq!(h2.pattern, "*"); // default
}

#[test]
fn test_command_spec_with_hint() {
    let source = r#"
return {
    commands = {
        daily = { desc = "Create daily note", hint = "[title]" },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.commands.len(), 1);
    assert_eq!(spec.commands[0].name, "daily");
    assert_eq!(spec.commands[0].description, "Create daily note");
    assert_eq!(spec.commands[0].input_hint, Some("[title]".to_string()));
}

/// A declared `params` list and a free-text `hint` are not alternatives.
///
/// The hint is one line of free text for a person; the params are the typed
/// declaration a dialog is generated from. A plugin that gains one must not
/// lose the other.
#[test]
fn test_command_carries_params_and_hint_together() {
    let source = r#"
return {
    commands = {
        daily = {
            desc = "Create daily note",
            hint = "[title]",
            effect = "write",
            params = {
                { name = "title", type = "string", desc = "Note title", optional = true },
            },
        },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.commands[0].input_hint, Some("[title]".to_string()));
    assert_eq!(spec.commands[0].params.len(), 1);
    assert_eq!(spec.commands[0].params[0].name, "title");
    assert!(spec.commands[0].params[0].optional);
    assert_eq!(spec.commands[0].effect, CommandEffect::Write);
}

#[test]
fn test_command_effect_is_read_when_declared_read() {
    let source = r#"
return {
    commands = {
        board = { desc = "Read the board", effect = "read" },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.commands[0].effect, CommandEffect::Read);
}

/// An undeclared command is unknown, and unknown must cost a question rather
/// than a file. See `command_effect.rs` for the whole argument.
#[test]
fn test_command_effect_defaults_to_write_when_absent() {
    let source = r#"
return {
    commands = {
        legacy = { desc = "Declared before effects existed" },
    },
}
"#;
    let spec = spec_of(source).unwrap();

    assert_eq!(spec.commands[0].effect, CommandEffect::Write);
}

/// A misspelt effect must refuse the load, exactly as a misspelt type does.
///
/// Falling back to the default would turn `effect = "raed"` into a write and
/// tell the author nothing — and the author who wrote it wanted a read, so the
/// silent answer is the opposite of the intent.
#[test]
fn test_command_effect_unreadable_refuses_the_load() {
    let source = r#"
return {
    commands = {
        board = { desc = "Read the board", effect = "raed" },
    },
}
"#;
    let error = spec_of(source).expect_err("an unreadable effect must refuse the load");

    let message = error.to_string();
    assert!(
        matches!(error, LifecycleError::InvalidDeclaration(_)),
        "expected InvalidDeclaration, got {error:?}"
    );
    assert!(
        message.contains("board") && message.contains("raed") && message.contains("read"),
        "the message must name the command, the bad text and the options: {message}"
    );
}

/// A command's declared types are checked, exactly as a tool's are.
///
/// They became the same JSON Schema and now generate the same dialog, so an
/// unreadable one is the same wrong answer on either surface. Only the tool
/// half was validated.
///
/// `array<` and not a misspelt `strig`: a bare name is a *legal* declaration
/// (`LuaType::Named`, a type the plugin declares elsewhere), so a typo of a
/// primitive is readable text that means something else. That gap is real and
/// is the parser's, not this check's — it is the same on the tool side.
#[test]
fn test_command_param_types_are_validated() {
    let source = r#"
return {
    commands = {
        board = {
            desc = "Read the board",
            params = {
                { name = "folder", type = "array<" },
            },
        },
    },
}
"#;
    let error =
        spec_of(source).expect_err("an unreadable command parameter type must refuse the load");

    assert!(
        matches!(error, LifecycleError::InvalidDeclaration(_)),
        "expected InvalidDeclaration, got {error:?}"
    );
    let message = error.to_string();
    assert!(
        message.contains("board") && message.contains("folder"),
        "the message must name the command and the parameter: {message}"
    );
}

/// A plugin keeps its declared name in a differently-named directory.
///
/// `plugin_name_for_dir` documents the hazard: a repo cloned as
/// `crucible-discord` whose fragment declares `name = "discord"`. Identity is
/// the directory name, and the declared name is recorded so
/// `[plugins.discord]` still reaches its `setup`. Moving a directory must not
/// change what a plugin IS.
#[test]
fn a_declared_name_is_recorded_for_config_lookup() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("crucible-discord");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("init.luau"), "return {}\n").unwrap();
    std::fs::write(
        dir.join(FRAGMENT_FILE),
        "return { name = 'discord', version = '1.0.0' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    let plugin = manager
        .get("crucible-discord")
        .expect("identity is the directory name");
    assert_eq!(
        plugin.manifest.declared_name.as_deref(),
        Some("discord"),
        "the declared name must be recorded for config lookup"
    );
}

/// A fragment name that is not a usable plugin name is refused, not adopted.
///
/// Deleting the YAML reader deleted the only `validate()` call site. The
/// fragment is now the only place a name comes from, so it takes the same
/// checks the manifest used to: a name with a path separator would otherwise
/// reach `[plugins.<name>]` lookups and the module search path.
#[test]
fn a_fragment_name_that_is_not_a_valid_plugin_name_is_refused() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("wellformed");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("init.luau"), "return {}\n").unwrap();
    std::fs::write(
        dir.join(FRAGMENT_FILE),
        "return { name = '../escape', version = '1.0.0' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    assert!(
        manager.get("../escape").is_none(),
        "a name with a path separator must not become a plugin identity"
    );
    assert_eq!(
        manager.get("wellformed").map(|p| p.manifest.name.clone()),
        Some("wellformed".to_string()),
        "identity is always the directory name"
    );
    assert_eq!(
        manager
            .get("wellformed")
            .and_then(|p| p.manifest.declared_name.clone()),
        None,
        "an unusable declared name is refused, not recorded for config lookup"
    );
    assert!(
        manager.discovery_errors().is_empty(),
        "an unusable name is a warning, not a discovery error"
    );
}

#[test]
fn test_spec_services_parsed() {
    let source = r#"
        return {
            services = {
                gateway = {
                    desc = "WebSocket gateway",
                    fn = function() end,
                },
                heartbeat = {
                    desc = "Keep-alive pinger",
                    fn = function() end,
                },
                no_fn_service = {
                    desc = "Missing fn field -- should be skipped",
                },
            },
        }
    "#;

    let spec = spec_of(source).unwrap();

    assert_eq!(spec.services.len(), 2);

    let names: Vec<&str> = spec.services.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"gateway"));
    assert!(names.contains(&"heartbeat"));

    let gw = spec.services.iter().find(|s| s.name == "gateway").unwrap();
    assert_eq!(gw.description, "WebSocket gateway");
    assert_eq!(gw.service_fn, "gateway");
}
