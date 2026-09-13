//! Activation: run a plugin's module once, bind what it exports, run its
//! entry's `config`. See `docs/Meta/CONTEXT.md`, "Activation".
//!
//! One body, whoever asks for it. The spec-driven pass at boot, a plugin
//! required from `init.lua`, a runtime install and a reload all end here.
//! The daemon VM is the only VM that runs plugin code.

use std::collections::HashMap;
use std::path::Path;

use mlua::{Function, LuaSerdeExt, RegistryKey, Table, Value};
use tracing::{debug, info, warn};

use super::boot::PluginBindings;
use super::resolve::{resolve_enabled, resolve_opts};
use super::{DaemonPluginLoader, PluginServiceFn};
use crucible_lua::manifest::PluginState;
use crucible_lua::LuaSource;

/// Callables extracted from a plugin's returned module table, live in the
/// daemon's Lua VM.
#[derive(Default)]
struct PluginExports {
    services: Vec<(String, Function)>,
    tools: HashMap<String, Function>,
    commands: HashMap<String, Function>,
}

/// Pull the service, tool and command `Function` handles out of a plugin's
/// module table.
fn extract_exports(module: &Table) -> PluginExports {
    let mut exports = PluginExports::default();
    if let Ok(svc_table) = module.get::<Table>("services") {
        for (name, entry) in svc_table.pairs::<String, Table>().flatten() {
            if let Ok(func) = entry.get::<Function>("fn") {
                exports.services.push((name, func));
            }
        }
    }
    for (field, target) in [
        ("tools", &mut exports.tools),
        ("commands", &mut exports.commands),
    ] {
        if let Ok(table) = module.get::<Table>(field) {
            for (name, entry) in table.pairs::<String, Table>().flatten() {
                if let Ok(func) = entry.get::<Function>("fn") {
                    target.insert(name, func);
                }
            }
        }
    }
    exports
}

/// Activate one discovered plugin.
///
/// Idempotent: a second call answers the module table the first call
/// produced. On any failure the plugin ends `Error` and inert, with the
/// declarations it managed to state kept for `plugin.list`.
///
/// The host runs the entry's `config` after `init.lua` has finished. When
/// the entry gives no `config`, the host calls the module's `setup(opts)`.
/// A user who writes `require("x").setup{ ... }` in `init.lua` calls setup
/// a second time, and theirs runs first. An operator who wants custom setup
/// writes `config = function(module, opts) ... end` in the entry, which
/// replaces the default call.
pub(super) async fn activate(loader: &mut DaemonPluginLoader, name: &str) -> anyhow::Result<Table> {
    let result = activate_inner(loader, name).await;
    if let Err(e) = &result {
        // A refusal for a disabled plugin is not a failure of the plugin.
        if loader.plugin_state(name) != Some(PluginState::Disabled) {
            // Adjacent, with no await between them: the loader mutex is what
            // makes the Error-but-still-registered window unobservable.
            loader.make_plugin_inert(name);
            loader.plugin_manager.mark_error(name, e.to_string());
        }
    }
    result
}

