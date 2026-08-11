//! Mode descriptor types for UI presentation
//!
//! This module contains types for describing chat modes with UI metadata.
//! `ModeDescriptor` wraps ACP's `SessionMode` with additional display information.
//!
//! # Default Mode
//!
//! The default mode is `normal` (full read/write access). This follows the pattern
//! of other AI coding assistants (OpenCode's "build", Codex's "workspace-write")
//! where developers expect to code by default, not just read.
//!
//! Use `plan` mode for read-only exploration when you want to prevent modifications.

use serde::{Deserialize, Serialize};

use crate::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

/// How much review a mode asks for before the agent writes.
///
/// Ordered weakest to strongest, and that order is load-bearing: a session's
/// effective policy is `min(what the mode asks for, what the agent can
/// enforce)`, so a mode can never ask for more gating than is deliverable.
/// See [`ReviewPolicy::effective_for`].
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicy {
    /// No gate at all. Changes are still attributed; nothing ever waits.
    None,
    /// Never blocks a tool call — the review queue surfaces at turn end.
    ///
    /// Auto-approve should mean "don't interrupt me", not "don't review".
    PostTurn,
    /// A writing tool call waits while its target file has unreviewed hunks.
    #[default]
    PreWrite,
}

impl ReviewPolicy {
    /// The policy a mode id asks for.
    ///
    /// Mode ids are open strings, so this answers for the modes the daemon
    /// ships and falls back to the conservative [`ReviewPolicy::PreWrite`] for
    /// everything else: a mode declared in Lua or advertised by an ACP agent
    /// is gated until it says otherwise.
    pub fn for_mode_id(id: &str) -> Self {
        BuiltinMode::from_id(id).map_or(Self::default(), BuiltinMode::review_policy)
    }

    /// The strongest policy an agent of this type can actually enforce.
    ///
    /// The daemon dispatches an internal agent's tools, so a gate placed
    /// before dispatch genuinely holds the write back. An ACP agent runs its
    /// own tools in its own process and only reports them afterwards —
    /// blocking there would announce a refusal for an edit that is already on
    /// disk, which is strictly worse than not gating. Same rule and same
    /// reason as `session_lifecycle::unenforceable_reason`.
    ///
    /// Attribution is unaffected: the ledger watches filesystem state, so an
    /// ACP session still gets a full composed diff. Only the blocking is
    /// unenforceable.
    pub fn enforceable_by(agent_type: &str) -> Self {
        if agent_type == "internal" {
            Self::PreWrite
        } else {
            Self::PostTurn
        }
    }

    /// This policy degraded to what an agent of `agent_type` can enforce.
    pub fn effective_for(self, agent_type: &str) -> Self {
        self.min(Self::enforceable_by(agent_type))
    }
}

/// The modes the daemon ships.
///
/// *Not* the set of valid modes — those are open string ids, and a mode
/// declared in Lua or advertised by an ACP agent is legitimate without
/// appearing here. This is only what can be answered about a mode id with no
/// declaration in hand, gathered in one place so those questions are asked
/// once instead of by comparing string literals at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinMode {
    /// Full read/write access.
    Normal,
    /// Read-only exploration.
    Plan,
    /// Auto-approve every operation.
    Auto,
}

impl BuiltinMode {
    /// The shipped mode with this id, or `None` for a mode the daemon does not
    /// ship (declared elsewhere, or gone).
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "normal" => Some(Self::Normal),
            "plan" => Some(Self::Plan),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Whether the mode confines the agent to the read-only tool set.
    ///
    /// This is what a tool-visibility fallback needs when no declaration is
    /// available for the mode — a different question from
    /// [`BuiltinMode::review_policy`], which happens to agree here only
    /// because there is nothing to review in a mode that cannot write.
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::Plan)
    }

    /// How much review this mode asks for.
    pub fn review_policy(self) -> ReviewPolicy {
        match self {
            // Read-only, so the gate is vacuous rather than merely disabled.
            Self::Plan => ReviewPolicy::None,
            Self::Normal => ReviewPolicy::PreWrite,
            // Auto keeps the receipt and surfaces the queue at turn end,
            // instead of leaving changes unexamined forever.
            Self::Auto => ReviewPolicy::PostTurn,
        }
    }
}

