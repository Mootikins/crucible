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

/// Every `cru.*` function a plugin can reach in a gated namespace, with its
/// path.
///
/// Walked off the RUNNING VM, in both shapes a namespace takes: a table of
/// functions (`cru.fs`), and a function sitting directly on `cru`
/// (`cru.embed`). A hand-written list of gated paths would be satisfiable
/// without gating anything, which is the failure mode the closed-set rule
/// exists to prevent.
fn gated_functions(loader: &DaemonPluginLoader) -> Vec<(String, mlua::Function)> {
    let lua = loader.plugin_lua();
    let cru: mlua::Table = lua.globals().get("cru").expect("cru global");
    let is_gated = |key: &str| {
        CruNamespace::for_member("cru", key)
            .and_then(CruNamespace::required_capability)
            .is_some()
    };
    let mut found = Vec::new();
    for pair in cru.pairs::<String, mlua::Value>() {
        let (key, value) = pair.expect("cru entry");
        if !is_gated(&key) {
            continue;
        }
        match value {
            // A function on `cru` itself: its own name is the namespace.
            mlua::Value::Function(f) => found.push((format!("cru.{key}"), f)),
            mlua::Value::Table(table) => {
                for entry in table.pairs::<String, mlua::Value>() {
                    let (member, value) = entry.expect("namespace entry");
                    // Values (`cru.kiln.active`, the statusline items) are not
                    // calls and have nothing to gate.
                    if let mlua::Value::Function(f) = value {
                        found.push((format!("cru.{key}.{member}"), f));
                    }
                }
            }
            _ => {}
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// A plugin that declared no capabilities is refused by every gated function
/// on the VM — before its arguments are even converted.
///
/// This is the whole of step 4a in one assertion, and it is derived rather
/// than listed: adding a function to `cru.http` puts it in this walk, and
/// deleting the wrap in `Ns::func` fails it for every gated function at once.
#[tokio::test]
async fn a_plugin_with_no_grants_is_refused_by_every_gated_function() {
    let loader = plugin_vm();
    let lua = loader.plugin_lua();
    let functions = gated_functions(&loader);
    assert!(
        functions.len() > 40,
        "the walk found only {} gated functions, which means it stopped \
         seeing the VM rather than that the surface shrank",
        functions.len()
    );

    crucible_lua::enter_plugin(
        &lua,
        "declares-nothing",
        crucible_lua::manifest::CapabilitySet::none(),
    );

    for (path, func) in &functions {
        let outcome = func.call_async::<mlua::MultiValue>(()).await;
        let text = match outcome {
            Ok(_) => panic!("{path}: an ungranted plugin must be refused, not answered"),
            Err(e) => e.to_string(),
        };
        assert!(
            text.contains("did not declare"),
            "{path}: expected a capability refusal, got: {text}"
        );
        assert!(
            text.contains("declares-nothing"),
            "{path}: the refusal must name the plugin: {text}"
        );
    }
    crucible_lua::set_plugin_context(&lua, None);
}

/// …and the same functions are NOT refused for a plugin that declared the
/// grant. Without this the gate could pass the test above by refusing
/// everything unconditionally, which is a wall rather than a gate.
#[tokio::test]
async fn a_plugin_that_declared_the_grant_passes_the_gate() {
    let loader = plugin_vm();
    let lua = loader.plugin_lua();
    let functions = gated_functions(&loader);

    let everything: crucible_lua::manifest::CapabilitySet =
        <crucible_lua::manifest::Capability as strum::IntoEnumIterator>::iter().collect();
    crucible_lua::enter_plugin(&lua, "declares-everything", everything);

    for (path, func) in &functions {
        // Most raise for a missing argument, and that is the point: the call
        // reached the function's own body. What must not appear is the gate's
        // refusal.
        if let Err(e) = func.call_async::<mlua::MultiValue>(()).await {
            let text = e.to_string();
            assert!(
                !text.contains("did not declare"),
                "{path}: a declared grant must pass the gate, got: {text}"
            );
        }
    }
    crucible_lua::set_plugin_context(&lua, None);
}

/// Code with no plugin context is the operator's own — their `init.lua`, a
/// session VM, the compiled-in defaults — and the gate lets it through. A gate
/// that refused here would refuse the operator access to their own daemon.
#[tokio::test]
async fn code_outside_every_plugin_is_not_gated() {
    let loader = plugin_vm();
    let functions = gated_functions(&loader);

    for (path, func) in &functions {
        if let Err(e) = func.call_async::<mlua::MultiValue>(()).await {
            let text = e.to_string();
            assert!(
                !text.contains("did not declare"),
                "{path}: code with no plugin context must not be gated: {text}"
            );
        }
    }
}

/// Deferring a call must not launder authority.
///
/// `cru.timer.spawn` and `cru.schedule` hand a plugin's body to a detached
/// task, and a task carries no plugin context of its own. "No context" is how
/// the host spells the OPERATOR's authority, so a plugin that declared nothing
/// could reach any gated namespace by wrapping the call in a spawn — a
/// one-line bypass that no test of the gate itself could see.
#[tokio::test]
async fn a_spawned_task_carries_the_spawning_plugins_grants() {
    let loader = plugin_vm();
    let lua = loader.plugin_lua();

    crucible_lua::enter_plugin(
        &lua,
        "declares-nothing",
        crucible_lua::manifest::CapabilitySet::none(),
    );
    lua.load(
        r#"
        cru.timer.spawn(function()
            local ok, err = pcall(cru.shell.exec, "echo", { "escaped" })
            _G.spawned_ok = ok
            _G.spawned_err = tostring(err)
        end)
        "#,
    )
    .exec()
    .expect("the spawn itself is ungated");
    // The registering call has returned, so the context is gone — which is
    // exactly the state the detached task used to run in.
    crucible_lua::set_plugin_context(&lua, None);

    for _ in 0..200 {
        if lua.globals().get::<Option<bool>>("spawned_ok").unwrap() == Some(false) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert_eq!(
        lua.globals().get::<Option<bool>>("spawned_ok").unwrap(),
        Some(false),
        "the spawned body must have been refused, not run with the operator's authority"
    );
    let err: String = lua.globals().get("spawned_err").unwrap();
    assert!(
        err.contains("did not declare") && err.contains("declares-nothing"),
        "the refusal must name the spawning plugin: {err}"
    );
}
