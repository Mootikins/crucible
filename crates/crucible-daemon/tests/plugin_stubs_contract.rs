//! The type stubs must describe the VM plugins actually run on.
//!
//! This contract lives in `crucible-daemon` and not next to the generator,
//! because the generator's crate cannot build a plugin VM — and that gap is
//! exactly how the two drifted. `crucible-lua`'s own stub tests assert the
//! generator produces *something*; only this one asserts it produces the
//! truth.
//!
//! `stubs.rs` states the rule these enforce: "A stub for a nonexistent
//! function is worse than a stale doc: it looks authoritative."

use std::collections::BTreeSet;

use crucible_daemon::daemon_plugins::DaemonPluginLoader;

/// `---@class cru.X` lines from a freshly generated stub file, top-level only
/// (`cru.notify.messages` is a nested table, not a namespace a user reaches
/// for).
fn stubbed_namespaces(loader: &DaemonPluginLoader) -> BTreeSet<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    loader.generate_stubs(dir.path()).expect("generate stubs");
    let src = std::fs::read_to_string(dir.path().join("cru.lua")).expect("cru.lua");

    src.lines()
        .filter_map(|line| line.strip_prefix("---@class cru."))
        .map(str::trim)
        .filter(|name| !name.contains('.'))
        .map(str::to_string)
        .collect()
}

/// Tables actually hanging off `cru` on the plugin VM.
fn live_namespaces(loader: &DaemonPluginLoader) -> BTreeSet<String> {
    let lua = loader.plugin_lua();
    let cru: mlua::Table = lua.globals().get("cru").expect("cru global");
    cru.pairs::<String, mlua::Value>()
        .filter_map(|pair| {
            let (name, value) = pair.ok()?;
            matches!(value, mlua::Value::Table(_)).then_some(name)
        })
        .collect()
}

fn loader() -> DaemonPluginLoader {
    DaemonPluginLoader::new(Default::default()).expect("plugin loader")
}

/// Autocomplete must not offer a namespace that does not exist.
///
/// The generator ran against a throwaway executor and then called
/// `mirror_modules_into_cru`, which *fabricated* `cru.ask`, `cru.graph`,
/// `cru.hooks`, `cru.mcp`, `cru.notify` and `cru.session` out of bare globals
/// and `crucible.*` functions. None of the six are on the plugin VM: a plugin
/// author typing `cru.graph.` got completions for an API that is nil at
/// runtime.
#[test]
fn every_stubbed_namespace_exists_on_the_plugin_vm() {
    let loader = loader();
    let stubbed = stubbed_namespaces(&loader);
    let live = live_namespaces(&loader);

    let fabricated: Vec<_> = stubbed.difference(&live).collect();
    assert!(
        fabricated.is_empty(),
        "stubs advertise {} namespace(s) absent from the plugin VM: {fabricated:?}",
        fabricated.len()
    );
}

/// …and must not hide one that does.
///
/// The walk was a hardcoded `UNIVERSAL_MODULES` list, so every module the
/// daemon registers after it — `check config emitter errors health json log
/// schedule service shell storage ws` — got no stubs at all.
#[test]
fn every_plugin_vm_namespace_is_stubbed() {
    let loader = loader();
    let stubbed = stubbed_namespaces(&loader);
    let live = live_namespaces(&loader);

    let undocumented: Vec<_> = live.difference(&stubbed).collect();
    assert!(
        undocumented.is_empty(),
        "the plugin VM exposes {} namespace(s) with no stubs: {undocumented:?}",
        undocumented.len()
    );
}
