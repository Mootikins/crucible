//! The post-`cleanup_session` invariant, in one function the compiler polices.
//!
//! Lives beside `AgentManager` rather than inside `mod.rs` because it names
//! every private field: a child module can see its parent's privates, so the
//! exhaustive destructure below works from here.

use super::AgentManager;

impl AgentManager {
    /// Every per-session store this manager owns that still holds something
    /// for `session_id`. Empty is the post-[`AgentManager::cleanup_session`]
    /// invariant.
    ///
    /// Destructured exhaustively with **no `..`** on purpose: adding a field to
    /// `AgentManager` fails to compile here until the author has classified it
    /// as per-session (add a check) or not (bind it to `_` with a reason). That
    /// compile error is the whole mechanism — a runtime list of maps is a list
    /// someone forgets to extend, which is exactly how
    /// `event_emitter::SESSION_SEQ_COUNTERS` came to leak an entry per session
    /// while sitting outside every cleanup test.
    ///
    /// It cannot detect state added *outside* `AgentManager`; the seq counters
    /// are covered by calling into their module, which at least puts the
    /// question inside the one function a reviewer reads.
    pub(crate) fn session_residue(&self, session_id: &str) -> Vec<&'static str> {
        let Self {
            // Per-session: every one of these must be empty after cleanup.
            request_state,
            slots,
            snapshots,
            review,

            // Not per-session, or per-call rather than per-session. Each name
            // here is a decision, not an oversight.
            titles_in_flight: _, // InFlightGuard owns its lifetime (title.rs:13)
            model_cache: _,      // keyed by provider classification, not session
            runtimepath: _,      // daemon config
            session_defaults: _, // global Lua defaults tier
            modes: _,            // global mode registry
            kiln_manager: _,     // shared service
            session_manager: _,  // shared service
            background_manager: _, // shared service
            delegation_service: _, // shared service; per-session records are its own
            mcp_gateway: _,      // shared service
            llm_config: _,       // daemon config
            acp_config: _,       // daemon config
            context_config: _,   // daemon config
            permission_config: _, // daemon config
            plugin_loader: _,    // shared service
            tool_dispatcher: _,  // the fallback dispatcher, not a per-session one
            lua_validators: _,   // startup-bound OnceLock
            plugin_handlers: _,  // startup-bound OnceLock
            isolation: _,        // startup-bound OnceLock
            context_attach: _,   // process-wide buffer, drained per turn
            statusline_exprs: _, // process-wide expression values
            plugin_tool_registry: _, // startup-bound OnceLock
            external_watch: _,   // startup-bound OnceLock; per-session watches are its own
            agent_factory_override: _, // test-support seam, set once
        } = self;

        let mut residue = Vec::new();
        if request_state.contains_key(session_id) {
            residue.push("request_state");
        }
        if slots.contains_key(session_id) {
            residue.push("slots");
        }
        if !snapshots.is_empty_for(session_id) {
            residue.push("snapshots");
        }
        if review.has_session(session_id) {
            residue.push("review");
        }
        if crate::event_emitter::has_seq_counter(session_id) {
            residue.push("seq_counters");
        }
        residue
    }

    /// The invariant [`AgentManager::cleanup_session`] exists to maintain,
    /// checked where it is established.
    ///
    /// `debug_assert!` expands to `if cfg!(debug_assertions)`, so
    /// [`Self::session_residue`]'s exhaustive destructure is type-checked in
    /// release too — the compile-time half of the mechanism holds in every
    /// profile, and every debug-profile test that ends a session exercises the
    /// runtime half for free.
    ///
    /// A delegated child's review teardown is the one step that cannot have
    /// finished by the time this runs: the harvest is ordered and therefore
    /// spawned, so its entries clear a beat later. Asserting on it would fire
    /// for every delegated child rather than for a real leak.
    pub(super) fn debug_assert_no_residue(&self, session_id: &str, review_spawned: bool) {
        let mut residue = self.session_residue(session_id);
        if review_spawned {
            residue.retain(|store| *store != "review");
        }
        debug_assert!(
            residue.is_empty(),
            "cleanup_session left per-session state behind: {residue:?}"
        );
    }
}
