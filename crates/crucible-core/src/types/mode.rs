//! Mode descriptor types for UI presentation
//!
//! This module contains types for describing chat modes with UI metadata.
//! `ModeDescriptor` wraps ACP's `SessionMode` with additional display information.
//!
//! # Default Mode
//!
//! The default mode is `ask` (every tool, permission asked first). This follows the pattern
//! of other AI coding assistants (OpenCode's "build", Codex's "workspace-write")
//! where developers expect to code by default, not just read.
//!
//! Use `plan` mode for read-only exploration when you want to prevent modifications.

use serde::{Deserialize, Serialize};

use crate::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

/// What a note write of a session in this mode does.
///
/// `Apply` writes the file. `Propose` records the write as a proposal and
/// leaves the disk unchanged, so the user accepts or rejects it later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    /// The note tools write the file.
    #[default]
    Apply,
    /// The note tools record a proposal. The file on disk does not change.
    Propose,
}

impl WriteMode {
    /// The write mode that an agent of `agent_type` can keep.
    ///
    /// The daemon runs the note tools of an internal agent, so it can hold
    /// their writes. An ACP agent writes with its own tools in its own
    /// process, so the daemon cannot hold the write. For that agent the
    /// effective value is always `Apply`, and the daemon attributes the write.
    pub fn effective_for(self, agent_type: &str) -> Self {
        if agent_type == "internal" {
            self
        } else {
            Self::Apply
        }
    }

    /// The wire name of the write mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Propose => "propose",
        }
    }

    /// The write mode with the wire name `s`, or `None` for another name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "apply" => Some(Self::Apply),
            "propose" => Some(Self::Propose),
            _ => None,
        }
    }
}

/// The modes the daemon ships.
///
/// *Not* the set of valid modes — those are open string ids, and a mode
/// declared in Lua or advertised by an ACP agent is legitimate without
/// appearing here. This is only what can be answered about a mode id with no
/// declaration in hand, gathered in one place so those questions are asked
/// once instead of by comparing string literals at each call site.
/// Mode ids that older sessions and user configs still name, and what each
/// means now.
///
/// A session persists its mode id, and neither the Lua registry nor the
/// handles have a fallback for an id they cannot find, so a rename turns
/// every session written before it into a hard failure. This table is what
/// absorbs that, and [`canonical_mode_id`] is the single place it is applied.
/// It is deliberately not merged into the advertised mode list: an alias
/// resolves, but offering it would show the same mode twice and put a dead
/// id in the mode cycle.
pub const DEPRECATED_MODE_ALIASES: &[(&str, &str)] = &[
    // `normal` named a position rather than a stance, which `plan` and `auto`
    // both do and which its own `permissions = "ask"` already said.
    ("normal", "ask"),
];

/// The id a mode is known by now, given an id from any era.
///
/// Unknown ids pass through unchanged — this canonicalises, it does not
/// validate, and a mode declared in Lua or advertised by an ACP agent is
/// legitimate without appearing in any table here.
pub fn canonical_mode_id(id: &str) -> &str {
    DEPRECATED_MODE_ALIASES
        .iter()
        .find(|(from, _)| *from == id)
        .map_or(id, |(_, to)| to)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinMode {
    /// Every tool is available, and each change is asked about first.
    Ask,
    /// Read-only exploration.
    Plan,
    /// Auto-approve every operation.
    Auto,
}

impl BuiltinMode {
    /// The shipped mode with this id, or `None` for a mode the daemon does not
    /// ship (declared elsewhere, or gone).
    pub fn from_id(id: &str) -> Option<Self> {
        match canonical_mode_id(id) {
            "ask" => Some(Self::Ask),
            "plan" => Some(Self::Plan),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Whether the mode confines the agent to the read-only tool set.
    ///
    /// This is what a tool-visibility fallback needs when no declaration is
    /// available for the mode.
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::Plan)
    }
}

/// A mode descriptor with UI presentation metadata
///
/// This type extends the ACP SessionMode with additional fields for UI display,
/// such as an icon. It can be created from a SessionMode for interoperability
/// with the ACP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeDescriptor {
    /// Unique identifier for the mode (e.g., "plan", "act")
    pub id: String,
    /// Human-readable name (e.g., "Plan Mode", "Act Mode")
    pub name: String,
    /// Optional description of the mode
    pub description: Option<String>,
    /// Optional icon for UI display (emoji or icon name)
    pub icon: Option<String>,
    /// What a note write in this mode does.
    ///
    /// Carries the *effective* value, see [`ModeDescriptor::degraded_for`].
    /// The default is [`WriteMode::Apply`], so a descriptor from an older
    /// daemon reads as the behavior that the older daemon has.
    #[serde(default)]
    pub writes: WriteMode,
}

impl ModeDescriptor {
    /// Degrade the write mode to what an agent of `agent_type` can keep, so
    /// a client shows the effective value and not the configured one.
    ///
    /// A mode that reads "proposes" on a session that writes the disk gives
    /// the user a false promise. The wire carries one field with the
    /// effective value, so a client has no two fields to reconcile.
    pub fn degraded_for(mut self, agent_type: &str) -> Self {
        self.writes = self.writes.effective_for(agent_type);
        self
    }
}

