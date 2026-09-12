//! Who a Lua registration belongs to, and what authority that owner holds.
//!
//! Three integrity-bearing markers used to live as ordinary Lua globals:
//! `cru._current_plugin` (the namespace every `cru.storage` call is scoped to),
//! `cru._current_plugin_may_intercept` (the right to replace a tool call's
//! execution) and `__crucible_loading_plugin__` (handler attribution). Lua code
//! could read and assign all three, so a plugin could take another plugin's
//! storage namespace, or grant itself interception rights, with one assignment
//! at the top of its own `init.lua`.
//!
//! The owner lives in the VM's Rust-side app data instead. Lua cannot reach
//! it. The Lua registry is not an alternative either: it is a VM-internal
//! implementation detail, not a capability boundary.
//!
//! # One value carried three meanings, and nobody decided that
//!
//! The owner used to be an `Option<PluginContext>`, and `None` meant three
//! different things at three different readers. The interception grant read
//! it as "trusted". The clear path skipped it, so an unowned registration
//! could never be removed. The config layer ranked it above `settings.json`
//! and pinned the leaf. That is correct for the user's own `init.lua`. It was
//! wrong for a `lua.eval` call that arrives over a socket.
//!
//! [`Owner`] is total, so a registration outside every group cannot exist. The
//! three meanings are now three separate readers of one value:
//!
//! - **Lifecycle** — `clear_owner` removes exactly one owner's registrations.
//! - **Authority** — [`Owner::may_intercept`] is a total function.
//! - **Provenance** — [`Owner::config_layer`] is a total function. It answers
//!   `None` for every owner that has a file, and the config store then
//!   classifies the write by the FILE that holds the call
//!   (`crate::authorship`). [`Owner::Eval`] has no file, so it answers the
//!   `Rpc` layer, which ranks highest and pins nothing.
//!
//! # The bracket sites
//!
//! The host assigns the owner; a plugin cannot name its own. Every bracket is
//! a place a plugin's code runs after its body has returned:
//!
//! 1. The plugin loader, around a plugin's execution, so the plugin's own body
//!    runs under its identity.
//! 2. The handler dispatcher, around each registered handler call, from the
//!    owner the registry recorded at registration. A per-plugin rebind of the
//!    shared `cru.storage` table cannot do this work: all plugins share one
//!    `cru` table, so the last rebind would win for every late caller.
//! 3. The session-lifecycle fire paths, around each `session:start` and
//!    `session:end` hook.
//! 4. Plugin command dispatch, and 5. plugin tool dispatch.
//! 6. `cru.schedule` and `cru.timer.spawn`, around each deferred callback.
//! 7. The user config loader ([`Owner::UserLua`]), the shipped defaults loader
//!    ([`Owner::Builtin`]) and the `lua.eval` RPC ([`Owner::Eval`]).
//!
//! Missing one of them is not a cosmetic gap. It attributes that code to
//! whatever owner the VM last held, and hands it that owner's authority.
//!
//! Only [`Owner::Plugin`] has a storage namespace, and `cru.storage` refuses
//! every other owner, as it always has.
//!
//! # The session is ambient for the same reason the owner is
//!
//! A registration can name the session it fires for
//! ([`Scope`](crate::handlers::Scope)). The host resolves that id, never the
//! caller — see [`current_session`] for the reason and for the sites that
//! bracket it.

use mlua::Lua;

/// Who a Lua registration belongs to.
///
/// Total by construction. Each variant names what it IS, not what it is not:
/// the whole defect this type removes is a value that carried more than one
/// meaning.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
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

impl Owner {
    /// The plugin this owner names, or `None` for every other owner.
    ///
    /// `cru.storage` keys on this, so a non-plugin owner has no namespace and
    /// the call is refused.
    #[must_use]
    pub fn plugin_name(&self) -> Option<&str> {
        match self {
            Self::Plugin(name) => Some(name.as_str()),
            Self::UserLua | Self::Builtin | Self::Eval => None,
        }
    }

    /// Whether code running under this owner may take a tool call over —
    /// return `{ handled = true, … }` or a transform from `pre_tool_call`.
    ///
    /// Total, with no wildcard arm: a new owner must name its own answer.
    ///
    /// - [`Self::Plugin`] reads what the operator installed, which the loader
    ///   recorded by name. An unrecorded name answers `false`: a plugin the
    ///   loader never admitted must not gain authority by being unknown.
    /// - [`Self::UserLua`] and [`Self::Builtin`] are the operator's own code
    ///   and hold the operator's authority.
    /// - [`Self::Eval`] is `false` for CONSISTENCY, not for security. An eval
    ///   already runs arbitrary code on this VM, so the answer protects
    ///   nothing; it keeps one rule — authority belongs to an installation —
    ///   true of every owner.
    ///
    /// `cancel` needs no grant from any owner: refusing a call can only
    /// narrow.
    #[must_use]
    pub fn may_intercept(&self, lua: &Lua) -> bool {
        match self {
            Self::Plugin(name) => intercept_for(lua, name),
            Self::UserLua | Self::Builtin => true,
            Self::Eval => false,
        }
    }

