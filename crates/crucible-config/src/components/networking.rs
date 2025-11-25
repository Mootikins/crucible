//! Networking component configuration
//!
//! Configuration for HTTP/gRPC servers, timeouts, and networking settings.

use serde::{Deserialize, Serialize};

/// Networking component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkingComponentConfig {
    pub enabled: bool,
    pub http: HttpConfig,
    pub grpc: GrpcConfig,
    pub timeouts: TimeoutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub bind_address: String,
    pub port: u16,
    pub enable_cors: bool,
    pub max_request_size_mb: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    pub bind_address: String,
    pub port: u16,
    pub max_message_size_mb: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub request_timeout_seconds: u64,
    pub connection_timeout_seconds: u64,
    pub keep_alive_seconds: u64,
}

impl Default for NetworkingComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            http: HttpConfig::default(),
            grpc: GrpcConfig::default(),
            timeouts: TimeoutConfig::default(),
        }
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            enable_cors: true,
            max_request_size_mb: 10,
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 50051,
            max_message_size_mb: 4,
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout_seconds: 30,
            connection_timeout_seconds: 10,
            keep_alive_seconds: 60,
        }
    }
}