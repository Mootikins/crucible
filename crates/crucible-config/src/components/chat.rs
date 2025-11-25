//! Chat component configuration
//!
//! Configuration for chat interface, UI preferences, and history management.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Chat component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatComponentConfig {
    pub enabled: bool,
    pub interface: ChatInterfaceConfig,
    pub history: ChatHistoryConfig,
    pub ui: ChatUiConfig,
    pub defaults: ChatDefaultsConfig,
}

/// Chat interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInterfaceConfig {
    pub default_mode: ChatMode,
    pub max_tokens_per_request: u32,
    pub temperature: f32,
    pub stream_responses: bool,
    pub show_thinking: bool,
}

/// Chat history configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryConfig {
    pub persistence_enabled: bool,
    pub max_history_entries: usize,
    pub storage_path: Option<PathBuf>,
    pub export_format: ExportFormat,
    pub privacy_settings: PrivacySettings,
}

/// Chat UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUiConfig {
    pub show_line_numbers: bool,
    pub syntax_highlighting: bool,
    pub word_wrap: bool,
    pub font_size: Option<u8>,
    pub color_scheme: ColorScheme,
}

/// Chat defaults configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDefaultsConfig {
    pub system_prompt: Option<String>,
    pub context_window_size: usize,
    pub auto_save_interval_seconds: u64,
    pub auto_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatMode {
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "act")]
    Act,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "txt")]
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub anonymize_content: bool,
    pub exclude_system_commands: bool,
    pub max_retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorScheme {
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "auto")]
    Auto,
}

impl Default for ChatComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interface: ChatInterfaceConfig::default(),
            history: ChatHistoryConfig::default(),
            ui: ChatUiConfig::default(),
            defaults: ChatDefaultsConfig::default(),
        }
    }
}

impl Default for ChatInterfaceConfig {
    fn default() -> Self {
        Self {
            default_mode: ChatMode::Plan,
            max_tokens_per_request: 2048,
            temperature: 0.7,
            stream_responses: true,
            show_thinking: false,
        }
    }
}

impl Default for ChatHistoryConfig {
    fn default() -> Self {
        Self {
            persistence_enabled: true,
            max_history_entries: 1000,
            storage_path: Some(PathBuf::from("./chat_history")),
            export_format: ExportFormat::Markdown,
            privacy_settings: PrivacySettings::default(),
        }
    }
}

impl Default for ChatUiConfig {
    fn default() -> Self {
        Self {
            show_line_numbers: false,
            syntax_highlighting: true,
            word_wrap: true,
            font_size: None,
            color_scheme: ColorScheme::Auto,
        }
    }
}

impl Default for ChatDefaultsConfig {
    fn default() -> Self {
        Self {
            system_prompt: None,
            context_window_size: 100,
            auto_save_interval_seconds: 300,
            auto_complete: true,
        }
    }
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            anonymize_content: false,
            exclude_system_commands: true,
            max_retention_days: 30,
        }
    }
}