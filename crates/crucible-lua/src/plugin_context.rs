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
//!    owner and grants the handler registry recorded at registration. A
//!    per-plugin rebind of the shared `cru.storage` table cannot do this work:
//!    all plugins share one `cru` table, so the last rebind would win for every
//!    late caller.
//! 3. The session-lifecycle fire paths, around each `on_session_start` and
//!    `on_session_end` hook, from the owner the hook table records.
//! 4. Plugin command dispatch, and 5. plugin tool dispatch.
//! 6. `cru.schedule` and `cru.timer.spawn`, around each deferred callback.
//!
//! Missing any of them is not a cosmetic gap. An ABSENT context means no
//! plugin is running — the user's own `init.lua`, or a session VM — and that
//! code carries the OPERATOR's authority: it may intercept, and it passes
//! every capability gate. So a seam that forgot to re-enter its plugin handed
//! that plugin more authority than its manifest granted, not less. Three of
//! the six above were added for exactly that reason.
//!
//! An absent context still has no storage namespace, and `cru.storage`
//! refuses it, as it always has.

use crate::manifest::{Capability, CapabilitySet};
use mlua::Lua;

/// The plugin a Lua call runs under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginContext {
    /// The plugin's name. Scopes `cru.storage` and attributes registrations.
    pub name: String,
    /// What this plugin's installation granted it.
    ///
    /// Decided by the manifest the operator installed, never by the plugin:
    /// the set is stamped here at load and read at each call, so a plugin has
    /// no assignment that widens it. Every gated `cru.*` namespace is checked
    /// against this set — see `CruNamespace::required_capability`.
    pub grants: CapabilitySet,
}

impl PluginContext {
    /// Whether this plugin may take a tool call over — return
    /// `{ handled = true, … }` or a transform from `pre_tool_call`.
    ///
    /// One bit of [`Self::grants`], named because the tool-call seam reads it
    /// on its own. `cancel` needs no grant: refusing a call can only narrow.
    pub fn may_intercept(&self) -> bool {
        self.grants.holds(Capability::InterceptTools)
    }
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
/// An unrecorded name answers with NO grants, not with every grant: a plugin
/// the loader never admitted must not gain authority by being unknown.
#[derive(Default)]
struct PluginGrants(std::collections::HashMap<String, CapabilitySet>);

/// Record what `name`'s installation granted it.
///
/// Called by the loaders, which read the manifest the operator installed.
/// Idempotent — a reload re-records, so a manifest edit takes effect.
pub fn record_plugin_grants(lua: &Lua, name: &str, grants: CapabilitySet) {
    let mut recorded = lua.remove_app_data::<PluginGrants>().unwrap_or_default();
    recorded.0.insert(name.to_string(), grants);
    lua.set_app_data(recorded);
}

/// What `name`'s installation granted it, or nothing for a name the loader
/// never recorded.
pub fn grants_for(lua: &Lua, name: &str) -> CapabilitySet {
    lua.app_data_ref::<PluginGrants>()
        .and_then(|recorded| recorded.0.get(name).cloned())
        .unwrap_or_default()
}

/// Enter `name`'s context; return what it replaced, for the caller to restore.
///
/// Shorthand for [`set_plugin_context`] with a fresh [`PluginContext`]. It
/// also RECORDS the grants, so a later seam holding only the name can re-enter
/// the same authority.
pub fn enter_plugin(lua: &Lua, name: &str, grants: CapabilitySet) -> Option<PluginContext> {
    record_plugin_grants(lua, name, grants.clone());
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            grants,
        }),
    )
}

/// Enter `name`'s context with the grants the loader recorded for it.
///
/// For the seams that hold a name and nothing else: a lifecycle hook's owner,
/// a plugin command's owner.
pub fn enter_recorded_plugin(lua: &Lua, name: &str) -> Option<PluginContext> {
    let grants = grants_for(lua, name);
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            grants,
        }),
    )
}

/// Enter `name`'s recorded context with `cap` dropped from it.
///
/// One caller, and the narrowing is the point: a plugin COMMAND runs under its
/// plugin's grants minus `intercept_tools`. It does not re-record, so the
/// narrowing applies to this call and not to the plugin.
pub fn enter_recorded_plugin_without(
    lua: &Lua,
    name: &str,
    cap: Capability,
) -> Option<PluginContext> {
    let grants = grants_for(lua, name).without(cap);
    set_plugin_context(
        lua,
        Some(PluginContext {
            name: name.to_string(),
            grants,
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
    current_plugin_context(lua).is_none_or(|context| context.may_intercept())
}

/// Refuse a call into a gated `cru.*` namespace that the running plugin's
/// installation did not grant.
///
/// **An ABSENT context is allowed, and that is the whole design.** No context
/// means no plugin is running: the user's own `init.lua`, a session VM, the
/// compiled-in defaults. That code carries the operator's authority, and
/// gating it would refuse the operator access to their own daemon. What is
/// gated is a PLUGIN, whose authority is exactly what its manifest declared.
///
/// The message names the plugin, the function, the missing grant and what to
/// add, because the first reader of this error is a plugin author who wrote
/// correct code and forgot one manifest line.
pub fn require_capability(lua: &Lua, cap: Capability, path: &str) -> mlua::Result<()> {
    let Some(context) = current_plugin_context(lua) else {
        return Ok(());
    };
    if context.grants.holds(cap) {
        return Ok(());
    }
    let held = context.grants.names();
    let held = if held.is_empty() {
        "none".to_string()
    } else {
        held.join(", ")
    };
    Err(mlua::Error::runtime(format!(
        "{path}: plugin '{}' did not declare the '{cap}' capability \
         (it declared: {held}). Add `{cap}` to its manifest capabilities.",
        context.name
    )))
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
            grants: CapabilitySet::none(),
        };
        assert_eq!(set_plugin_context(&lua, Some(alpha.clone())), None);
        let beta = PluginContext {
            name: "beta".to_string(),
            grants: [Capability::InterceptTools].into_iter().collect(),
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
                grants: CapabilitySet::none(),
            }),
        );

        let found = lua
            .load(
                r#"
                for _, value in pairs(debug.getregistry()) do
                    if type(value) == "table" and value.grants ~= nil then
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

    /// The gate's default. Code with no plugin context is the operator's own,
    /// and gating it would refuse the operator access to their own daemon.
    #[test]
    fn an_absent_context_holds_every_capability() {
        let lua = Lua::new();
        for cap in <Capability as strum::IntoEnumIterator>::iter() {
            require_capability(&lua, cap, "cru.probe.fn")
                .unwrap_or_else(|e| panic!("an absent context must hold '{cap}': {e}"));
        }
    }

    /// …and a plugin holds exactly what it declared, no more.
    #[test]
    fn a_plugin_holds_only_what_it_declared() {
        let lua = Lua::new();
        enter_plugin(&lua, "narrow", [Capability::Network].into_iter().collect());
        require_capability(&lua, Capability::Network, "cru.http.get").expect("declared");

        let err = require_capability(&lua, Capability::Shell, "cru.shell.exec")
            .expect_err("an undeclared capability must be refused");
        let text = err.to_string();
        // The author reading this wrote correct code and forgot one manifest
        // line, so the message must name all four of these.
        for expected in ["cru.shell.exec", "narrow", "shell", "network"] {
            assert!(text.contains(expected), "{expected:?} missing from: {text}");
        }
    }
}
