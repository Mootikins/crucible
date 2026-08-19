//! Per-session active tool sets — what `cru.tools.set_active` writes.
//!
//! A plugin narrows the tools one session offers:
//!
//! ```lua
//! cru.tools.set_active(ctx.session_id, { "read_*", "grep_notes" })
//! local names = cru.tools.get_active(ctx.session_id)  -- the patterns above
//! cru.tools.set_active(ctx.session_id, nil)           -- back to automatic
//! ```
//!
//! ## The rule: an active set only ever narrows
//!
//! The set is applied inside
//! [`visible_tools`](crate::provider::genai_handle) after the mode filter and
//! before the tool-schema budget check. Three consequences, each deliberate:
//!
//! 1. **A tool the mode already removed stays removed.** `set_active` cannot
//!    re-add a write tool in plan mode, or a tool a declared mode's `tools`
//!    selector excluded. The floor belongs to the operator; a plugin may only
//!    cut below it. Widening from Lua would let third-party code hand itself
//!    what the operator's mode refused.
//! 2. **Narrowing usually removes the reason to defer.** Progressive tool
//!    disclosure (`TOOL_SCHEMA_BUDGET_SHARE`) triggers on the estimated token
//!    cost of the attached schemas, and that estimate is computed over the
//!    narrowed set — so a small active set simply stays under the threshold
//!    and nothing is deferred.
//! 3. **If the narrowed set is still over budget, deferral still runs.** An
//!    active set is not an override of the budget. Deferral does not remove
//!    capability — deferred tools stay callable through `discover_tools` /
//!    `invoke_tool` — so the two controls do not contradict each other: the
//!    active set decides *which* tools a session has, deferral decides *how*
//!    they are presented. Letting an explicit set suppress the budget check
//!    would let a plugin silently spend a session's whole context window on
//!    tool schemas, with no way for the daemon to recover.
//!
//! ## Enforced at dispatch too, not only in the advertisement
//!
//! [`ActiveToolSets::dispatch_refusal`] is asked before the daemon executes a
//! tool call. Filtering only the advertised set would leave every excluded
//! tool callable by a model that names it anyway — the mirror image of the
//! half-landed grant `visible_tools` warns about, where a tool the operator
//! admitted was never advertised.
//!
//! The three progressive-disclosure bridge tools are exempt: they are not the
//! session's tools, they are how a deferred tool is reached. `invoke_tool` is
//! unwrapped to its inner tool before this gate, so the inner name is what
//! gets checked.
//!
//! ## What it does not cover
//!
//! Three limits, stated here because each of them once read as covered:
//!
//! - **`discover_tools` and `get_tool_schema` still list excluded tools.**
//!   They enumerate the dispatcher's whole catalog, which the active set does
//!   not filter. Calling one of the tools they name is still refused by
//!   [`ActiveToolSets::dispatch_refusal`], so the set holds as a control over
//!   what *runs*; what leaks is the knowledge that the tool exists.
//! - **It lives in memory only.** Nothing persists a set, so a daemon restart
//!   drops every one of them and a resumed session comes back with its full
//!   automatic tool list. A plugin that wants its narrowing to survive has to
//!   re-apply it from a `session:start` hook.
//! - **It does not reach an ACP session.** Crucible does not assemble the
//!   tool list an external agent offers its model, so `cru.tools.set_active`
//!   refuses one outright rather than narrowing the MCP half and leaving the
//!   agent's own tools whole. See `tools_bridge::active_set_refusal`.

use crucible_lua::ToolSelector;
use dashmap::DashMap;
use std::sync::Arc;

/// Whether `name` is a progressive-disclosure bridge tool, which an active
/// set never hides. See the module docs.
///
/// Composed from the two places that already own these names rather than
/// spelled out a fourth time: the dispatcher owns the discovery pair, and
/// `invoke_tool` is named here for the same reason
/// [`crate::tools::surface::reserved_tool_names`] names it — it reaches no
/// executor, so no table of executable tools contains it.
/// `the_exempt_set_is_exactly_the_bridge_the_provider_attaches` below is what
/// catches a fourth bridge tool being added without this list moving.
fn is_bridge_tool(name: &str) -> bool {
    name == "invoke_tool" || crate::tool_dispatch::DISCOVERY_TOOL_NAMES.contains(&name)
}

/// Explicit tool sets, keyed by session id.
///
/// Cloneable and shared: the agent handle reads it once per request (so a set
/// written mid-turn applies to the next request rather than to the next
/// rebuilt agent), the dispatcher reads it per tool call, and
/// `cru.tools.set_active` writes it.
#[derive(Clone, Default)]
pub struct ActiveToolSets {
    sets: Arc<DashMap<String, Vec<String>>>,
}

