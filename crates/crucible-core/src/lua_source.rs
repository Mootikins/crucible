//! Who defined a piece of Lua.
//!
//! One axis, and only one. [`LuaSource`] answers "who wrote this", and nothing
//! else reads it as a permission, a lifetime or a scope.
//!
//! # Why it lives in `crucible-core` rather than beside the VM
//!
//! Five consumers key on it, and only one of them is the VM:
//! handler registrations, timers, `cru.schedule` callbacks, `cru.storage`
//! namespaces, and the config store's own provenance
//! ([`crate::config::ConfigSource`], which embeds it). Filing it under
//! `config/` would name it for one consumer out of five, and leaving it in
//! `crucible-lua` would force `crucible-core` to depend on the VM crate to
//! describe where a config value came from.
//!
//! The half that needs `mlua` stays in `crucible-lua/src/plugin_context.rs`:
//! the ambient slot, the brackets that set it, and the recorded interception
//! declaration. That module re-exports this type, so a caller inside the VM
//! crate keeps one path to it.
//!
//! # It grants nothing
//!
//! This type carried a `may_intercept` method, which made a provenance tag
//! grant a capability. Neovim's `sctx_T` grants nothing anywhere, and neither
//! does this. The question "may this take a tool call over" is answered at
//! the one seam that gates it, `may_take_a_tool_call_over` in the daemon's
//! `agent_manager/messaging/tool_call.rs`.

use serde::{Deserialize, Serialize};

/// Who defined a piece of Lua.
///
/// Total by construction. Each variant names what it IS, not what it is not:
/// the whole defect this type removes is a value that carried more than one
/// meaning.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LuaSource {
    /// A plugin load. The name scopes `cru.storage` and attributes every
    /// registration the load makes.
    Plugin(String),
    /// The user's own `init.lua`, and what it includes.
    UserLua,
    /// `runtime/defaults/init.luau`, which ships with the daemon.
    Builtin,
    /// One `lua.eval` RPC call.
    Eval,
}

impl LuaSource {
    /// The plugin this source names, or `None` for every other source.
    ///
    /// `cru.storage` keys on this, so a non-plugin source has no namespace and
    /// the call is refused. The daemon's interception seam keys on it too: the
    /// recorded declaration table is keyed by plugin name, so a source that
    /// names no plugin can hold no declaration.
    #[must_use]
    pub fn plugin_name(&self) -> Option<&str> {
        match self {
            Self::Plugin(name) => Some(name.as_str()),
            Self::UserLua | Self::Builtin | Self::Eval => None,
        }
    }

    /// Whether this source is the operator's own code, as opposed to
    /// third-party code or a socket caller.
    ///
    /// `Plugin` is third-party, so its powers come from its declarations.
    /// `Eval` arrives over the daemon socket, so it is not the operator even
    /// though a human typed `cru lua`.
    #[must_use]
    pub fn is_operator(&self) -> bool {
        match self {
            Self::UserLua | Self::Builtin => true,
            Self::Plugin(_) | Self::Eval => false,
        }
    }
}

impl std::fmt::Display for LuaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plugin(name) => f.write_str(name),
            Self::UserLua => f.write_str("init.lua"),
            Self::Builtin => f.write_str("builtin"),
            Self::Eval => f.write_str("lua.eval"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plugin_source_names_a_plugin() {
        assert_eq!(
            LuaSource::Plugin("alpha".into()).plugin_name(),
            Some("alpha")
        );
        for source in [LuaSource::UserLua, LuaSource::Builtin, LuaSource::Eval] {
            assert_eq!(source.plugin_name(), None, "'{source}' named a plugin");
        }
    }

    /// The two sources the host itself loads from a trusted location are the
    /// operator's. A plugin is code the operator installed but did not write,
    /// and an eval is a socket call.
    #[test]
    fn the_operator_is_the_users_own_lua_and_the_shipped_defaults() {
        let operators: Vec<LuaSource> = [
            LuaSource::Plugin("alpha".into()),
            LuaSource::UserLua,
            LuaSource::Builtin,
            LuaSource::Eval,
        ]
        .into_iter()
        .filter(LuaSource::is_operator)
        .collect();
        assert_eq!(
            operators,
            vec![LuaSource::UserLua, LuaSource::Builtin],
            "an eval is a socket call, and a plugin is third-party code"
        );
    }

    /// Every source renders a distinct name. Two that shared one would be
    /// indistinguishable in `cru config show --sources` and in a log line.
    #[test]
    fn every_source_renders_its_own_name() {
        let names: Vec<String> = [
            LuaSource::Plugin("alpha".into()),
            LuaSource::UserLua,
            LuaSource::Builtin,
            LuaSource::Eval,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "two sources share a name");
        assert!(names.iter().all(|name| !name.is_empty()));
    }
}
