//! # IPC Client Implementation
//!
//! High-level client for plugin IPC communication with automatic connection management,
//! retries, and error handling.

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::{IpcMessage, MessageType, RequestPayload, ResponsePayload},
    security::SecurityManager,
    transport::{ConnectionPool, TransportType},
    config::IpcConfig,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// High-level IPC client
pub struct IpcClient {
    /// Client configuration
    config: IpcConfig,
    /// Connection pool
    connection_pool: Arc<ConnectionPool>,
    /// Security manager
    security_manager: Arc<SecurityManager>,
    /// Client state
    state: Arc<RwLock<ClientState>>,
    /// Request ID generator
    request_id_counter: Arc<RwLock<u64>>,
}

impl IpcClient {
    /// Create a new IPC client
    pub async fn new(config: IpcConfig) -> IpcResult<Self> {
        config.validate()?;

        let security_manager = Arc::new(SecurityManager::new(
            config.security.auth.clone(),
            config.security.encryption.clone(),
            config.security.authorization.clone(),
        ));

        let connection_pool = Arc::new(ConnectionPool::new(
            config.transport.connection_pool.clone(),
            security_manager.clone(),
        ));

        Ok(Self {
            config,
            connection_pool,
            security_manager,
            state: Arc::new(RwLock::new(ClientState::new())),
            request_id_counter: Arc::new(RwLock::new(0)),
        })
    }

    /// Connect to a plugin endpoint
    pub async fn connect(&self, endpoint: &str) -> IpcResult<Connection> {
        info!("Connecting to endpoint: {}", endpoint);

        let connection = self.connection_pool.get_connection(endpoint, TransportType::Auto).await?;
        let connection_id = connection.id.clone();

        // Perform handshake
        self.perform_handshake(&connection).await?;

        info!("Successfully connected to endpoint: {}", endpoint);
        Ok(Connection {
            id: connection_id,
            endpoint: endpoint.to_string(),
            client: self.clone(),
        })
    }

    /// Send a request and wait for response
    pub async fn send_request(
        &self,
        endpoint: &str,
        operation: &str,
        parameters: serde_json::Value,
    ) -> IpcResult<serde_json::Value> {
        let connection = self.connection_pool.get_connection(endpoint, TransportType::Auto).await?;

        // Create request
        let request_id = self.generate_request_id().await;
        let request_payload = RequestPayload {
            operation: operation.to_string(),
            parameters,
            context: self.create_execution_context().await,
            timeout_ms: Some(self.config.performance.request_timeout_ms),
            metadata: std::collections::HashMap::new(),
        };

        let mut message = IpcMessage::request(endpoint.to_string(), request_payload);
        message.header.correlation_id = Some(request_id.clone());

        // Send request
        self.send_message_with_retry(&connection, &mut message).await?;

        // Wait for response
        let response = self.wait_for_response(&connection, &request_id).await?;

        // Return connection to pool
        self.connection_pool.return_connection(connection).await;

        // Extract response data
        match response.payload {
            crate::plugin_ipc::message::MessagePayload::Response(response_payload) => {
                if response_payload.success {
                    Ok(response_payload.data.unwrap_or(serde_json::Value::Null))
                } else {
                    let error_info = response_payload.error.unwrap_or_default();
                    Err(IpcError::Plugin {
                        message: error_info.message,
                        code: crate::plugin_ipc::error::PluginErrorCode::ExecutionFailed,
                        plugin_id: endpoint.to_string(),
                        execution_id: response.header.correlation_id,
                    })
                }
            }
            _ => Err(IpcError::Message {
                message: "Unexpected response message type".to_string(),
                code: crate::plugin_ipc::error::MessageErrorCode::InvalidMessageFormat,
                message_id: Some(response.header.message_id),
            }),
        }
    }

    /// Send an event without expecting a response
    pub async fn send_event(
        &self,
        endpoint: &str,
        event_type: &str,
        data: serde_json::Value,
    ) -> IpcResult<()> {
        let connection = self.connection_pool.get_connection(endpoint, TransportType::Auto).await?;

        let event_payload = crate::plugin_ipc::message::EventPayload {
            event_type: event_type.to_string(),
            data,
            source: "ipc_client".to_string(),
            event_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            severity: crate::plugin_ipc::message::EventSeverity::Info,
            metadata: std::collections::HashMap::new(),
        };

        let mut message = IpcMessage::event(event_type.to_string(), event_payload);
        message.header.destination = Some(endpoint.to_string());

        self.send_message_with_retry(&connection, &mut message).await?;
        self.connection_pool.return_connection(connection).await;

        Ok(())
    }

    /// Get client statistics
    pub async fn get_stats(&self) -> ClientStats {
        let state = self.state.read().await;
        let pool_stats = self.connection_pool.get_stats().await;

        ClientStats {
            total_requests: state.total_requests,
            successful_requests: state.successful_requests,
            failed_requests: state.failed_requests,
            active_connections: pool_stats.active_connections,
            pooled_connections: pool_stats.total_connections,
            average_response_time: state.average_response_time,
        }
    }

