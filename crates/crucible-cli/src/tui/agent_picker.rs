//! Agent selection types for the TUI
//!
//! Provides AgentSelection enum for specifying which agent to use.

/// Result of agent selection
#[derive(Debug, Clone)]
pub enum AgentSelection {
    /// User selected an ACP agent by name
    Acp(String),
    /// User selected the internal agent
    Internal,
    /// User cancelled (quit)
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_selection_variants() {
        let acp = AgentSelection::Acp("opencode".to_string());
        assert!(matches!(acp, AgentSelection::Acp(_)));

        let internal = AgentSelection::Internal;
        assert!(matches!(internal, AgentSelection::Internal));

        let cancelled = AgentSelection::Cancelled;
        assert!(matches!(cancelled, AgentSelection::Cancelled));
    }
}