/// A mode descriptor with UI presentation metadata
///
/// This type extends the ACP SessionMode with additional fields for UI display,
/// such as icon and color. It can be created from a SessionMode for interoperability
/// with the ACP protocol.
///
/// # Example
///
/// ```rust
/// use crucible_core::types::mode::ModeDescriptor;
///
/// let mode = ModeDescriptor::new("plan", "Plan Mode")
///     .with_description("Read-only exploration mode")
///     .with_icon("📖")
///     .with_color("#3b82f6");
///
/// assert_eq!(mode.id, "plan");
/// assert_eq!(mode.name, "Plan Mode");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeDescriptor {
    /// Unique identifier for the mode (e.g., "plan", "act")
    pub id: String,
    /// Human-readable name (e.g., "Plan Mode", "Act Mode")
    pub name: String,
    /// Optional description of the mode
    pub description: Option<String>,
    /// Optional icon for UI display (emoji or icon name)
    pub icon: Option<String>,
    /// Optional color for UI display (hex color code)
    pub color: Option<String>,
    /// How much review this mode asks for before the agent writes.
    ///
    /// Carries the *effective* policy — see [`ModeDescriptor::degraded_for`].
    /// `#[serde(default)]` so a client older than the field still deserialises;
    /// the default is the conservative [`ReviewPolicy::PreWrite`].
    #[serde(default)]
    pub review_policy: ReviewPolicy,
}

