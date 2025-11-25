//! Monitoring component configuration
//!
//! Configuration for logging, metrics, and debugging.

use serde::{Deserialize, Serialize};

/// Monitoring component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringComponentConfig {
    pub enabled: bool,
    pub logging: MonitoringLoggingConfig,
    pub metrics: MetricsConfig,
    pub debugging: DebuggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringLoggingConfig {
    pub level: String,
    pub format: LogFormat,
    pub file_path: Option<String>,
    pub max_file_size_mb: usize,
    pub max_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "pretty")]
    Pretty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub port: u16,
    pub collection_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggingConfig {
    pub enabled: bool,
    pub profile: bool,
    pub trace: bool,
    pub dump_requests: bool,
}

impl Default for MonitoringComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            logging: MonitoringLoggingConfig::default(),
            metrics: MetricsConfig::default(),
            debugging: DebuggingConfig::default(),
        }
    }
}

impl Default for MonitoringLoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Text,
            file_path: None,
            max_file_size_mb: 100,
            max_files: 10,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_address: "127.0.0.1".to_string(),
            port: 9090,
            collection_interval_seconds: 30,
        }
    }
}

impl Default for DebuggingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: false,
            trace: false,
            dump_requests: false,
        }
    }
}