/// The ten steps, numbered in the order the code runs them. Each comment
/// names what the step decides.
async fn activate_inner(loader: &mut DaemonPluginLoader, name: &str) -> anyhow::Result<Table> {
    let lua = loader.executor.lua().clone();
    let plugin = loader
        .plugin_manager
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not discovered"))?;
    let init_path = plugin.main_path();
    let plugin_dir = plugin.dir.clone();
    let intercepts_tools = plugin.manifest.intercepts_tools;
    let manifest_opts = plugin.manifest.opts.clone();
    let declared_name = plugin.manifest.declared_name.clone();
    let state = plugin.state;

    // 1. Idempotent: an active plugin answers the table it already holds.
    if let Some(key) = loader.active_modules.get(name) {
        return Ok(lua.registry_value::<Table>(key)?);
    }

    // The operator's runtime `disable` holds until something enables the
    // plugin again. A reload of a disabled plugin is refused here.
    if state == PluginState::Disabled {
        anyhow::bail!("plugin '{name}' is disabled; enable it before activating it");
    }

    // The instance a boot `require` in `init.lua` created, when there is
    // one. Matched by FILE identity through the resolver's own record, so a
    // `require("x.init")` counts too, and a table a plugin put in
    // `package.loaded` itself does not.
    let boot_instance = boot_required_instance(loader, &init_path)?;

    // 2. `enabled`, first answer wins: the operator's entry, the config
    // leaf, the fragments, `true`. A boot-required plugin the spec disables
    // was asked for twice with two answers; the require was explicit, so it
    // activates, and the log names both sites.
    let spec = crucible_lua::spec_of(&lua);
    let section = loader.config_section(name, declared_name.as_deref());
    let leaf = section
        .as_ref()
        .and_then(|s| s.get("enabled"))
        .and_then(serde_json::Value::as_bool);
    if !resolve_enabled(&spec, name, leaf) {
        match boot_instance {
            Some(_) => warn!(
                "plugin '{name}' is disabled by its spec entry or its config section, but \
                 init.lua requires it; the require wins and the plugin activates. Remove one \
                 of the two."
            ),
            None => {
                if let Err(e) = loader.plugin_manager.disable(name) {
                    warn!("plugin '{name}' could not be marked disabled: {e}");
                }
                info!("plugin '{name}' is disabled; its code does not run");
                anyhow::bail!("plugin '{name}' is disabled");
            }
        }
    }

    // 3. This plugin's own `lua/` directory is resolvable while the guard
    // lives, and only while it lives, so one plugin's private `config`
    // module cannot answer another plugin's `require`.
    let _module_scope = loader
        .executor
        .enter_plugin_root(&plugin_dir)
        .map_err(|e| anyhow::anyhow!("enter plugin module root: {e}"))?;
    let bindings = PluginBindings {
        publications: loader.publications.clone(),
        options: loader.options.clone(),
    };

    // 4. The intercept grant the fragment declared, admitted here and
    // nowhere else. An unrecorded name answers "no". Recorded BEFORE the
    // body runs: a reload whose fragment dropped the grant must not keep the
    // previous generation's `true` while the body's async eval is under way.
    crucible_lua::record_plugin_intercept(&lua, name, intercepts_tools);

    let module: Table = match boot_instance {
        Some(instance) => {
            // The body already ran under the boot `require`, with its own
            // binding and its own source. Rebind WITHOUT releasing and do
            // not clear the source: the body's publications and
            // registrations are this instance's.
            bindings.bind(&lua, name)?;
            info!("Activated plugin '{name}' from the module instance init.lua required");
            instance
        }
        None => {
            // A reload must re-read this plugin's private modules.
            loader
                .executor
                .invalidate_private_modules_under(&plugin_dir)
                .map_err(|e| anyhow::anyhow!("invalidate plugin modules: {e}"))?;
            // Release-then-bind `cru.plugin.publish` and `cru.plugin.options`
            // to THIS plugin before its body runs: one VM serves every
            // plugin, and the loader knows who it is about to execute.
            bindings.rebind(&lua, name)?;
            // 5. A reload starts clean: the previous generation's handlers,
            // session hooks, permission hooks, auth hooks and schedules go.
            crucible_lua::clear_source(
                &lua,
                &loader.handler_registry,
                &LuaSource::Plugin(name.to_string()),
            );
            run_module(&lua, name, &init_path).await?
        }
    };

    // 7. Declarations first, then the callables. Read and remembered BEFORE
    // `config` runs, so a raise in `setup` keeps the declarations visible
    // beside `state: Error` with no error smuggling.
    let declarations = crucible_lua::spec_from_table(&module, &init_path)
        .map_err(|e| anyhow::anyhow!("spec of '{name}': {e}"))?;
    if !declarations.handlers.is_empty() {
        // Spec-table handlers are parsed for display but never dispatched;
        // `cru.on` is the working API. Say so instead of letting the
        // declaration look registered.
        warn!(
            "Plugin '{name}' declares {} spec-table handler(s), which are not dispatched; \
             register them with cru.on(...) instead",
            declarations.handlers.len(),
        );
    }
    loader.remember_spec(name, declarations.clone());
    let exports = extract_exports(&module);
    for (service, func) in exports.services {
        debug!("Extracted service function '{service}' from plugin '{name}'");
        loader.service_fns.push(PluginServiceFn {
            plugin: name.to_string(),
            service,
            func,
        });
    }
    loader.plugin_registry.register_plugin(
        name,
        &lua,
        &declarations.tools,
        &declarations.commands,
        exports.tools,
        exports.commands,
    );

    // 8. The table, and the lifecycle hooks it carries.
    loader
        .active_modules
        .insert(name.to_string(), lua.create_registry_value(module.clone())?);
    let hook_key = |field: &str| -> anyhow::Result<Option<RegistryKey>> {
        match module.get::<Value>(field)? {
            Value::Function(f) => Ok(Some(lua.create_registry_value(f)?)),
            _ => Ok(None),
        }
    };
    loader
        .plugin_manager
        .set_lifecycle_hooks(name, hook_key("on_load")?, hook_key("on_unload")?);

    // 9. `config`: the entry's function, else the module's `setup(opts)`,
    // else nothing. Under the plugin's source, so a handler `setup`
    // registers belongs to the plugin and a reload clears it.
    let opts = resolve_opts(
        &spec,
        name,
        &manifest_opts,
        section.as_ref().unwrap_or(&serde_json::Value::Null),
    );
    let opts = lua
        .to_value(&opts)
        .map_err(|e| anyhow::anyhow!("opts for '{name}': {e}"))?;
    let previous = crucible_lua::enter_plugin(&lua, name);
    let configured = run_config(&lua, name, &module, opts).await;
    crucible_lua::set_source(&lua, previous);
    configured?;

    // 10. Active, then `on_load`.
    loader.plugin_manager.mark_active(name);
    loader.plugin_manager.call_on_load_hook(&lua, name);
    Ok(module)
}

