//! The closed set of names `crucible.on()` can register for.
//!
//! One list used to hold every name: `HOOK_NAMES`, a `&[&str]`. Two things were
//! wrong with it.
//!
//! **It was a union of two contracts.** Ten of the names are *events* — the
//! daemon broadcasts them, fan-out, nobody replies, the thing already happened.
//! Seventeen are *stages* — synchronous interception points, run in
//! registration order, where a handler's return value changes what happens
//! next.
//! [`ScriptHandlerResult`](crate::ScriptHandlerResult) carries the same four
//! variants for both, so `Cancel` meant "stop the remaining handlers" on one
//! side and "block the operation" on the other, decided only by which name the
//! author had registered. `Transform` and `Handled` mean nothing at all for an
//! event that has already been broadcast, and `server/file_event_hooks.rs` has
//! to log-and-ignore them. The two contracts are now two types, so a dispatch
//! site cannot route one through the other's loop.
//!
//! **Its completeness was checked by reading source text.** A test walked every
//! `.rs` file under `crates/`, grepped for `runtime_handlers_for(` and for
//! constant declarations, and compared what it found against the list. That
//! gate was satisfiable without adding the entry: its needle accepted a bare
//! constant declaration, which is exactly what `event_map.rs` instructs a
//! contributor to write. A name is now a variant, `as_str` has no wildcard arm,
//! and the dispatch sites name the variant — so rustc is the gate and the
//! 60-line source walk is gone.
//!
//! Adding a name is one variant plus one `as_str` arm plus one [`Self::ALL`]
//! entry, and the `every_variant_is_listed` tests below fail on the third if
//! you forget it.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use std::time::Duration;

use crate::handler_budget::{LIFECYCLE_BUDGET, PERMISSION_BUDGET, TURN_STAGE_BUDGET};

/// A daemon broadcast event a Lua handler can observe.
///
/// **Fan-out with no reply.** The event already happened and was already put on
/// the bus before any handler runs, so a handler cannot change it: only
/// `Cancel` is meaningful, and it stops the remaining handlers rather than the
/// event. `crucible-daemon/src/event_map.rs` holds the wire name and the
/// pattern identifier for each of these; this enum holds the name a plugin
/// registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum EventName {
    /// A watched file was created or modified.
    FileChanged,
    /// A watched file was removed.
    FileDeleted,
    /// A watched file was renamed or moved.
    FileMoved,
    /// A note reached the index for the first time.
    NoteCreated,
    /// An already-indexed note was written again.
    NoteModified,
    /// A note left the index.
    NoteDeleted,
    /// A note moved, with its inbound links repointed.
    NoteRenamed,
    /// A signed webhook delivery arrived at `POST /api/webhook/{name}`.
    WebhookReceived,
    /// A session was created. Daemon-wide, not scoped to the new session.
    ///
    /// Distinct from the per-session `ended` turn event a client attached to
    /// one session reads. A plugin that watches every session — a session list
    /// surface, for example — needs the daemon-wide pair, because it is not
    /// attached to the session that started or stopped.
    SessionCreated,
    /// A session ended. Daemon-wide; see [`Self::SessionCreated`].
    SessionEnded,
}

