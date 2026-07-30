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
    "text_search",
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
