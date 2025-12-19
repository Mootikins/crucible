//! Mode descriptor types for UI presentation
//!
//! This module contains types for describing chat modes with UI metadata.
//! `ModeDescriptor` wraps ACP's `SessionMode` with additional display information.

use serde::{Deserialize, Serialize};

use crate::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

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
}

impl ModeDescriptor {
    /// Create a new mode descriptor with required fields
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            icon: None,
            color: None,
        }
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

impl From<SessionMode> for ModeDescriptor {
    fn from(mode: SessionMode) -> Self {
        Self {
            id: mode.id.to_string(),
            name: mode.name,
            description: mode.description,
            icon: None,
            color: None,
        }
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
        }
    }
}

/// Create default internal modes for internal agents
///
/// Returns a `SessionModeState` with the standard Plan/Act/Auto modes.
/// This is used by internal agents that don't connect to an external ACP agent.
///
/// # Modes
///
/// - **plan**: Read-only exploration mode
/// - **act**: Write-enabled execution mode
/// - **auto**: Auto-approve all operations
///
/// # Example
///
/// ```rust
/// use crucible_core::types::mode::default_internal_modes;
///
/// let modes = default_internal_modes();
/// assert_eq!(modes.current_mode_id.0.as_ref(), "plan");
/// assert_eq!(modes.available_modes.len(), 3);
/// ```
pub fn default_internal_modes() -> SessionModeState {
    SessionModeState::new(
        SessionModeId::new("plan"),
        vec![
            SessionMode::new(SessionModeId::new("plan"), "Plan".to_string())
                .description("Read-only exploration mode".to_string()),
            SessionMode::new(SessionModeId::new("act"), "Act".to_string())
                .description("Write-enabled execution mode".to_string()),
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
        let mode = ModeDescriptor::new("plan", "Plan Mode");

        assert_eq!(mode.id, "plan");
        assert_eq!(mode.name, "Plan Mode");
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
        let mode1 = ModeDescriptor::new("plan", "Plan Mode").with_icon("📖");
        let mode2 = ModeDescriptor::new("plan", "Plan Mode").with_icon("📖");
        let mode3 = ModeDescriptor::new("act", "Act Mode");

        assert_eq!(mode1, mode2);
        assert_ne!(mode1, mode3);
    }

    #[test]
    fn test_mode_descriptor_serialization() {
        let mode = ModeDescriptor::new("plan", "Plan")
            .with_description("desc")
            .with_icon("📖")
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
    fn test_default_internal_modes_current_is_plan() {
        let state = default_internal_modes();
        assert_eq!(state.current_mode_id.0.as_ref(), "plan");
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

    #[test]
    fn test_default_internal_modes_mode_ids() {
        let state = default_internal_modes();
        let ids: Vec<_> = state
            .available_modes
            .iter()
            .map(|m| m.id.0.as_ref())
            .collect();

        assert!(ids.contains(&"plan"));
        assert!(ids.contains(&"act"));
        assert!(ids.contains(&"auto"));
    }
}
