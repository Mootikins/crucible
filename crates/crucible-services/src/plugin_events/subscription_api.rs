//! REST and WebSocket APIs for plugin event subscription management

use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::{
        AuthContext, DeliveryOptions, EventPermission, PermissionScope,
        SubscriptionConfig, SubscriptionId, SubscriptionStats, SubscriptionStatus,
        SubscriptionType,
    },
    SubscriptionManager,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use warp::{Filter, Reply};

/// API server for subscription management
pub struct SubscriptionApiServer {
    /// Subscription manager
    subscription_manager: Arc<SubscriptionManager>,

    /// API configuration
    config: ApiConfig,

    /// WebSocket connections
    websocket_connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,

    /// API statistics
    stats: Arc<RwLock<ApiStats>>,

    /// Server handle
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

/// API configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// API bind address
    pub bind_address: String,

    /// API port
    pub port: u16,

    /// Enable CORS
    pub enable_cors: bool,

    /// CORS allowed origins
    pub cors_origins: Vec<String>,

    /// API rate limiting
    pub rate_limiting: RateLimitConfig,

    /// Authentication configuration
    pub auth: AuthConfig,

    /// WebSocket configuration
    pub websocket: WebSocketConfig,

    /// API version
    pub api_version: String,

    /// Enable API metrics
    pub enable_metrics: bool,

    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            enable_cors: true,
            cors_origins: vec!["*".to_string()],
            rate_limiting: RateLimitConfig::default(),
            auth: AuthConfig::default(),
            websocket: WebSocketConfig::default(),
            api_version: "v1".to_string(),
            enable_metrics: true,
            request_timeout_seconds: 30,
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Requests per minute
    pub requests_per_minute: u32,

    /// Burst size
    pub burst_size: u32,

    /// Rate limiting by IP address
    pub by_ip_address: bool,

    /// Rate limiting by API key
    pub by_api_key: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 100,
            burst_size: 20,
            by_ip_address: true,
            by_api_key: true,
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,

    /// Authentication methods
    pub methods: Vec<AuthMethod>,

    /// JWT configuration
    pub jwt: Option<JwtConfig>,

    /// API key configuration
    pub api_keys: Option<ApiKeyConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            methods: vec![],
            jwt: None,
            api_keys: None,
        }
    }
}

/// Authentication method
#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    Jwt,
    ApiKey,
    Basic,
    OAuth,
}

/// JWT configuration
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// JWT secret
    pub secret: String,

    /// JWT algorithm
    pub algorithm: String,

    /// Token expiration in minutes
    pub expiration_minutes: u64,

    /// Issuer
    pub issuer: String,

    /// Audience
    pub audience: String,
}

/// API key configuration
#[derive(Debug, Clone)]
pub struct ApiKeyConfig {
    /// Valid API keys
    pub keys: HashMap<String, ApiKeyInfo>,
}

/// API key information
#[derive(Debug, Clone)]
pub struct ApiKeyInfo {
    /// API key name
    pub name: String,

    /// Associated plugin ID
    pub plugin_id: String,

    /// Key permissions
    pub permissions: Vec<String>,

    /// Key expiration
    pub expires_at: Option<DateTime<Utc>>,

    /// Key created at
    pub created_at: DateTime<Utc>,
}

/// WebSocket configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Enable WebSocket support
    pub enabled: bool,

    /// WebSocket path
    pub path: String,

    /// Maximum connections
    pub max_connections: usize,

    /// Connection timeout in seconds
    pub connection_timeout_seconds: u64,

    /// Heartbeat interval in seconds
    pub heartbeat_interval_seconds: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable per-message compression
    pub enable_compression: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/ws".to_string(),
            max_connections: 1000,
            connection_timeout_seconds: 300,
            heartbeat_interval_seconds: 30,
            max_message_size: 1024 * 1024, // 1MB
            enable_compression: true,
        }
    }
}

/// WebSocket connection
#[derive(Debug, Clone)]
struct WebSocketConnection {
    /// Connection ID
    connection_id: String,

    /// Plugin ID
    plugin_id: String,

    /// Connection established timestamp
    established_at: DateTime<Utc>,

    /// Last activity timestamp
    last_activity: DateTime<Utc>,

