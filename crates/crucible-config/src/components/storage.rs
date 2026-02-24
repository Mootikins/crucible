//! Storage configuration for daemon-backed access

use serde::{Deserialize, Serialize};

/// Storage configuration
///
/// The daemon is the only storage backend. Old `storage.mode` values
/// (`sqlite`, `lightweight`, `daemon`) are silently accepted for backward
/// compatibility but have no effect — the daemon is always used.
#[derive(Debug, Clone, Serialize)]
pub struct StorageConfig {
    /// Idle timeout in seconds before daemon auto-shuts down
    pub idle_timeout_secs: u64,
}

fn default_idle_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: default_idle_timeout(),
        }
    }
}

/// Raw deserialization helper that accepts old `mode` field for backward compat
#[derive(Deserialize)]
struct StorageConfigRaw {
    #[serde(default = "default_idle_timeout")]
    idle_timeout_secs: u64,
    /// Old field — silently ignored, daemon is always used
    #[serde(default)]
    mode: Option<serde_json::Value>,
}

impl<'de> Deserialize<'de> for StorageConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = StorageConfigRaw::deserialize(deserializer)?;
        if raw.mode.is_some() {
            tracing::warn!(
                "storage.mode is deprecated and has no effect. \
                 The daemon is the only storage backend. \
                 Remove `storage.mode` from your crucible.toml."
            );
        }
        Ok(StorageConfig {
            idle_timeout_secs: raw.idle_timeout_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_timeout_default() {
        let config = StorageConfig::default();
        assert_eq!(config.idle_timeout_secs, 300); // 5 minutes
    }

    #[test]
    fn test_backward_compat_mode_sqlite() {
        let toml = r#"mode = "sqlite""#;
        let config: StorageConfig = toml::from_str(toml).expect("should parse without error");
        assert_eq!(config.idle_timeout_secs, 300);
    }

    #[test]
    fn test_backward_compat_mode_lightweight() {
        let toml = r#"mode = "lightweight""#;
        let config: StorageConfig = toml::from_str(toml).expect("should parse without error");
        assert_eq!(config.idle_timeout_secs, 300);
    }

    #[test]
    fn test_backward_compat_mode_daemon() {
        let toml = r#"mode = "daemon""#;
        let config: StorageConfig = toml::from_str(toml).expect("should parse without error");
        assert_eq!(config.idle_timeout_secs, 300);
    }

    #[test]
    fn test_backward_compat_no_mode() {
        let toml = r#"idle_timeout_secs = 600"#;
        let config: StorageConfig = toml::from_str(toml).expect("should parse without error");
        assert_eq!(config.idle_timeout_secs, 600);
    }

    #[test]
    fn test_backward_compat_mode_with_timeout() {
        let toml = r#"
mode = "sqlite"
idle_timeout_secs = 120
"#;
        let config: StorageConfig = toml::from_str(toml).expect("should parse without error");
        assert_eq!(config.idle_timeout_secs, 120);
    }
}
