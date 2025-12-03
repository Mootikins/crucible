//! Session Orchestrator - Interactive Chat Loop
//!
//! Orchestrates the interactive chat session, handling user input, command execution,
//! message processing, and agent communication. Extracted from commands/chat.rs for
//! reusability and testability.

use anyhow::Result;
use std::sync::Arc;

use crate::acp::ContextEnricher;
use crate::chat::{ChatAgent, ChatMode};
use crate::core_facade::CrucibleCoreFacade;

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Initial chat mode (Plan/Act/AutoApprove)
    pub initial_mode: ChatMode,
    /// Enable context enrichment for messages
    pub context_enabled: bool,
    /// Number of context results to include (if context enabled)
    pub context_size: Option<usize>,
}

impl SessionConfig {
    /// Create a new session configuration
    pub fn new(initial_mode: ChatMode, context_enabled: bool, context_size: Option<usize>) -> Self {
        Self {
            initial_mode,
            context_enabled,
            context_size,
        }
    }

    /// Create default configuration (Plan mode, context enabled, 5 results)
    pub fn default() -> Self {
        Self {
            initial_mode: ChatMode::Plan,
            context_enabled: true,
            context_size: Some(5),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // TODO: Add validation logic
        Ok(())
    }
}

/// Interactive chat session orchestrator
pub struct ChatSession {
    config: SessionConfig,
    core: Arc<CrucibleCoreFacade>,
    enricher: ContextEnricher,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new(config: SessionConfig, core: Arc<CrucibleCoreFacade>) -> Self {
        let context_size = config.context_size.unwrap_or(5);
        let enricher = ContextEnricher::new(core.clone(), Some(context_size));

        Self {
            config,
            core,
            enricher,
        }
    }

    /// Run the interactive session loop
    pub async fn run<A: ChatAgent>(&self, agent: &mut A) -> Result<()> {
        // TODO: Implement interactive loop
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SessionConfig tests
    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new(ChatMode::Plan, true, Some(10));
        assert_eq!(config.initial_mode, ChatMode::Plan);
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(10));
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.initial_mode, ChatMode::Plan);
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(5));
    }

    #[test]
    fn test_session_config_clone() {
        let config = SessionConfig::new(ChatMode::Act, false, None);
        let cloned = config.clone();
        assert_eq!(config.initial_mode, cloned.initial_mode);
        assert_eq!(config.context_enabled, cloned.context_enabled);
        assert_eq!(config.context_size, cloned.context_size);
    }

    #[test]
    fn test_session_config_validate_success() {
        let config = SessionConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_context_disabled_no_size() {
        let config = SessionConfig::new(ChatMode::Plan, false, None);
        assert!(!config.context_enabled);
        assert_eq!(config.context_size, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_all_modes() {
        let plan_config = SessionConfig::new(ChatMode::Plan, true, Some(5));
        assert_eq!(plan_config.initial_mode, ChatMode::Plan);

        let act_config = SessionConfig::new(ChatMode::Act, true, Some(5));
        assert_eq!(act_config.initial_mode, ChatMode::Act);

        let auto_config = SessionConfig::new(ChatMode::AutoApprove, true, Some(5));
        assert_eq!(auto_config.initial_mode, ChatMode::AutoApprove);
    }

    // ChatSession creation tests
    // Note: Full session tests require mock agent and core facade
    // These will be added when we implement the run() method
}