impl EventName {
    /// Every variant. [`tests::every_event_variant_is_listed`] proves it.
    pub const ALL: &'static [Self] = &[
        Self::FileChanged,
        Self::FileDeleted,
        Self::FileMoved,
        Self::NoteCreated,
        Self::NoteModified,
        Self::NoteDeleted,
        Self::NoteRenamed,
        Self::WebhookReceived,
        Self::SessionCreated,
        Self::SessionEnded,
    ];

    /// The name a plugin registers, and the `type` field the handler reads.
    ///
    /// **No wildcard arm, ever.** A new variant must fail to compile until
    /// someone names it.
    ///
    /// The three file events keep their Rust `type_name()` spelling
    /// (`FileChanged`, not `file:changed`) because every config that already
    /// registers one names them that way. Everything added since is
    /// colon-namespaced.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileChanged => "FileChanged",
            Self::FileDeleted => "FileDeleted",
            Self::FileMoved => "FileMoved",
            Self::NoteCreated => "note:created",
            Self::NoteModified => "note:modified",
            Self::NoteDeleted => "note:deleted",
            Self::NoteRenamed => "note:renamed",
            Self::WebhookReceived => "webhook:received",
            Self::SessionCreated => "session:created",
            Self::SessionEnded => "session:ended",
        }
    }

    /// How long one handler at this event may run.
    ///
    /// **No wildcard arm, ever** — same reason as [`Self::as_str`]. A new
    /// event must name its budget rather than inherit one.
    #[must_use]
    pub const fn budget(self) -> Duration {
        match self {
            Self::FileChanged
            | Self::FileDeleted
            | Self::FileMoved
            | Self::NoteCreated
            | Self::NoteModified
            | Self::NoteDeleted
            | Self::NoteRenamed
            | Self::WebhookReceived
            | Self::SessionCreated
            | Self::SessionEnded => TURN_STAGE_BUDGET,
        }
    }

    /// Whether a dispatch of this event names a session.
    ///
    /// **No wildcard arm, ever** — same reason as [`Self::as_str`]. A new
    /// event must answer for itself.
    ///
    /// The file, note and webhook events belong to the daemon, not to a
    /// session: the watcher fires them with nobody's turn running. The two
    /// session events are ABOUT a session, and the dispatcher reads its id
    /// from the payload.
    #[must_use]
    pub const fn carries_session(self) -> bool {
        match self {
            Self::FileChanged
            | Self::FileDeleted
            | Self::FileMoved
            | Self::NoteCreated
            | Self::NoteModified
            | Self::NoteDeleted
            | Self::NoteRenamed
            | Self::WebhookReceived => false,
            Self::SessionCreated | Self::SessionEnded => true,
        }
    }

    /// The variant for a registered name, or `None` when nothing broadcasts it.
    ///
    /// Derived from [`Self::ALL`] rather than a second `match`, so the two
    /// directions cannot disagree.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|e| e.as_str() == name)
    }
}

/// A synchronous interception point in a host flow.
///
/// **A chain, not a broadcast.** Handlers run in registration order and the
/// caller waits for each; the return value decides what happens next. `Cancel` blocks
/// the operation, `Transform` rewrites the value the next link sees, and
/// `Handled` replaces execution outright — which is why `Handled` and
/// `Transform` are capability-grade on [`Self::PreToolCall`] and gated by
/// the `intercepts_tools` declaration.
///
/// Most of these sit on the turn loop. Four do not — [`Self::PermissionRequest`],
/// [`Self::SessionStart`], [`Self::SessionEnd`] and [`Self::ProviderAuth`] —
/// and they are stages all the same, because a stage is defined by its
/// contract and not by its caller: each one runs synchronously, in
/// registration order, and each one's answer changes what the host does next. Each of the
/// four had its own registry and its own clear path before they merged here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum StageId {
    /// Before a tool runs, and before the permission gate.
    PreToolCall,
    /// After a tool returns, over its result.
    ToolResult,
    /// Before the request goes to the model.
    PreLlmCall,
    /// After the model's response arrives.
    PostLlmCall,
    /// Over the assembled context, before it is sent.
    TransformContext,
    /// Which notes precognition retrieves.
    PrecognitionSelect,
    /// How the retrieved notes are rendered into the prompt.
    PrecognitionFormat,
    /// The turn finished.
    TurnComplete,
    /// Immediately before execution, after admission.
    ToolBeforeExecute,
    /// A tool call is about to be drawn.
    ToolDisplayStart,
    /// A tool call finished and its display is final.
    ToolDisplayComplete,
    /// Over the merged search hits, before the cut to the caller's limit.
    SearchRerank,
    /// Over a note's block rows, before the pipeline writes them.
    IndexBlocks,
    /// Before the permission prompt for one tool call. `cru.permissions.on_request`
    /// registers here; the first hook that answers `allow` or `deny` wins.
    PermissionRequest,
    /// A session started. `cru.on_session_start` registers here.
    ///
    /// Distinct from [`EventName::SessionCreated`], which is the daemon-wide
    /// broadcast: this one runs BEFORE the session is handed back, and a hook
    /// registered `{ required = true }` can refuse the session.
    SessionStart,
    /// A session ended. `cru.on_session_end` registers here.
    ///
    /// Distinct from [`EventName::SessionEnded`], for the reason
    /// [`Self::SessionStart`] gives.
    SessionEnd,
    /// The daemon is about to call a provider. `cru.on_provider_auth`
    /// registers here; the first hook that answers headers wins.
    ProviderAuth,
}