    // Private methods

    async fn perform_handshake(&self, connection: &crate::plugin_ipc::transport::PooledConnection) -> IpcResult<()> {
        // Create handshake message
        let handshake_payload = crate::plugin_ipc::message::HandshakePayload {
            protocol_version: crate::plugin_ipc::PROTOCOL_VERSION,
            client_id: "ipc_client".to_string(),
            auth_token: "client_token".to_string(), // This would be properly generated
            supported_types: vec![
                MessageType::Request,
                MessageType::Response,
                MessageType::Event,
                MessageType::Heartbeat,
            ],
            compression_algos: vec!["lz4".to_string()],
            encryption_algos: vec!["aes256gcm".to_string()],
            max_message_size: crate::plugin_ipc::MAX_MESSAGE_SIZE,
            capabilities: crate::plugin_ipc::message::ClientCapabilities {
                plugin_types: vec![],
                operations: vec![],
                data_formats: vec!["json".to_string()],
                max_concurrent_requests: self.config.performance.max_concurrent_requests,
                supports_streaming: false,
                supports_batching: self.config.performance.enable_batching,
                supports_compression: self.config.performance.enable_compression,
                supports_encryption: true,
            },
            metadata: std::collections::HashMap::new(),
        };

        let mut handshake_message = IpcMessage::new(
            MessageType::Handshake,
            crate::plugin_ipc::message::MessagePayload::Handshake(handshake_payload),
        );

        // Send handshake
        self.send_message_with_retry(connection, &mut handshake_message).await?;

        // Wait for handshake response
        let response = self.receive_message_with_timeout(connection, Duration::from_secs(10)).await?;

        match response.payload {
            crate::plugin_ipc::message::MessagePayload::Response(_) => {
                debug!("Handshake successful");
                Ok(())
            }
            crate::plugin_ipc::message::MessagePayload::Error(error_response) => {
                Err(IpcError::Authentication {
                    message: format!("Handshake failed: {}", error_response.message),
                    code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                    retry_after: None,
                })
            }
            _ => Err(IpcError::Protocol {
                message: "Unexpected handshake response".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::ProtocolViolation,
                source: None,
            }),
        }
    }

    async fn send_message_with_retry(
        &self,
        connection: &mut crate::plugin_ipc::transport::PooledConnection,
        message: &mut IpcMessage,
    ) -> IpcResult<()> {
        let mut retry_count = 0;
        let max_retries = 3;

        loop {
            match connection.send_message(message).await {
                Ok(_) => {
                    let mut state = self.state.write().await;
                    state.total_requests += 1;
                    return Ok(());
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries || !e.is_retryable() {
                        let mut state = self.state.write().await;
                        state.failed_requests += 1;
                        return Err(e);
                    }

                    if let Some(delay) = e.retry_delay() {
                        warn!("Send failed, retrying in {:?} (attempt {}/{}): {}", delay, retry_count, max_retries, e);
                        tokio::time::sleep(delay).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(100 * retry_count)).await;
                    }
                }
            }
        }
    }

    async fn receive_message_with_timeout(
        &self,
        connection: &mut crate::plugin_ipc::transport::PooledConnection,
        timeout: Duration,
    ) -> IpcResult<IpcMessage> {
        tokio::time::timeout(timeout, async {
            connection.receive_message().await
        })
        .await
        .map_err(|_| IpcError::Timeout {
            message: "Receive timeout".to_string(),
            operation: "receive_message".to_string(),
            timeout,
            elapsed: timeout,
        })?
    }

    async fn wait_for_response(
        &self,
        connection: &mut crate::plugin_ipc::transport::PooledConnection,
        request_id: &str,
    ) -> IpcResult<IpcMessage> {
        let timeout = Duration::from_millis(self.config.performance.request_timeout_ms);

        loop {
            let response = self.receive_message_with_timeout(connection, timeout).await?;

            if response.header.correlation_id.as_ref() == Some(&request_id.to_string()) {
                let mut state = self.state.write().await;
                state.successful_requests += 1;
                return Ok(response);
            }

            // Not our response, continue waiting
            debug!("Received response for different request, continuing to wait");
        }
    }

    async fn generate_request_id(&self) -> String {
        let mut counter = self.request_id_counter.write().await;
        *counter += 1;
        format!("req_{}_{}", counter, uuid::Uuid::new_v4())
    }

    async fn create_execution_context(&self) -> crate::plugin_ipc::message::ExecutionContext {
        crate::plugin_ipc::message::ExecutionContext {
            user_id: Some("system".to_string()),
            session_id: Some("client_session".to_string()),
            request_id: self.generate_request_id().await,
            working_directory: std::env::current_dir().ok().and_then(|p| p.to_str().map(|s| s.to_string())),
            environment: std::env::vars().collect(),
            security_context: crate::plugin_ipc::message::SecurityContext {
                security_level: "standard".to_string(),
                allowed_operations: vec!["*".to_string()],
                blocked_operations: vec![],
                sandbox_config: serde_json::json!({}),
            },
            time_limits: crate::plugin_ipc::message::TimeLimits {
                max_cpu_time: Some(30),
                max_wall_time: Some(60),
                max_real_time: Some(90),
            },
            resource_limits: crate::plugin_ipc::message::ResourceLimits {
                max_memory: Some(self.config.plugins.resource_limits.max_memory_mb * 1024 * 1024),
                max_disk: Some(self.config.plugins.resource_limits.max_disk_mb * 1024 * 1024),
                max_processes: Some(self.config.plugins.resource_limits.max_processes),
                max_files: Some(self.config.plugins.resource_limits.max_file_descriptors),
            },
        }
    }
}

