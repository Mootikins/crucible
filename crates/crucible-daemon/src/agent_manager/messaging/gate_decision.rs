//! Whether a tool call must face the permission gate at all.
//!
//! Its own module because this is a trust boundary, not a detail of tool
//! dispatch: answering `false` here skips the session's mode stance, the
//! mode's rules, every Lua `on_request` hook and the saved patterns, leaving
//! only the global `[permissions]` deny.

use crate::agent_manager::is_safe;

/// Whether a tool call must go through [`AgentManager::handle_permission_request`].
///
/// Extracted so the trust boundary is testable at the point it is decided.
/// Answering `false` here does not merely skip a prompt — it skips the
/// session's mode stance, the mode's rules, every Lua `on_request` hook and
/// the saved patterns, leaving only the global `[permissions]` deny.
///
/// So this consults [`is_safe`] and never `believed_read_only`: the latter
/// includes what a third-party MCP server claims about its own tools via
/// `readOnlyHint`, and an upstream must not be able to annotate its way past a
/// mode's `default = "deny"`.
pub(crate) fn requires_permission_gate(
    card_policy: Option<crucible_core::agent::ToolPolicy>,
    tool_name: &str,
) -> bool {
    use crucible_core::agent::ToolPolicy;
    match card_policy {
        Some(ToolPolicy::Allow) => false,
        Some(ToolPolicy::Ask) => true,
        Some(ToolPolicy::Deny) => unreachable!("denied above"),
        None => !is_safe(tool_name),
    }
}

#[cfg(test)]
mod tests {
    use super::requires_permission_gate;

    /// The trust boundary, asserted at the point it is decided.
    ///
    /// Asserting `is_safe` alone is not enough — it proves the function is
    /// narrow, not that this decision consults it. Swapping the body below to
    /// `believed_read_only` leaves an `is_safe`-only test green while an
    /// upstream MCP server can annotate its way past every mode rule and Lua
    /// hook, because a `false` here skips `handle_permission_request` entirely.
    #[test]
    fn an_mcp_read_only_hint_cannot_skip_the_permission_gate() {
        // `gh_create_pr` is in no built-in safe list, so the only thing that
        // could make this `false` is trusting the upstream's annotation.
        assert!(
            requires_permission_gate(None, "gh_create_pr"),
            "a tool a third party annotated read-only must still reach the mode \
             stance, the mode rules and the Lua hooks"
        );
        assert!(
            !requires_permission_gate(None, "read_file"),
            "a genuinely built-in read-only tool still skips the gate"
        );
        assert!(
            requires_permission_gate(None, "bash"),
            "and a built-in mutating tool always gates"
        );
    }
}
