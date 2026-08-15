//! The plan-mode tool set — a FALLBACK, not the definition.
//!
//! Modes are declared in Lua (`cru.modes.plan = { tools = { … } }`), and
//! `runtime/defaults/init.lua` declares plan like any user would. This list is
//! what the daemon falls back to when the Lua registry declares nothing at all
//! — a broken defaults file must not leave plan mode wide open. See
//! `agent_factory::mode_exposes_tool`, which consults the registry first.
//!
//! It deliberately does not match `agent_manager::is_safe`: that answers "may
//! this tool skip the permission prompt" and includes the workspace read tools
//! (`read_file`, `glob`, `grep`); this answers "may plan mode see this tool at
//! all" and includes `skill_view`. Two questions, two lists.

/// Read-only tools available in "plan" mode when Lua declares no modes.
pub const PLAN_TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "grep_notes",
    "property_search",
    "list_notes",
    "read_note",
    "read_metadata",
    "get_kiln_info",
    "list_jobs",
    "skill_view",
];

/// The modes the daemon ships and can reason about with no Lua at all.
///
/// The discriminator for "is this mode unknown". An empty registry is
/// ambiguous on its own — it means both "the defaults never loaded" and "the
/// user removed the last declaration" — so neither emptiness nor
/// `Option::is_some` can decide it. Membership here can: a session in `normal`
/// with no Lua is the ordinary un-configured state, while a session in
/// `review` with no declaration is a mode that went away.
pub const BUILTIN_MODE_NAMES: &[&str] = &["normal", "plan", "auto"];

/// Whether `name` is a mode the daemon ships.
#[must_use]
pub fn is_builtin_mode(name: &str) -> bool {
    BUILTIN_MODE_NAMES.contains(&name)
}

/// Whether a plugin-declared tool must be refused in `mode_id`.
///
/// Plan mode refused plugin tools *categorically*, because a plugin's side
/// effects are unknown and the write-name blocklist cannot classify them. That
/// is the right default and stays the default — but it also made plan mode
/// unusable for the one class of plugin tool that genuinely belongs there:
/// research.
///
/// The exception is deliberately two-key, matching the split this codebase
/// already makes between [`crate::agent_manager::is_safe`] (what the daemon
/// knows) and [`crate::agent_manager::believed_read_only`] (what a third party
/// claims). A plugin declaring itself safe grants nothing; the operator naming
/// it in the mode's `tools` list is what admits it. No harness surveyed trusts
/// plugin self-attestation either, and MCP's own spec says a human stays in
/// the loop.
///
/// [`ToolSelector::names_exactly`] is the operator half: a glob does not
/// count, so a plugin cannot pick a name that slips through a selector written
/// before it existed.
pub fn plugin_tool_barred(
    mode_id: &str,
    tool_name: &str,
    plugin_tool_names: &std::collections::HashSet<String>,
    modes: Option<&crucible_lua::ModeRegistry>,
) -> bool {
    if mode_id != "plan" || !plugin_tool_names.contains(tool_name) {
        return false;
    }
    // No registry is the un-configured state, and it fails CLOSED: with no
    // declaration there is no grant, so the categorical refusal stands.
    !modes
        .and_then(|m| m.get(mode_id))
        .is_some_and(|m| m.tools.names_exactly(tool_name))
}

#[cfg(test)]
mod plugin_admission_tests {
    use super::*;
    use crucible_lua::{ModeDefinition, ModePermissions, ToolSelector};
    use std::collections::HashSet;

    fn registry(selector: ToolSelector) -> crucible_lua::ModeRegistry {
        let reg = crucible_lua::ModeRegistry::new();
        reg.set(ModeDefinition {
            name: "plan".to_string(),
            description: None,
            tools: selector,
            permissions: ModePermissions::default(),
        });
        reg
    }

    fn plugin_tools() -> HashSet<String> {
        HashSet::from(["web_search".to_string()])
    }

    /// The default stays the default: a plugin tool nobody named is refused.
    #[test]
    fn a_plugin_tool_is_barred_from_plan_mode_by_default() {
        let modes = registry(ToolSelector::Patterns(vec!["read_*".into()]));
        assert!(plugin_tool_barred(
            "plan",
            "web_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }

    /// The operator's half: naming it outright admits it.
    #[test]
    fn naming_a_plugin_tool_in_the_mode_admits_it() {
        let modes = registry(ToolSelector::Patterns(vec![
            "read_*".into(),
            "web_search".into(),
        ]));
        assert!(!plugin_tool_barred(
            "plan",
            "web_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }

    /// A glob must not admit it. `*_search` is a plausible thing to have
    /// written before any plugin existed; honouring it here would let a plugin
    /// choose a name that walks through a rule already on disk.
    #[test]
    fn a_glob_does_not_admit_a_plugin_tool() {
        let modes = registry(ToolSelector::Patterns(vec!["*_search".into()]));
        assert!(plugin_tool_barred(
            "plan",
            "web_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }

    /// `tools = "*"` was written about the tools that existed then.
    #[test]
    fn the_all_selector_does_not_admit_a_plugin_tool() {
        let modes = registry(ToolSelector::All);
        assert!(plugin_tool_barred(
            "plan",
            "web_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }

    /// Outside a restricted mode the question does not arise.
    #[test]
    fn nothing_is_barred_outside_plan_mode() {
        let modes = registry(ToolSelector::Patterns(vec!["read_*".into()]));
        assert!(!plugin_tool_barred(
            "normal",
            "web_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }

    /// No mode registry at all: nothing granted, so the refusal stands.
    #[test]
    fn an_absent_registry_bars_a_plugin_tool() {
        assert!(plugin_tool_barred(
            "plan",
            "web_search",
            &plugin_tools(),
            None
        ));
    }

    /// A mode that is neither `plan` nor declared has no grant to give.
    ///
    /// `plugin_tool_barred` answers only for `plan` and returns false for any
    /// other mode id, so a caller that also strips on "mode unknown" must keep
    /// doing so itself. Wiring this function into the advertisement filter
    /// without that arm reopened every plugin tool for a mode that had gone
    /// away — the most permissive possible reading of a missing config.
    #[test]
    fn this_rule_answers_only_for_plan_and_callers_must_handle_unknown_modes() {
        let modes = registry(ToolSelector::Patterns(vec!["read_*".into()]));
        assert!(
            !plugin_tool_barred("review", "web_search", &plugin_tools(), Some(&modes)),
            "an undeclared mode is not this function's question to answer"
        );
    }

    /// A built-in tool is never subject to this rule.
    #[test]
    fn a_builtin_tool_is_not_a_plugin_tool() {
        let modes = registry(ToolSelector::Patterns(vec!["read_*".into()]));
        assert!(!plugin_tool_barred(
            "plan",
            "semantic_search",
            &plugin_tools(),
            Some(&modes)
        ));
    }
}
