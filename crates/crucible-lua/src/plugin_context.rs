//! Who a Lua registration belongs to.
//!
//! Three integrity-bearing markers used to live as ordinary Lua globals:
//! `cru._current_plugin` (the namespace every `cru.storage` call is scoped to),
//! `cru._current_plugin_may_intercept` (the right to replace a tool call's
//! execution) and `__crucible_loading_plugin__` (handler attribution). Lua code
//! could read and assign all three, so a plugin could take another plugin's
//! storage namespace, or grant itself interception rights, with one assignment
//! at the top of its own `init.lua`.
//!
//! The source lives in the VM's Rust-side app data instead. Lua cannot reach
//! it. The Lua registry is not an alternative either: it is a VM-internal
//! implementation detail, not a capability boundary.
//!
//! # One value carried three meanings, and nobody decided that
//!
//! The source used to be an `Option<PluginContext>`, and `None` meant three
//! different things at three different readers. The interception grant read
//! it as "trusted". The clear path skipped it, so an unowned registration
//! could never be removed. The config layer ranked it above `settings.json`
//! and pinned the leaf. That is correct for the user's own `init.lua`. It was
//! wrong for a `lua.eval` call that arrives over a socket.
//!
//! [`LuaSource`] is total, so a registration outside every group cannot exist.
//! Two of the three meanings are now two separate readers of one value:
//!
//! - **Lifecycle** — `clear_source` removes exactly one source's registrations.
//! - **Provenance** — [`LuaSource::config_layer`] is a total function. It answers
//!   `None` for every source that has a file, and the config store then
//!   classifies the write by the FILE that holds the call
//!   (`crate::authorship`). [`LuaSource::Eval`] has no file, so it answers the
//!   `Rpc` layer, which ranks highest and pins nothing.
//!
//! # Authority is NOT the third meaning, and this type does not answer it
//!
//! A source used to answer "may this intercept a tool call" through a
//! `may_intercept` method, which made a provenance tag grant a capability.
//! The answer now lives at the ONE seam that gates the power —
//! `may_take_a_tool_call_over` in `agent_manager/messaging/tool_call.rs` —
//! and that function carries the reasoning for each arm.
//!
//! What this module still owns is the DECLARATION a plugin makes. A plugin is
//! third-party code, so `intercepts_tools` in its own spec table
//! (`crate::lifecycle::spec`) is its boundary; a loader records it by name
//! with [`record_plugin_intercept`] and the seam reads it with
//! [`intercept_for`]. The recorded table, not the source, is what a plugin
//! cannot forge: it lives in the VM's Rust-side app data, which Lua cannot
//! reach, and only a loader writes it.
//!
//! The operator's own sources need no declaration and have none to make —
//! there is no spec table for `init.lua`. The seam admits them by trust root
//! instead, and says why there rather than here.
//!
//! # The bracket sites
//!
//! The host assigns the source; a plugin cannot name its own. Every bracket is
//! a place a plugin's code runs after its body has returned:
//!
//! 1. The plugin loader, around a plugin's execution, so the plugin's own body
//!    runs under its identity.
//! 2. The handler dispatcher, around each registered handler call, from the
//!    source the registry recorded at registration. A per-plugin rebind of the
//!    shared `cru.storage` table cannot do this work: all plugins share one
//!    `cru` table, so the last rebind would win for every late caller.
//! 3. The session-lifecycle fire paths, around each `session:start` and
//!    `session:end` hook.
//! 4. Plugin command dispatch, and 5. plugin tool dispatch.
//! 6. `cru.schedule` and `cru.timer.spawn`, around each deferred callback.
//! 7. The user config loader ([`LuaSource::UserLua`]), the shipped defaults loader
//!    ([`LuaSource::Builtin`]) and the `lua.eval` RPC ([`LuaSource::Eval`]).
//!
//! Missing one of them is not a cosmetic gap. It attributes that code to
//! whatever source the VM last held, and hands it that source's authority.
//!
//! Only [`LuaSource::Plugin`] has a storage namespace, and `cru.storage` refuses
//! every other source, as it always has.
//!
//! # The session is ambient for the same reason the source is
//!
//! A registration can name the session it fires for
//! ([`Scope`](crate::handlers::Scope)). The host resolves that id, never the
//! caller — see [`current_session`] for the reason and for the sites that
//! bracket it.

use mlua::Lua;

/// Who defined a piece of Lua.
///
/// The type itself lives in `crucible-core` — see
/// [`crucible_core::lua_source`] for why — because the config store's own
/// provenance embeds it and `crucible-core` must not depend on this crate.
/// Re-exported here so a caller inside the VM crate keeps one path to it.
pub use crucible_core::lua_source::LuaSource;

