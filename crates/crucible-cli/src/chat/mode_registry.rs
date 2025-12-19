//! Mode registry for managing chat modes
//!
//! Provides a registry for chat modes that stores agent-provided modes
//! from the ACP SessionModeState. The registry no longer has built-in
//! "reserved" modes - all modes come from the connected agent.

use crucible_core::types::{ModeDescriptor, SessionModeState};
use thiserror::Error;

/// Errors that can occur during mode operations
#[derive(Debug, Clone, Error)]
pub enum ModeError {
    /// The requested mode does not exist
    #[error("Invalid mode: {0}")]
    InvalidMode(String),
}

/// Result type for mode operations
pub type ModeResult<T> = Result<T, ModeError>;

/// Registry of available chat modes
///
/// This registry stores modes provided by the connected agent via ACP.
/// There are no built-in "reserved" modes - all modes come from the agent.
#[derive(Debug, Clone)]
pub struct ModeRegistry {
    /// Agent-provided modes (from ACP SessionModeState)
    modes: Option<SessionModeState>,
    /// Current active mode ID
    current_mode_id: String,
}

impl ModeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            modes: None,
            current_mode_id: String::new(),
        }
    }

    /// Create a registry from an agent's SessionModeState
    pub fn from_agent(state: SessionModeState) -> Self {
        let current_mode_id = state.current_mode_id.0.to_string();
        Self {
            modes: Some(state),
            current_mode_id,
        }
    }

    /// Find a mode by ID, returning a ModeDescriptor if found
    pub fn find(&self, id: &str) -> Option<ModeDescriptor> {
        self.modes.as_ref().and_then(|state| {
            state
                .available_modes
                .iter()
                .find(|m| m.id.0.as_ref() == id)
                .map(ModeDescriptor::from)
        })
    }

    /// Check if a mode exists
    pub fn exists(&self, id: &str) -> bool {
        self.modes
            .as_ref()
            .map(|state| state.available_modes.iter().any(|m| m.id.0.as_ref() == id))
            .unwrap_or(false)
    }

    /// Get the current mode as a ModeDescriptor
    pub fn current(&self) -> ModeDescriptor {
        self.find(&self.current_mode_id)
            .unwrap_or_else(|| ModeDescriptor::new("unknown", "Unknown"))
    }

    /// Get the current mode ID
    pub fn current_id(&self) -> &str {
        &self.current_mode_id
    }

    /// List all available modes as ModeDescriptors
    pub fn list_all(&self) -> Vec<ModeDescriptor> {
        self.modes
            .as_ref()
            .map(|state| {
                state
                    .available_modes
                    .iter()
                    .map(ModeDescriptor::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Set the current mode by ID
    ///
    /// Returns an error if the mode doesn't exist in the registry.
    pub fn set_mode(&mut self, id: &str) -> ModeResult<()> {
        if !self.exists(id) {
            return Err(ModeError::InvalidMode(id.to_string()));
        }
        self.current_mode_id = id.to_string();
        Ok(())
    }

    /// Update the registry with new modes from an agent
    ///
    /// This replaces the existing modes with the new state.
    pub fn update(&mut self, state: SessionModeState) {
        self.current_mode_id = state.current_mode_id.0.to_string();
        self.modes = Some(state);
    }

    /// Check if the registry is empty (no modes available)
    pub fn is_empty(&self) -> bool {
        self.modes
            .as_ref()
            .map(|s| s.available_modes.is_empty())
            .unwrap_or(true)
    }

    /// Get the underlying SessionModeState if available
    pub fn agent_state(&self) -> Option<&SessionModeState> {
        self.modes.as_ref()
    }
}

impl Default for ModeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::{SessionMode, SessionModeId};
    use serde_json::json;
    use std::sync::Arc;

    // Helper to create SessionMode (workaround for non_exhaustive)
    fn test_session_mode(
        id: &str,
        name: &str,
        description: Option<&str>,
    ) -> SessionMode {
        serde_json::from_value(json!({
            "id": id,
            "name": name,
            "description": description,
            "_meta": null,
        }))
        .expect("Failed to create test SessionMode")
    }

    // Helper to create a test SessionModeState
    fn test_agent_mode_state() -> SessionModeState {
        serde_json::from_value(json!({
            "currentModeId": "plan",
            "availableModes": [
                {"id": "plan", "name": "Plan", "description": "Read-only exploration mode", "_meta": null},
                {"id": "act", "name": "Act", "description": "Write-enabled execution mode", "_meta": null},
                {"id": "auto", "name": "Auto", "description": "Auto-approve all operations", "_meta": null},
            ],
            "_meta": null,
        }))
        .expect("Failed to create test SessionModeState")
    }

    // Helper to create a custom agent mode state
    fn custom_agent_mode_state() -> SessionModeState {
        serde_json::from_value(json!({
            "currentModeId": "custom",
            "availableModes": [
                {"id": "custom", "name": "Custom Mode", "description": "Custom agent mode", "_meta": null},
                {"id": "special", "name": "Special", "description": null, "_meta": null},
            ],
            "_meta": null,
        }))
        .expect("Failed to create custom SessionModeState")
    }

    // 3.1.1: Simplified registry tests
    #[test]
    fn test_new_creates_empty_registry() {
        let registry = ModeRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.current_id(), "");
    }

    #[test]
    fn test_from_agent_stores_agent_modes() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        assert!(!registry.is_empty());
        assert!(registry.exists("plan"));
        assert!(registry.exists("act"));
        assert!(registry.exists("auto"));
    }

    #[test]
    fn test_no_reserved_field_exists() {
        // Test that registry only stores agent modes, not reserved modes
        let registry = ModeRegistry::new();
        // The registry should be empty - no built-in modes
        assert!(registry.is_empty());
        assert!(!registry.exists("plan"));
    }

    // 3.2.1: Registry from agent tests
    #[test]
    fn test_from_agent_sets_current_from_state() {
        let state = custom_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        assert_eq!(registry.current_id(), "custom");
    }

    #[test]
    fn test_list_all_returns_all_agent_modes() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        let all = registry.list_all();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|m| m.id == "plan"));
        assert!(all.iter().any(|m| m.id == "act"));
        assert!(all.iter().any(|m| m.id == "auto"));
    }

    #[test]
    fn test_find_searches_agent_modes() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        let mode = registry.find("plan");
        assert!(mode.is_some());
        assert_eq!(mode.unwrap().name, "Plan");
    }

    #[test]
    fn test_find_returns_none_when_not_exists() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        assert!(registry.find("nonexistent").is_none());
    }

    // 3.2.3: Updated method tests
    #[test]
    fn test_exists_checks_modes_only() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        assert!(registry.exists("plan"));
        assert!(registry.exists("act"));
        assert!(!registry.exists("nonexistent"));
    }

    #[test]
    fn test_set_mode_validates_against_modes() {
        let state = test_agent_mode_state();
        let mut registry = ModeRegistry::from_agent(state);
        assert!(registry.set_mode("act").is_ok());
        assert_eq!(registry.current_id(), "act");
    }

    #[test]
    fn test_set_mode_returns_error_for_invalid() {
        let state = test_agent_mode_state();
        let mut registry = ModeRegistry::from_agent(state);
        let result = registry.set_mode("invalid");
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("expected error"),
            Err(ModeError::InvalidMode(id)) => assert_eq!(id, "invalid"),
        }
    }

    #[test]
    fn test_update_replaces_modes() {
        let state1 = test_agent_mode_state();
        let mut registry = ModeRegistry::from_agent(state1);
        assert!(registry.exists("plan"));

        // Update with custom modes
        let state2 = custom_agent_mode_state();
        registry.update(state2);

        // Old modes should be gone, new modes should be present
        assert!(!registry.exists("plan"));
        assert!(registry.exists("custom"));
        assert!(registry.exists("special"));
        // Current should be updated from new state
        assert_eq!(registry.current_id(), "custom");
    }

    #[test]
    fn test_current_returns_mode_descriptor() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        let current = registry.current();
        assert_eq!(current.id, "plan");
        assert_eq!(current.name, "Plan");
    }

    #[test]
    fn test_current_returns_unknown_for_empty_registry() {
        let registry = ModeRegistry::new();
        let current = registry.current();
        assert_eq!(current.id, "unknown");
        assert_eq!(current.name, "Unknown");
    }

    // 3.3.1: Default trait tests
    #[test]
    fn test_default_creates_empty_registry() {
        let registry = ModeRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_is_empty_returns_true_for_default() {
        let registry = ModeRegistry::default();
        assert!(registry.is_empty());
        assert_eq!(registry.current_id(), "");
    }

    // Edge case tests
    #[test]
    fn test_empty_registry_operations() {
        let mut registry = ModeRegistry::new();
        assert!(registry.find("plan").is_none());
        assert!(!registry.exists("plan"));
        assert!(registry.set_mode("plan").is_err());
        assert_eq!(registry.list_all().len(), 0);
    }

    #[test]
    fn test_update_on_empty_registry() {
        let mut registry = ModeRegistry::new();
        let state = test_agent_mode_state();
        registry.update(state);
        assert!(!registry.is_empty());
        assert!(registry.exists("plan"));
    }

    #[test]
    fn test_list_all_converts_to_mode_descriptor() {
        let state = test_agent_mode_state();
        let registry = ModeRegistry::from_agent(state);
        let all = registry.list_all();

        // Verify they're ModeDescriptor types with proper fields
        let plan = all.iter().find(|m| m.id == "plan").unwrap();
        assert_eq!(plan.name, "Plan");
        assert_eq!(
            plan.description,
            Some("Read-only exploration mode".to_string())
        );
    }
}