    /// The config layer a `cru.config.set` under this owner lands in, when the
    /// owner decides it — and `None` when the FILE decides it.
    ///
    /// Total, with no wildcard arm, and `None` is the common answer on
    /// purpose. `crate::authorship` argues the general rule correctly: the file
    /// that holds the call is the honest signal, because
    /// `require("alpha").setup{}` written by the user runs alpha's file with no
    /// plugin context installed, and a plugin that calls its own `setup` from a
    /// handler runs with one. So [`Self::Plugin`], [`Self::UserLua`] and
    /// [`Self::Builtin`] all answer `None` and let the path classification
    /// speak.
    ///
    /// [`Self::Eval`] is the one owner with NO file. Its chunk name is
    /// `=lua.eval`, which matches no config root and no plugin root, so the
    /// path classification fell back to
    /// [`crucible_core::config::SourceTag::Lua`] — a layer that PINS. One
    /// `cru lua 'cru.config.set{…}'` then made `config.save` refuse that leaf
    /// for the rest of the daemon's life, and the settings UI reported the
    /// value as a line a human wrote in a file that does not exist.
    ///
    /// The answer is [`crucible_core::config::SourceTag::Rpc`], and no new
    /// layer is needed, because `Rpc` already means exactly "set at run time,
    /// not written in a file": it ranks highest, so an eval may override
    /// anything for this run; it pins nothing; and a `config.save` drops it. An
    /// eval is a socket call, which is what the `config.set` RPC is, so the two
    /// land on one layer.
    #[must_use]
    pub fn config_layer(&self) -> Option<crucible_core::config::SourceTag> {
        match self {
            Self::Plugin(_) | Self::UserLua | Self::Builtin => None,
            Self::Eval => Some(crucible_core::config::SourceTag::Rpc),
        }
    }
}

impl std::fmt::Display for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plugin(name) => f.write_str(name),
            Self::UserLua => f.write_str("init.lua"),
            Self::Builtin => f.write_str("builtin"),
            Self::Eval => f.write_str("lua.eval"),
        }
    }
}

/// The app-data slot. A newtype so the stored value is exactly one [`Owner`].
struct CurrentOwner(Owner);

/// Install `owner` as the current owner; return what it replaced.
///
/// Callers MUST restore the previous value on every exit path, error paths
/// included — an owner left behind attributes whatever runs next to the wrong
/// author, up to and including the user's `init.lua`.
///
/// A VM that has entered no bracket answers [`Owner::UserLua`]. Every other
/// owner reaches the VM through a bracket the host installs, so the code
/// running outside all of them is the host's own or the user's own.
pub fn set_owner(lua: &Lua, owner: Owner) -> Owner {
    lua.set_app_data(CurrentOwner(owner))
        .map_or(Owner::UserLua, |previous| previous.0)
}

/// The owner a Lua call runs under.
pub fn current_owner(lua: &Lua) -> Owner {
    lua.app_data_ref::<CurrentOwner>()
        .map_or(Owner::UserLua, |current| current.0.clone())
}

/// The app-data slot holding the session the host is inside.
struct CurrentSessionId(String);

/// The session whose work runs now, or `None` outside every session.
///
/// # Why the host resolves the id and a plugin never writes one
///
/// `cru.on("pre_tool_call", { session = id }, h)` checks `id` against this
/// value and refuses anything else. Three reasons, and the first is the one
/// that matters:
///
/// 1. [`Owner::Eval`] exists because a socket call is not the operator. If a
///    literal id were taken as written, one `lua.eval` could put a
///    `pre_tool_call` handler on a session it merely NAMES — somebody else's
///    turn, intercepted by a caller that never held the session. That is the
///    exact harm a session scope exists to prevent, arriving through the
///    scope itself.
/// 2. `docs/Meta/Analysis/The Plugin Contract.md` states the neighbouring
///    rule for publication scopes: "a scope binding resolves server-side …
///    A client never writes an id it chose."
/// 3. A literal nobody checks cannot be wrong out loud. A typo would register
///    a handler that never fires, silently — which is the failure
///    `resolve_hook_name` already refuses for a misspelt hook name.
///
/// # The bracket sites
///
/// Every path that runs a plugin's code inside a session sets this, beside
/// the owner:
///
/// 1. The handler dispatcher, from the session the dispatch site named. This
///    covers `cru.on` and every turn-loop stage.
/// 2. The `session:start` and `session:end` fire paths, from the session
///    handle they already hold. `session:start` is the natural place to
///    activate a workflow plugin for one session.
/// 3. The permission gate, from the session whose turn asked.
///
/// Plugin command dispatch and plugin tool dispatch hold NO session: the
/// `plugin.run_command` RPC carries none, so nothing reaches them to bracket.
/// A command therefore cannot register a scoped handler yet, and answers a
/// registration error rather than taking an id the caller chose.
pub fn current_session(lua: &Lua) -> Option<String> {
    lua.app_data_ref::<CurrentSessionId>()
        .map(|current| current.0.clone())
}

