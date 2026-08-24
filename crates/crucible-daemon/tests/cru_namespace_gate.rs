//! The top-level `cru` namespace is a closed, gated set.
//!
//! Per the closed-set rule in AGENTS.md: one enumerated table
//! ([`crucible_lua::namespace::CruNamespace`]), completeness proved by walking
//! `strum::EnumIter`, with the expectation derived from the RUNNING plugin VM
//! rather than from source text. A namespace added to the VM without a
//! variant — or a variant whose name is nowhere on the VM — fails here.

use std::collections::BTreeSet;

use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_lua::namespace::CruNamespace;

/// The plugin VM as production boots it: `DaemonPluginLoader::new` plus the
/// UI-config registration `plugin_boot` performs on the same VM.
fn plugin_vm() -> DaemonPluginLoader {
    let loader = DaemonPluginLoader::new(Default::default()).expect("plugin loader");
    crucible_lua::config::register_ui_namespaces(&loader.plugin_lua()).expect("ui namespaces");
    loader
}

/// Every top-level key on the live `cru` table.
///
/// No exemption by prefix. The two loader-internal markers that used to earn
/// one (`_current_plugin`, `_current_plugin_may_intercept`) were forgeable
/// authority globals and now live in Rust-side app data, so any key here is
/// API and must have a variant.
fn live_names(loader: &DaemonPluginLoader) -> BTreeSet<String> {
    let lua = loader.plugin_lua();
    let cru: mlua::Table = lua.globals().get("cru").expect("cru global");
    cru.pairs::<String, mlua::Value>()
        .filter_map(|pair| pair.ok().map(|(name, _)| name))
        .collect()
}

#[test]
fn the_plugin_vm_exposes_exactly_the_declared_namespaces() {
    let loader = plugin_vm();
    let live = live_names(&loader);
    let declared: BTreeSet<String> = <CruNamespace as strum::IntoEnumIterator>::iter()
        .filter(|v| v.on_plugin_vm())
        .map(|v| v.name().to_string())
        .collect();

    let undeclared: Vec<_> = live.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "the plugin VM exposes {} name(s) not declared in CruNamespace \
         (add a variant and state its VM placement): {undeclared:?}",
        undeclared.len()
    );

    let missing: Vec<_> = declared.difference(&live).collect();
    assert!(
        missing.is_empty(),
        "CruNamespace declares {} name(s) as on the plugin VM that the \
         running VM does not have: {missing:?}",
        missing.len()
    );
}

/// The `crucible` global is deleted. A plugin that still indexes it must get
/// Lua's ordinary "attempt to index a nil value", not a stale alias.
#[test]
fn the_crucible_global_does_not_exist_on_the_plugin_vm() {
    let loader = plugin_vm();
    let lua = loader.plugin_lua();
    let value: mlua::Value = lua.globals().get("crucible").expect("read global");
    assert!(
        matches!(value, mlua::Value::Nil),
        "the crucible global must not exist, found: {value:?}"
    );
}