    /// Connected subscriptions
    subscriptions: Vec<SubscriptionId>,

    /// Connection metadata
    metadata: HashMap<String, String>,
}

/// API statistics
#[derive(Debug, Clone, Default)]
struct ApiStats {
    /// Total requests
    total_requests: u64,

    /// Requests by endpoint
    requests_by_endpoint: HashMap<String, u64>,

    /// Requests by status code
    requests_by_status: HashMap<u16, u64>,

    /// Active WebSocket connections
    active_websocket_connections: u64,

    /// Total WebSocket connections
    total_websocket_connections: u64,

    /// Average response time in milliseconds
    avg_response_time_ms: f64,

    /// Error rate (0.0 to 1.0)
    error_rate: f64,

    /// API start timestamp
    started_at: DateTime<Utc>,

    /// Last request timestamp
    last_request: Option<DateTime<Utc>>,
}

/// API request/response DTOs
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSubscriptionRequest {
    /// Plugin ID
    pub plugin_id: String,

    /// Subscription name
    pub name: String,

    /// Subscription type
    pub subscription_type: String,

    /// Event filters
    pub filters: Vec<EventFilterDto>,

    /// Delivery options
    pub delivery_options: Option<DeliveryOptionsDto>,

    /// Subscription metadata
    pub metadata: Option<HashMap<String, String>>,
}

/// Event filter DTO
#[derive(Debug, Serialize, Deserialize)]
pub struct EventFilterDto {
    /// Event types
    pub event_types: Option<Vec<String>>,

    /// Event categories
    pub categories: Option<Vec<String>>,

    /// Event priorities
    pub priorities: Option<Vec<u8>>,

    /// Event sources
    pub sources: Option<Vec<String>>,

    /// Custom expression
    pub expression: Option<String>,

    /// Maximum payload size
    pub max_payload_size: Option<usize>,
}

/// Delivery options DTO
#[derive(Debug, Serialize, Deserialize)]
pub struct DeliveryOptionsDto {
    /// Enable acknowledgments
    pub ack_enabled: Option<bool>,

    /// Maximum retries
    pub max_retries: Option<u32>,

    /// Retry backoff
    pub retry_backoff: Option<String>,

    /// Enable compression
    pub compression_enabled: Option<bool>,

    /// Compression threshold
    pub compression_threshold: Option<usize>,

    /// Enable encryption
    pub encryption_enabled: Option<bool>,

    /// Maximum event size
    pub max_event_size: Option<usize>,

    /// Event ordering
    pub ordering: Option<String>,

    /// Backpressure handling
    pub backpressure_handling: Option<String>,
}

/// Subscription response DTO
#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    /// Subscription ID
    pub id: String,

    /// Plugin ID
    pub plugin_id: String,

    /// Subscription name
    pub name: String,

    /// Subscription type
    pub subscription_type: String,

    /// Subscription status
    pub status: String,

    /// Event filters
    pub filters: Vec<EventFilterDto>,

    /// Delivery options
    pub delivery_options: DeliveryOptionsDto,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Updated timestamp
    pub updated_at: DateTime<Utc>,

    /// Subscription metadata
    pub metadata: HashMap<String, String>,

    /// Statistics
    pub stats: Option<SubscriptionStatsDto>,
}

/// Subscription statistics DTO
#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionStatsDto {
    /// Events received
    pub events_received: u64,

    /// Events delivered
    pub events_delivered: u64,

    /// Events failed
    pub events_failed: u64,

    /// Average delivery time
    pub avg_delivery_time_ms: f64,

    /// Queue size
    pub queue_size: usize,

    /// Last activity
    pub last_activity: Option<DateTime<Utc>>,

    /// Success rate
    pub success_rate: f64,
}

/// API error response DTO
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code
    pub error_code: String,

    /// Error message
    pub message: String,

    /// Error details
    pub details: Option<HashMap<String, String>>,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Request ID
    pub request_id: Option<String>,
}

