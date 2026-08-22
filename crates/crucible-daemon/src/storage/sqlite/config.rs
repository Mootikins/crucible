//! SQLite configuration types

use crucible_core::serde_helpers::default_true;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for SQLite storage backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Path to the SQLite database file
    pub path: PathBuf,

    /// Connection pool size (default: 10)
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// Enable WAL mode (recommended, default: true)
    #[serde(default = "default_true")]
    pub wal_mode: bool,

    /// Enable foreign keys (default: true)
    #[serde(default = "default_true")]
    pub foreign_keys: bool,

    /// Busy timeout in milliseconds (default: 5000)
    #[serde(default = "default_busy_timeout")]
    pub busy_timeout_ms: u64,

    /// Cache size in pages (default: 2000, ~8MB)
    #[serde(default = "default_cache_size")]
    pub cache_size: i32,

    /// MMAP size in bytes (default: 1GB)
    #[serde(default = "default_mmap_size")]
    pub mmap_size: u64,
}

fn default_pool_size() -> u32 {
    10
}

fn default_busy_timeout() -> u64 {
    5000
}

fn default_cache_size() -> i32 {
    2000
}

fn default_mmap_size() -> u64 {
    1024 * 1024 * 1024 // 1GB
}

impl SqliteConfig {
    /// Create a new configuration with the given database path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            pool_size: default_pool_size(),
            wal_mode: true,
            foreign_keys: true,
            busy_timeout_ms: default_busy_timeout(),
            cache_size: default_cache_size(),
            mmap_size: default_mmap_size(),
        }
    }

    /// Create an in-memory database configuration (for testing)
    pub fn memory() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            pool_size: 1, // Memory DBs can't be shared across connections
            wal_mode: false,
            foreign_keys: true,
            busy_timeout_ms: default_busy_timeout(),
            cache_size: default_cache_size(),
            mmap_size: 0,
        }
    }
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self::new("./crucible.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = SqliteConfig::default();
        assert_eq!(config.pool_size, 10);
        assert!(config.wal_mode);
        assert!(config.foreign_keys);
        assert_eq!(config.busy_timeout_ms, 5000);
    }

    #[test]
    fn test_memory_config() {
        let config = SqliteConfig::memory();
        assert_eq!(config.path.to_str().unwrap(), ":memory:");
        assert_eq!(config.pool_size, 1);
        assert!(!config.wal_mode);
    }
}
