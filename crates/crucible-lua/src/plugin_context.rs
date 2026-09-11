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
//! it. The Lua registry is not an alternative either: it is a VM-internal
//! implementation detail, not a capability boundary.
//!
//! Six writers bracket the context, and every one of them is a place a
//! plugin's code runs after its body has returned:
//!
//! 1. The plugin loader, around a plugin's execution, so the plugin's own body
//!    runs under its identity.
//! 2. The handler dispatcher, around each `cru.on` handler call, from the
//!    owner the handler registry recorded at registration. A
//!    per-plugin rebind of the shared `cru.storage` table cannot do this work:
//!    all plugins share one `cru` table, so the last rebind would win for every
//!    late caller.
//! 3. The session-lifecycle fire paths, around each `on_session_start` and
//!    `on_session_end` hook, from the owner the hook table records.
//! 4. Plugin command dispatch, and 5. plugin tool dispatch.
//! 6. `cru.schedule` and `cru.timer.spawn`, around each deferred callback.
//!
//! Missing any of them is not a cosmetic gap. An ABSENT context means no
//! plugin is running — the user's own `init.lua`, or the host itself — and
//! that code carries the OPERATOR's authority: it may intercept. So a seam
//! that forgot to re-enter its plugin ATTRIBUTED that plugin's work to the
//! operator, and handed it more authority than it declared. Three of the six
//! above were added for exactly that reason.
//!
//! An absent context still has no storage namespace, and `cru.storage`
//! refuses it, as it always has.

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
    /// declaration), never by the plugin: the bit is stamped here at load, so
    /// a plugin has no assignment that widens it. `cancel` needs no grant:
    /// refusing a call can only narrow.
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

/// What each loaded plugin's installation granted it, by name.
///
/// Several seams hold a plugin NAME and nothing else — a session-lifecycle
/// hook's owner, a runtime handler's owner, the plugin a command belongs to —
/// and each must re-enter that plugin's authority when it runs. Threading the
/// grant set through all three would have put four copies of one fact in the
/// process. The loader records it once, here, and the seams read it by name.
///
/// An unrecorded name answers with NO interception right, not with every
/// right: a plugin the loader never admitted must not gain authority by being
/// unknown.
#[derive(Default)]
struct PluginIntercepts(std::collections::HashMap<String, bool>);

/// Record whether `name`'s installation lets it intercept.
///
/// Called by the loaders, which read what the operator installed. Idempotent —
/// a reload re-records, so an edit to the declaration takes effect.
pub fn record_plugin_intercept(lua: &Lua, name: &str, may_intercept: bool) {
    let mut recorded = lua.remove_app_data::<PluginIntercepts>().unwrap_or_default();
    recorded.0.insert(name.to_string(), may_intercept);
    lua.set_app_data(recorded);
}

/// Whether `name`'s installation lets it intercept, or `false` for a name the
/// loader never recorded.
pub fn intercept_for(lua: &Lua, name: &str) -> bool {
    lua.app_data_ref::<PluginIntercepts>()
        .and_then(|recorded| recorded.0.get(name).copied())
        .unwrap_or(false)
}

/// Enter `name`'s context; return what it replaced, for the caller to restore.
///
/// Shorthand for [`set_plugin_context`] with a fresh [`PluginContext`]. It
/// also RECORDS the interception bit, so a later seam holding only the name
/// can re-enter the same authority.
pub fn enter_plugin(lua: &Lua, name: &str, may_intercept: bool) -> Option<PluginContext> {
    record_plugin_intercept(lua, name, may_intercept);
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            may_intercept,
        }),
    )
}

/// Enter `name`'s context with the interception bit the loader recorded.
///
/// For the seams that hold a name and nothing else: a lifecycle hook's owner,
/// a plugin command's owner.
pub fn enter_recorded_plugin(lua: &Lua, name: &str) -> Option<PluginContext> {
    let may_intercept = intercept_for(lua, name);
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            may_intercept,
        }),
    )
}

/// Enter `name`'s recorded context with the interception right dropped.
///
/// Two callers, and the narrowing is the point: a plugin COMMAND and a plugin
/// TOOL run under their plugin's name without interception, because neither is
/// a tool-call hook and neither has interception to do. It does not re-record,
/// so the narrowing applies to this call and not to the plugin.
pub fn enter_recorded_plugin_without_intercept(lua: &Lua, name: &str) -> Option<PluginContext> {
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            may_intercept: false,
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

        let found = lua
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
            .eval::<bool>();
        assert!(
            found.is_err() || !found.expect("a successful registry walk returns a boolean"),
            "the plugin context must not be reachable from Lua"
        );
        assert!(!current_may_intercept(&lua));
    }
}
