use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::traits::*;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid;

/// Authentication middleware
pub struct AuthenticationMiddleware {
    /// Authentication service
    auth_service: Arc<dyn AuthService>,
    /// Required permissions for different service types
    required_permissions: Arc<RwLock<HashMap<ServiceType, Vec<String>>>>,
}

impl AuthenticationMiddleware {
    /// Create a new authentication middleware
    pub fn new(auth_service: Arc<dyn AuthService>) -> Self {
        Self {
            auth_service,
            required_permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set required permissions for a service type
    pub async fn set_required_permissions(&self, service_type: ServiceType, permissions: Vec<String>) {
        let mut perms = self.required_permissions.write().await;
        perms.insert(service_type, permissions);
    }
}

#[async_trait]
impl ServiceMiddleware for AuthenticationMiddleware {
    async fn process_request(&self, mut request: ServiceRequest) -> ServiceResult<ServiceRequest> {
        // Extract auth token from metadata
        let auth_token = request.metadata.auth_token.as_ref().ok_or_else(|| {
            ServiceError::access_denied("Missing authentication token")
        })?;

        // Validate the token
        let user_info = self.auth_service.validate_token(auth_token).await.map_err(|e| {
            ServiceError::access_denied(format!("Invalid authentication token: {}", e))
        })?;

        // Check user permissions for the service type
        let required_permissions = self.required_permissions.read().await;
        if let Some(permissions) = required_permissions.get(&request.service_type) {
            for permission in permissions {
                if !self.auth_service.check_permission(&user_info.user_id, permission).await.unwrap_or(false) {
                    return Err(ServiceError::access_denied(format!(
                        "Insufficient permissions: {} required",
                        permission
                    )));
                }
            }
        }

        // Add user information to request metadata
        request.metadata.user_id = Some(user_info.user_id);

        Ok(request)
    }

    async fn process_response(&self, response: ServiceResponse) -> ServiceResult<ServiceResponse> {
        Ok(response)
    }

    async fn handle_error(&self, error: ServiceError, _request: &ServiceRequest) -> ServiceResult<ServiceResponse> {
        // Don't override authentication errors
        if matches!(error, ServiceError::AccessDenied { .. }) {
            return Err(error);
        }

        // For other errors, return a generic error response
        let response = ServiceResponse {
            request_id: uuid::Uuid::new_v4(), // This should come from request
            status: ResponseStatus::Error,
            payload: serde_json::json!({
                "error": error.to_string(),
                "code": "INTERNAL_ERROR"
            }),
            metadata: ResponseMetadata {
                timestamp: chrono::Utc::now(),
                duration_ms: 0,
                service_id: uuid::Uuid::new_v4(),
                metadata: HashMap::new(),
            },
        };

        Ok(response)
    }
}

/// Rate limiting middleware
pub struct RateLimitMiddleware {
    /// Rate limiter service
    rate_limiter: Arc<dyn ServiceRateLimiter>,
    /// Rate limits by service type
    rate_limits: Arc<RwLock<HashMap<ServiceType, RateLimitConfig>>>,
}

impl RateLimitMiddleware {
    /// Create a new rate limiting middleware
    pub fn new(rate_limiter: Arc<dyn ServiceRateLimiter>) -> Self {
        Self {
            rate_limiter,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set rate limits for a service type
    pub async fn set_rate_limits(&self, service_type: ServiceType, config: RateLimitConfig) {
        let mut limits = self.rate_limits.write().await;
        limits.insert(service_type, config);
    }
}

#[async_trait]
impl ServiceMiddleware for RateLimitMiddleware {
    async fn process_request(&self, request: ServiceRequest) -> ServiceResult<ServiceRequest> {
        // Get rate limit configuration for this service type
        let rate_limits = self.rate_limits.read().await;
        let config = rate_limits.get(&request.service_type).cloned().unwrap_or_default();

        // Determine rate limit key (user-based or IP-based)
        let key = request.metadata.user_id.clone()
            .or_else(|| request.metadata.client_id.clone())
            .unwrap_or_else(|| "anonymous".to_string());

        // Check rate limit
        let allowed = self.rate_limiter.is_allowed(&key, config.requests_per_minute, 60).await?;
        if !allowed {
            return Err(ServiceError::rate_limit_exceeded(format!(
                "Rate limit exceeded: {} requests per minute",
                config.requests_per_minute
            )));
        }

        Ok(request)
    }

    async fn process_response(&self, mut response: ServiceResponse) -> ServiceResult<ServiceResponse> {
        // Add rate limit headers to response metadata
        response.metadata.metadata.insert(
            "X-RateLimit-Limit".to_string(),
            "60".to_string(), // This should come from the actual config
        );
        response.metadata.metadata.insert(
            "X-RateLimit-Remaining".to_string(),
            "59".to_string(), // This should come from the actual rate limiter
        );

        Ok(response)
    }

    async fn handle_error(&self, error: ServiceError, _request: &ServiceRequest) -> ServiceResult<ServiceResponse> {
        if matches!(error, ServiceError::RateLimitExceeded { .. }) {
            let response = ServiceResponse {
                request_id: uuid::Uuid::new_v4(),
                status: ResponseStatus::Error,
                payload: serde_json::json!({
                    "error": error.to_string(),
                    "code": "RATE_LIMIT_EXCEEDED"
                }),
                metadata: ResponseMetadata {
                    timestamp: chrono::Utc::now(),
                    duration_ms: 0,
                    service_id: uuid::Uuid::new_v4(),
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("Retry-After".to_string(), "60".to_string());
                        meta
                    },
                },
            };
            return Ok(response);
        }

        Err(error)
    }
}

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per minute
    pub requests_per_minute: u32,
    /// Requests per hour
    pub requests_per_hour: Option<u32>,
    /// Requests per day
    pub requests_per_day: Option<u32>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 100,
            requests_per_hour: None,
            requests_per_day: None,
        }
    }
}