/// Install `session` as the session the host is inside; answer what it
/// replaced.
///
/// `None` REMOVES the slot rather than leaving the previous id behind: a
/// sessionless dispatch that inherited a stale id would let a scoped
/// registration fire outside its session, which is the whole harm.
fn set_current_session(lua: &Lua, session: Option<String>) -> Option<String> {
    let previous = lua
        .remove_app_data::<CurrentSessionId>()
        .map(|current| current.0);
    if let Some(id) = session {
        lua.set_app_data(CurrentSessionId(id));
    }
    previous
}

/// Enter `session` for as long as the guard lives.
///
/// A guard rather than a set-then-restore pair, because the dispatch paths
/// that need it return early with `?`: a `Drop` cannot forget the restore.
#[must_use = "the session is restored when the guard drops"]
pub struct SessionGuard<'lua> {
    lua: &'lua Lua,
    previous: Option<String>,
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        set_current_session(self.lua, self.previous.take());
    }
}

/// Bracket `session` around the caller's work. See [`current_session`].
pub fn enter_session<'lua>(lua: &'lua Lua, session: Option<&str>) -> SessionGuard<'lua> {
    let previous = set_current_session(lua, session.map(str::to_string));
    SessionGuard { lua, previous }
}

/// What each loaded plugin's installation granted it, by name.
///
/// Several seams hold a plugin NAME and nothing else — a registration's owner,
/// the plugin a command belongs to — and each must re-enter that plugin's
/// authority when it runs. Threading the grant set through all of them would
/// have put four copies of one fact in the process. The loader records it
/// once, here, and the seams read it by name.
#[derive(Default)]
struct PluginIntercepts(std::collections::HashMap<String, bool>);