impl ActiveToolSets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Put `patterns` in force for `session_id`.
    ///
    /// An empty vector is a real answer, not a clear: it means "this session
    /// offers no tools". [`Self::clear`] is how automatic behaviour comes
    /// back.
    pub fn set(&self, session_id: &str, patterns: Vec<String>) {
        self.sets.insert(session_id.to_string(), patterns);
    }

    /// Drop the session's set, restoring the automatic behaviour.
    pub fn clear(&self, session_id: &str) {
        self.sets.remove(session_id);
    }

    /// The patterns in force, or `None` when the session has no explicit set.
    pub fn get(&self, session_id: &str) -> Option<Vec<String>> {
        self.sets.get(session_id).map(|e| e.clone())
    }

    /// The selector to filter with, or `None` when nothing is in force.
    ///
    /// Glob syntax is `ToolSelector`'s — the same language a mode's `tools`
    /// selector and `crucible.on`'s `pattern` speak, rather than a third one.
    pub fn selector(&self, session_id: &str) -> Option<ToolSelector> {
        self.get(session_id).map(ToolSelector::Patterns)
    }

    /// Whether this session has a set (for the cleanup invariant).
    pub fn has_session(&self, session_id: &str) -> bool {
        self.sets.contains_key(session_id)
    }

    /// `Some(reason)` when the session's active set excludes `tool_name`.
    ///
    /// `None` when no set is in force, when the set names the tool, or for a
    /// disclosure bridge tool.
    pub fn dispatch_refusal(&self, session_id: &str, tool_name: &str) -> Option<String> {
        if is_bridge_tool(tool_name) {
            return None;
        }
        let selector = self.selector(session_id)?;
        if selector.matches(tool_name) {
            return None;
        }
        Some(format!(
            "Tool '{tool_name}' is not in this session's active tool set. A plugin narrowed it \
             with cru.tools.set_active; cru.tools.set_active(session, nil) restores the full set."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_set_leaves_every_tool_alone() {
        let sets = ActiveToolSets::new();
        assert!(sets.selector("s1").is_none());
        assert!(sets.dispatch_refusal("s1", "bash").is_none());
        assert!(!sets.has_session("s1"));
    }

    #[test]
    fn a_set_round_trips() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec!["read_file".into(), "grep_notes".into()]);
        assert_eq!(
            sets.get("s1"),
            Some(vec!["read_file".to_string(), "grep_notes".to_string()])
        );
        assert!(sets.has_session("s1"));
    }

    #[test]
    fn clearing_restores_the_automatic_behaviour() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec!["read_file".into()]);
        sets.clear("s1");
        assert_eq!(sets.get("s1"), None);
        assert!(sets.dispatch_refusal("s1", "bash").is_none());
    }

    #[test]
    fn a_set_is_per_session() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec!["read_file".into()]);
        assert!(sets.dispatch_refusal("s1", "bash").is_some());
        assert!(sets.dispatch_refusal("s2", "bash").is_none());
    }

    #[test]
    fn dispatch_refuses_a_tool_outside_the_set() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec!["read_file".into()]);
        let reason = sets
            .dispatch_refusal("s1", "bash")
            .expect("bash is outside the set");
        assert!(reason.contains("active tool set"), "{reason}");
        assert!(sets.dispatch_refusal("s1", "read_file").is_none());
    }

    #[test]
    fn dispatch_matches_the_same_globs_a_mode_selector_does() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec!["read_*".into(), "{grep,property}_search".into()]);
        assert!(sets.dispatch_refusal("s1", "read_note").is_none());
        assert!(sets.dispatch_refusal("s1", "grep_search").is_none());
        assert!(sets.dispatch_refusal("s1", "write_file").is_some());
    }

    /// An empty set means "no tools", not "no set" — otherwise a plugin
    /// asking for nothing would silently get everything.
    #[test]
    fn an_empty_set_refuses_everything() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec![]);
        assert!(sets.dispatch_refusal("s1", "read_file").is_some());
        assert!(sets.get("s1").is_some());
    }

    /// The disclosure bridge is how a deferred tool is reached, so hiding it
    /// behind the active set would strand every deferred tool.
    #[test]
    fn the_disclosure_bridge_is_never_refused() {
        let sets = ActiveToolSets::new();
        sets.set("s1", vec![]);
        for name in bridge_names() {
            assert!(
                sets.dispatch_refusal("s1", &name).is_none(),
                "{name} must stay callable"
            );
        }
    }

    /// The names the provider actually attaches when it defers.
    fn bridge_names() -> Vec<String> {
        crate::provider::genai_handle::bridge_tool_defs()
            .into_iter()
            .map(|d| d.function.name)
            .collect()
    }

    /// The exemption and the bridge must name the same tools.
    ///
    /// A fourth bridge tool added to `bridge_tool_defs` and not here would be
    /// advertised to the model and then refused at dispatch by the very set it
    /// exists to work around — and a name dropped from the bridge but kept
    /// here would be a hole in the set nothing closes. Asserting against the
    /// provider's own list is what makes the drift a test failure rather than
    /// a fourth copy nobody diffs.
    #[test]
    fn the_exempt_set_is_exactly_the_bridge_the_provider_attaches() {
        let attached = bridge_names();
        assert_eq!(attached.len(), 3, "the bridge changed shape: {attached:?}");
        for name in &attached {
            assert!(is_bridge_tool(name), "{name} is attached but not exempt");
        }
        // ...and nothing else is exempt: every name the dispatcher can route
        // that is not part of the bridge must still be refusable.
        for name in ["read_file", "bash", "get_kiln_info", "create_note"] {
            assert!(!is_bridge_tool(name), "{name} must not be exempt");
        }
    }

    /// The registry is shared by clone, so a write through one handle is seen
    /// by the agent handle that was built before it.
    #[test]
    fn a_clone_shares_one_registry() {
        let sets = ActiveToolSets::new();
        let held_by_the_agent = sets.clone();
        sets.set("s1", vec!["read_file".into()]);
        assert_eq!(
            held_by_the_agent.get("s1"),
            Some(vec!["read_file".to_string()])
        );
    }
}
