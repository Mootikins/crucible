//! Service-related type definitions
//!
//! This module contains type definitions for services, health checks,
//! and related functionality used across the crucible services.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use chrono::{DateTime, Utc};

/// Service health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but functional
    Degraded,
    /// Service is unhealthy
    Unhealthy,
    /// Service status is unknown
    Unknown,
}

/// Service health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Service name
    pub name: String,
    /// Service status
    pub status: ServiceStatus,
    /// Health check message
    pub message: Option<String>,
    /// Last health check timestamp
    pub last_check: DateTime<Utc>,
    /// Service uptime
    pub uptime: Duration,
    /// Additional health metrics
    pub metrics: HashMap<String, f64>,
}

/// Service metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    /// Service name
    pub name: String,
    /// Request count
    pub request_count: u64,
    /// Error count
    pub error_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Metrics collection timestamp
    pub timestamp: DateTime<Utc>,
}