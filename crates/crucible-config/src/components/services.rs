//! Services component configuration
//!
//! Configuration for service orchestration, discovery, and health monitoring.

use serde::{Deserialize, Serialize};

/// Services component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesComponentConfig {
    pub enabled: bool,
    pub orchestration: OrchestrationConfig,
    pub discovery: ServicesDiscoveryConfig,
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub enable_auto_scaling: bool,
    pub min_instances: usize,
    pub max_instances: usize,
    pub health_check_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesDiscoveryConfig {
    pub enabled: bool,
    pub service_registry_url: Option<String>,
    pub refresh_interval_seconds: u64,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub port: u16,
    pub check_interval_seconds: u64,
    pub failure_threshold: usize,
}

impl Default for ServicesComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            orchestration: OrchestrationConfig::default(),
            discovery: ServicesDiscoveryConfig::default(),
            health: HealthConfig::default(),
        }
    }
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            enable_auto_scaling: false,
            min_instances: 1,
            max_instances: 3,
            health_check_interval_seconds: 30,
        }
    }
}

impl Default for ServicesDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_registry_url: None,
            refresh_interval_seconds: 60,
            timeout_seconds: 10,
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "127.0.0.1".to_string(),
            port: 8081,
            check_interval_seconds: 15,
            failure_threshold: 3,
        }
    }
}