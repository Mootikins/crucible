//! Which plugin is running, and what authority it holds.
//!
//! Three integrity-bearing markers used to live as ordinary Lua globals:
//! `cru._current_plugin` (the namespace every `cru.storage` call is scoped to),
//! `cru._current_plugin_may_intercept` (the right to replace a tool call's
//! execution) and `__crucible_loading_plugin__` (handler attribution). Lua code
//! could read and assign all three, so a plugin could take another plugin's
//! storage namespace, or grant itself interception rights, with one assignment
//! at the top of its own `init.lua`.
//!
//! The context lives in the VM's Rust-side app data instead. Lua cannot reach
//! it. The Lua registry is NOT an alternative here: the plugin VM opens
//! `StdLib::DEBUG` for Fennel, so `debug.getregistry()` hands Lua every
//! registry entry, named entries included.
//!
//! Two writers bracket the context:
//!
//! 1. The plugin loader, around a plugin's execution, so the plugin's own body
//!    runs under its identity.
//! 2. The handler dispatcher, around each handler call, from the owner and
//!    grant the handler registry recorded when the handler registered. A
//!    per-plugin rebind of the shared `cru.storage` table cannot do this work:
//!    all plugins share one `cru` table, so the last rebind would win for every
//!    late caller.
//!
//! An ABSENT context means no plugin is running — the user's own `init.lua`,
//! or a session VM. That code carries the operator's authority, so it may
//! intercept; it has no storage namespace, and `cru.storage` refuses it, as it
//! always has.

use mlua::Lua;

/// The plugin a Lua call runs under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginContext {
    /// The plugin's name. Scopes `cru.storage` and attributes registrations.
    pub name: String,
    /// Whether this plugin may take a tool call over — return
    /// `{ handled = true, … }` or a transform from `pre_tool_call`.
    ///
    /// Decided by the operator's installation (the `intercept_tools`
    /// capability), never by the plugin.
    pub may_intercept: bool,
}

/// The app-data slot. A newtype so the `Option` is the whole stored value:
/// `remove_app_data` cannot express "present but empty", and the loader must
/// be able to restore an absent context.
struct CurrentPlugin(Option<PluginContext>);

/// Install `context` as the current plugin context; return what it replaced.
///
/// Callers MUST restore the previous value on every exit path, error paths
/// included — a context left behind attributes whatever runs next to the wrong
/// plugin, up to and including the user's `init.lua`.
pub fn set_plugin_context(lua: &Lua, context: Option<PluginContext>) -> Option<PluginContext> {
    lua.set_app_data(CurrentPlugin(context))
        .and_then(|previous| previous.0)
}

/// Enter `name`'s context; return what it replaced, for the caller to restore.
///
/// Shorthand for [`set_plugin_context`] with a fresh [`PluginContext`].
pub fn enter_plugin(lua: &Lua, name: &str, may_intercept: bool) -> Option<PluginContext> {
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            may_intercept,
        }),
    )
}

/// The plugin a call runs under, or `None` outside every plugin.
pub fn current_plugin_context(lua: &Lua) -> Option<PluginContext> {
    lua.app_data_ref::<CurrentPlugin>()
        .and_then(|current| current.0.clone())
}

/// The name of the plugin a call runs under, or `None` outside every plugin.
pub fn current_plugin_name(lua: &Lua) -> Option<String> {
    current_plugin_context(lua).map(|context| context.name)
}

/// Whether the code that runs now may replace a tool call's execution.
///
/// No context means no plugin: the user's own configuration, which holds the
/// operator's authority. A LOADING plugin holds only what its installation
/// granted.
pub fn current_may_intercept(lua: &Lua) -> bool {
    current_plugin_context(lua).is_none_or(|context| context.may_intercept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_context_is_trusted_and_has_no_namespace() {
        let lua = Lua::new();
        assert_eq!(current_plugin_name(&lua), None);
        assert!(current_may_intercept(&lua));
    }

    #[test]
    fn setting_a_context_returns_the_previous_one() {
        let lua = Lua::new();
        let alpha = PluginContext {
            name: "alpha".to_string(),
            may_intercept: false,
        };
        assert_eq!(set_plugin_context(&lua, Some(alpha.clone())), None);
        let beta = PluginContext {
            name: "beta".to_string(),
            may_intercept: true,
        };
        assert_eq!(set_plugin_context(&lua, Some(beta)), Some(alpha));
        assert_eq!(current_plugin_name(&lua), Some("beta".to_string()));
        assert!(current_may_intercept(&lua));
    }

    /// The whole point of the app-data slot: Lua has no path to it. The plugin
    /// VM runs with the DEBUG library, so `debug.getregistry` would expose a
    /// registry-backed context.
    #[test]
    fn lua_cannot_reach_the_context_through_the_registry() {
        let lua = unsafe {
            Lua::unsafe_new_with(
                mlua::StdLib::ALL_SAFE | mlua::StdLib::DEBUG,
                mlua::LuaOptions::default(),
            )
        };
        set_plugin_context(
            &lua,
            Some(PluginContext {
                name: "grabby".to_string(),
                may_intercept: false,
            }),
        );

        let found: bool = lua
            .load(
                r#"
                for _, value in pairs(debug.getregistry()) do
                    if type(value) == "table" and value.may_intercept ~= nil then
                        return true
                    end
                end
                return false
                "#,
            )
            .eval()
            .expect("walking the registry succeeds");
        assert!(!found, "the plugin context must not be reachable from Lua");
        assert!(!current_may_intercept(&lua));
    }
}
