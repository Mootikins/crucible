//! # IPC Server Implementation
//!
//! High-performance server for plugin IPC communication with connection management,
//! request routing, and plugin hosting capabilities.

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::{IpcMessage, MessageType, HandshakePayload, ResponsePayload},
    protocol::ProtocolHandler,
    security::SecurityManager,
    transport::{TransportManager, TransportType},
    config::IpcConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

/// IPC server for handling plugin connections
pub struct IpcServer {
    /// Server configuration
    config: IpcConfig,
    /// Protocol handler
    protocol_handler: Arc<ProtocolHandler>,
    /// Security manager
    security_manager: Arc<SecurityManager>,
    /// Transport manager
    transport_manager: Arc<TransportManager>,
    /// Connected plugins
    plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,
    /// Request handlers
    handlers: Arc<RwLock<HashMap<String, RequestHandler>>>,
    /// Server state
    state: Arc<RwLock<ServerState>>,
    /// Shutdown signal
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl IpcServer {
    /// Create a new IPC server
    pub async fn new(config: IpcConfig) -> IpcResult<Self> {
        config.validate()?;

        let security_manager = Arc::new(SecurityManager::new(
            config.security.auth.clone(),
            config.security.encryption.clone(),
            config.security.authorization.clone(),
        ));

        let protocol_handler = Arc::new(ProtocolHandler::new(security_manager.clone()));
        let transport_manager = Arc::new(TransportManager::new(
            config.transport.connection_pool.clone(),
            security_manager.clone(),
        ));

        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        Ok(Self {
            config,
            protocol_handler,
            security_manager,
            transport_manager,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(ServerState::new())),
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Start the server
    pub async fn start(&mut self) -> IpcResult<()> {
        info!("Starting IPC server");

        // Initialize default handlers
        self.register_default_handlers().await;

        // Start listening on configured endpoints
        self.start_listeners().await?;

        // Start background tasks
        self.start_background_tasks().await;

        // Update server state
        {
            let mut state = self.state.write().await;
            state.running = true;
            state.start_time = std::time::SystemTime::now();
        }

        info!("IPC server started successfully");
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) -> IpcResult<()> {
        info!("Stopping IPC server");

        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(true);
        }

        // Update server state
        {
            let mut state = self.state.write().await;
            state.running = false;
        }

        // Disconnect all plugins
        {
            let mut plugins = self.plugins.write().await;
            for (_, plugin) in plugins.drain() {
                info!("Disconnecting plugin: {}", plugin.id);
            }
        }

        // Shutdown transport manager
        self.transport_manager.shutdown().await?;

        info!("IPC server stopped");
        Ok(())
    }

    /// Register a request handler
    pub async fn register_handler(&self, operation: &str, handler: RequestHandler) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(operation.to_string(), handler);
        info!("Registered handler for operation: {}", operation);
    }

    /// Get connected plugins
    pub async fn get_plugins(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.values().cloned().collect()
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> ServerStats {
        let state = self.state.read().await;
        let transport_metrics = self.transport_manager.get_metrics().await;
        let pool_stats = self.transport_manager.get_pool_stats().await;

        ServerStats {
            running: state.running,
            uptime: state.start_time.elapsed().unwrap_or_default(),
            connected_plugins: self.plugins.read().await.len(),
            total_requests: state.total_requests,
            successful_requests: state.successful_requests,
            failed_requests: state.failed_requests,
            active_connections: pool_stats.active_connections,
            total_connections: pool_stats.total_connections,
            messages_per_second: transport_metrics.performance.messages_per_second,
            error_rate: if state.total_requests > 0 {
                state.failed_requests as f64 / state.total_requests as f64
            } else {
                0.0
            },
        }
    }

    // Private methods

    async fn start_listeners(&mut self) -> IpcResult<()> {
        // Start Unix domain socket listener
        let socket_path = self.config.transport.socket_path.to_string_lossy().to_string();
        self.transport_manager.listen_unix(&socket_path).await?;

        // Start TCP listener as fallback
        let tcp_addr = format!("0.0.0.0:{}", self.config.transport.tcp_port_range.start);
        self.transport_manager.listen_tcp(&tcp_addr).await?;

        Ok(())
    }

    async fn start_background_tasks(&self) {
        let transport_manager = self.transport_manager.clone();
        let plugins = self.plugins.clone();
        let shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();

        // Connection handler task
        let connection_task = {
            let transport_manager = transport_manager.clone();
            let plugins = plugins.clone();
            let protocol_handler = self.protocol_handler.clone();
            let handlers = self.handlers.clone();
            let state = self.state.clone();

            tokio::spawn(async move {
                Self::connection_handler_loop(
                    transport_manager,
                    plugins,
                    protocol_handler,
                    handlers,
                    state,
                    shutdown_rx.clone(),
                ).await;
            })
        };

        // Health check task
        let health_task = {
            let plugins = plugins.clone();
            let shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();

            tokio::spawn(async move {
                Self::health_check_loop(plugins, shutdown_rx.clone()).await;
            })
        };

        // Metrics collection task
        let metrics_task = {
            let transport_manager = transport_manager.clone();
            let shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();

            tokio::spawn(async move {
                Self::metrics_collection_loop(transport_manager, shutdown_rx.clone()).await;
            })
        };

        // Keep task handles
        tokio::spawn(async move {
            let _ = tokio::join!(connection_task, health_task, metrics_task);
        });
    }

    async fn register_default_handlers(&self) {
        // Register heartbeat handler
        self.register_handler("heartbeat", RequestHandler::new(|request| {
            Box::pin(async move {
                let response_payload = ResponsePayload {
                    success: true,
                    data: Some(serde_json::json!({
                        "status": "healthy",
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })),
                    error: None,
                    execution_time_ms: 1,
                    resource_usage: crate::plugin_ipc::message::ResourceUsage {
                        memory_bytes: 0,
                        cpu_percentage: 0.0,
                        disk_bytes: 0,
                        network_bytes: 0,
                        open_files: 0,
                        active_threads: 1,
                    },
                    metadata: std::collections::HashMap::new(),
                };

                Ok(IpcMessage::response(
                    request.header.correlation_id.clone().unwrap_or_default(),
                    response_payload,
                ))
            })
        })).await;

        // Register info handler
        self.register_handler("info", RequestHandler::new(|request| {
            Box::pin(async move {
                let response_payload = ResponsePayload {
                    success: true,
                    data: Some(serde_json::json!({
                        "server_version": env!("CARGO_PKG_VERSION"),
                        "protocol_version": crate::plugin_ipc::PROTOCOL_VERSION,
                        "uptime": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })),
                    error: None,
                    execution_time_ms: 1,
                    resource_usage: crate::plugin_ipc::message::ResourceUsage::default(),
                    metadata: std::collections::HashMap::new(),
                };

                Ok(IpcMessage::response(
                    request.header.correlation_id.clone().unwrap_or_default(),
                    response_payload,
                ))
            })
        })).await;
    }

    async fn connection_handler_loop(
        transport_manager: Arc<TransportManager>,
        plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,
        protocol_handler: Arc<ProtocolHandler>,
        handlers: Arc<RwLock<HashMap<String, RequestHandler>>>,
        state: Arc<RwLock<ServerState>>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        // This would implement the main connection handling loop
        // For now, we'll just log that it's running
        info!("Connection handler loop started");

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Connection handler loop shutting down");
                        break;
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Periodic maintenance
                    Self::cleanup_expired_plugins(&plugins).await;
                }
            }
        }
    }

    async fn health_check_loop(
        plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Health check loop shutting down");
                        break;
                    }
                }
                _ = interval.tick() => {
                    Self::perform_health_checks(&plugins).await;
                }
            }
        }
    }

    async fn metrics_collection_loop(
        transport_manager: Arc<TransportManager>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Metrics collection loop shutting down");
                        break;
                    }
                }
                _ = interval.tick() => {
                    transport_manager.cleanup().await;
                }
            }
        }
    }

    async fn cleanup_expired_plugins(plugins: &Arc<RwLock<HashMap<String, PluginInfo>>>) {
        let mut plugins_guard = plugins.write().await;
        let now = std::time::SystemTime::now();

        plugins_guard.retain(|_, plugin| {
            now.duration_since(plugin.last_activity).unwrap_or_default() < Duration::from_secs(300)
        });
    }

    async fn perform_health_checks(plugins: &Arc<RwLock<HashMap<String, PluginInfo>>>) {
        let plugins_guard = plugins.read().await;

        for (_, plugin) in plugins_guard.iter() {
            // Perform health check for each plugin
            debug!("Performing health check for plugin: {}", plugin.id);
            // In a real implementation, this would send a health check message
        }
    }

    async fn handle_handshake(
        &self,
        message: &IpcMessage,
        client_info: &ClientInfo,
    ) -> IpcResult<IpcMessage> {
        if let crate::plugin_ipc::message::MessagePayload::Handshake(handshake_payload) = &message.payload {
            // Validate handshake
            handshake_payload.validate()?;

            // Authenticate plugin
            let auth_result = self.security_manager.authenticate(handshake_payload).await?;

            // Create plugin info
            let plugin_info = PluginInfo {
                id: auth_result.plugin_id.clone(),
                name: handshake_payload.client_id.clone(),
                version: handshake_payload.plugin_version.clone(),
                capabilities: handshake_payload.capabilities.clone(),
                client_info: client_info.clone(),
                auth_result,
                connected_at: std::time::SystemTime::now(),
                last_activity: std::time::SystemTime::now(),
                status: PluginStatus::Connected,
            };

            // Register plugin
            {
                let mut plugins = self.plugins.write().await;
                plugins.insert(plugin_info.id.clone(), plugin_info.clone());
            }

            info!("Plugin connected: {} ({})", plugin_info.name, plugin_info.id);

            // Create successful response
            let response_payload = ResponsePayload {
                success: true,
                data: Some(serde_json::json!({
                    "session_id": plugin_info.auth_result.session_id,
                    "server_capabilities": {
                        "version": crate::plugin_ipc::PROTOCOL_VERSION,
                        "max_message_size": crate::plugin_ipc::MAX_MESSAGE_SIZE,
                        "supported_features": ["heartbeat", "batching", "streaming"]
                    }
                })),
                error: None,
                execution_time_ms: 0,
                resource_usage: crate::plugin_ipc::message::ResourceUsage::default(),
                metadata: std::collections::HashMap::new(),
            };

            Ok(IpcMessage::response(
                message.header.correlation_id.clone().unwrap_or_default(),
                response_payload,
            ))
        } else {
            Err(IpcError::Message {
                message: "Expected handshake payload".to_string(),
                code: crate::plugin_ipc::error::MessageErrorCode::InvalidMessageFormat,
                message_id: Some(message.header.message_id.clone()),
            })
        }
    }

    async fn handle_request(
        &self,
        message: &IpcMessage,
    ) -> IpcResult<IpcMessage> {
        if let crate::plugin_ipc::message::MessagePayload::Request(request_payload) = &message.payload {
            let handlers = self.handlers.read().await;

            if let Some(handler) = handlers.get(&request_payload.operation) {
                // Update request statistics
                {
                    let mut state = self.state.write().await;
                    state.total_requests += 1;
                }

                // Handle request
                let start_time = std::time::Instant::now();
                let result = handler.handle(message.clone()).await;
                let duration = start_time.elapsed();

                match result {
                    Ok(response) => {
                        // Update success statistics
                        {
                            let mut state = self.state.write().await;
                            state.successful_requests += 1;
                        }
                        debug!("Request handled successfully in {:?}", duration);
                        Ok(response)
                    }
                    Err(e) => {
                        // Update failure statistics
                        {
                            let mut state = self.state.write().await;
                            state.failed_requests += 1;
                        }
                        error!("Request handling failed: {}", e);

                        // Return error response
                        let error_response = ResponsePayload {
                            success: false,
                            data: None,
                            error: Some(crate::plugin_ipc::message::ErrorInfo {
                                code: "HANDLER_ERROR".to_string(),
                                message: e.to_string(),
                                details: None,
                                stack_trace: None,
                            }),
                            execution_time_ms: duration.as_millis() as u64,
                            resource_usage: crate::plugin_ipc::message::ResourceUsage::default(),
                            metadata: std::collections::HashMap::new(),
                        };

                        Ok(IpcMessage::response(
                            message.header.correlation_id.clone().unwrap_or_default(),
                            error_response,
                        ))
                    }
                }
            } else {
                Err(IpcError::Message {
                    message: format!("No handler for operation: {}", request_payload.operation),
                    code: crate::plugin_ipc::error::MessageErrorCode::MessageNotFound,
                    message_id: Some(message.header.message_id.clone()),
                })
            }
        } else {
            Err(IpcError::Message {
                message: "Expected request payload".to_string(),
                code: crate::plugin_ipc::error::MessageErrorCode::InvalidMessageFormat,
                message_id: Some(message.header.message_id.clone()),
            })
        }
    }
}