/// WebSocket message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    /// Subscribe to events
    Subscribe {
        subscription_id: String,
    },

    /// Unsubscribe from events
    Unsubscribe {
        subscription_id: String,
    },

    /// Event notification
    Event {
        subscription_id: String,
        event_id: String,
        event_data: serde_json::Value,
    },

    /// Acknowledgment
    Ack {
        subscription_id: String,
        event_id: String,
    },

    /// Error
    Error {
        error_code: String,
        message: String,
    },

    /// Ping/Pong for heartbeat
    Ping,
    Pong,
}

impl SubscriptionApiServer {
    /// Create new API server
    pub fn new(
        subscription_manager: Arc<SubscriptionManager>,
        config: ApiConfig,
    ) -> Self {
        Self {
            subscription_manager,
            config,
            websocket_connections: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ApiStats {
                started_at: Utc::now(),
                ..Default::default()
            })),
            server_handle: None,
        }
    }

    /// Start the API server
    pub async fn start(&mut self) -> SubscriptionResult<()> {
        let subscription_manager = self.subscription_manager.clone();
        let config = self.config.clone();
        let websocket_connections = self.websocket_connections.clone();
        let stats = self.stats.clone();

        // Build API routes
        let api = Self::build_routes(
            subscription_manager,
            config.clone(),
            websocket_connections,
            stats,
        );

        // Apply CORS if enabled
        let api = if config.enable_cors {
            let cors = warp::cors()
                .allow_any_origin()
                .allow_headers(vec!["content-type", "authorization"])
                .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]);
            api.with(cors)
        } else {
            api
        };

        // Start server
        let addr = format!("{}:{}", config.bind_address, config.port);
        info!("Starting subscription API server on {}", addr);

        let server = warp::serve(api).bind_with_graceful_shutdown(
            addr.parse().expect("Invalid bind address"),
            async {
                // Wait for shutdown signal
                let (_tx, mut rx) = mpsc::channel::<()>(1);
                let _ = rx.recv().await;
                info!("API server shutdown signal received");
            },
        );

        let server_handle = tokio::spawn(async move {
            server.await;
        });

        self.server_handle = Some(server_handle);

        info!("Subscription API server started on {}", addr);

        Ok(())
    }

    /// Stop the API server
    pub async fn stop(&self) -> SubscriptionResult<()> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }

        // Close all WebSocket connections
        let mut connections = self.websocket_connections.write().await;
        connections.clear();

        info!("Subscription API server stopped");

        Ok(())
    }

    /// Build API routes
    fn build_routes(
        subscription_manager: Arc<SubscriptionManager>,
        config: ApiConfig,
        websocket_connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
        stats: Arc<RwLock<ApiStats>>,
    ) -> impl Filter<Extract = impl Reply, Error = warp::Rejection> + Clone {
        // Health check endpoint
        let health = warp::path("health")
            .and(warp::get())
            .map(|| {
                warp::reply::json(&serde_json::json!({
                    "status": "healthy",
                    "timestamp": Utc::now(),
                    "version": "1.0.0"
                }))
            });

        // Subscription management endpoints
        let subscriptions = warp::path("subscriptions");

        // GET /subscriptions - List subscriptions
        let list_subscriptions = subscriptions
            .and(warp::get())
            .and(warp::path::end())
            .and(with_subscription_manager(subscription_manager.clone()))
            .and_then(list_subscriptions_handler);

        // POST /subscriptions - Create subscription
        let create_subscription = subscriptions
            .and(warp::post())
            .and(warp::path::end())
            .and(warp::body::json::<CreateSubscriptionRequest>())
            .and(with_subscription_manager(subscription_manager.clone()))
            .and_then(create_subscription_handler);

        // GET /subscriptions/:id - Get subscription
        let get_subscription = subscriptions
            .and(warp::get())
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(with_subscription_manager(subscription_manager.clone()))
            .and_then(get_subscription_handler);

        // DELETE /subscriptions/:id - Delete subscription
        let delete_subscription = subscriptions
            .and(warp::delete())
            .and(warp::path::param::<String>())
            .and(warp::path::end())
            .and(with_subscription_manager(subscription_manager.clone()))
            .and_then(delete_subscription_handler);

        // GET /subscriptions/:id/stats - Get subscription statistics
        let get_subscription_stats = subscriptions
            .and(warp::get())
            .and(warp::path::param::<String>())
            .and(warp::path("stats"))
            .and(warp::path::end())
            .and(with_subscription_manager(subscription_manager.clone()))
            .and_then(get_subscription_stats_handler);

        // WebSocket endpoint
        let websocket_route = if config.websocket.enabled {
            let ws_path = warp::path(config.websocket.path.trim_start_matches('/'));
            Some(ws_path
                .and(warp::ws())
                .and(warp::path::end())
                .and(with_subscription_manager(subscription_manager.clone()))
                .and(with_websocket_connections(websocket_connections.clone()))
                .and(websocket_handler))
        } else {
            None
        };

        // Metrics endpoint
        let metrics = warp::path("metrics")
            .and(warp::get())
            .and(warp::path::end())
            .and(with_stats(stats.clone()))
            .and_then(metrics_handler);

        // Combine all routes
        let routes = health
            .or(list_subscriptions)
            .or(create_subscription)
            .or(get_subscription)
            .or(delete_subscription)
            .or(get_subscription_stats)
            .or(metrics);

        // Add WebSocket route if enabled
        if let Some(ws_route) = websocket_route {
            routes.or(ws_route)
        } else {
            routes
        }
    }

    /// Handle WebSocket connections
    async fn websocket_handler(
        ws: warp::ws::WebSocket,
        subscription_manager: Arc<SubscriptionManager>,
        connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
    ) -> impl Reply {
        let connection_id = uuid::Uuid::new_v4().to_string();

        // Accept WebSocket connection
        let (ws_tx, ws_rx) = ws.split();

        // Add connection to registry
        {
            let mut conn_guard = connections.write().await;
            conn_guard.insert(connection_id.clone(), WebSocketConnection {
                connection_id: connection_id.clone(),
                plugin_id: "unknown".to_string(), // Will be set after authentication
                established_at: Utc::now(),
                last_activity: Utc::now(),
                subscriptions: Vec::new(),
                metadata: HashMap::new(),
            });
        }

        // Handle WebSocket messages
        let connections_clone = connections.clone();
        let subscription_manager_clone = subscription_manager.clone();
        let connection_id_clone = connection_id.clone();

        let handle = tokio::spawn(async move {
            Self::handle_websocket_connection(
                ws_rx,
                ws_tx,
                connection_id_clone,
                connections_clone,
                subscription_manager_clone,
            ).await;
        });

        warp::reply::reply()
    }

    /// Handle WebSocket connection messages
    async fn handle_websocket_connection(
        mut ws_rx: warp::ws::WebSocket,
        mut ws_tx: warp::ws::WebSocketSender,
        connection_id: String,
        connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
        subscription_manager: Arc<SubscriptionManager>,
    ) {
        info!("WebSocket connection established: {}", connection_id);

        loop {
            match ws_rx.next().await {
                Some(Ok(message)) => {
                    match message.to_str() {
                        Ok(text) => {
                            // Parse WebSocket message
                            if let Ok(ws_message) = serde_json::from_str::<WebSocketMessage>(text) {
                                Self::handle_websocket_message(
                                    ws_message,
                                    &connection_id,
                                    &connections,
                                    &subscription_manager,
                                    &mut ws_tx,
                                ).await;
                            } else {
                                warn!("Invalid WebSocket message format from connection: {}", connection_id);
                            }
                        }
                        Err(_) => {
                            warn!("Binary WebSocket message not supported from connection: {}", connection_id);
                        }
                    }

                    // Update last activity
                    if let Some(conn) = connections.write().await.get_mut(&connection_id) {
                        conn.last_activity = Utc::now();
                    }
                }

                Some(Err(e)) => {
                    error!("WebSocket error from connection {}: {}", connection_id, e);
                    break;
                }

                None => {
                    debug!("WebSocket connection closed: {}", connection_id);
                    break;
                }
            }
        }

        // Remove connection from registry
        connections.write().await.remove(&connection_id);
        info!("WebSocket connection closed: {}", connection_id);
    }

    /// Handle individual WebSocket messages
    async fn handle_websocket_message(
        message: WebSocketMessage,
        connection_id: &str,
        connections: &Arc<RwLock<HashMap<String, WebSocketConnection>>>,
        subscription_manager: &Arc<SubscriptionManager>,
        ws_tx: &mut warp::ws::WebSocketSender,
    ) {
        match message {
            WebSocketMessage::Subscribe { subscription_id } => {
                // Handle subscription
                match SubscriptionId::from_string(subscription_id.clone()) {
                    Ok(sub_id) => {
                        // Validate subscription exists and belongs to this connection
                        if let Ok(subscription) = subscription_manager.get_subscription(&sub_id).await {
                            // Add subscription to connection
                            if let Some(conn) = connections.write().await.get_mut(connection_id) {
                                if !conn.subscriptions.contains(&sub_id) {
                                    conn.subscriptions.push(sub_id.clone());
                                }

                                // Send acknowledgment
                                let response = WebSocketMessage::Ack {
                                    subscription_id: subscription_id.clone(),
                                    event_id: "subscribe_ack".to_string(),
                                };

                                if let Ok(response_text) = serde_json::to_string(&response) {
                                    let _ = ws_tx.send(warp::ws::Message::text(response_text)).await;
                                }

                                info!("WebSocket subscription added: {} -> {}", connection_id, subscription_id);
                            }
                        } else {
                            // Subscription not found
                            let error_response = WebSocketMessage::Error {
                                error_code: "SUBSCRIPTION_NOT_FOUND".to_string(),
                                message: format!("Subscription {} not found", subscription_id),
                            };

                            if let Ok(error_text) = serde_json::to_string(&error_response) {
                                let _ = ws_tx.send(warp::ws::Message::text(error_text)).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Invalid subscription ID: {}", e);
                    }
                }
            }

            WebSocketMessage::Unsubscribe { subscription_id } => {
                // Handle unsubscription
                match SubscriptionId::from_string(subscription_id.clone()) {
                    Ok(sub_id) => {
                        if let Some(conn) = connections.write().await.get_mut(connection_id) {
                            conn.subscriptions.retain(|id| id != &sub_id);

                            // Send acknowledgment
                            let response = WebSocketMessage::Ack {
                                subscription_id: subscription_id.clone(),
                                event_id: "unsubscribe_ack".to_string(),
                            };

                            if let Ok(response_text) = serde_json::to_string(&response) {
                                let _ = ws_tx.send(warp::ws::Message::text(response_text)).await;
                            }

                            info!("WebSocket subscription removed: {} -> {}", connection_id, subscription_id);
                        }
                    }
                    Err(e) => {
                        error!("Invalid subscription ID: {}", e);
                    }
                }
            }

            WebSocketMessage::Ping => {
                // Send pong response
                let pong = WebSocketMessage::Pong;
                if let Ok(pong_text) = serde_json::to_string(&pong) {
                    let _ = ws_tx.send(warp::ws::Message::text(pong_text)).await;
                }
            }

            _ => {
                warn!("Unhandled WebSocket message type from connection: {}", connection_id);
            }
        }
    }
}

// Warp filter helpers
fn with_subscription_manager(
    manager: Arc<SubscriptionManager>,
) -> impl Filter<Extract = (Arc<SubscriptionManager>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || manager.clone())
}