/// Step 6: evaluate the entry file under the plugin's source, and restore
/// the source on every exit path. A source left behind would attribute whatever
/// runs next to a dead plugin. The value must be a table.
async fn run_module(lua: &mlua::Lua, name: &str, init_path: &Path) -> anyhow::Result<Table> {
    // Read before entering the plugin context, so a read failure cannot
    // leave it behind.
    let source = std::fs::read_to_string(init_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", init_path.display()))?;
    let previous = crucible_lua::enter_plugin(lua, name);
    let evaluated: mlua::Result<Value> = lua
        .load(&source)
        .set_name(format!("@{}", init_path.display()))
        .eval_async()
        .await;
    crucible_lua::set_source(lua, previous);
    match evaluated {
        Err(e) => Err(anyhow::anyhow!("exec {}: {e}", init_path.display())),
        Ok(Value::Table(module)) => {
            info!("Executed plugin in daemon runtime: {}", init_path.display());
            Ok(module)
        }
        Ok(other) => Err(anyhow::anyhow!(
            "plugin '{name}' returned {}, not a table",
            other.type_name()
        )),
    }
}

/// Run the entry's `config(module, opts)`, else the module's `setup(opts)`.
async fn run_config(
    lua: &mlua::Lua,
    name: &str,
    module: &Table,
    opts: Value,
) -> anyhow::Result<()> {
    if let Some(config) = crucible_lua::config_of(lua, name) {
        config
            .call_async::<()>((module.clone(), opts))
            .await
            .map_err(|e| anyhow::anyhow!("config for '{name}': {e}"))?;
        debug!("Called the entry's config for plugin '{name}'");
        return Ok(());
    }
    if let Ok(setup) = module.get::<Function>("setup") {
        setup
            .call_async::<()>(opts)
            .await
            .map_err(|e| anyhow::anyhow!("setup() for '{name}': {e}"))?;
        debug!("Called setup() for plugin '{name}'");
    }
    Ok(())
}

/// The `package.loaded` instance a boot `require` created for this plugin's
/// entry file, when there is one. The resolver records which file answered
/// each public name, so the match is by file and not by name.
fn boot_required_instance(
    loader: &DaemonPluginLoader,
    init_path: &Path,
) -> anyhow::Result<Option<Table>> {
    let Some(module_name) = loader.boot_required_module(init_path) else {
        return Ok(None);
    };
    let loaded: Table = loader
        .executor
        .lua()
        .globals()
        .get::<Table>("package")
        .and_then(|package| package.get("loaded"))
        .map_err(|e| anyhow::anyhow!("package.loaded: {e}"))?;
    match loaded.get::<Value>(module_name.as_str())? {
        Value::Table(table) => Ok(Some(table)),
        _ => Ok(None),
    }
}