/// Plugin information
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: crate::plugin_ipc::message::ClientCapabilities,
    pub client_info: ClientInfo,
    pub auth_result: crate::plugin_ipc::security::AuthResult,
    pub connected_at: std::time::SystemTime,
    pub last_activity: std::time::SystemTime,
    pub status: PluginStatus,
}

/// Client information from connection
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub endpoint: String,
    pub transport_type: TransportType,
    pub remote_address: String,
    pub user_agent: Option<String>,
}

/// Plugin connection status
#[derive(Debug, Clone, PartialEq)]
pub enum PluginStatus {
    Connected,
    Active,
    Idle,
    Disconnected,
    Error(String),
}

/// Request handler trait
pub struct RequestHandler {
    handler: Box<dyn Fn(IpcMessage) -> std::pin::Pin<Box<dyn std::future::Future<Output = IpcResult<IpcMessage>> + Send>> + Send + Sync>,
}

impl RequestHandler {
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: Fn(IpcMessage) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = IpcResult<IpcMessage>> + Send + 'static,
    {
        Self {
            handler: Box::new(move |msg| Box::pin(f(msg))),
        }
    }

    pub async fn handle(&self, message: IpcMessage) -> IpcResult<IpcMessage> {
        (self.handler)(message).await
    }
}

