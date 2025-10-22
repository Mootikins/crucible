//! # Mock Implementations
//!
//! Mock implementations of external dependencies for isolated testing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::{IpcMessage, MessageHeader, MessagePayload, MessageType, ClientCapabilities},
    security::{SecurityManager, AuthConfig, EncryptionConfig, AuthorizationConfig},
    transport::{TransportManager, TransportConfig},
    metrics::MetricsCollector,
};

/// Mock security manager for testing
#[derive(Debug)]
pub struct MockSecurityManager {
    pub should_fail_auth: Arc<Mutex<bool>>,
    pub should_fail_encryption: Arc<Mutex<bool>>,
    pub should_fail_authorization: Arc<Mutex<bool>>,
    pub valid_tokens: Arc<RwLock<HashMap<String, TokenInfo>>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    pub auth_delay: Arc<Mutex<Duration>>,
}

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: String,
    pub expires_at: SystemTime,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub encryption_key: Vec<u8>,
}

impl MockSecurityManager {
    pub fn new() -> Self {
        Self {
            should_fail_auth: Arc::new(Mutex::new(false)),
            should_fail_encryption: Arc::new(Mutex::new(false)),
            should_fail_authorization: Arc::new(Mutex::new(false)),
            valid_tokens: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            auth_delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    pub fn with_failures() -> Self {
        Self {
            should_fail_auth: Arc::new(Mutex::new(true)),
            should_fail_encryption: Arc::new(Mutex::new(true)),
            should_fail_authorization: Arc::new(Mutex::new(true)),
            valid_tokens: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            auth_delay: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    pub async fn add_valid_token(&self, token: String, capabilities: Vec<String>) {
        let info = TokenInfo {
            token: token.clone(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
            capabilities,
        };
        self.valid_tokens.write().await.insert(token, info);
    }

    pub async fn create_session(&self, session_id: String) -> SessionInfo {
        let info = SessionInfo {
            session_id: session_id.clone(),
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            encryption_key: vec![0u8; 32], // Mock encryption key
        };
        self.sessions.write().await.insert(session_id.clone(), info.clone());
        info
    }

    pub async fn set_auth_delay(&self, delay: Duration) {
        *self.auth_delay.lock().await = delay;
    }
}

#[async_trait]
impl SecurityManager for MockSecurityManager {
    async fn authenticate(&self, token: &str) -> IpcResult<String> {
        let delay = *self.auth_delay.lock().await;
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        if *self.should_fail_auth.lock().await {
            return Err(IpcError::Authentication {
                message: "Mock authentication failure".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            });
        }

        let tokens = self.valid_tokens.read().await;
        if let Some(token_info) = tokens.get(token) {
            if token_info.expires_at > SystemTime::now() {
                Ok(Uuid::new_v4().to_string())
            } else {
                Err(IpcError::Authentication {
                    message: "Token expired".to_string(),
                    code: crate::plugin_ipc::error::AuthErrorCode::TokenExpired,
                    retry_after: None,
                })
            }
        } else {
            Err(IpcError::Authentication {
                message: "Invalid token".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })
        }
    }

    async fn authorize(&self, session_id: &str, operation: &str) -> IpcResult<bool> {
        if *self.should_fail_authorization.lock().await {
            return Err(IpcError::Authentication {
                message: "Mock authorization failure".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InsufficientPermissions,
                retry_after: None,
            });
        }

        // Simple mock authorization - allow all operations
        Ok(true)
    }

    async fn encrypt_message(&self, session_id: &str, data: &[u8]) -> IpcResult<Vec<u8>> {
        if *self.should_fail_encryption.lock().await {
            return Err(IpcError::Protocol {
                message: "Mock encryption failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::EncryptionFailed,
                source: None,
            });
        }

        // Simple mock encryption - just XOR with a fixed key
        let key = b"mock_encryption_key_12345678";
        let mut encrypted = Vec::with_capacity(data.len());
        for (i, &byte) in data.iter().enumerate() {
            encrypted.push(byte ^ key[i % key.len()]);
        }
        Ok(encrypted)
    }

    async fn decrypt_message(&self, session_id: &str, encrypted_data: &[u8]) -> IpcResult<Vec<u8>> {
        if *self.should_fail_encryption.lock().await {
            return Err(IpcError::Protocol {
                message: "Mock decryption failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::DecryptionFailed,
                source: None,
            });
        }

        // Simple mock decryption - just XOR with the same key
        let key = b"mock_encryption_key_12345678";
        let mut decrypted = Vec::with_capacity(encrypted_data.len());
        for (i, &byte) in encrypted_data.iter().enumerate() {
            decrypted.push(byte ^ key[i % key.len()]);
        }
        Ok(decrypted)
    }

    async fn generate_token(&self, user_id: &str, capabilities: Vec<String>) -> IpcResult<String> {
        if *self.should_fail_auth.lock().await {
            return Err(IpcError::Authentication {
                message: "Mock token generation failure".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::TokenGenerationFailed,
                retry_after: None,
            });
        }

        let token = format!("mock_token_{}_{}", user_id, Uuid::new_v4());
        self.add_valid_token(token.clone(), capabilities).await;
        Ok(token)
    }

    async fn revoke_token(&self, token: &str) -> IpcResult<()> {
        self.valid_tokens.write().await.remove(token);
        Ok(())
    }

    async fn validate_token(&self, token: &str) -> IpcResult<bool> {
        let tokens = self.valid_tokens.read().await;
        if let Some(token_info) = tokens.get(token) {
            Ok(token_info.expires_at > SystemTime::now())
        } else {
            Ok(false)
        }
    }

    async fn refresh_token(&self, token: &str) -> IpcResult<String> {
        let mut tokens = self.valid_tokens.write().await;
        if let Some(token_info) = tokens.get_mut(token) {
            token_info.expires_at = SystemTime::now() + Duration::from_secs(3600);
            Ok(token.clone())
        } else {
            Err(IpcError::Authentication {
                message: "Token not found for refresh".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })
        }
    }
}

/// Mock transport manager for testing
#[derive(Debug)]
pub struct MockTransportManager {
    pub should_fail_connect: Arc<Mutex<bool>>,
    pub should_fail_send: Arc<Mutex<bool>>,
    pub should_fail_receive: Arc<Mutex<bool>>,
    pub connection_delay: Arc<Mutex<Duration>>,
    pub send_delay: Arc<Mutex<Duration>>,
    pub receive_delay: Arc<Mutex<Duration>>,
    pub connections: Arc<RwLock<HashMap<String, MockConnection>>>,
    pub messages_sent: Arc<RwLock<Vec<IpcMessage>>>,
    pub messages_received: Arc<RwLock<Vec<IpcMessage>>>,
}

#[derive(Debug, Clone)]
pub struct MockConnection {
    pub id: String,
    pub created_at: SystemTime,
    pub is_connected: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl MockTransportManager {
    pub fn new() -> Self {
        Self {
            should_fail_connect: Arc::new(Mutex::new(false)),
            should_fail_send: Arc::new(Mutex::new(false)),
            should_fail_receive: Arc::new(Mutex::new(false)),
            connection_delay: Arc::new(Mutex::new(Duration::ZERO)),
            send_delay: Arc::new(Mutex::new(Duration::ZERO)),
            receive_delay: Arc::new(Mutex::new(Duration::ZERO)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            messages_sent: Arc::new(RwLock::new(Vec::new())),
            messages_received: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn create_connection(&self, endpoint: &str) -> MockConnection {
        let connection = MockConnection {
            id: Uuid::new_v4().to_string(),
            created_at: SystemTime::now(),
            is_connected: true,
            bytes_sent: 0,
            bytes_received: 0,
        };
        self.connections.write().await.insert(endpoint.to_string(), connection.clone());
        connection
    }

    pub async fn get_sent_messages(&self) -> Vec<IpcMessage> {
        self.messages_sent.read().await.clone()
    }

    pub async fn get_received_messages(&self) -> Vec<IpcMessage> {
        self.messages_received.read().await.clone()
    }

    pub async fn clear_messages(&self) {
        self.messages_sent.write().await.clear();
        self.messages_received.write().await.clear();
    }

    pub async fn set_delays(&self, connect: Duration, send: Duration, receive: Duration) {
        *self.connection_delay.lock().await = connect;
        *self.send_delay.lock().await = send;
        *self.receive_delay.lock().await = receive;
    }
}

#[async_trait]
impl TransportManager for MockTransportManager {
    async fn connect(&self, endpoint: &str) -> IpcResult<String> {
        let delay = *self.connection_delay.lock().await;
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        if *self.should_fail_connect.lock().await {
            return Err(IpcError::Connection {
                message: "Mock connection failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionRefused,
                endpoint: endpoint.to_string(),
                retry_count: 0,
            });
        }

        let connection = self.create_connection(endpoint).await;
        Ok(connection.id)
    }

    async fn disconnect(&self, connection_id: &str) -> IpcResult<()> {
        // Remove connection from connections map
        let mut connections = self.connections.write().await;
        connections.retain(|_, conn| conn.id != connection_id);
        Ok(())
    }

    async fn send_message(&self, connection_id: &str, message: IpcMessage) -> IpcResult<()> {
        let delay = *self.send_delay.lock().await;
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        if *self.should_fail_send.lock().await {
            return Err(IpcError::Connection {
                message: "Mock send failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: connection_id.to_string(),
                retry_count: 0,
            });
        }

        self.messages_sent.write().await.push(message);
        Ok(())
    }

    async fn receive_message(&self, connection_id: &str) -> IpcResult<IpcMessage> {
        let delay = *self.receive_delay.lock().await;
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        if *self.should_fail_receive.lock().await {
            return Err(IpcError::Connection {
                message: "Mock receive failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: connection_id.to_string(),
                retry_count: 0,
            });
        }

        // Return a mock heartbeat message
        let message = IpcMessage::new(
            MessageType::Heartbeat,
            MessagePayload::Heartbeat(crate::plugin_ipc::message::HeartbeatPayload {
                status: crate::plugin_ipc::message::HeartbeatStatus::Healthy,
                last_activity: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                resource_usage: crate::plugin_ipc::message::ResourceUsage {
                    memory_bytes: 1024,
                    cpu_percentage: 50.0,
                    disk_bytes: 2048,
                    network_bytes: 512,
                    open_files: 10,
                    active_threads: 2,
                },
                metrics: HashMap::new(),
                status_data: HashMap::new(),
            }),
        );

        self.messages_received.write().await.push(message.clone());
        Ok(message)
    }

    async fn is_connected(&self, connection_id: &str) -> IpcResult<bool> {
        let connections = self.connections.read().await;
        Ok(connections.values().any(|conn| conn.id == connection_id && conn.is_connected))
    }

    async fn get_connection_stats(&self, connection_id: &str) -> IpcResult<HashMap<String, Value>> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.values().find(|conn| conn.id == connection_id) {
            let mut stats = HashMap::new();
            stats.insert("connection_id".to_string(), Value::String(connection.id.clone()));
            stats.insert("is_connected".to_string(), Value::Bool(connection.is_connected));
            stats.insert("bytes_sent".to_string(), Value::Number(serde_json::Number::from(connection.bytes_sent)));
            stats.insert("bytes_received".to_string(), Value::Number(serde_json::Number::from(connection.bytes_received)));
            Ok(stats)
        } else {
            Err(IpcError::Connection {
                message: "Connection not found".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                endpoint: connection_id.to_string(),
                retry_count: 0,
            })
        }
    }
}

/// Mock metrics collector for testing
#[derive(Debug)]
pub struct MockMetricsCollector {
    pub metrics: Arc<RwLock<HashMap<String, f64>>>,
    pub counters: Arc<RwLock<HashMap<String, u64>>>,
    pub histograms: Arc<RwLock<HashMap<String, Vec<f64>>>>,
    pub should_fail_recording: Arc<Mutex<bool>>,
}

impl MockMetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            counters: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            should_fail_recording: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn get_metric(&self, name: &str) -> Option<f64> {
        self.metrics.read().await.get(name).copied()
    }

    pub async fn get_counter(&self, name: &str) -> Option<u64> {
        self.counters.read().await.get(name).copied()
    }

    pub async fn get_histogram(&self, name: &str) -> Vec<f64> {
        self.histograms.read().await.get(name).cloned().unwrap_or_default()
    }

    pub async fn clear_all(&self) {
        self.metrics.write().await.clear();
        self.counters.write().await.clear();
        self.histograms.write().await.clear();
    }

    pub async fn set_failure(&self, should_fail: bool) {
        *self.should_fail_recording.lock().await = should_fail;
    }
}

#[async_trait]
impl MetricsCollector for MockMetricsCollector {
    async fn record_counter(&self, name: &str, value: u64) -> IpcResult<()> {
        if *self.should_fail_recording.lock().await {
            return Err(IpcError::Protocol {
                message: "Mock metrics recording failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InternalError,
                source: None,
            });
        }

        let mut counters = self.counters.write().await;
        *counters.entry(name.to_string()).or_insert(0) += value;
        Ok(())
    }

    async fn record_gauge(&self, name: &str, value: f64) -> IpcResult<()> {
        if *self.should_fail_recording.lock().await {
            return Err(IpcError::Protocol {
                message: "Mock metrics recording failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InternalError,
                source: None,
            });
        }

        self.metrics.write().await.insert(name.to_string(), value);
        Ok(())
    }

    async fn record_histogram(&self, name: &str, value: f64) -> IpcResult<()> {
        if *self.should_fail_recording.lock().await {
            return Err(IpcError::Protocol {
                message: "Mock metrics recording failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InternalError,
                source: None,
            });
        }

        let mut histograms = self.histograms.write().await;
        histograms.entry(name.to_string()).or_insert_with(Vec::new).push(value);
        Ok(())
    }

    async fn get_metrics_summary(&self) -> IpcResult<HashMap<String, Value>> {
        let mut summary = HashMap::new();

        // Add counters
        let counters = self.counters.read().await;
        let counter_map: HashMap<String, Value> = counters.iter()
            .map(|(k, v)| (k.clone(), Value::Number(serde_json::Number::from(*v))))
            .collect();
        summary.insert("counters".to_string(), Value::Object(counter_map.into_iter().collect()));

        // Add gauges
        let metrics = self.metrics.read().await;
        let gauge_map: HashMap<String, Value> = metrics.iter()
            .map(|(k, v)| (k.clone(), Value::Number(serde_json::Number::from_f64(*v).unwrap_or(serde_json::Number::from(0)))))
            .collect();
        summary.insert("gauges".to_string(), Value::Object(gauge_map.into_iter().collect()));

        Ok(summary)
    }
}

/// Mock configuration for testing
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub protocol_version: u8,
    pub max_message_size: usize,
    pub timeout_duration: Duration,
    pub enable_compression: bool,
    pub enable_encryption: bool,
    pub auth_required: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            protocol_version: 1,
            max_message_size: 1024 * 1024,
            timeout_duration: Duration::from_secs(30),
            enable_compression: true,
            enable_encryption: true,
            auth_required: true,
        }
    }
}

impl MockConfig {
    pub fn fast() -> Self {
        Self {
            timeout_duration: Duration::from_millis(100),
            ..Default::default()
        }
    }

    pub fn no_security() -> Self {
        Self {
            enable_compression: false,
            enable_encryption: false,
            auth_required: false,
            ..Default::default()
        }
    }

    pub fn with_limits(max_message_size: usize, timeout: Duration) -> Self {
        Self {
            max_message_size,
            timeout_duration: timeout,
            ..Default::default()
        }
    }
}