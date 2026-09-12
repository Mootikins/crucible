//! THE COMPATIBILITY GATE (T5.4).
//!
//! For every fixture TOML, the `cru config migrate` emitter converts it to
//! Lua, and evaluating that Lua through the one-VM path (a throwaway
//! executor over the defaults layer, no TOML seed) extracts a config equal
//! to the oracle's (`CliAppConfig::load`): `serde_json::to_value` equality
//! PLUS `display_as_toml` byte equality.
//!
//! If this gate fails, the emitter is wrong. A fixture edit that turns it
//! green is the self-satisfying-gate failure the house rules name.

use crucible_core::config::{emit_lua_config, CliAppConfig, ConfigSource};
use std::path::Path;

/// The oracle: today's TOML-only reference implementation.
fn oracle(path: &Path) -> CliAppConfig {
    CliAppConfig::load(Some(path.to_path_buf()), None, None).expect("the oracle must load")
}

/// Convert one fixture through the emitter: the file's own keys, parsed
/// through the oracle's seed path (legacy rejections, include pass), then
/// rendered as one `cru.config.set({...})` chunk.
fn convert(path: &Path) -> String {
    let seed = CliAppConfig::load_seed_value(path).expect("the seed value must parse");
    emit_lua_config(&seed)
}

/// The one-VM path: defaults as layer 0, evaluate the Lua in a fresh
/// executor, extract from the store. No TOML seed — the Lua must carry
/// everything the file did.
fn evaluate_one_vm(lua_source: &str) -> CliAppConfig {
    crucible_lua::begin_boot_store();
    let defaults = serde_json::to_value(CliAppConfig::default()).expect("defaults serialize");
    crucible_lua::merge_app_config_tagged(defaults, ConfigSource::Default);

    let executor = crucible_lua::LuaExecutor::new().expect("executor");
    executor
        .lua()
        .load(lua_source)
        .exec()
        .expect("the emitted Lua must evaluate");

    let store = crucible_lua::snapshot_store().expect("the store is live");
    store.extract().expect("the merged store must extract")
}

/// Re-render one `display_as_toml` output with every table's keys sorted.
///
/// `CliAppConfig` holds `HashMap` fields (`llm.providers`, `kilns`,
/// `plugins`, ...), and a `HashMap`'s iteration order is arbitrary PER MAP —
/// the ORACLE's own rendering moves `[llm.providers.*]` blocks between runs.
/// Byte equality over the raw rendering is therefore unachievable against
/// the oracle itself; both sides are canonicalized identically and the byte
/// comparison runs over that. Everything else the rendering pins — value
/// formatting, path rendering, which keys render at all — is still pinned.
fn canonical_toml(rendered: &str) -> String {
    fn sort(value: toml::Value) -> toml::Value {
        match value {
            toml::Value::Table(table) => {
                let mut entries: Vec<(String, toml::Value)> =
                    table.into_iter().map(|(k, v)| (k, sort(v))).collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                toml::Value::Table(entries.into_iter().collect())
            }
            toml::Value::Array(items) => toml::Value::Array(items.into_iter().map(sort).collect()),
            scalar => scalar,
        }
    }
    let parsed: toml::Table = toml::from_str(rendered).expect("display_as_toml output re-parses");
    toml::to_string_pretty(&sort(toml::Value::Table(parsed))).expect("canonical form serializes")
}

/// The gate's equality: the serialized projection, then the rendered TOML
/// byte for byte over the canonical form. `source_map` is `#[serde(skip)]`
/// and therefore out by construction.
fn assert_configs_equal(oracle: &CliAppConfig, converted: &CliAppConfig, fixture: &str) {
    assert_eq!(
        serde_json::to_value(oracle).unwrap(),
        serde_json::to_value(converted).unwrap(),
        "fixture '{fixture}': the converted config's projection differs from the oracle's"
    );
    assert_eq!(
        canonical_toml(&oracle.display_as_toml().unwrap()),
        canonical_toml(&converted.display_as_toml().unwrap()),
        "fixture '{fixture}': the rendered TOML differs byte for byte"
    );
}

fn run_fixture(name: &str, toml: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, toml).unwrap();

    let expected = oracle(&path);
    let lua = convert(&path);
    let actual = evaluate_one_vm(&lua);
    assert_configs_equal(&expected, &actual, name);
}

/// The serde identity the defaults layer rests on: the JSON projection of
/// the default config round-trips through `from_value`.
#[test]
fn the_default_config_round_trips_through_its_json_projection() {
    let default = CliAppConfig::default();
    let value = serde_json::to_value(&default).unwrap();
    let back: CliAppConfig = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), value);
}