impl ModeDescriptor {
    /// Create a new mode descriptor with required fields
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: name.into(),
            description: None,
            icon: None,
            color: None,
            review_policy: ReviewPolicy::for_mode_id(&id),
            id,
        }
    }

    /// Degrade the policy to what an agent of `agent_type` can actually
    /// enforce, so a client renders the effective policy and not the
    /// configured one.
    ///
    /// A mode chip reading "gated" on a session that cannot gate is a lie
    /// about a safety property. That is why the wire carries one field holding
    /// the effective value rather than two fields a client has to reconcile.
    pub fn degraded_for(mut self, agent_type: &str) -> Self {
        self.review_policy = self.review_policy.effective_for(agent_type);
        self
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the color
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
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
pub struct SessionModes {
    /// The mode the session is in. Always present in `modes`.
    pub current_mode_id: String,
    /// Every mode the session may switch to, in declaration order.
    pub modes: Vec<ModeDescriptor>,
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
            color: None,
            // ACP's `SessionMode` is `#[non_exhaustive]` upstream and has no
            // policy field, so the policy cannot ride the conversion — it is
            // derived from the id here. Without this, every mode reaching a
            // client through `session.list_modes` would report the default and
            // the chip would be wrong for `plan` and `auto`.
            review_policy: ReviewPolicy::for_mode_id(&mode.id.0),
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
/// - **normal**: Full read/write access (default)
/// - **plan**: Read-only exploration mode
/// - **auto**: Auto-approve all operations
///
/// # Example
///
/// ```rust
/// use crucible_core::types::mode::default_internal_modes;
///
/// let modes = default_internal_modes();
/// assert_eq!(modes.current_mode_id.0.as_ref(), "normal");
/// assert_eq!(modes.available_modes.len(), 3);
/// ```
pub fn default_internal_modes() -> SessionModeState {
    SessionModeState::new(
        SessionModeId::new("normal"),
        vec![
            SessionMode::new(SessionModeId::new("normal"), "Normal".to_string())
                .description("Full read/write access".to_string()),
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
    fn test_mode_descriptor_new() {
        let mode = ModeDescriptor::new("normal", "Normal Mode");

        assert_eq!(mode.id, "normal");
        assert_eq!(mode.name, "Normal Mode");
        assert_eq!(mode.description, None);
        assert_eq!(mode.icon, None);
        assert_eq!(mode.color, None);
    }

    #[test]
    fn test_mode_descriptor_with_all_fields() {
        let mode = ModeDescriptor::new("act", "Act Mode")
            .with_description("Write-enabled mode")
            .with_icon("✏️")
            .with_color("#22c55e");

        assert_eq!(mode.id, "act");
        assert_eq!(mode.name, "Act Mode");
        assert_eq!(mode.description, Some("Write-enabled mode".to_string()));
        assert_eq!(mode.icon, Some("✏️".to_string()));
        assert_eq!(mode.color, Some("#22c55e".to_string()));
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
        assert_eq!(descriptor.color, None);
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
    fn test_mode_descriptor_equality() {
        let mode1 = ModeDescriptor::new("normal", "Normal Mode").with_icon("⚡");
        let mode2 = ModeDescriptor::new("normal", "Normal Mode").with_icon("⚡");
        let mode3 = ModeDescriptor::new("plan", "Plan Mode");

        assert_eq!(mode1, mode2);
        assert_ne!(mode1, mode3);
    }

    #[test]
    fn test_mode_descriptor_serialization() {
        let mode = ModeDescriptor::new("normal", "Normal")
            .with_description("desc")
            .with_icon("⚡")
            .with_color("#000");

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
    fn test_default_internal_modes_current_is_normal() {
        let state = default_internal_modes();
        assert_eq!(state.current_mode_id.0.as_ref(), "normal");
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
    // Review policy
    // ========================================================================

    #[test]
    fn plan_mode_asks_for_no_review() {
        assert_eq!(ReviewPolicy::for_mode_id("plan"), ReviewPolicy::None);
    }

    #[test]
    fn normal_mode_gates_before_the_write() {
        assert_eq!(ReviewPolicy::for_mode_id("normal"), ReviewPolicy::PreWrite);
    }

    #[test]
    fn auto_mode_reviews_after_the_turn_instead_of_never() {
        assert_eq!(ReviewPolicy::for_mode_id("auto"), ReviewPolicy::PostTurn);
    }

    /// A mode nobody declared may be anything, so it gates. Deleting a
    /// declaration must not be a way to turn review off.
    #[test]
    fn an_undeclared_mode_gates_conservatively() {
        assert_eq!(
            ReviewPolicy::for_mode_id("architect"),
            ReviewPolicy::PreWrite
        );
        assert_eq!(ReviewPolicy::for_mode_id(""), ReviewPolicy::PreWrite);
    }

    /// The daemon dispatches an internal agent's tools, so a pre-write gate
    /// genuinely holds the write back.
    #[test]
    fn an_internal_agent_can_gate_before_the_write() {
        assert_eq!(
            ReviewPolicy::PreWrite.effective_for("internal"),
            ReviewPolicy::PreWrite
        );
    }

    /// An ACP agent already ran the tool by the time we hear about it, so
    /// PreWrite degrades rather than pretending to block.
    #[test]
    fn an_external_agent_degrades_prewrite_to_postturn() {
        assert_eq!(
            ReviewPolicy::PreWrite.effective_for("acp"),
            ReviewPolicy::PostTurn
        );
    }

    /// Degrading is a floor, never a ceiling: a mode that asked for nothing
    /// does not acquire a gate by running on an external agent.
    #[test]
    fn degrading_never_strengthens_a_policy() {
        assert_eq!(
            ReviewPolicy::None.effective_for("acp"),
            ReviewPolicy::None,
            "plan mode must stay ungated on an external agent"
        );
        assert_eq!(
            ReviewPolicy::PostTurn.effective_for("acp"),
            ReviewPolicy::PostTurn
        );
        assert_eq!(
            ReviewPolicy::None.effective_for("internal"),
            ReviewPolicy::None
        );
    }

    #[test]
    fn descriptor_derives_its_policy_from_the_mode_id() {
        assert_eq!(
            ModeDescriptor::new("plan", "Plan").review_policy,
            ReviewPolicy::None
        );
        assert_eq!(
            ModeDescriptor::new("auto", "Auto").review_policy,
            ReviewPolicy::PostTurn
        );
        assert_eq!(
            ModeDescriptor::new("normal", "Normal").review_policy,
            ReviewPolicy::PreWrite
        );
    }

    /// The conversion is the only source `session.list_modes` has, so a policy
    /// that did not survive it would leave every mode reporting the default.
    #[test]
    fn descriptor_from_session_mode_carries_the_policy() {
        let descriptor: ModeDescriptor = (&test_session_mode("auto", "Auto", None)).into();
        assert_eq!(descriptor.review_policy, ReviewPolicy::PostTurn);
    }

    #[test]
    fn descriptor_degraded_for_an_external_agent_reports_the_effective_policy() {
        let descriptor = ModeDescriptor::new("normal", "Normal").degraded_for("acp");
        assert_eq!(descriptor.review_policy, ReviewPolicy::PostTurn);
    }

    /// Wire compatibility: a client that predates the field must still
    /// deserialise, and must land on the conservative value.
    #[test]
    fn a_descriptor_without_a_policy_field_defaults_to_gating() {
        let restored: ModeDescriptor = serde_json::from_value(json!({
            "id": "normal",
            "name": "Normal",
            "description": null,
            "icon": null,
            "color": null,
        }))
        .expect("descriptor without review_policy must deserialise");

        assert_eq!(restored.review_policy, ReviewPolicy::PreWrite);
    }

    #[test]
    fn review_policy_wire_names_are_snake_case() {
        assert_eq!(
            serde_json::to_value(ReviewPolicy::PostTurn).unwrap(),
            json!("post_turn")
        );
        assert_eq!(
            serde_json::to_value(ReviewPolicy::PreWrite).unwrap(),
            json!("pre_write")
        );
        assert_eq!(
            serde_json::to_value(ReviewPolicy::None).unwrap(),
            json!("none")
        );
    }

    #[test]
    fn plan_is_the_only_shipped_read_only_mode() {
        assert!(BuiltinMode::from_id("plan").unwrap().is_read_only());
        assert!(!BuiltinMode::from_id("normal").unwrap().is_read_only());
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

        assert!(ids.contains(&"normal"));
        assert!(ids.contains(&"plan"));
        assert!(ids.contains(&"auto"));
    }
}
