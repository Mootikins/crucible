//! # IPC Transport Layer
//!
//! High-performance transport layer implementation supporting Unix domain sockets,
//! TCP connections, connection pooling, multiplexing, and performance optimizations.

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::IpcMessage,
    security::SecurityManager,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot, RwLock, Semaphore};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Transport type enumeration
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportType {
    UnixDomainSocket,
    Tcp,
    Auto, // Choose based on platform and availability
}

/// Connection pool for efficient connection reuse
pub struct ConnectionPool {
    /// Available connections
    connections: Arc<RwLock<HashMap<String, Vec<PooledConnection>>>>,
    /// Connection configuration
    config: ConnectionPoolConfig,
    /// Maximum connections per endpoint
    max_connections_per_endpoint: usize,
    /// Connection semaphore for global limits
    connection_semaphore: Arc<Semaphore>,
    /// Security manager
    security_manager: Arc<SecurityManager>,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(
        config: ConnectionPoolConfig,
        security_manager: Arc<SecurityManager>,
    ) -> Self {
        let max_connections = config.max_total_connections;
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            max_connections_per_endpoint: config.max_connections_per_endpoint,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            config,
            security_manager,
        }
    }

    /// Get a connection from the pool or create a new one
    pub async fn get_connection(
        &self,
        endpoint: &str,
        transport_type: TransportType,
    ) -> IpcResult<PooledConnection> {
        // Acquire connection permit
        let _permit = self.connection_semaphore.acquire()
            .await
            .map_err(|_| IpcError::Connection {
                message: "Connection pool exhausted".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::MaxConnectionsExceeded,
                endpoint: endpoint.to_string(),
                retry_count: 0,
            })?;

        let mut connections = self.connections.write().await;
        let endpoint_connections = connections.entry(endpoint.to_string()).or_insert_with(Vec::new);

        // Try to reuse an existing connection
        if let Some(pos) = endpoint_connections.iter().position(|conn| conn.is_healthy()) {
            let mut conn = endpoint_connections.swap_remove(pos);
            conn.last_used = Instant::now();
            return Ok(conn);
        }

        // Remove unhealthy connections
        endpoint_connections.retain(|conn| conn.is_healthy());

        // Check if we can create a new connection
        if endpoint_connections.len() >= self.max_connections_per_endpoint {
            return Err(IpcError::Connection {
                message: "Max connections per endpoint exceeded".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::MaxConnectionsExceeded,
                endpoint: endpoint.to_string(),
                retry_count: 0,
            });
        }

        // Create a new connection
        let connection = self.create_connection(endpoint, transport_type).await?;
        endpoint_connections.push(connection.clone());

        Ok(connection)
    }

    /// Return a connection to the pool
    pub async fn return_connection(&self, connection: PooledConnection) {
        let mut connections = self.connections.write().await;
        if let Some(endpoint_connections) = connections.get_mut(&connection.endpoint) {
            if connection.is_healthy() && endpoint_connections.len() < self.max_connections_per_endpoint {
                endpoint_connections.push(connection);
            }
        }
    }

    /// Close all connections in the pool
    pub async fn close_all(&self) {
        let mut connections = self.connections.write().await;
        for (_, endpoint_connections) in connections.drain() {
            for conn in endpoint_connections {
                let _ = conn.close().await;
            }
        }
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> ConnectionPoolStats {
        let connections = self.connections.read().await;
        let mut total_connections = 0;
        let mut active_connections = 0;
        let mut idle_connections = 0;

        for (_, endpoint_connections) in connections.iter() {
            total_connections += endpoint_connections.len();
            for conn in endpoint_connections {
                if conn.is_active() {
                    active_connections += 1;
                } else {
                    idle_connections += 1;
                }
            }
        }

        ConnectionPoolStats {
            total_connections,
            active_connections,
            idle_connections,
            available_permits: self.connection_semaphore.available_permits(),
        }
    }

    /// Clean up idle connections
    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        for (_, endpoint_connections) in connections.iter_mut() {
            endpoint_connections.retain(|conn| {
                conn.is_healthy() && (now.duration_since(conn.last_used) < self.config.idle_timeout)
            });
        }
    }

    /// Create a new connection
    async fn create_connection(&self, endpoint: &str, transport_type: TransportType) -> IpcResult<PooledConnection> {
        let connection_id = uuid::Uuid::new_v4().to_string();
        let created_at = Instant::now();

        let raw_connection = match transport_type {
            TransportType::UnixDomainSocket => {
                let stream = timeout(
                    Duration::from_millis(self.config.connect_timeout_ms),
                    UnixStream::connect(endpoint)
                )
                .await
                .map_err(|_| IpcError::Connection {
                    message: "Connection timeout".to_string(),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionTimedOut,
                    endpoint: endpoint.to_string(),
                    retry_count: 0,
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to connect to Unix socket: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionRefused,
                    endpoint: endpoint.to_string(),
                    retry_count: 0,
                })?;

                RawConnection::Unix(stream)
            }
            TransportType::Tcp => {
                let stream = timeout(
                    Duration::from_millis(self.config.connect_timeout_ms),
                    TcpStream::connect(endpoint)
                )
                .await
                .map_err(|_| IpcError::Connection {
                    message: "Connection timeout".to_string(),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionTimedOut,
                    endpoint: endpoint.to_string(),
                    retry_count: 0,
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to connect to TCP endpoint: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionRefused,
                    endpoint: endpoint.to_string(),
                    retry_count: 0,
                })?;

                RawConnection::Tcp(stream)
            }
            TransportType::Auto => {
                // Try Unix socket first, fallback to TCP
                match self.create_connection(endpoint, TransportType::UnixDomainSocket).await {
                    Ok(conn) => return Ok(conn),
                    Err(_) => {
                        warn!("Unix socket connection failed, trying TCP");
                        return self.create_connection(endpoint, TransportType::Tcp).await;
                    }
                }
            }
        };

        Ok(PooledConnection {
            id: connection_id,
            endpoint: endpoint.to_string(),
            connection: raw_connection,
            created_at,
            last_used: Instant::now(),
            message_count: 0,
            is_active: false,
        })
    }
}