impl StageId {
    /// Every variant. [`tests::every_stage_variant_is_listed`] proves it.
    pub const ALL: &'static [Self] = &[
        Self::PreToolCall,
        Self::ToolResult,
        Self::PreLlmCall,
        Self::PostLlmCall,
        Self::TransformContext,
        Self::PrecognitionSelect,
        Self::PrecognitionFormat,
        Self::TurnComplete,
        Self::ToolBeforeExecute,
        Self::ToolDisplayStart,
        Self::ToolDisplayComplete,
        Self::SearchRerank,
        Self::IndexBlocks,
        Self::PermissionRequest,
        Self::SessionStart,
        Self::SessionEnd,
        Self::ProviderAuth,
    ];

    /// The name a plugin registers.
    ///
    /// **No wildcard arm, ever** — same reason as [`EventName::as_str`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreToolCall => "pre_tool_call",
            Self::ToolResult => "tool_result",
            Self::PreLlmCall => "pre_llm_call",
            Self::PostLlmCall => "post_llm_call",
            Self::TransformContext => "transform_context",
            Self::PrecognitionSelect => "precognition_select",
            Self::PrecognitionFormat => "precognition_format",
            Self::TurnComplete => "turn:complete",
            Self::ToolBeforeExecute => "tool:before_execute",
            Self::ToolDisplayStart => "tool:display_start",
            Self::ToolDisplayComplete => "tool:display_complete",
            Self::SearchRerank => "search:rerank",
            Self::IndexBlocks => "index:blocks",
            Self::PermissionRequest => "permission:request",
            Self::SessionStart => "session:start",
            Self::SessionEnd => "session:end",
            Self::ProviderAuth => "provider:auth",
        }
    }

    /// How long one handler at this stage may run.
    ///
    /// **No wildcard arm, ever** — same reason as [`Self::as_str`]. A new
    /// stage must name its budget rather than inherit one.
    ///
    /// Every stage carries the turn loop's own dispatch timeout today. They
    /// are written out one by one so that a stage which needs a different
    /// number can have one without a second table to keep in step.
    #[must_use]
    pub const fn budget(self) -> Duration {
        match self {
            Self::PreToolCall
            | Self::ToolResult
            | Self::PreLlmCall
            | Self::PostLlmCall
            | Self::TransformContext
            | Self::PrecognitionSelect
            | Self::PrecognitionFormat
            | Self::TurnComplete
            | Self::ToolBeforeExecute
            | Self::ToolDisplayStart
            | Self::ToolDisplayComplete
            | Self::SearchRerank
            | Self::IndexBlocks
            | Self::ProviderAuth => TURN_STAGE_BUDGET,
            // A permission answer blocks the turn and the user, so it gets the
            // short budget the gate already armed for it.
            Self::PermissionRequest => PERMISSION_BUDGET,
            // `oci` pulls a container image in `session:start`, so a
            // turn-stage budget would break a shipped plugin.
            Self::SessionStart | Self::SessionEnd => LIFECYCLE_BUDGET,
        }
    }

    /// Whether a dispatch of this stage names a session.
    ///
    /// **No wildcard arm, ever** — same reason as [`Self::as_str`]. A new
    /// stage must answer for itself, because the answer decides whether a
    /// plugin may scope a handler to one session here.
    ///
    /// Two answer `false`, and neither is an oversight:
    ///
    /// - [`Self::IndexBlocks`] runs in the note pipeline, over a kiln's own
    ///   rows. No turn is running.
    /// - [`Self::ProviderAuth`] runs while the agent factory builds a chat
    ///   client. `build_chat_client_for_agent` holds an agent config and no
    ///   session, so there is no id to compare against.
    ///
    /// [`Self::SearchRerank`] answers `true` even though `RerankStage` may
    /// carry no session: a search made inside a session dispatches one, and a
    /// scope is meaningful for exactly those.
    #[must_use]
    pub const fn carries_session(self) -> bool {
        match self {
            Self::PreToolCall
            | Self::ToolResult
            | Self::PreLlmCall
            | Self::PostLlmCall
            | Self::TransformContext
            | Self::PrecognitionSelect
            | Self::PrecognitionFormat
            | Self::TurnComplete
            | Self::ToolBeforeExecute
            | Self::ToolDisplayStart
            | Self::ToolDisplayComplete
            | Self::SearchRerank
            | Self::PermissionRequest
            | Self::SessionStart
            | Self::SessionEnd => true,
            Self::IndexBlocks | Self::ProviderAuth => false,
        }
    }

    /// The variant for a registered name, or `None` when nothing dispatches it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == name)
    }
}

/// Anything `crucible.on()` accepts: an [`EventName`] or a [`StageId`].
///
/// The two halves keep their own types everywhere the contract differs. This
/// exists for the one place that genuinely does not care — validating what a
/// plugin passed to `crucible.on` — and for the "did you mean" hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookName {
    /// A broadcast event: fan-out, already happened, no reply.
    Event(EventName),
    /// An interception point: synchronous, ordered, the return value matters.
    Stage(StageId),
}