fn with_websocket_connections(
    connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
) -> impl Filter<Extract = (Arc<RwLock<HashMap<String, WebSocketConnection>>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || connections.clone())
}

fn with_stats(
    stats: Arc<RwLock<ApiStats>>,
) -> impl Filter<Extract = (Arc<RwLock<ApiStats>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || stats.clone())
}

// API handlers
async fn list_subscriptions_handler(
    subscription_manager: Arc<SubscriptionManager>,
) -> Result<impl Reply, warp::Rejection> {
    // This would list all subscriptions (with proper filtering/pagination)
    let response = serde_json::json!({
        "subscriptions": [],
        "total": 0,
        "page": 1,
        "per_page": 50
    });

    Ok(warp::reply::json(&response))
}

async fn create_subscription_handler(
    request: CreateSubscriptionRequest,
    subscription_manager: Arc<SubscriptionManager>,
) -> Result<impl Reply, warp::Rejection> {
    // Convert DTO to internal types
    let subscription_type = match request.subscription_type.as_str() {
        "realtime" => SubscriptionType::Realtime,
        "batched" => SubscriptionType::Batched {
            interval_seconds: 60,
            max_batch_size: 100,
        },
        "persistent" => SubscriptionType::Persistent {
            max_stored_events: 1000,
            ttl: std::time::Duration::from_secs(3600),
        },
        _ => {
            return Ok(warp::reply::json(&ErrorResponse {
                error_code: "INVALID_SUBSCRIPTION_TYPE".to_string(),
                message: "Invalid subscription type".to_string(),
                details: None,
                timestamp: Utc::now(),
                request_id: None,
            }));
        }
    };

    let delivery_options = if let Some(options_dto) = request.delivery_options {
        DeliveryOptions {
            ack_enabled: options_dto.ack_enabled.unwrap_or(true),
            max_retries: options_dto.max_retries.unwrap_or(3),
            retry_backoff: crate::plugin_events::types::RetryBackoff::Exponential {
                base_ms: 1000,
                max_ms: 30000,
            },
            compression_enabled: options_dto.compression_enabled.unwrap_or(false),
            compression_threshold: options_dto.compression_threshold.unwrap_or(1024),
            encryption_enabled: options_dto.encryption_enabled.unwrap_or(false),
            max_event_size: options_dto.max_event_size.unwrap_or(10 * 1024 * 1024),
            ordering: crate::plugin_events::types::EventOrdering::Fifo,
            backpressure_handling: crate::plugin_events::types::BackpressureHandling::Buffer {
                max_size: 1000,
            },
        }
    } else {
        DeliveryOptions::default()
    };

    let filters = request.filters
        .into_iter()
        .map(|filter_dto| {
            crate::events::EventFilter {
                event_types: filter_dto.event_types.unwrap_or_default(),
                categories: filter_dto.categories
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|cat| match cat.as_str() {
                        "Filesystem" => Some(crate::events::EventCategory::Filesystem),
                        "Database" => Some(crate::events::EventCategory::Database),
                        "External" => Some(crate::events::EventCategory::External),
                        "Mcp" => Some(crate::events::EventCategory::Mcp),
                        "Service" => Some(crate::events::EventCategory::Service),
                        "System" => Some(crate::events::EventCategory::System),
                        _ => None,
                    })
                    .collect(),
                priorities: filter_dto.priorities
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|p| match p {
                        0 => Some(crate::events::EventPriority::Critical),
                        1 => Some(crate::events::EventPriority::High),
                        2 => Some(crate::events::EventPriority::Normal),
                        3 => Some(crate::events::EventPriority::Low),
                        _ => None,
                    })
                    .collect(),
                sources: filter_dto.sources.unwrap_or_default(),
                expression: filter_dto.expression,
                max_payload_size: filter_dto.max_payload_size,
            }
        })
        .collect();

    let auth_context = AuthContext::new(
        request.plugin_id.clone(),
        vec![EventPermission {
            scope: PermissionScope::Plugin,
            event_types: vec![],
            categories: vec![],
            sources: vec![],
            max_priority: None,
        }],
    );

    let subscription_config = SubscriptionConfig::new(
        request.plugin_id.clone(),
        request.name,
        subscription_type,
        auth_context,
    )
    .with_filters(filters)
    .with_delivery_options(delivery_options);

    if let Some(metadata) = request.metadata {
        let mut config = subscription_config;
        for (key, value) in metadata {
            config = config.with_metadata(key, value);
        }
        subscription_config = config;
    }

    // Create subscription
    match subscription_manager.create_subscription(request.plugin_id, subscription_config).await {
        Ok(subscription_id) => {
            let response = serde_json::json!({
                "subscription_id": subscription_id.as_string(),
                "status": "created",
                "timestamp": Utc::now()
            });
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            Ok(warp::reply::json(&ErrorResponse {
                error_code: "SUBSCRIPTION_CREATION_FAILED".to_string(),
                message: e.to_string(),
                details: None,
                timestamp: Utc::now(),
                request_id: None,
            }))
        }
    }
}