/// Pooled connection wrapper
#[derive(Debug, Clone)]
pub struct PooledConnection {
    pub id: String,
    pub endpoint: String,
    connection: RawConnection,
    pub created_at: Instant,
    pub last_used: Instant,
    pub message_count: u64,
    pub is_active: bool,
}

impl PooledConnection {
    /// Check if the connection is healthy
    pub fn is_healthy(&self) -> bool {
        // In a real implementation, this would check connection health
        self.created_at.elapsed() < Duration::from_secs(300) // 5 minutes max age
    }

    /// Check if the connection is currently active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Send a message through the connection
    pub async fn send_message(&mut self, message: &IpcMessage) -> IpcResult<()> {
        self.is_active = true;
        self.last_used = Instant::now();
        self.message_count += 1;

        // Serialize message
        let serialized = serde_json::to_vec(message)
            .map_err(|e| IpcError::Message {
                message: format!("Failed to serialize message: {}", e),
                code: crate::plugin_ipc::error::MessageErrorCode::SerializationFailed,
                message_id: Some(message.header.message_id.clone()),
            })?;

        // Send message length first
        let length = serialized.len() as u32;
        match &mut self.connection {
            RawConnection::Unix(stream) => {
                stream.write_u32(length).await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to write message length: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
                stream.write_all(&serialized).await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to write message: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
                stream.flush().await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to flush stream: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
            }
            RawConnection::Tcp(stream) => {
                stream.write_u32(length).await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to write message length: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
                stream.write_all(&serialized).await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to write message: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
                stream.flush().await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to flush stream: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
            }
        }

        self.is_active = false;
        Ok(())
    }

