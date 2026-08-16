//! Per-*tool* trust classification.
//!
//! [`ToolSurface`] answers "if nothing intercepts this call, does running it
//! touch the host?" — and that question belongs to the tool, not to the
//! executor that happens to route it.
//!
//! Declaring it per executor is a confused deputy. `McpToolExecutor` answered
//! [`ToolSurface::Daemon`] for all ~20 of its tools on the theory that "all of
//! it lands in daemon-side storage", which was false for three of them:
//! `create_note` / `update_note` reach `std::fs::write` and `delete_note`
//! reaches `std::fs::remove_file`, in the daemon process, on the host. An
//! isolated session therefore refused `bash` and `write_file` while
//! `create_note` wrote the host filesystem — with a `.lua` under a runtimepath
//! tree the daemon executes on next start. Worse, the executor's single answer
//! was *inherited*: any tool added to `CrucibleMcpServer` became `Daemon` with
//! nobody deciding.
//!
//! So the classification is a total function of a tool identity enum, and the
//! `match` in [`BuiltinTool::surface`] carries no wildcard arm. rustc's
//! exhaustiveness check is the enforcement: a new variant does not compile
//! until someone classifies it. The module-level
//! `deny(clippy::wildcard_enum_match_arm)` is what stops that compile error
//! being silenced with `_ => ToolSurface::Daemon` later.
//!
//! The other half of the guarantee is [`classify`]: a name with no variant is
//! [`ToolSurface::Unknown`], which the isolation gate refuses exactly as it
//! refuses `Host`. **Absence denies.** There is deliberately no bypass — no
//! `Default` on `ToolSurface` and no `unwrap_or_default`, because either would
//! reintroduce "unclassified means allowed" as a one-line edit. If a tool ever
//! genuinely needs to skip classification it must be a named, greppable call
//! site here, never an implicit fallback.
//!
//! Executors that serve *foreign* code — plugin Lua, MCP gateway upstreams —
//! do not consult this table at all. They are `Unknown` by construction, so a
//! plugin tool that names itself `read_note` cannot borrow the built-in's
//! classification.

#![deny(clippy::wildcard_enum_match_arm)]

use crucible_core::traits::tools::ToolSurface;

/// A tool Crucible itself defines and runs in the daemon process.
///
/// Only built-ins appear here. Plugin and gateway tools are foreign code whose
/// reach the daemon cannot know, and they are classified `Unknown` without
/// consulting this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum BuiltinTool {
    // -- WorkspaceTools: arbitrary host paths and host processes -------------
    ReadFile,
    EditFile,
    WriteFile,
    Bash,
    Glob,
    Grep,

    // -- NoteTools: the kiln's markdown corpus -------------------------------
    CreateNote,
    ReadNote,
    ReadMetadata,
    UpdateNote,
    DeleteNote,
    ListNotes,

    // -- SearchTools + KilnTools: the kiln index -----------------------------
    SemanticSearch,
    GrepNotes,
    PropertySearch,
    GetKilnInfo,

    // -- Daemon-managed catalogs, jobs and delegation ------------------------
    SkillView,
    DelegateSession,
    ListJobs,
    GetJobResult,
    CancelJob,

    // -- Progressive disclosure, answered by the dispatcher itself -----------
    DiscoverTools,
    GetToolSchema,
}