impl HookName {
    /// The name a plugin registers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Event(e) => e.as_str(),
            Self::Stage(s) => s.as_str(),
        }
    }

    /// How long one handler registered under this name may run.
    #[must_use]
    pub const fn budget(self) -> Duration {
        match self {
            Self::Event(e) => e.budget(),
            Self::Stage(s) => s.budget(),
        }
    }

    /// Whether a dispatch of this name carries a session.
    ///
    /// `cru.on` and the other registration APIs refuse a
    /// [`Scope::Session`](super::registry::Scope::Session) on a name that
    /// answers `false`. Refused at REGISTRATION rather than ignored at fire
    /// time: a handler that can never fire is a broken plugin, and a silent
    /// one is worse than a loud one.
    #[must_use]
    pub const fn carries_session(self) -> bool {
        match self {
            Self::Event(e) => e.carries_session(),
            Self::Stage(s) => s.carries_session(),
        }
    }

    /// The variant for a registered name, or `None` when nothing can fire it.
    ///
    /// This is the whole of `crucible.on`'s validation: a `None` here is a
    /// registration error, not a warning, because a handler that can never fire
    /// is a broken plugin.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        EventName::parse(name)
            .map(Self::Event)
            .or_else(|| StageId::parse(name).map(Self::Stage))
    }

    /// Every name, events first.
    pub fn all() -> impl Iterator<Item = Self> {
        EventName::ALL
            .iter()
            .copied()
            .map(Self::Event)
            .chain(StageId::ALL.iter().copied().map(Self::Stage))
    }

    /// The API that registers this name, when `cru.on` is not it.
    ///
    /// The four merged names share one STORE with `cru.on`, and nothing else.
    /// Each carries a payload that is not `cru.on`'s `(ctx, event)` pair — a
    /// session handle, a permission request, a provider context — so a
    /// handler written for `cru.on` would read the wrong argument and fail at
    /// fire time rather than at registration. `cru.on` therefore refuses them
    /// and names the API whose argument shape matches.
    ///
    /// **No wildcard arm, ever.** A new name must say which API registers it.
    #[must_use]
    pub const fn own_api(self) -> Option<&'static str> {
        match self {
            Self::Event(_) => None,
            Self::Stage(stage) => match stage {
                StageId::PreToolCall
                | StageId::ToolResult
                | StageId::PreLlmCall
                | StageId::PostLlmCall
                | StageId::TransformContext
                | StageId::PrecognitionSelect
                | StageId::PrecognitionFormat
                | StageId::TurnComplete
                | StageId::ToolBeforeExecute
                | StageId::ToolDisplayStart
                | StageId::ToolDisplayComplete
                | StageId::SearchRerank
                | StageId::IndexBlocks => None,
                StageId::PermissionRequest => Some("cru.permissions.on_request"),
                StageId::SessionStart => Some("cru.on_session_start"),
                StageId::SessionEnd => Some("cru.on_session_end"),
                StageId::ProviderAuth => Some("cru.on_provider_auth"),
            },
        }
    }
}

impl From<EventName> for HookName {
    fn from(event: EventName) -> Self {
        Self::Event(event)
    }
}

impl From<StageId> for HookName {
    fn from(stage: StageId) -> Self {
        Self::Stage(stage)
    }
}

impl std::fmt::Display for EventName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for StageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for HookName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every name `crucible.on()` accepts, for error messages and documentation.
///
/// The four names with their own registration API are NOT here: `cru.on`
/// refuses them, so listing them would advertise a registration that fails.
/// See [`HookName::own_api`].
pub fn hook_names() -> impl Iterator<Item = &'static str> {
    HookName::all()
        .filter(|hook| hook.own_api().is_none())
        .map(HookName::as_str)
}

/// Every name a registration can carry, the four with their own registration
/// API included.
///
/// [`hook_names`] omits those four because `cru.on` refuses them. `cru.clear`
/// must accept them: a `session:start` row lives in the same store as every
/// other row, and the plugin that registered it has to be able to retire it.
pub fn every_hook_name() -> impl Iterator<Item = &'static str> {
    HookName::all().map(HookName::as_str)
}