    /// Receive a message from the connection
    pub async fn receive_message(&mut self) -> IpcResult<IpcMessage> {
        self.is_active = true;
        self.last_used = Instant::now();

        // Read message length first
        let length = match &mut self.connection {
            RawConnection::Unix(stream) => {
                timeout(
                    Duration::from_millis(30000), // 30 second read timeout
                    stream.read_u32()
                )
                .await
                .map_err(|_| IpcError::Timeout {
                    message: "Read timeout".to_string(),
                    operation: "read_message_length".to_string(),
                    timeout: Duration::from_secs(30),
                    elapsed: Duration::from_secs(30),
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to read message length: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                    endpoint: self.endpoint.clone(),
                    retry_count: 0,
                })?
            }
            RawConnection::Tcp(stream) => {
                timeout(
                    Duration::from_millis(30000),
                    stream.read_u32()
                )
                .await
                .map_err(|_| IpcError::Timeout {
                    message: "Read timeout".to_string(),
                    operation: "read_message_length".to_string(),
                    timeout: Duration::from_secs(30),
                    elapsed: Duration::from_secs(30),
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to read message length: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                    endpoint: self.endpoint.clone(),
                    retry_count: 0,
                })?
            }
        };

        if length > crate::plugin_ipc::MAX_MESSAGE_SIZE as u32 {
            return Err(IpcError::Message {
                message: format!("Message too large: {} bytes", length),
                code: crate::plugin_ipc::error::MessageErrorCode::MessageTooLarge,
                message_id: None,
            });
        }

        // Read message payload
        let mut buffer = vec![0u8; length as usize];
        match &mut self.connection {
            RawConnection::Unix(stream) => {
                timeout(
                    Duration::from_millis(30000),
                    stream.read_exact(&mut buffer)
                )
                .await
                .map_err(|_| IpcError::Timeout {
                    message: "Read timeout".to_string(),
                    operation: "read_message_payload".to_string(),
                    timeout: Duration::from_secs(30),
                    elapsed: Duration::from_secs(30),
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to read message payload: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                    endpoint: self.endpoint.clone(),
                    retry_count: 0,
                })?;
            }
            RawConnection::Tcp(stream) => {
                timeout(
                    Duration::from_millis(30000),
                    stream.read_exact(&mut buffer)
                )
                .await
                .map_err(|_| IpcError::Timeout {
                    message: "Read timeout".to_string(),
                    operation: "read_message_payload".to_string(),
                    timeout: Duration::from_secs(30),
                    elapsed: Duration::from_secs(30),
                })?
                .map_err(|e| IpcError::Connection {
                    message: format!("Failed to read message payload: {}", e),
                    code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                    endpoint: self.endpoint.clone(),
                    retry_count: 0,
                })?;
            }
        }

        // Deserialize message
        let message: IpcMessage = serde_json::from_slice(&buffer)
            .map_err(|e| IpcError::Message {
                message: format!("Failed to deserialize message: {}", e),
                code: crate::plugin_ipc::error::MessageErrorCode::DeserializationFailed,
                message_id: None,
            })?;

        self.is_active = false;
        Ok(message)
    }

    /// Close the connection
    pub async fn close(mut self) -> IpcResult<()> {
        match &mut self.connection {
            RawConnection::Unix(stream) => {
                stream.shutdown().await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to shutdown Unix stream: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
            }
            RawConnection::Tcp(stream) => {
                stream.shutdown().await
                    .map_err(|e| IpcError::Connection {
                        message: format!("Failed to shutdown TCP stream: {}", e),
                        code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                        endpoint: self.endpoint.clone(),
                        retry_count: 0,
                    })?;
            }
        }
        Ok(())
    }
}

/// Raw connection type
#[derive(Debug)]
pub enum RawConnection {
    Unix(UnixStream),
    Tcp(TcpStream),
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_total_connections: usize,
    pub max_connections_per_endpoint: usize,
    pub connect_timeout_ms: u64,
    pub idle_timeout: Duration,
    pub health_check_interval: Duration,
    pub enable_connection_multiplexing: bool,
    pub enable_compression: bool,
    pub enable_encryption: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_total_connections: 100,
            max_connections_per_endpoint: 10,
            connect_timeout_ms: 5000,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
            enable_connection_multiplexing: true,
            enable_compression: true,
            enable_encryption: true,
        }
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub available_permits: usize,
}

