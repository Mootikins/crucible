//! The closed set of names `crucible.on()` can register for.
//!
//! One list used to hold every name: `HOOK_NAMES`, a `&[&str]`. Two things were
//! wrong with it.
//!
//! **It was a union of two contracts.** Eight of the names are *events* — the
//! daemon broadcasts them, fan-out, nobody replies, the thing already happened.
//! Eleven are *stages* — synchronous interception points on the turn loop, run
//! in priority order, where a handler's return value changes what happens next.
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

/// A synchronous interception point on the turn loop.
///
/// **A chain, not a broadcast.** Handlers run in priority order and the loop
/// waits for each; the return value decides what happens next. `Cancel` blocks
/// the operation, `Transform` rewrites the value the next link sees, and
/// `Handled` replaces execution outright — which is why `Handled` and
/// `Transform` are capability-grade on [`Self::PreToolCall`] and gated by
/// `Capability::InterceptTools`.
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
pub fn hook_names() -> impl Iterator<Item = &'static str> {
    HookName::all().map(HookName::as_str)
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