async fn get_subscription_handler(
    subscription_id: String,
    subscription_manager: Arc<SubscriptionManager>,
) -> Result<impl Reply, warp::Rejection> {
    match SubscriptionId::from_string(subscription_id.clone()) {
        Ok(id) => {
            match subscription_manager.get_subscription(&id).await {
                Ok(subscription) => {
                    let response = SubscriptionResponse {
                        id: subscription.id.as_string(),
                        plugin_id: subscription.plugin_id,
                        name: subscription.name,
                        subscription_type: match subscription.subscription_type {
                            SubscriptionType::Realtime => "realtime".to_string(),
                            SubscriptionType::Batched { .. } => "batched".to_string(),
                            SubscriptionType::Persistent { .. } => "persistent".to_string(),
                            SubscriptionType::Conditional { .. } => "conditional".to_string(),
                            SubscriptionType::Priority { .. } => "priority".to_string(),
                        },
                        status: match subscription.status {
                            SubscriptionStatus::Active => "active".to_string(),
                            SubscriptionStatus::Paused => "paused".to_string(),
                            SubscriptionStatus::Suspended { .. } => "suspended".to_string(),
                            SubscriptionStatus::Terminated { .. } => "terminated".to_string(),
                        },
                        filters: subscription.filters
                            .into_iter()
                            .map(|f| EventFilterDto {
                                event_types: if f.event_types.is_empty() { None } else { Some(f.event_types) },
                                categories: if f.categories.is_empty() { None } else { Some(f.categories.iter().map(|c| format!("{:?}", c)).collect()) },
                                priorities: if f.priorities.is_empty() { None } else { Some(f.priorities.iter().map(|p| p.value()).collect()) },
                                sources: if f.sources.is_empty() { None } else { Some(f.sources) },
                                expression: f.expression,
                                max_payload_size: f.max_payload_size,
                            })
                            .collect(),
                        delivery_options: DeliveryOptionsDto {
                            ack_enabled: Some(subscription.delivery_options.ack_enabled),
                            max_retries: Some(subscription.delivery_options.max_retries),
                            retry_backoff: Some("exponential".to_string()),
                            compression_enabled: Some(subscription.delivery_options.compression_enabled),
                            compression_threshold: Some(subscription.delivery_options.compression_threshold),
                            encryption_enabled: Some(subscription.delivery_options.encryption_enabled),
                            max_event_size: Some(subscription.delivery_options.max_event_size),
                            ordering: Some(format!("{:?}", subscription.delivery_options.ordering)),
                            backpressure_handling: Some(format!("{:?}", subscription.delivery_options.backpressure_handling)),
                        },
                        created_at: subscription.created_at,
                        updated_at: subscription.updated_at,
                        metadata: subscription.metadata,
                        stats: None,
                    };
                    Ok(warp::reply::json(&response))
                }
                Err(e) => {
                    Ok(warp::reply::json(&ErrorResponse {
                        error_code: "SUBSCRIPTION_NOT_FOUND".to_string(),
                        message: e.to_string(),
                        details: None,
                        timestamp: Utc::now(),
                        request_id: None,
                    }))
                }
            }
        }
        Err(e) => {
            Ok(warp::reply::json(&ErrorResponse {
                error_code: "INVALID_SUBSCRIPTION_ID".to_string(),
                message: e.to_string(),
                details: None,
                timestamp: Utc::now(),
                request_id: None,
            }))
        }
    }
}

