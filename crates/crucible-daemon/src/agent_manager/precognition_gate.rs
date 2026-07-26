//! The per-turn gate deciding whether Precognition runs at all.
//! Split from `precognition.rs` for the 1000-line module budget.

// Pure gate — no imports needed beyond std.

/// Decide whether Precognition should run for this turn.
///
/// Pi-style heuristic: even when Precognition is enabled, only inject
/// on the first user message of a session. Running every turn bloats
/// context and degrades cache hits over a long conversation, with
/// diminishing relevance — subsequent turns are usually about the same
/// topic the first injection already covered.
///
/// Other gates: `/search` is a manual search command that shouldn't
/// trigger auto-RAG; kiln must be configured. The handler hook seam
/// (`transform_context`) is a separate, per-turn surface — Lua plugins
/// can implement richer per-turn heuristics there.
pub(super) fn should_run_precognition(
    precognition_enabled: bool,
    original_content: &str,
    session_kiln: &std::path::Path,
    is_first_user_message: bool,
) -> bool {
    precognition_enabled
        && !original_content.starts_with("/search")
        && !session_kiln.as_os_str().is_empty()
        && is_first_user_message
}

#[cfg(test)]
mod should_run_precognition_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn runs_on_first_user_message_with_precognition_enabled() {
        assert!(should_run_precognition(
            true,
            "tell me about widgets",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_on_subsequent_user_messages_even_when_enabled() {
        // Pi-style: don't re-inject every turn — bloats context, hurts
        // cache, redundant for same-topic follow-ups.
        assert!(!should_run_precognition(
            true,
            "follow-up question",
            Path::new("/some/kiln"),
            false,
        ));
    }

    #[test]
    fn skipped_when_disabled_in_agent_config() {
        assert!(!should_run_precognition(
            false,
            "x",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_for_explicit_search_command() {
        assert!(!should_run_precognition(
            true,
            "/search widgets",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_when_no_kiln_configured() {
        assert!(!should_run_precognition(true, "x", Path::new(""), true,));
    }
}