/// Logging middleware
pub struct LoggingMiddleware {
    /// Logger configuration
    config: LoggingConfig,
}

impl LoggingMiddleware {
    /// Create a new logging middleware
    pub fn new(config: LoggingConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ServiceMiddleware for LoggingMiddleware {
    async fn process_request(&self, request: ServiceRequest) -> ServiceResult<ServiceRequest> {
        if self.config.log_requests {
            tracing::info!(
                request_id = %request.request_id,
                service_type = ?request.service_type,
                method = %request.method,
                user_id = ?request.metadata.user_id,
                "Processing service request"
            );
        }

        Ok(request)
    }

    async fn process_response(&self, response: ServiceResponse) -> ServiceResult<ServiceResponse> {
        if self.config.log_responses {
            tracing::info!(
                request_id = %response.request_id,
                status = ?response.status,
                duration_ms = response.metadata.duration_ms,
                "Service request completed"
            );
        }

        Ok(response)
    }

    async fn handle_error(&self, error: ServiceError, request: &ServiceRequest) -> ServiceResult<ServiceResponse> {
        if self.config.log_errors {
            tracing::error!(
                request_id = %request.request_id,
                service_type = ?request.service_type,
                method = %request.method,
                error = %error,
                "Service request failed"
            );
        }

        Err(error)
    }
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Whether to log incoming requests
    pub log_requests: bool,
    /// Whether to log outgoing responses
    pub log_responses: bool,
    /// Whether to log errors
    pub log_errors: bool,
    /// Whether to log request/response payloads
    pub log_payloads: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_requests: true,
            log_responses: true,
            log_errors: true,
            log_payloads: false,
        }
    }
}

/// Metrics collection middleware
pub struct MetricsMiddleware {
    /// Metrics collector
    metrics_collector: Arc<dyn MetricsCollector>,
    /// Service performance metrics
    performance_metrics: Arc<RwLock<HashMap<ServiceType, ServicePerformanceMetrics>>>,
}