/// Parse `event_type`, or refuse it and name the nearest `candidates` entry.
///
/// One reader of the refusal, because the two callers accept different sets:
/// `cru.on` offers [`hook_names`] and refuses the four with their own API
/// separately, with a message that names the API to use instead; `cru.clear`
/// offers [`every_hook_name`].
pub fn parse_or_suggest(
    api: &str,
    event_type: &str,
    candidates: impl Iterator<Item = &'static str>,
) -> Result<HookName, mlua::Error> {
    if let Some(name) = HookName::parse(event_type) {
        return Ok(name);
    }
    let names: Vec<&'static str> = candidates.collect();
    let suggestion = names
        .iter()
        .copied()
        .min_by_key(|n| crucible_core::fuzzy::levenshtein(n, event_type))
        .filter(|n| crucible_core::fuzzy::levenshtein(n, event_type) <= 3);
    Err(mlua::Error::RuntimeError(match suggestion {
        Some(s) => format!("{api}: unknown event `{event_type}` — did you mean `{s}`?"),
        None => format!(
            "{api}: unknown event `{event_type}`. Valid: {}",
            names.join(", ")
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// `ALL` is hand-written; the compiler does not check it. `EnumIter` walks
    /// what the compiler *does* know, so a variant added without an `ALL` entry
    /// fails here rather than becoming a name `crucible.on` silently rejects.
    #[test]
    fn every_event_variant_is_listed() {
        let listed: Vec<EventName> = EventName::ALL.to_vec();
        let known: Vec<EventName> = EventName::iter().collect();
        assert_eq!(listed, known, "EventName::ALL is missing a variant");
    }

    #[test]
    fn every_stage_variant_is_listed() {
        let listed: Vec<StageId> = StageId::ALL.to_vec();
        let known: Vec<StageId> = StageId::iter().collect();
        assert_eq!(listed, known, "StageId::ALL is missing a variant");
    }

    /// Two names that collide would make [`HookName::parse`] answer `Event` for
    /// something the turn loop dispatches as a stage, which is the exact
    /// confusion this split exists to remove.
    #[test]
    fn no_name_is_both_an_event_and_a_stage() {
        for event in EventName::ALL {
            assert!(
                StageId::parse(event.as_str()).is_none(),
                "`{}` is both an event and a stage",
                event.as_str()
            );
        }
    }

    #[test]
    fn every_name_is_distinct() {
        let mut names: Vec<&str> = hook_names().collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two hooks share a name");
    }

    #[test]
    fn a_name_round_trips_through_parse() {
        for hook in HookName::all() {
            assert_eq!(
                HookName::parse(hook.as_str()),
                Some(hook),
                "`{}` does not parse back to itself",
                hook.as_str()
            );
        }
    }

    /// The documented table in `docs/Help/Extending/Event Hooks.md` is the
    /// third copy of this list, and the one plugin authors actually read.
    ///
    /// A name missing from it is a working hook nobody can discover; a name
    /// only it knows is a hook an author writes and which never fires. The
    /// expectation comes from the enums, so the doc is checked against the
    /// running system rather than the other way round.
    #[test]
    fn the_documented_table_lists_every_hook() {
        let doc = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("repo root")
            .join("docs/Help/Extending/Event Hooks.md");
        let src = std::fs::read_to_string(&doc).expect("the hooks reference is readable");

        // The page has several tables. Only the first one under
        // `## Event Types` lists hook names, so take rows from the separator
        // row until the table ends, and read the first backticked cell of each.
        let section = src
            .split_once("\n## Event Types\n")
            .map(|(_, rest)| rest)
            .expect("the hooks reference has an `## Event Types` section");
        let documented: std::collections::BTreeSet<&str> = section
            .lines()
            .skip_while(|line| !line.starts_with("|---"))
            .skip(1)
            .take_while(|line| line.starts_with('|'))
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|rest| rest.split('`').next())
            .collect();
        assert!(
            documented.len() > 10,
            "read only {} rows from {} — the table's shape moved, fix this test",
            documented.len(),
            doc.display()
        );

        let declared: std::collections::BTreeSet<&str> = hook_names().collect();
        let undocumented: Vec<_> = declared.difference(&documented).collect();
        assert!(
            undocumented.is_empty(),
            "these hooks fire but the reference does not list them: {undocumented:?}"
        );
        let unregisterable: Vec<_> = documented.difference(&declared).collect();
        assert!(
            unregisterable.is_empty(),
            "the reference lists these but `crucible.on` rejects them: {unregisterable:?}"
        );
    }

    #[test]
    fn an_unknown_name_does_not_parse() {
        assert_eq!(HookName::parse("pre_toolcall"), None);
        assert_eq!(HookName::parse(""), None);
    }
}
