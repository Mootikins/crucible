//! Storage configuration for embedded vs daemon mode

use serde::{Deserialize, Serialize};

/// Storage mode for database access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// Direct in-process SurrealDB (single session, file-locked)
    #[default]
    Embedded,
    /// Daemon-backed SurrealDB (multi-session via Unix socket)
    Daemon,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage mode: "embedded" (default) or "daemon"
    #[serde(default)]
    pub mode: StorageMode,

    /// Idle timeout in seconds before daemon auto-shuts down (daemon mode only)
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

fn default_idle_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            mode: StorageMode::Embedded,
            idle_timeout_secs: default_idle_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_mode_default_is_embedded() {
        let config = StorageConfig::default();
        assert_eq!(config.mode, StorageMode::Embedded);
    }

    #[test]
    fn test_storage_mode_deserialize_daemon() {
        let toml = r#"
            mode = "daemon"
            idle_timeout_secs = 300
        "#;
        let config: StorageConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.mode, StorageMode::Daemon);
        assert_eq!(config.idle_timeout_secs, 300);
    }

    #[test]
    fn test_storage_mode_deserialize_embedded() {
        let toml = r#"mode = "embedded""#;
        let config: StorageConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.mode, StorageMode::Embedded);
    }

    #[test]
    fn test_idle_timeout_default() {
        let config = StorageConfig::default();
        assert_eq!(config.idle_timeout_secs, 300); // 5 minutes
    }

    #[test]
    fn test_missing_mode_defaults_to_embedded() {
        let toml = r#"idle_timeout_secs = 600"#;
        let config: StorageConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.mode, StorageMode::Embedded);
    }
}