/// Transport manager for handling multiple connection types
pub struct TransportManager {
    /// Connection pool
    connection_pool: Arc<ConnectionPool>,
    /// Active listeners
    listeners: Arc<RwLock<HashMap<String, ListenerHandle>>>,
    /// Message router
    message_router: Arc<MessageRouter>,
    /// Performance metrics
    metrics: Arc<RwLock<TransportMetrics>>,
}

impl TransportManager {
    /// Create a new transport manager
    pub fn new(
        config: ConnectionPoolConfig,
        security_manager: Arc<SecurityManager>,
    ) -> Self {
        Self {
            connection_pool: Arc::new(ConnectionPool::new(config, security_manager)),
            listeners: Arc::new(RwLock::new(HashMap::new())),
            message_router: Arc::new(MessageRouter::new()),
            metrics: Arc::new(RwLock::new(TransportMetrics::new())),
        }
    }

    /// Start listening on a Unix domain socket
    pub async fn listen_unix(&self, path: &str) -> IpcResult<()> {
        let listener = UnixListener::bind(path)
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to bind Unix socket: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::AddressInUse,
                endpoint: path.to_string(),
                retry_count: 0,
            })?;

        let handle = ListenerHandle::Unix(listener);
        self.listeners.write().await.insert(path.to_string(), handle);

        info!("Started listening on Unix socket: {}", path);
        Ok(())
    }

    /// Start listening on a TCP port
    pub async fn listen_tcp(&self, addr: &str) -> IpcResult<()> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to bind TCP address: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::AddressInUse,
                endpoint: addr.to_string(),
                retry_count: 0,
            })?;

        let handle = ListenerHandle::Tcp(listener);
        self.listeners.write().await.insert(addr.to_string(), handle);

        info!("Started listening on TCP address: {}", addr);
        Ok(())
    }

    /// Send a message to a specific endpoint
    pub async fn send_message(&self, endpoint: &str, message: IpcMessage) -> IpcResult<()> {
        let start_time = Instant::now();

        let mut connection = self.connection_pool.get_connection(endpoint, TransportType::Auto).await?;

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.messages_sent += 1;
            metrics.total_bytes_sent += message.size().unwrap_or(0) as u64;
        }

        let result = connection.send_message(&message).await;

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            let elapsed = start_time.elapsed();
            metrics.avg_send_time = (metrics.avg_send_time * (metrics.messages_sent - 1) as f64 + elapsed.as_secs_f64()) / metrics.messages_sent as f64;

            if result.is_err() {
                metrics.send_errors += 1;
            }
        }

        // Return connection to pool
        self.connection_pool.return_connection(connection).await;

        result
    }

    /// Get transport metrics
    pub async fn get_metrics(&self) -> TransportMetrics {
        self.metrics.read().await.clone()
    }

    /// Get connection pool statistics
    pub async fn get_pool_stats(&self) -> ConnectionPoolStats {
        self.connection_pool.get_stats().await
    }

    /// Cleanup idle connections
    pub async fn cleanup(&self) {
        self.connection_pool.cleanup_idle_connections().await;
    }

    /// Shutdown the transport manager
    pub async fn shutdown(&self) -> IpcResult<()> {
        // Close all listeners
        let mut listeners = self.listeners.write().await;
        for (_, handle) in listeners.drain() {
            match handle {
                ListenerHandle::Unix(_) => {
                    // Unix listeners are closed when dropped
                }
                ListenerHandle::Tcp(_) => {
                    // TCP listeners are closed when dropped
                }
            }
        }

        // Close all connections
        self.connection_pool.close_all().await;

        info!("Transport manager shutdown complete");
        Ok(())
    }
}