/// The wire shape of `session.list_modes`: which modes a session may enter,
/// and which one it is in now.
///
/// Both fields come from the same daemon call on purpose. A client that
/// fetched the list and the current mode separately can render a mode that is
/// not in its own list — exactly the state a restored session lands in when
/// its mode was declared in Lua the client has never seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionModes {
    /// The mode the session is in. Always present in `modes`.
    pub current_mode_id: String,
    /// Every mode the session may switch to, in declaration order.
    pub modes: Vec<ModeDescriptor>,
}

impl SessionModes {
    /// The mode after the current one, wrapping. `None` when the current
    /// mode is not in the list: see [`next_mode`].
    pub fn next_mode(&self) -> Option<&str> {
        next_after(&self.current_mode_id, &self.modes, |m| &m.id).map(|m| m.id.as_str())
    }
}

/// The mode after `current` in `available`, wrapping.
///
/// An empty list, or a current mode the daemon no longer offers, cycles
/// nowhere: advancing into a mode `set_mode` would reject is worse than
/// leaving the mode alone.
pub fn next_mode<'a, S: AsRef<str>>(current: &str, available: &'a [S]) -> Option<&'a str> {
    next_after(current, available, AsRef::as_ref).map(AsRef::as_ref)
}

fn next_after<'a, T>(current: &str, items: &'a [T], id: impl Fn(&T) -> &str) -> Option<&'a T> {
    let idx = items
        .iter()
        .position(|item| id(item).eq_ignore_ascii_case(current))?;
    items.get((idx + 1) % items.len())
}

impl From<SessionMode> for ModeDescriptor {
    fn from(mode: SessionMode) -> Self {
        Self::from(&mode)
    }
}

impl From<&SessionMode> for ModeDescriptor {
    fn from(mode: &SessionMode) -> Self {
        Self {
            id: mode.id.to_string(),
            name: mode.name.clone(),
            description: mode.description.clone(),
            icon: None,
            // ACP's `SessionMode` has no field for this. The Lua mode
            // declaration holds it, so `session.list_modes` sets it from the
            // mode registry after this conversion.
            writes: WriteMode::Apply,
        }
    }
}