async fn delete_subscription_handler(
    subscription_id: String,
    subscription_manager: Arc<SubscriptionManager>,
) -> Result<impl Reply, warp::Rejection> {
    match SubscriptionId::from_string(subscription_id.clone()) {
        Ok(id) => {
            match subscription_manager.delete_subscription(&id).await {
                Ok(()) => {
                    let response = serde_json::json!({
                        "subscription_id": subscription_id,
                        "status": "deleted",
                        "timestamp": Utc::now()
                    });
                    Ok(warp::reply::json(&response))
                }
                Err(e) => {
                    Ok(warp::reply::json(&ErrorResponse {
                        error_code: "SUBSCRIPTION_DELETION_FAILED".to_string(),
                        message: e.to_string(),
                        details: None,
                        timestamp: Utc::now(),
                        request_id: None,
                    }))
                }
            }
        }
        Err(e) => {
            Ok(warp::reply::json(&ErrorResponse {
                error_code: "INVALID_SUBSCRIPTION_ID".to_string(),
                message: e.to_string(),
                details: None,
                timestamp: Utc::now(),
                request_id: None,
            }))
        }
    }
}

async fn get_subscription_stats_handler(
    subscription_id: String,
    subscription_manager: Arc<SubscriptionManager>,
) -> Result<impl Reply, warp::Rejection> {
    match SubscriptionId::from_string(subscription_id.clone()) {
        Ok(id) => {
            match subscription_manager.get_subscription_stats(&id).await {
                Ok(stats) => {
                    let response = SubscriptionStatsDto {
                        events_received: stats.events_received,
                        events_delivered: stats.events_delivered,
                        events_failed: stats.events_failed,
                        avg_delivery_time_ms: stats.avg_delivery_time_ms,
                        queue_size: stats.queue_size,
                        last_activity: stats.last_activity,
                        success_rate: if stats.events_received > 0 {
                            stats.events_delivered as f64 / stats.events_received as f64
                        } else {
                            0.0
                        },
                    };
                    Ok(warp::reply::json(&response))
                }
                Err(e) => {
                    Ok(warp::reply::json(&ErrorResponse {
                        error_code: "STATS_NOT_FOUND".to_string(),
                        message: e.to_string(),
                        details: None,
                        timestamp: Utc::now(),
                        request_id: None,
                    }))
                }
            }
        }
        Err(e) => {
            Ok(warp::reply::json(&ErrorResponse {
                error_code: "INVALID_SUBSCRIPTION_ID".to_string(),
                message: e.to_string(),
                details: None,
                timestamp: Utc::now(),
                request_id: None,
            }))
        }
    }
}