impl MetricsMiddleware {
    /// Create a new metrics middleware
    pub fn new(metrics_collector: Arc<dyn MetricsCollector>) -> Self {
        Self {
            metrics_collector,
            performance_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ServiceMiddleware for MetricsMiddleware {
    async fn process_request(&self, request: ServiceRequest) -> ServiceResult<ServiceRequest> {
        // Record request start time
        let start_time = std::time::Instant::now();

        // Add start time to request metadata for later use
        let mut modified_request = request;
        modified_request.metadata.context.insert(
            "request_start_time".to_string(),
            start_time.elapsed().as_millis().to_string(),
        );

        Ok(modified_request)
    }

    async fn process_response(&self, response: ServiceResponse) -> ServiceResult<ServiceResponse> {
        // Calculate request duration
        let duration_ms = response.metadata.duration_ms;

        // Update performance metrics
        let mut metrics = self.performance_metrics.write().await;
        let service_metrics = metrics.entry(response.metadata.service_id.into()).or_default();
        service_metrics.total_requests += 1;
        service_metrics.total_duration_ms += duration_ms as u64;
        service_metrics.min_duration_ms = service_metrics.min_duration_ms.min(duration_ms as u64);
        service_metrics.max_duration_ms = service_metrics.max_duration_ms.max(duration_ms as u64);

        // Record metrics
        self.metrics_collector.record_request_duration(
            &response.metadata.service_id.to_string(),
            duration_ms,
        ).await;

        Ok(response)
    }

    async fn handle_error(&self, error: ServiceError, request: &ServiceRequest) -> ServiceResult<ServiceResponse> {
        // Record error metrics
        self.metrics_collector.record_error(
            &request.service_type.to_string(),
            &error.to_string(),
        ).await;

        Err(error)
    }
}

/// Service performance metrics
#[derive(Debug, Clone, Default)]
pub struct ServicePerformanceMetrics {
    /// Total requests
    pub total_requests: u64,
    /// Total duration in milliseconds
    pub total_duration_ms: u64,
    /// Minimum duration in milliseconds
    pub min_duration_ms: u64,
    /// Maximum duration in milliseconds
    pub max_duration_ms: u64,
}

impl ServicePerformanceMetrics {
    /// Get average duration in milliseconds
    pub fn avg_duration_ms(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.total_requests as f64
        }
    }
}

/// Metrics collector trait
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    /// Record request duration
    async fn record_request_duration(&self, service_id: &str, duration_ms: u64);

    /// Record error
    async fn record_error(&self, service_type: &str, error: &str);

    /// Get service metrics
    async fn get_service_metrics(&self, service_id: &str) -> Option<ServicePerformanceMetrics>;
}

/// Caching middleware
pub struct CachingMiddleware {
    /// Cache service
    cache: Arc<dyn ServiceCache>,
    /// Cache configuration by service type
    cache_configs: Arc<RwLock<HashMap<ServiceType, CacheConfig>>>,
}

impl CachingMiddleware {
    /// Create a new caching middleware
    pub fn new(cache: Arc<dyn ServiceCache>) -> Self {
        Self {
            cache,
            cache_configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set cache configuration for a service type
    pub async fn set_cache_config(&self, service_type: ServiceType, config: CacheConfig) {
        let mut configs = self.cache_configs.write().await;
        configs.insert(service_type, config);
    }

    /// Generate cache key for a request
    fn generate_cache_key(&self, request: &ServiceRequest) -> String {
        format!(
            "{}:{}:{}:{}",
            request.service_type.as_str(),
            request.method,
            serde_json::to_string(&request.payload).unwrap_or_default(),
            serde_json::to_string(&request.metadata).unwrap_or_default()
        )
    }
}

#[async_trait]
impl ServiceMiddleware for CachingMiddleware {
    async fn process_request(&self, request: ServiceRequest) -> ServiceResult<ServiceRequest> {
        // Check if caching is enabled for this service type
        let cache_configs = self.cache_configs.read().await;
        if !cache_configs.contains_key(&request.service_type) {
            return Ok(request);
        }

        // Generate cache key
        let cache_key = self.generate_cache_key(&request);

        // Try to get cached response
        if let Some(cached_response) = self.cache.get(&cache_key).await? {
            let response: ServiceResponse = serde_json::from_value(cached_response)
                .map_err(|e| ServiceError::serialization_error(e.to_string()))?;

            tracing::debug!(
                request_id = %request.request_id,
                cache_key = %cache_key,
                "Cache hit"
            );

            // Return cached response as an error to short-circuit the request
            return Err(ServiceError::internal_error(format!(
                "Cache hit: {}",
                serde_json::to_string(&response).unwrap_or_default()
            )));
        }

        Ok(request)
    }

    async fn process_response(&self, mut response: ServiceResponse) -> ServiceResult<ServiceResponse> {
        // Note: In a real implementation, we'd need access to the original request
        // to generate the cache key. This is a simplified version.

        // For now, we'll add cache metadata to the response
        response.metadata.metadata.insert(
            "X-Cache".to_string(),
            "MISS".to_string(),
        );

        Ok(response)
    }

    async fn handle_error(&self, error: ServiceError, request: &ServiceRequest) -> ServiceResult<ServiceResponse> {
        // Check if this is a cache hit error
        if let ServiceError::InternalError { message } = &error {
            if message.starts_with("Cache hit: ") {
                // Extract the cached response from the error message
                let response_json = message.strip_prefix("Cache hit: ").unwrap_or("{}");
                if let Ok(response) = serde_json::from_str::<ServiceResponse>(response_json) {
                    return Ok(response);
                }
            }
        }

        Err(error)
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// TTL in seconds
    pub ttl_seconds: u32,
    /// Maximum cache size
    pub max_size: Option<usize>,
    /// Whether to cache error responses
    pub cache_errors: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: 300, // 5 minutes
            max_size: None,
            cache_errors: false,
        }
    }
}

/// Mock authentication service for testing
pub struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
    async fn validate_token(&self, token: &str) -> ServiceResult<AuthUserInfo> {
        if token == "valid_token" {
            Ok(AuthUserInfo {
                user_id: "test_user".to_string(),
                username: "testuser".to_string(),
                roles: vec!["user".to_string()],
                permissions: vec!["read".to_string(), "write".to_string()],
            })
        } else {
            Err(ServiceError::access_denied("Invalid token"))
        }
    }

    async fn check_permission(&self, user_id: &str, permission: &str) -> ServiceResult<bool> {
        Ok(user_id == "test_user" && (permission == "read" || permission == "write"))
    }
}

/// Auth user information
#[derive(Debug, Clone)]
pub struct AuthUserInfo {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// Authentication service trait
#[async_trait]
pub trait AuthService: Send + Sync {
    /// Validate authentication token
    async fn validate_token(&self, token: &str) -> ServiceResult<AuthUserInfo>;

    /// Check user permission
    async fn check_permission(&self, user_id: &str, permission: &str) -> ServiceResult<bool>;
}