impl BuiltinTool {
    /// The wire name the model calls this tool by.
    ///
    /// Exhaustive on purpose, same as [`Self::surface`]: a variant with no name
    /// is a compile error rather than a tool that silently stops resolving.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadFile => "read_file",
            Self::EditFile => "edit_file",
            Self::WriteFile => "write_file",
            Self::Bash => "bash",
            Self::Glob => "glob",
            Self::Grep => "grep",
            Self::CreateNote => "create_note",
            Self::ReadNote => "read_note",
            Self::ReadMetadata => "read_metadata",
            Self::UpdateNote => "update_note",
            Self::DeleteNote => "delete_note",
            Self::ListNotes => "list_notes",
            Self::SemanticSearch => "semantic_search",
            Self::GrepNotes => "grep_notes",
            Self::PropertySearch => "property_search",
            Self::GetKilnInfo => "get_kiln_info",
            Self::SkillView => "skill_view",
            Self::DelegateSession => "delegate_session",
            Self::ListJobs => "list_jobs",
            Self::GetJobResult => "get_job_result",
            Self::CancelJob => "cancel_job",
            Self::DiscoverTools => "discover_tools",
            Self::GetToolSchema => "get_tool_schema",
        }
    }

    /// The variant for a wire name, or `None` when nothing classifies it.
    ///
    /// `None` is a security-relevant answer, not a lookup miss — see
    /// [`classify`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let tool = match name {
            "read_file" => Self::ReadFile,
            "edit_file" => Self::EditFile,
            "write_file" => Self::WriteFile,
            "bash" => Self::Bash,
            "glob" => Self::Glob,
            "grep" => Self::Grep,
            "create_note" => Self::CreateNote,
            "read_note" => Self::ReadNote,
            "read_metadata" => Self::ReadMetadata,
            "update_note" => Self::UpdateNote,
            "delete_note" => Self::DeleteNote,
            "list_notes" => Self::ListNotes,
            "semantic_search" => Self::SemanticSearch,
            "grep_notes" => Self::GrepNotes,
            "property_search" => Self::PropertySearch,
            "get_kiln_info" => Self::GetKilnInfo,
            "skill_view" => Self::SkillView,
            "delegate_session" => Self::DelegateSession,
            "list_jobs" => Self::ListJobs,
            "get_job_result" => Self::GetJobResult,
            "cancel_job" => Self::CancelJob,
            "discover_tools" => Self::DiscoverTools,
            "get_tool_schema" => Self::GetToolSchema,
            _ => return None,
        };
        Some(tool)
    }

    /// What running this tool reaches if nothing intercepts it.
    ///
    /// **No wildcard arm, ever.** A new tool must fail to compile until someone
    /// answers this question for it; that compile error is the whole mechanism.
    ///
    /// `Host` is the classification for the tools that take an arbitrary
    /// host path or run a host process. `Daemon` is for the tools whose reach
    /// is a structure the daemon owns — the kiln corpus, the kiln index, the
    /// job table, the skill catalog — because containerizing a session's
    /// *workspace* has nothing to say about those, and refusing them is what
    /// made "turning on the sandbox turns off Crucible" true.
    ///
    /// NOTE(crucible): the note tools are `Daemon` because their reach is
    /// *defined* as the kiln corpus, and that is only honest while two things
    /// hold it there: `RootSet` confinement plus the protected set on every
    /// note path (`FsScope::resolve_for_write`), and the rule that what a note
    /// tool writes is a NOTE (`notes::helpers::reject_non_note`, sourced from
    /// `KilnFileKind`). See `thoughts/Agent Filesystem Containment.md` ("The
    /// container escape"): `ensure_md_suffix` preserved a caller-supplied
    /// extension, so `create_note {"path": "plugins/evil/init.lua"}` wrote
    /// executable Lua into a runtimepath tree from inside a container, where
    /// `write_file` and `bash` are refused. If either half is ever removed,
    /// these six move to `Host` — the classification follows the confinement,
    /// not the other way round.
    #[must_use]
    pub fn surface(self) -> ToolSurface {
        match self {
            // Arbitrary host paths, and `bash` executes host processes
            // outright. This is the family an isolating plugin exists to
            // intercept.
            Self::ReadFile
            | Self::EditFile
            | Self::WriteFile
            | Self::Bash
            | Self::Glob
            | Self::Grep => ToolSurface::Host,

            // The kiln corpus, confined to the kiln by `RootSet`.
            Self::CreateNote
            | Self::ReadNote
            | Self::ReadMetadata
            | Self::UpdateNote
            | Self::DeleteNote
            | Self::ListNotes => ToolSurface::Daemon,

            // The kiln index and its metadata.
            Self::SemanticSearch | Self::GrepNotes | Self::PropertySearch | Self::GetKilnInfo => {
                ToolSurface::Daemon
            }

            // Daemon-owned catalogs and the job table. `delegate_session` is
            // `Daemon` only because `SessionLifecycle` now fires plugin start
            // hooks for a delegated child, so the child acquires its own claim
            // and its own container; before that it was a hole wide enough to
            // run every tool on the host.
            Self::SkillView
            | Self::DelegateSession
            | Self::ListJobs
            | Self::GetJobResult
            | Self::CancelJob => ToolSurface::Daemon,

            // Read-only queries over the dispatcher's own catalog. Refusing
            // these would leave a sandboxed agent unable to ask what tools it
            // has.
            Self::DiscoverTools | Self::GetToolSchema => ToolSurface::Daemon,
        }
    }
}

/// The surface of a built-in tool by name — `Unknown` when nothing classifies
/// it.
///
/// The `Unknown` is the point. An unclassified name is refused by the isolation
/// gate exactly as `Host` is, so a tool added to an executor without a
/// [`BuiltinTool`] variant fails closed instead of inheriting its neighbours'
/// trust. `builtin_surfaces_cover_every_advertised_tool` then turns that silent
/// refusal into a loud test failure.
#[must_use]
pub fn classify(name: &str) -> ToolSurface {
    BuiltinTool::from_name(name).map_or(ToolSurface::Unknown, BuiltinTool::surface)
}

#[cfg(test)]
mod tests;