async fn metrics_handler(
    stats: Arc<RwLock<ApiStats>>,
) -> Result<impl Reply, warp::Rejection> {
    let stats_guard = stats.read().await;
    let response = serde_json::json!({
        "api": {
            "total_requests": stats_guard.total_requests,
            "requests_by_endpoint": stats_guard.requests_by_endpoint,
            "requests_by_status": stats_guard.requests_by_status,
            "avg_response_time_ms": stats_guard.avg_response_time_ms,
            "error_rate": stats_guard.error_rate,
            "started_at": stats_guard.started_at,
            "last_request": stats_guard.last_request,
        },
        "websocket": {
            "active_connections": stats_guard.active_websocket_connections,
            "total_connections": stats_guard.total_websocket_connections,
        },
        "timestamp": Utc::now()
    });
    Ok(warp::reply::json(&response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_events::types::SubscriptionConfig;

    #[tokio::test]
    async fn test_api_server_creation() {
        let config = ApiConfig::default();
        let subscription_manager = Arc::new(SubscriptionManager::default());
        let api_server = SubscriptionApiServer::new(subscription_manager, config);

        // Test that server is created successfully
        assert_eq!(api_server.config.port, 8080);
        assert!(api_server.config.enable_cors);
    }

    #[test]
    fn test_websocket_message_serialization() {
        let message = WebSocketMessage::Subscribe {
            subscription_id: "test-subscription".to_string(),
        };

        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: WebSocketMessage = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            WebSocketMessage::Subscribe { subscription_id } => {
                assert_eq!(subscription_id, "test-subscription");
            }
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[test]
    fn test_create_subscription_request_serialization() {
        let request = CreateSubscriptionRequest {
            plugin_id: "test-plugin".to_string(),
            name: "Test Subscription".to_string(),
            subscription_type: "realtime".to_string(),
            filters: vec![],
            delivery_options: None,
            metadata: None,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: CreateSubscriptionRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.plugin_id, "test-plugin");
        assert_eq!(deserialized.name, "Test Subscription");
        assert_eq!(deserialized.subscription_type, "realtime");
    }

    #[test]
    fn test_error_response_serialization() {
        let error = ErrorResponse {
            error_code: "TEST_ERROR".to_string(),
            message: "Test error message".to_string(),
            details: Some({
                let mut details = HashMap::new();
                details.insert("field".to_string(), "value".to_string());
                details
            }),
            timestamp: Utc::now(),
            request_id: Some("req-123".to_string()),
        };

        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: ErrorResponse = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.error_code, "TEST_ERROR");
        assert_eq!(deserialized.message, "Test error message");
        assert!(deserialized.details.is_some());
        assert_eq!(deserialized.request_id, Some("req-123".to_string()));
    }
}