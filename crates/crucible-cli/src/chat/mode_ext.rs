//! CLI-specific extensions for ChatMode
//!
//! Provides terminal display utilities for ChatMode enum from crucible-core.
//! This separates UI concerns (icons, labels) from core business logic.

use crucible_core::traits::chat::ChatMode;

/// CLI display extensions for ChatMode
///
/// Provides terminal-specific formatting for chat modes:
/// - Icons for visual identification
/// - Display names for prompts
/// - Descriptions for help text
pub trait ChatModeDisplay {
    /// Get the display name for prompts
    ///
    /// Returns lowercase name suitable for terminal prompts.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crucible_cli::chat::{ChatMode, ChatModeDisplay};
    ///
    /// assert_eq!(ChatMode::Plan.display_name(), "plan");
    /// assert_eq!(ChatMode::Act.display_name(), "act");
    /// ```
    fn display_name(&self) -> &'static str;

    /// Get human-readable description
    ///
    /// Returns short description of mode behavior.
    fn description(&self) -> &'static str;

    /// Get emoji icon for visual identification
    ///
    /// Returns emoji suitable for terminal display.
    fn icon(&self) -> &'static str;
}

impl ChatModeDisplay for ChatMode {
    fn display_name(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
            ChatMode::AutoApprove => "auto",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ChatMode::Plan => "read-only",
            ChatMode::Act => "write-enabled",
            ChatMode::AutoApprove => "auto-approve",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            ChatMode::Plan => "📖",
            ChatMode::Act => "✏️",
            ChatMode::AutoApprove => "⚡",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_names() {
        assert_eq!(ChatMode::Plan.display_name(), "plan");
        assert_eq!(ChatMode::Act.display_name(), "act");
        assert_eq!(ChatMode::AutoApprove.display_name(), "auto");
    }

    #[test]
    fn test_descriptions() {
        assert_eq!(ChatMode::Plan.description(), "read-only");
        assert_eq!(ChatMode::Act.description(), "write-enabled");
        assert_eq!(ChatMode::AutoApprove.description(), "auto-approve");
    }

    #[test]
    fn test_icons() {
        assert_eq!(ChatMode::Plan.icon(), "📖");
        assert_eq!(ChatMode::Act.icon(), "✏️");
        assert_eq!(ChatMode::AutoApprove.icon(), "⚡");
    }
}