/// Record whether `name`'s installation lets it intercept.
///
/// Called by the loaders, which read what the operator installed. Idempotent —
/// a reload re-records, so an edit to the declaration takes effect.
pub fn record_plugin_intercept(lua: &Lua, name: &str, may_intercept: bool) {
    let mut recorded = lua
        .remove_app_data::<PluginIntercepts>()
        .unwrap_or_default();
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

/// Enter `name`'s ownership; return what it replaced, for the caller to
/// restore.
///
/// It also RECORDS the interception bit, so a later seam holding only the name
/// re-enters the same authority.
pub fn enter_plugin(lua: &Lua, name: &str, may_intercept: bool) -> Owner {
    record_plugin_intercept(lua, name, may_intercept);
    set_owner(lua, Owner::Plugin(name.to_string()))
}

/// Enter `name`'s ownership with the interception bit the loader recorded.
///
/// For the seams that hold a name and nothing else: a lifecycle hook's owner,
/// a plugin command's owner, a plugin tool's owner.
pub fn enter_recorded_plugin(lua: &Lua, name: &str) -> Owner {
    set_owner(lua, Owner::Plugin(name.to_string()))
}

/// The name of the plugin a call runs under, or `None` under every other
/// owner.
pub fn current_plugin_name(lua: &Lua) -> Option<String> {
    current_owner(lua).plugin_name().map(str::to_string)
}

/// Whether the code that runs now may replace a tool call's execution.
pub fn current_may_intercept(lua: &Lua) -> bool {
    current_owner(lua).may_intercept(lua)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vm_outside_every_bracket_is_the_users_own_lua() {
        let lua = Lua::new();
        assert_eq!(current_owner(&lua), Owner::UserLua);
        assert_eq!(current_plugin_name(&lua), None);
        assert!(current_may_intercept(&lua));
    }

    #[test]
    fn setting_an_owner_returns_the_previous_one() {
        let lua = Lua::new();
        assert_eq!(
            set_owner(&lua, Owner::Plugin("alpha".into())),
            Owner::UserLua
        );
        assert_eq!(
            set_owner(&lua, Owner::Plugin("beta".into())),
            Owner::Plugin("alpha".into())
        );
        assert_eq!(current_plugin_name(&lua), Some("beta".to_string()));
    }

    /// The defect this type removes: an eval used to be indistinguishable
    /// from the user's own `init.lua`, so it read as the operator.
    #[test]
    fn an_eval_may_not_intercept_and_names_no_plugin() {
        let lua = Lua::new();
        set_owner(&lua, Owner::Eval);
        assert!(!current_may_intercept(&lua));
        assert_eq!(current_plugin_name(&lua), None);
    }

    /// Exactly one owner answers the provenance question, and the layer it
    /// names is the one a `config.save` can overwrite.
    ///
    /// The properties are DERIVED from the layer rather than restated: a
    /// reordering of the layers in `crucible-core` fails this test instead of
    /// silently giving an eval the power to lock a key.
    #[test]
    fn only_the_owner_with_no_file_names_its_own_config_layer() {
        use crucible_core::config::SourceTag;

        let deciders: Vec<Owner> = [
            Owner::Plugin("alpha".into()),
            Owner::UserLua,
            Owner::Builtin,
            Owner::Eval,
        ]
        .into_iter()
        .filter(|owner| owner.config_layer().is_some())
        .collect();
        assert_eq!(
            deciders,
            vec![Owner::Eval],
            "an owner with a file must let the file decide, or a plugin's \
             `setup()` called from the user's own `init.lua` is misfiled"
        );

        let layer = Owner::Eval.config_layer().expect("an eval names its layer");
        assert_eq!(layer, SourceTag::Rpc);
        assert_eq!(
            layer.pin(),
            None,
            "an eval holds no file, so its write must not refuse a later \
             `config.save`"
        );
        assert!(
            layer.reset_drops(),
            "a save drops the layer it can overwrite, and this is that layer"
        );
        assert!(
            layer.rank() > SourceTag::Settings.rank(),
            "an eval sets a value for this run, so it must outrank what is saved"
        );
        assert_eq!(
            layer.origin().file,
            None,
            "the settings UI must not report an eval as a line in a file"
        );
    }

    #[test]
    fn a_plugin_reads_the_grant_the_loader_recorded() {
        let lua = Lua::new();
        enter_plugin(&lua, "quiet", false);
        assert!(!current_may_intercept(&lua));
        enter_plugin(&lua, "loud", true);
        assert!(current_may_intercept(&lua));
        // A name the loader never admitted holds no authority.
        assert!(!Owner::Plugin("stranger".into()).may_intercept(&lua));
    }

    #[test]
    fn the_shipped_defaults_hold_the_operators_authority() {
        let lua = Lua::new();
        set_owner(&lua, Owner::Builtin);
        assert!(current_may_intercept(&lua));
        assert_eq!(current_plugin_name(&lua), None);
    }

    #[test]
    fn a_vm_outside_every_session_is_in_no_session() {
        let lua = Lua::new();
        assert_eq!(current_session(&lua), None);
    }

    #[test]
    fn a_session_guard_restores_what_it_replaced() {
        let lua = Lua::new();
        {
            let _outer = enter_session(&lua, Some("outer"));
            assert_eq!(current_session(&lua), Some("outer".to_string()));
            {
                let _inner = enter_session(&lua, Some("inner"));
                assert_eq!(current_session(&lua), Some("inner".to_string()));
            }
            assert_eq!(current_session(&lua), Some("outer".to_string()));
        }
        assert_eq!(current_session(&lua), None);
    }

    /// A sessionless dispatch must not inherit the session of whatever ran
    /// before it. A scoped registration would then fire outside its session.
    #[test]
    fn entering_no_session_clears_the_previous_one() {
        let lua = Lua::new();
        let _outer = enter_session(&lua, Some("s1"));
        {
            let _sessionless = enter_session(&lua, None);
            assert_eq!(current_session(&lua), None);
        }
        assert_eq!(current_session(&lua), Some("s1".to_string()));
    }

    /// The whole point of the app-data slot: Lua has no path to it. The plugin
    /// VM runs with the DEBUG library, so `debug.getregistry` would expose a
    /// registry-backed owner.
    #[test]
    fn lua_cannot_reach_the_owner_through_the_registry() {
        let lua = unsafe {
            Lua::unsafe_new_with(
                mlua::StdLib::ALL_SAFE | mlua::StdLib::DEBUG,
                mlua::LuaOptions::default(),
            )
        };
        enter_plugin(&lua, "grabby", false);

        let found = lua
            .load(
                r#"
                for _, value in pairs(debug.getregistry()) do
                    if type(value) == "string" and value == "grabby" then
                        return true
                    end
                end
                return false
                "#,
            )
            .eval::<bool>();
        assert!(
            found.is_err() || !found.expect("a successful registry walk returns a boolean"),
            "the owner must not be reachable from Lua"
        );
        assert!(!current_may_intercept(&lua));
    }
}