/// Fixture 1: no file at the given path — pure defaults on both sides.
#[test]
fn an_absent_file_converts_to_the_default_config() {
    let expected = CliAppConfig::default();
    let actual = evaluate_one_vm(&emit_lua_config(&serde_json::json!({})));
    assert_configs_equal(&expected, &actual, "absent");
}

/// Fixture 2: an empty file.
#[test]
fn an_empty_file_converts_losslessly() {
    run_fixture("empty", "");
}

/// Fixture 3: the shipped example.
#[test]
fn the_shipped_example_converts_losslessly() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    CliAppConfig::create_example(&path).expect("example writes");

    let expected = oracle(&path);
    let actual = evaluate_one_vm(&convert(&path));
    assert_configs_equal(&expected, &actual, "shipped-example");
}

/// Fixture 4: the kitchen sink — every top-level section, BOTH `KilnEntry`
/// shapes, two providers, one MCP server, free-form plugin keys.
const KITCHEN_SINK: &str = r#"
kiln_path = "/tmp/gate/kiln"
session_kiln = "/tmp/gate/sessions"
data_home = "/tmp/gate/data"
default_kiln = "notes"
agent_directories = ["/tmp/gate/agents", "cards"]
runtimepath = ["/tmp/gate/rtp-a", "~/rtp-b"]

[kilns]
notes = "/tmp/gate/notes"

[kilns.work]
path = "/tmp/gate/work"
lazy = true
auto = true

[projects.crucible]
path = "/tmp/gate/proj"
kilns = ["notes"]

[acp]
default_agent = "claude"
streaming_timeout_minutes = 20

[chat]
model = "sonnet"
endpoint = "http://localhost:1234"
temperature = 0.7
max_tokens = 2048
show_thinking = true

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"
temperature = 0.5

[llm.providers.cloud]
type = "anthropic"
default_model = "claude-sonnet"

[enrichment.provider]
type = "ollama"
model = "nomic-embed-text"
base_url = "http://localhost:11434"

[enrichment.pipeline]
max_precognition_chars = 900

[cli.highlighting]
enabled = false

[logging]
level = "debug"

[context]
rules_files = ["AGENTS.md", ".rules"]

[[mcp.servers]]
name = "files"
prefix = "files"

[mcp.servers.transport]
type = "stdio"
command = "mcp-files"
args = ["--root", "/tmp"]

[permissions]
default = "ask"
allow = ["read"]
deny = ["bash:rm *"]

[[schedules]]
name = "tick"
every = "1h"
action = "lua:print('hi')"
enabled = true

[plugins.myplug]
flag = true

[plugins.myplug.nested]
deep = 3

[web]
port = 9000

[server]
auto_archive_hours = 48

[workspace]
root_dir = "~/Projects"
"#;

#[test]
fn the_kitchen_sink_converts_losslessly() {
    run_fixture("kitchen-sink", KITCHEN_SINK);
}

/// Fixture 5: the include pass (`{file:...}` references) resolves BEFORE
/// conversion, so the emitted Lua carries the included values.
#[test]
fn a_file_reference_resolves_before_conversion() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("extra.toml"),
        "show_thinking = true\nmodel = \"included-model\"\n",
    )
    .unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(&path, "chat = \"{file:extra.toml}\"\n").unwrap();

    let expected = oracle(&path);
    assert_eq!(
        expected.chat.model.as_deref(),
        Some("included-model"),
        "the fixture must actually exercise the include pass"
    );
    let actual = evaluate_one_vm(&convert(&path));
    assert_configs_equal(&expected, &actual, "include");
}

/// Fixture 6: the three legacy-key rejections error on the CONVERSION path
/// with the same key name and the `cru doctor` pointer the oracle gives.
#[test]
fn the_legacy_key_rejections_survive_on_the_conversion_path() {
    let cases = [
        ("[embedding]\nprovider = \"openai\"\n", "[embedding]"),
        ("[providers.x]\ntype = \"ollama\"\n", "[providers]"),
        ("[chat]\nprovider = \"ollama\"\n", "chat.provider"),
    ];
    for (toml, key) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, toml).unwrap();

        let oracle_err = CliAppConfig::load(Some(path.clone()), None, None)
            .expect_err("the oracle rejects the legacy key")
            .to_string();
        let convert_err = CliAppConfig::load_seed_value(&path)
            .expect_err("the conversion path rejects the legacy key")
            .to_string();

        for err in [&oracle_err, &convert_err] {
            assert!(err.contains(key), "'{key}' missing from: {err}");
            assert!(err.contains("cru doctor"), "no doctor pointer in: {err}");
        }
    }
}
