//! Storage component configuration
//!
//! Configuration for database, caching, and persistence settings.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Storage component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageComponentConfig {
    pub enabled: bool,
    pub database: StorageDatabaseConfig,
    pub cache: StorageCacheConfig,
    pub backup: BackupConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDatabaseConfig {
    pub path: PathBuf,
    pub max_connections: usize,
    pub connection_timeout_seconds: u64,
    pub query_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCacheConfig {
    pub enabled: bool,
    pub max_size_mb: usize,
    pub ttl_seconds: u64,
    pub cleanup_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub backup_path: PathBuf,
    pub interval_hours: u64,
    pub retention_days: u64,
}

impl Default for StorageComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database: StorageDatabaseConfig::default(),
            cache: StorageCacheConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for StorageDatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./crucible.db"),
            max_connections: 64,
            connection_timeout_seconds: 30,
            query_timeout_seconds: 60,
        }
    }
}

impl Default for StorageCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size_mb: 100,
            ttl_seconds: 3600,
            cleanup_interval_seconds: 300,
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backup_path: PathBuf::from("./backups"),
            interval_hours: 24,
            retention_days: 30,
        }
    }
}