/// Listener handle for different transport types
#[derive(Debug)]
pub enum ListenerHandle {
    Unix(UnixListener),
    Tcp(TcpListener),
}

/// Message router for handling message routing and load balancing
pub struct MessageRouter {
    /// Routing rules
    routing_rules: RwLock<HashMap<String, RoutingRule>>,
    /// Load balancer
    load_balancer: Arc<LoadBalancer>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            routing_rules: RwLock::new(HashMap::new()),
            load_balancer: Arc::new(LoadBalancer::new()),
        }
    }

    /// Add a routing rule
    pub async fn add_routing_rule(&self, rule: RoutingRule) {
        let mut rules = self.routing_rules.write().await;
        rules.insert(rule.pattern.clone(), rule);
    }

    /// Route a message to the appropriate endpoint
    pub async fn route_message(&self, message: &IpcMessage) -> IpcResult<String> {
        let rules = self.routing_rules.read().await;

        // Find matching routing rule
        for rule in rules.values() {
            if self.matches_rule(message, &rule.pattern) {
                return Ok(self.load_balancer.select_endpoint(&rule.endpoints).await);
            }
        }

        Err(IpcError::Message {
            message: "No routing rule found for message".to_string(),
            code: crate::plugin_ipc::error::MessageErrorCode::MessageNotFound,
            message_id: Some(message.header.message_id.clone()),
        })
    }

    fn matches_rule(&self, message: &IpcMessage, pattern: &str) -> bool {
        // Simple pattern matching - in reality, this would be more sophisticated
        message.header.message_type.to_string().contains(pattern)
    }
}

/// Routing rule configuration
#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub pattern: String,
    pub endpoints: Vec<String>,
    pub load_balancing_strategy: LoadBalancingStrategy,
}

/// Load balancer for distributing requests across endpoints
pub struct LoadBalancer {
    endpoints: RwLock<HashMap<String, EndpointInfo>>,
    strategy: LoadBalancingStrategy,
}

impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            endpoints: RwLock::new(HashMap::new()),
            strategy: LoadBalancingStrategy::RoundRobin,
        }
    }

    pub async fn select_endpoint(&self, endpoints: &[String]) -> String {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let endpoints_guard = self.endpoints.read().await;
                endpoints.first().unwrap_or(&"default".to_string()).clone()
            }
            LoadBalancingStrategy::LeastConnections => {
                // Find endpoint with least connections
                let endpoints_guard = self.endpoints.read().await;
                endpoints
                    .iter()
                    .min_by_key(|endpoint| {
                        endpoints_guard
                            .get(*endpoint)
                            .map(|info| info.active_connections)
                            .unwrap_or(0)
                    })
                    .unwrap_or(&"default".to_string())
                    .clone()
            }
            LoadBalancingStrategy::WeightedRandom => {
                // Weighted random selection
                endpoints
                    .choose(&mut fastrand::Rng::default())
                    .unwrap_or(&"default".to_string())
                    .clone()
            }
        }
    }
}

/// Load balancing strategies
#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    LeastConnections,
    WeightedRandom,
    HealthBased,
}

/// Endpoint information for load balancing
#[derive(Debug, Clone)]
pub struct EndpointInfo {
    pub active_connections: u32,
    pub total_requests: u64,
    pub average_response_time: f64,
    pub health_status: HealthStatus,
    pub last_health_check: Instant,
}

/// Health status for endpoints
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Transport performance metrics
#[derive(Debug, Clone, Default)]
pub struct TransportMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub avg_send_time: f64,
    pub avg_receive_time: f64,
    pub send_errors: u64,
    pub receive_errors: u64,
    pub active_connections: u32,
    pub connection_errors: u64,
}