/// Create default internal modes for internal agents
///
/// Returns a `SessionModeState` with the standard Normal/Plan/Auto modes.
/// This is used by internal agents that don't connect to an external ACP agent.
///
/// # Modes
///
/// - **ask**: Every tool, permission asked before each change (default)
/// - **plan**: Read-only exploration mode
/// - **auto**: Auto-approve all operations
///
/// # Example
///
/// ```rust
/// use crucible_core::types::mode::default_internal_modes;
///
/// let modes = default_internal_modes();
/// assert_eq!(modes.current_mode_id.0.as_ref(), "ask");
/// assert_eq!(modes.available_modes.len(), 3);
/// ```
pub fn default_internal_modes() -> SessionModeState {
    SessionModeState::new(
        SessionModeId::new("ask"),
        vec![
            SessionMode::new(SessionModeId::new("ask"), "Ask".to_string())
                .description("Ask before each change".to_string()),
            SessionMode::new(SessionModeId::new("plan"), "Plan".to_string())
                .description("Read-only exploration mode".to_string()),
            SessionMode::new(SessionModeId::new("auto"), "Auto".to_string())
                .description("Auto-approve all operations".to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper to create SessionMode (workaround for non_exhaustive)
    fn test_session_mode(id: &str, name: &str, description: Option<&str>) -> SessionMode {
        serde_json::from_value(json!({
            "id": id,
            "name": name,
            "description": description,
            "_meta": null,
        }))
        .expect("Failed to create test SessionMode")
    }

    #[test]
    fn test_mode_descriptor_from_session_mode() {
        let session_mode =
            test_session_mode("plan", "Plan Mode", Some("Read-only exploration mode"));

        let descriptor: ModeDescriptor = session_mode.into();

        assert_eq!(descriptor.id, "plan");
        assert_eq!(descriptor.name, "Plan Mode");
        assert_eq!(
            descriptor.description,
            Some("Read-only exploration mode".to_string())
        );
        assert_eq!(descriptor.icon, None);
    }

    #[test]
    fn test_mode_descriptor_from_session_mode_ref() {
        let session_mode = test_session_mode("act", "Act Mode", None);

        let descriptor: ModeDescriptor = (&session_mode).into();

        assert_eq!(descriptor.id, "act");
        assert_eq!(descriptor.name, "Act Mode");
        assert_eq!(descriptor.description, None);
    }

    #[test]
    fn test_mode_descriptor_serialization() {
        let mode = ModeDescriptor {
            id: "ask".to_string(),
            name: "Ask".to_string(),
            description: Some("desc".to_string()),
            icon: Some("⚡".to_string()),
            writes: WriteMode::Propose,
        };

        let json = serde_json::to_string(&mode).unwrap();
        let restored: ModeDescriptor = serde_json::from_str(&json).unwrap();

        assert_eq!(mode, restored);
    }

    // ========================================================================
    // Phase 1: Tests for default_internal_modes
    // ========================================================================

    #[test]
    fn test_default_internal_modes_creates_three_modes() {
        let state = default_internal_modes();
        assert_eq!(state.available_modes.len(), 3);
    }

    #[test]
    fn test_default_internal_modes_current_is_ask() {
        let state = default_internal_modes();
        assert_eq!(state.current_mode_id.0.as_ref(), "ask");
    }

    #[test]
    fn test_default_internal_modes_has_all_names_and_descriptions() {
        let state = default_internal_modes();

        for mode in &state.available_modes {
            assert!(
                !mode.name.is_empty(),
                "Mode {} should have a name",
                mode.id.0
            );
            assert!(
                mode.description.is_some(),
                "Mode {} should have a description",
                mode.id.0
            );
        }
    }

    // ========================================================================
    // Write mode
    // ========================================================================

    /// The conversion has no `writes` source, so it gives `apply`. The
    /// `session.list_modes` handler then sets the value from the Lua mode.
    #[test]
    fn descriptor_from_session_mode_applies_its_writes() {
        let descriptor: ModeDescriptor = (&test_session_mode("auto", "Auto", None)).into();
        assert_eq!(descriptor.writes, WriteMode::Apply);
    }

    /// The wire carries the write mode and no review policy.
    #[test]
    fn a_descriptor_on_the_wire_names_no_review_policy() {
        let descriptor = ModeDescriptor::from(&test_session_mode("ask", "Ask", None));
        let value = serde_json::to_value(&descriptor).unwrap();
        assert_eq!(value["writes"], json!("apply"));
        assert!(
            value.get("review_policy").is_none(),
            "the descriptor still sends a review policy: {value}"
        );
    }

    /// A daemon older than this change sends `review_policy`. A client
    /// ignores that field and reads the write mode.
    #[test]
    fn a_descriptor_with_an_old_review_policy_still_deserialises() {
        let restored: ModeDescriptor = serde_json::from_value(json!({
            "id": "propose",
            "name": "Propose",
            "description": null,
            "icon": null,
            "color": null,
            "review_policy": "pre_write",
            "writes": "propose",
        }))
        .expect("descriptor with review_policy must deserialise");
        assert_eq!(restored.writes, WriteMode::Propose);
    }

    /// An internal agent keeps a `propose` mode.
    #[test]
    fn descriptor_degraded_for_an_internal_agent_keeps_propose() {
        let mut descriptor = ModeDescriptor::from(&test_session_mode("propose", "Propose", None));
        descriptor.writes = WriteMode::Propose;
        assert_eq!(
            descriptor.degraded_for("internal").writes,
            WriteMode::Propose
        );
    }

    /// An ACP agent writes with its own tools, so the daemon cannot hold the
    /// write for a proposal.
    #[test]
    fn write_mode_is_apply_for_acp() {
        assert_eq!(WriteMode::Propose.effective_for("acp"), WriteMode::Apply);
        assert_eq!(WriteMode::Apply.effective_for("acp"), WriteMode::Apply);
        assert_eq!(
            WriteMode::Propose.effective_for("internal"),
            WriteMode::Propose
        );
        let mut descriptor = ModeDescriptor::from(&test_session_mode("propose", "Propose", None));
        descriptor.writes = WriteMode::Propose;
        assert_eq!(descriptor.degraded_for("acp").writes, WriteMode::Apply);
    }

    /// A descriptor from a daemon older than the field reads as `apply`.
    #[test]
    fn a_descriptor_without_writes_reads_as_apply() {
        let restored: ModeDescriptor = serde_json::from_value(json!({
            "id": "ask",
            "name": "Ask",
            "description": null,
            "icon": null,
            "color": null,
        }))
        .expect("descriptor without writes must deserialise");

        assert_eq!(restored.writes, WriteMode::Apply);
        assert_eq!(
            serde_json::to_value(WriteMode::Propose).unwrap(),
            json!("propose")
        );
        for mode in [WriteMode::Apply, WriteMode::Propose] {
            assert_eq!(WriteMode::parse(mode.as_str()), Some(mode));
            assert_eq!(serde_json::to_value(mode).unwrap(), json!(mode.as_str()));
        }
    }

    #[test]
    fn plan_is_the_only_shipped_read_only_mode() {
        assert!(BuiltinMode::from_id("plan").unwrap().is_read_only());
        assert!(!BuiltinMode::from_id("ask").unwrap().is_read_only());
        assert!(!BuiltinMode::from_id("auto").unwrap().is_read_only());
        assert!(BuiltinMode::from_id("architect").is_none());
    }

    #[test]
    fn test_default_internal_modes_mode_ids() {
        let state = default_internal_modes();
        let ids: Vec<_> = state
            .available_modes
            .iter()
            .map(|m| m.id.0.as_ref())
            .collect();

        assert!(ids.contains(&"ask"));
        assert!(ids.contains(&"plan"));
        assert!(ids.contains(&"auto"));
    }
}
