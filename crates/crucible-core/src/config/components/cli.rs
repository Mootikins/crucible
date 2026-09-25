//! CLI configuration for terminal display settings.

use serde::{Deserialize, Serialize};

use crate::config::serde_helpers::default_true;

/// CLI configuration for terminal display and behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliConfig {
    /// Syntax highlighting configuration.
    #[serde(default)]
    pub highlighting: HighlightingConfig,
    /// Where the chat TUI draws. `cru chat --inline` overrides it for one run.
    #[serde(default)]
    pub screen: ChatScreen,
}

/// Where the chat TUI draws.
///
/// This is display state of one terminal client, not a session knob: two
/// clients on one session can each use their own screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatScreen {
    /// The alternate screen. The TUI owns the scroll, selection and copy,
    /// and prints the transcript to the main screen on exit.
    #[default]
    Fullscreen,
    /// The main screen. The terminal owns the scroll and keeps finished
    /// content in its own scrollback.
    Inline,
}

/// Configuration for syntax highlighting in code blocks and diffs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightingConfig {
    /// Enable syntax highlighting (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Theme name for syntax highlighting (default: "base16-ocean.dark").
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "base16-ocean.dark".to_string()
}

impl Default for HighlightingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            theme: default_theme(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_enabled_by_default() {
        let config = CliConfig::default();
        assert!(config.highlighting.enabled);
    }

    #[test]
    fn highlighting_theme_has_default() {
        let config = CliConfig::default();
        assert_eq!(config.highlighting.theme, "base16-ocean.dark");
    }

    #[test]
    fn highlighting_config_deserializes_from_toml() {
        let toml = r#"
            [highlighting]
            enabled = false
            theme = "Solarized (dark)"
        "#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        assert!(!config.highlighting.enabled);
        assert_eq!(config.highlighting.theme, "Solarized (dark)");
    }

    #[test]
    fn the_chat_draws_full_screen_by_default() {
        assert_eq!(CliConfig::default().screen, ChatScreen::Fullscreen);
        let config: CliConfig = toml::from_str("").unwrap();
        assert_eq!(config.screen, ChatScreen::Fullscreen);
    }

    #[test]
    fn the_screen_setting_reads_both_names() {
        let config: CliConfig = toml::from_str(r#"screen = "inline""#).unwrap();
        assert_eq!(config.screen, ChatScreen::Inline);
        let config: CliConfig = toml::from_str(r#"screen = "fullscreen""#).unwrap();
        assert_eq!(config.screen, ChatScreen::Fullscreen);
        assert!(toml::from_str::<CliConfig>(r#"screen = "full""#).is_err());
    }

    #[test]
    fn highlighting_config_uses_defaults_when_missing() {
        let toml = r#"
            show_progress = true
        "#;
        let config: CliConfig = toml::from_str(toml).unwrap();
        assert!(config.highlighting.enabled);
        assert_eq!(config.highlighting.theme, "base16-ocean.dark");
    }
}
