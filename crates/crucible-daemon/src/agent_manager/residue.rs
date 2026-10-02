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
    /// the former global sequence counters came to leak an entry per session
    /// while sitting outside every cleanup test.
    ///
    /// It cannot detect state added *outside* `AgentManager`; the seq counters
    /// are covered by the supplied bus, which at least puts the
    /// question inside the one function a reviewer reads.
    pub(crate) fn session_residue(
        &self,
        session_id: &str,
        events: &crate::EventBus,
    ) -> Vec<&'static str> {
        let Self {
            // Per-session: every one of these must be empty after cleanup.
            request_state,
            slots,
            snapshots,
            review,
            active_tools,
            proposals,

            // Not per-session, or per-call rather than per-session. Each name
            // here is a decision, not an oversight.
            title_attempts: _, // terminal memo survives pause, like persisted eligibility
            titles_in_flight: _, // InFlightGuard owns its lifetime (title.rs:13)
            model_cache: _,    // keyed by provider classification, not session
            agent_profiles_cache: _, // unkeyed catalog cache, not session
            providers_cache: _, // keyed by call params, not session
            runtimepath: _,    // daemon config
            modes: _,          // global mode registry
            kiln_manager: _,   // shared service
            session_manager: _, // shared service
            background_manager: _, // shared service
            delegation_service: _, // shared service; per-session records are its own
            mcp_gateway: _,    // shared service
            llm_config: _,     // daemon config
            acp_config: _,     // daemon config
            context_config: _, // daemon config
            permission_config: _, // daemon config
            source_roots: _,   // daemon config
            plugin_loader: _,  // shared service
            plugin_handlers: _, // startup-bound OnceLock
            isolation: _,      // startup-bound OnceLock
            context_attach: _, // process-wide buffer, drained per turn
            // Session-keyed, and swept by `SessionLifecycle::stop` rather
            // than here: a pause sweeps it and runs no `cleanup_session`, and
            // the sweep must follow the plugin end hooks. Asserting here would
            // fire for any caller of `cleanup_session` that fired no end hooks.
            statusline_exprs: _,
            status: _, // startup-bound OnceLock; the registry releases its own sessions
            notifications: _, // startup-bound OnceLock; the hub keeps its own ring
            no_kiln_noticed: _, // daemon-wide, keyed by workspace, not by session
            plugin_tool_registry: _, // startup-bound OnceLock
            external_watch: _, // startup-bound OnceLock; per-session watches are its own
            agent_factory_override: _, // test-support seam, set once
            activity: _, // daemon-wide work registry; a turn's guard lives in request_state
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
        if proposals.has_turn(session_id) {
            residue.push("proposals");
        }
        if active_tools.has_session(session_id) {
            residue.push("active_tools");
        }
        if events.has_seq_counter(session_id) {
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
    /// It asserts on [`leaked_stores`] rather than the raw residue: two of the
    /// stores can be re-populated by a task that is still running, and this
    /// check has no way to wait for one.
    pub(super) fn debug_assert_no_residue(
        &self,
        session_id: &str,
        review_spawned: bool,
        events: &crate::EventBus,
    ) {
        let leaked = leaked_stores(self.session_residue(session_id, events), review_spawned);
        debug_assert!(
            leaked.is_empty(),
            "cleanup_session left per-session state behind: {leaked:?}"
        );
    }
}

/// The residue that is a genuine leak, as opposed to the residue a still-running
/// task can put back after teardown released it.
///
/// Two stores are dropped, and both for the same reason: asserting on them is
/// asserting on a race, not on this manager's bookkeeping.
///
/// - `review`, when the harvest was spawned. A delegated child's ledger cannot
///   have cleared by the time the check runs — the harvest is ordered, and
///   therefore spawned — so asserting would fire for every delegated child.
/// - `seq_counters`, always. `forget_session` is called *last* precisely
///   because "the spawns above can still emit, and a re-created counter is
///   cheaper than a duplicate `seq`" — so a re-minted counter is the documented,
///   intended outcome, not a leak. It is also reachable well outside teardown's
///   own spawns: a session's startup status burst is emitted from its own task,
///   and a trailing `providers_listed` landing between `forget_session` and this
///   check re-mints the counter. That turned an ordinary `session.end` into a
///   debug-build panic answering INTERNAL_ERROR, about once in ten.
///
/// Dropping it here costs no coverage. The counter is still checked by
/// [`AgentManager::session_residue`], which
/// `cleanup_session_leaves_no_per_session_residue` asserts empty with nothing
/// else running, and its growth bound is asserted directly by
/// `ending_sessions_does_not_grow_the_seq_counter_map`.
fn leaked_stores(mut residue: Vec<&'static str>, review_spawned: bool) -> Vec<&'static str> {
    residue.retain(|store| match *store {
        "seq_counters" => false,
        "review" => !review_spawned,
        _ => true,
    });
    residue
}

#[cfg(test)]
mod tests {
    use super::leaked_stores;

    /// The seq counter is the one store whose presence after teardown proves
    /// nothing: any task still emitting for the session re-mints it, which
    /// `forget_session` documents as harmless. Everything else is a leak.
    #[test]
    fn a_re_minted_seq_counter_is_not_a_leak() {
        assert_eq!(
            leaked_stores(vec!["seq_counters"], false),
            Vec::<&str>::new()
        );
        assert_eq!(
            leaked_stores(vec!["slots", "seq_counters"], false),
            ["slots"]
        );
    }

    /// The pre-existing carve-out, kept: a delegated child's ledger clears a
    /// beat after the spawned harvest, but an *unspawned* one had its chance.
    #[test]
    fn a_spawned_review_harvest_is_not_a_leak_but_a_synchronous_one_is() {
        assert_eq!(leaked_stores(vec!["review"], true), Vec::<&str>::new());
        assert_eq!(leaked_stores(vec!["review"], false), ["review"]);
    }
}