impl Clone for IpcClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            connection_pool: self.connection_pool.clone(),
            security_manager: self.security_manager.clone(),
            state: self.state.clone(),
            request_id_counter: self.request_id_counter.clone(),
        }
    }
}

/// Connection handle for a specific endpoint
pub struct Connection {
    pub id: String,
    pub endpoint: String,
    client: IpcClient,
}

impl Connection {
    /// Send a request through this connection
    pub async fn send_request(
        &self,
        operation: &str,
        parameters: serde_json::Value,
    ) -> IpcResult<serde_json::Value> {
        self.client.send_request(&self.endpoint, operation, parameters).await
    }

    /// Send an event through this connection
    pub async fn send_event(
        &self,
        event_type: &str,
        data: serde_json::Value,
    ) -> IpcResult<()> {
        self.client.send_event(&self.endpoint, event_type, data).await
    }

    /// Close the connection
    pub async fn close(self) -> IpcResult<()> {
        info!("Closing connection to endpoint: {}", self.endpoint);
        // Connection is automatically returned to pool when dropped
        Ok(())
    }
}

/// Client state
#[derive(Debug)]
struct ClientState {
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    average_response_time: Duration,
}

impl ClientState {
    fn new() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: Duration::ZERO,
        }
    }
}

/// Client statistics
#[derive(Debug, Clone)]
pub struct ClientStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub active_connections: usize,
    pub pooled_connections: usize,
    pub average_response_time: Duration,
}

/// Client builder for easy configuration
pub struct IpcClientBuilder {
    config: IpcConfig,
}

impl IpcClientBuilder {
    /// Create a new client builder
    pub fn new() -> Self {
        Self {
            config: IpcConfig::default(),
        }
    }

    /// Set the configuration
    pub fn config(mut self, config: IpcConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the environment
    pub fn environment(mut self, env: crate::plugin_ipc::config::Environment) -> Self {
        self.config = IpcConfig::for_environment(env);
        self
    }

    /// Enable compression
    pub fn enable_compression(mut self, enabled: bool) -> Self {
        self.config.performance.enable_compression = enabled;
        self
    }

    /// Set request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.performance.request_timeout_ms = timeout.as_millis() as u64;
        self
    }

    /// Set maximum concurrent requests
    pub fn max_concurrent_requests(mut self, max: u32) -> Self {
        self.config.performance.max_concurrent_requests = max;
        self
    }

    /// Build the client
    pub async fn build(self) -> IpcResult<IpcClient> {
        IpcClient::new(self.config).await
    }
}

impl Default for IpcClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_builder() {
        let client = IpcClientBuilder::new()
            .environment(crate::plugin_ipc::config::Environment::Testing)
            .enable_compression(true)
            .request_timeout(Duration::from_secs(30))
            .max_concurrent_requests(10)
            .build()
            .await;

        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_client_creation() {
        let config = IpcConfig::testing();
        let client = IpcClient::new(config).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_client_stats() {
        let config = IpcConfig::testing();
        let client = IpcClient::new(config).await.unwrap();
        let stats = client.get_stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.successful_requests, 0);
        assert_eq!(stats.failed_requests, 0);
    }

    #[test]
    fn test_client_state() {
        let state = ClientState::new();
        assert_eq!(state.total_requests, 0);
        assert_eq!(state.successful_requests, 0);
        assert_eq!(state.failed_requests, 0);
    }

    #[tokio::test]
    async fn test_request_id_generation() {
        let config = IpcConfig::testing();
        let client = IpcClient::new(config).await.unwrap();

        let id1 = client.generate_request_id().await;
        let id2 = client.generate_request_id().await;

        assert_ne!(id1, id2);
        assert!(id1.starts_with("req_"));
        assert!(id2.starts_with("req_"));
    }

    #[test]
    fn test_connection_creation() {
        let config = IpcConfig::testing();
        let client = IpcClient::new(config).await.unwrap();

        let connection = Connection {
            id: "test_connection".to_string(),
            endpoint: "test_endpoint".to_string(),
            client,
        };

        assert_eq!(connection.id, "test_connection");
        assert_eq!(connection.endpoint, "test_endpoint");
    }
}