/// The app-data slot. A newtype so the stored value is exactly one [`LuaSource`].
struct CurrentOwner(LuaSource);

/// Install `source` as the current source; return what it replaced.
///
/// Callers MUST restore the previous value on every exit path, error paths
/// included — an source left behind attributes whatever runs next to the wrong
/// author, up to and including the user's `init.lua`.
///
/// A VM that has entered no bracket answers [`LuaSource::UserLua`]. Every other
/// source reaches the VM through a bracket the host installs, so the code
/// running outside all of them is the host's own or the user's own.
pub fn set_source(lua: &Lua, source: LuaSource) -> LuaSource {
    lua.set_app_data(CurrentOwner(source))
        .map_or(LuaSource::UserLua, |previous| previous.0)
}

/// The source a Lua call runs under.
pub fn current_source(lua: &Lua) -> LuaSource {
    lua.app_data_ref::<CurrentOwner>()
        .map_or(LuaSource::UserLua, |current| current.0.clone())
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
/// 1. [`LuaSource::Eval`] exists because a socket call is not the operator. If a
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
/// the source:
///
/// 1. The handler dispatcher, from the session the dispatch site named. This
///    covers `cru.on` and every turn-loop stage.
/// 2. The `session:start` and `session:end` fire paths, from the session
///    handle they already hold. `session:start` is the natural place to
///    activate a workflow plugin for one session.
/// 3. The permission gate, from the session whose turn asked.
///
/// 4. Plugin TOOL dispatch, from the turn that called the tool.
///    `ExecutionContext::session_id` carries it.
///
/// Plugin COMMAND dispatch holds no session: the `plugin.run_command` RPC
/// carries none, so nothing reaches it to bracket. A command therefore cannot
/// register a scoped handler, and answers a registration error rather than
/// taking an id the caller chose.
/// # A known race, and what it costs
///
/// The ambient source and the ambient session each live in ONE slot of VM app
/// data. Every seam sets the slot, awaits the plugin's Lua, then restores it.
/// Nothing serialises dispatch, so two sessions running in this one VM
/// interleave at any await point, and the second overwrites the slot the first
/// is still inside.
///
/// So a handler can read a session id that is not the one it fires for. That
/// matters most in [`crate::handlers::registry`], where the registration path
/// reads this function as its CHECK that a caller may not name another
/// session: under interleaving the check can resolve the wrong id.
///
/// This is not fixed. It is written down because a reader of the scope
/// machinery will otherwise assume the slot is safe. The shape of a fix is a
/// task-local value rather than VM app data, or a dispatch lock — and the
/// second costs concurrency the daemon currently has.
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

/// What each loaded plugin DECLARED, by name.
///
/// The declaration is `intercepts_tools` in the plugin's own spec table. It
/// reaches the daemon as `PluginManifest::intercepts_tools`, and a loader
/// records it here at the moment it admits the plugin.
///
/// It lives in VM app data for the reason the source does: Lua cannot reach
/// it, so a plugin cannot grant itself the right at call time. It is keyed by
/// NAME because the seam that gates on it holds a registration whose source
/// names a plugin and nothing more.
#[derive(Default)]
struct PluginIntercepts(std::collections::HashMap<String, bool>);

/// Record whether `name`'s installation declares that it intercepts tools.
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

/// Enter `name`'s source; return what it replaced, for the caller to restore.
///
/// It records nothing. A loader that admits a plugin calls
/// [`record_plugin_intercept`] separately, because entering a source and
/// admitting a declaration are two different acts. The seams that hold a name
/// and nothing else — a lifecycle hook, a plugin command, a plugin tool —
/// enter the source and re-admit nothing.
pub fn enter_plugin(lua: &Lua, name: &str) -> LuaSource {
    set_source(lua, LuaSource::Plugin(name.to_string()))
}

/// The name of the plugin a call runs under, or `None` under every other
/// source.
pub fn current_plugin_name(lua: &Lua) -> Option<String> {
    current_source(lua).plugin_name().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vm_outside_every_bracket_is_the_users_own_lua() {
        let lua = Lua::new();
        assert_eq!(current_source(&lua), LuaSource::UserLua);
        assert_eq!(current_plugin_name(&lua), None);
    }

    #[test]
    fn setting_an_owner_returns_the_previous_one() {
        let lua = Lua::new();
        assert_eq!(
            set_source(&lua, LuaSource::Plugin("alpha".into())),
            LuaSource::UserLua
        );
        assert_eq!(
            set_source(&lua, LuaSource::Plugin("beta".into())),
            LuaSource::Plugin("alpha".into())
        );
        assert_eq!(current_plugin_name(&lua), Some("beta".to_string()));
    }

    /// The defect this type removes: an eval used to be indistinguishable
    /// from the user's own `init.lua`, so it read as the operator.
    #[test]
    fn an_eval_names_no_plugin() {
        let lua = Lua::new();
        set_source(&lua, LuaSource::Eval);
        assert_eq!(current_plugin_name(&lua), None);
    }

    /// Only a loader's record answers the interception question. A name the
    /// loader never admitted answers `false`, so a plugin the host does not
    /// know cannot gain the right by being unknown.
    #[test]
    fn only_a_recorded_declaration_admits_interception() {
        let lua = Lua::new();
        record_plugin_intercept(&lua, "quiet", false);
        record_plugin_intercept(&lua, "loud", true);

        assert!(!intercept_for(&lua, "quiet"));
        assert!(intercept_for(&lua, "loud"));
        assert!(!intercept_for(&lua, "stranger"));
    }

    /// A reload re-records, so an edit to the declaration takes effect — and
    /// a plugin that drops the declaration loses the right.
    #[test]
    fn a_reload_re_records_the_declaration_in_both_directions() {
        let lua = Lua::new();
        record_plugin_intercept(&lua, "oci", true);
        assert!(intercept_for(&lua, "oci"));
        record_plugin_intercept(&lua, "oci", false);
        assert!(!intercept_for(&lua, "oci"));
    }

    /// Entering a source records no declaration. A declaration is a thing a
    /// loader admits, never a thing entering a source implies — otherwise the
    /// seams that enter a source holding only a name (a lifecycle hook, a
    /// plugin command, a plugin tool) would each re-decide it.
    ///
    /// The list is the whole enum, so a new source cannot arrive already
    /// admitted.
    #[test]
    fn entering_a_source_admits_no_declaration() {
        for source in [
            LuaSource::Plugin("alpha".into()),
            LuaSource::UserLua,
            LuaSource::Builtin,
            LuaSource::Eval,
        ] {
            let lua = Lua::new();
            set_source(&lua, source.clone());
            // The name every seam would key on, for the one source that has
            // one, plus the two names a non-plugin source renders as.
            for name in ["alpha", &source.to_string()] {
                assert!(
                    !intercept_for(&lua, name),
                    "'{source}' admitted '{name}' without a loader recording it"
                );
            }
        }
    }

    /// `enter_plugin` used to record the declaration as a side effect of
    /// entering. It must not: a seam that holds only a name — a lifecycle
    /// hook, a plugin command, a plugin tool — enters the source, and an
    /// entering-records rule let the last such seam overwrite what the loader
    /// admitted.
    #[test]
    fn entering_a_plugin_does_not_overwrite_what_the_loader_recorded() {
        let lua = Lua::new();
        record_plugin_intercept(&lua, "oci", true);
        enter_plugin(&lua, "oci");
        assert!(
            intercept_for(&lua, "oci"),
            "entering the plugin must not clear its declaration"
        );
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
    /// registry-backed source.
    #[test]
    fn lua_cannot_reach_the_owner_through_the_registry() {
        let lua = unsafe {
            Lua::unsafe_new_with(
                mlua::StdLib::ALL_SAFE | mlua::StdLib::DEBUG,
                mlua::LuaOptions::default(),
            )
        };
        enter_plugin(&lua, "grabby");

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
            "the source must not be reachable from Lua"
        );
    }

    /// The recorded declaration is the value a plugin must not be able to
    /// forge, so it must be as unreachable from Lua as the source is.
    #[test]
    fn lua_cannot_reach_the_recorded_declaration_through_the_registry() {
        let lua = unsafe {
            Lua::unsafe_new_with(
                mlua::StdLib::ALL_SAFE | mlua::StdLib::DEBUG,
                mlua::LuaOptions::default(),
            )
        };
        record_plugin_intercept(&lua, "grabby", true);

        let found = lua
            .load(
                r#"
                for _, value in pairs(debug.getregistry()) do
                    if type(value) == "table" then
                        for k, v in pairs(value) do
                            if k == "grabby" or v == "grabby" then
                                return true
                            end
                        end
                    end
                end
                return false
                "#,
            )
            .eval::<bool>();
        assert!(
            found.is_err() || !found.expect("a successful registry walk returns a boolean"),
            "the recorded declaration must not be reachable from Lua"
        );
    }
}