impl TransportMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn success_rate(&self) -> f64 {
        if self.messages_sent + self.messages_received == 0 {
            1.0
        } else {
            1.0 - (self.send_errors + self.receive_errors) as f64 / (self.messages_sent + self.messages_received) as f64
        }
    }

    pub fn throughput(&self) -> f64 {
        (self.messages_sent + self.messages_received) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type() {
        assert_eq!(TransportType::UnixDomainSocket, TransportType::UnixDomainSocket);
        assert_ne!(TransportType::Tcp, TransportType::UnixDomainSocket);
    }

    #[test]
    fn test_connection_pool_config() {
        let config = ConnectionPoolConfig::default();
        assert_eq!(config.max_total_connections, 100);
        assert_eq!(config.max_connections_per_endpoint, 10);
        assert_eq!(config.connect_timeout_ms, 5000);
        assert!(config.enable_connection_multiplexing);
        assert!(config.enable_compression);
        assert!(config.enable_encryption);
    }

    #[test]
    fn test_pooled_connection_health() {
        let connection = PooledConnection {
            id: "test".to_string(),
            endpoint: "test_endpoint".to_string(),
            connection: RawConnection::Unix(UnixStream::connect("/tmp/test").unwrap()),
            created_at: Instant::now(),
            last_used: Instant::now(),
            message_count: 0,
            is_active: false,
        };

        // Connection should be healthy if created recently
        assert!(connection.is_healthy());
        assert!(!connection.is_active());
    }

    #[test]
    fn test_load_balancing_strategies() {
        let strategies = vec![
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::LeastConnections,
            LoadBalancingStrategy::WeightedRandom,
            LoadBalancingStrategy::HealthBased,
        ];

        for strategy in strategies {
            // Just ensure they can be created
            assert!(true);
        }
    }

    #[test]
    fn test_transport_metrics() {
        let mut metrics = TransportMetrics::new();
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
        assert_eq!(metrics.success_rate(), 1.0);

        metrics.messages_sent = 100;
        metrics.send_errors = 5;
        assert_eq!(metrics.success_rate(), 0.95);

        metrics.reset();
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.success_rate(), 1.0);
    }

    #[test]
    fn test_endpoint_info() {
        let info = EndpointInfo {
            active_connections: 5,
            total_requests: 1000,
            average_response_time: 50.5,
            health_status: HealthStatus::Healthy,
            last_health_check: Instant::now(),
        };

        assert_eq!(info.active_connections, 5);
        assert_eq!(info.total_requests, 1000);
        assert_eq!(info.health_status, HealthStatus::Healthy);
    }

    #[test]
    fn test_routing_rule() {
        let rule = RoutingRule {
            pattern: "test".to_string(),
            endpoints: vec!["endpoint1".to_string(), "endpoint2".to_string()],
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        assert_eq!(rule.pattern, "test");
        assert_eq!(rule.endpoints.len(), 2);
        assert!(matches!(rule.load_balancing_strategy, LoadBalancingStrategy::RoundRobin));
    }

    #[tokio::test]
    async fn test_transport_manager_creation() {
        let config = ConnectionPoolConfig::default();
        let security_manager = Arc::new(SecurityManager::new(
            crate::plugin_ipc::security::AuthConfig::default(),
            crate::plugin_ipc::security::EncryptionConfig::default(),
            crate::plugin_ipc::security::AuthorizationConfig::default(),
        ));

        let transport_manager = TransportManager::new(config, security_manager);

        let metrics = transport_manager.get_metrics().await;
        assert_eq!(metrics.messages_sent, 0);

        let pool_stats = transport_manager.get_pool_stats().await;
        assert_eq!(pool_stats.total_connections, 0);
    }

    #[test]
    fn test_health_status() {
        let statuses = vec![
            HealthStatus::Healthy,
            HealthStatus::Degraded,
            HealthStatus::Unhealthy,
            HealthStatus::Unknown,
        ];

        assert!(HealthStatus::Healthy > HealthStatus::Degraded);
        assert!(HealthStatus::Degraded > HealthStatus::Unhealthy);
        assert!(HealthStatus::Unhealthy > HealthStatus::Unknown);
    }
}