/// Server state
#[derive(Debug)]
struct ServerState {
    running: bool,
    start_time: std::time::SystemTime,
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
}

impl ServerState {
    fn new() -> Self {
        Self {
            running: false,
            start_time: std::time::SystemTime::now(),
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
        }
    }
}

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub running: bool,
    pub uptime: Duration,
    pub connected_plugins: usize,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub active_connections: usize,
    pub total_connections: usize,
    pub messages_per_second: f64,
    pub error_rate: f64,
}

/// Server builder for easy configuration
pub struct IpcServerBuilder {
    config: IpcConfig,
}

impl IpcServerBuilder {
    /// Create a new server builder
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

    /// Set the socket path
    pub fn socket_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.config.transport.socket_path = path.into();
        self
    }

    /// Set the TCP port range
    pub fn tcp_port_range(mut self, range: std::ops::Range<u16>) -> Self {
        self.config.transport.tcp_port_range = range;
        self
    }

    /// Set maximum concurrent requests
    pub fn max_concurrent_requests(mut self, max: u32) -> Self {
        self.config.performance.max_concurrent_requests = max;
        self
    }

    /// Build the server
    pub async fn build(self) -> IpcResult<IpcServer> {
        IpcServer::new(self.config).await
    }
}

impl Default for IpcServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_builder() {
        let server = IpcServerBuilder::new()
            .environment(crate::plugin_ipc::config::Environment::Testing)
            .socket_path("/tmp/test-server")
            .tcp_port_range(9000..9100)
            .max_concurrent_requests(50)
            .build()
            .await;

        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = IpcConfig::testing();
        let server = IpcServer::new(config).await;
        assert!(server.is_ok());
    }

    #[test]
    fn test_plugin_info() {
        let plugin = PluginInfo {
            id: "test_plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: crate::plugin_ipc::message::ClientCapabilities::default(),
            client_info: ClientInfo {
                endpoint: "test_endpoint".to_string(),
                transport_type: TransportType::UnixDomainSocket,
                remote_address: "local".to_string(),
                user_agent: None,
            },
            auth_result: crate::plugin_ipc::security::AuthResult {
                session_id: "session123".to_string(),
                plugin_id: "test_plugin".to_string(),
                permissions: vec![],
                expires_at: std::time::SystemTime::now(),
                security_level: crate::plugin_ipc::security::SecurityLevel::Medium,
            },
            connected_at: std::time::SystemTime::now(),
            last_activity: std::time::SystemTime::now(),
            status: PluginStatus::Connected,
        };

        assert_eq!(plugin.id, "test_plugin");
        assert_eq!(plugin.name, "Test Plugin");
        assert_eq!(plugin.status, PluginStatus::Connected);
    }

    #[test]
    fn test_server_state() {
        let state = ServerState::new();
        assert!(!state.running);
        assert_eq!(state.total_requests, 0);
        assert_eq!(state.successful_requests, 0);
        assert_eq!(state.failed_requests, 0);
    }

    #[tokio::test]
    async fn test_request_handler() {
        let handler = RequestHandler::new(|message| {
            Box::pin(async move {
                let response_payload = ResponsePayload {
                    success: true,
                    data: Some(serde_json::json!({"result": "ok"})),
                    error: None,
                    execution_time_ms: 1,
                    resource_usage: crate::plugin_ipc::message::ResourceUsage::default(),
                    metadata: std::collections::HashMap::new(),
                };

                Ok(IpcMessage::response(
                    message.header.correlation_id.clone().unwrap_or_default(),
                    response_payload,
                ))
            })
        });

        let test_message = IpcMessage::new(
            MessageType::Request,
            crate::plugin_ipc::message::MessagePayload::Request(crate::plugin_ipc::message::RequestPayload {
                operation: "test".to_string(),
                parameters: serde_json::json!({}),
                context: crate::plugin_ipc::message::ExecutionContext::default(),
                timeout_ms: None,
                metadata: std::collections::HashMap::new(),
            }),
        );

        let result = handler.handle(test_message).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_info() {
        let info = ClientInfo {
            endpoint: "test_endpoint".to_string(),
            transport_type: TransportType::Tcp,
            remote_address: "127.0.0.1:12345".to_string(),
            user_agent: Some("test-client/1.0".to_string()),
        };

        assert_eq!(info.endpoint, "test_endpoint");
        assert_eq!(info.transport_type, TransportType::Tcp);
        assert_eq!(info.remote_address, "127.0.0.1:12345");
        assert_eq!(info.user_agent, Some("test-client/1.0".to_string()));
    }

    #[test]
    fn test_plugin_status() {
        let statuses = vec![
            PluginStatus::Connected,
            PluginStatus::Active,
            PluginStatus::Idle,
            PluginStatus::Disconnected,
            PluginStatus::Error("test error".to_string()),
        ];

        assert_eq!(statuses[0], PluginStatus::Connected);
        assert_eq!(statuses[4], PluginStatus::Error("test error".to_string()));
    }
}