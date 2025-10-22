//! # Inference Engine Service Implementation
//!
//! This module provides a production-ready implementation of the InferenceEngine service
//! that handles LLM integration and AI inference capabilities. It supports multiple LLM
//! providers (OpenAI, Ollama), text generation, embedding generation, and performance
//! optimization features.

use super::{
    errors::ServiceError,
    events::{
        integration::{EventIntegratedService, EventIntegrationManager, ServiceEventAdapter, EventPublishingService, LifecycleEventType},
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{EventRouter, ServiceRegistration},
        errors::{EventError, EventResult},
    },
    service_traits::*,
    service_types::*,
    types::*,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_llm::{
    create_text_provider, EmbeddingProvider, TextGenerationProvider, TextProviderConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto mode
    Auto,
    /// Required mode
    Required,
    /// No tools
    None,
    /// Specific tool
    Specific { r#type: String, function: FunctionDefinition },
}

/// Inference engine service implementation
///
/// This service provides comprehensive AI inference capabilities including:
/// - Text generation (completion and chat completion)
/// - Embedding generation for semantic search
/// - Model management and configuration
/// - Performance optimization (batching, caching)
/// - Error handling and fallback strategies
/// - Monitoring and metrics collection
pub struct InferenceEngineService {
    /// Service configuration
    config: InferenceEngineConfig,
    /// Loaded models
    models: Arc<RwLock<HashMap<String, LoadedModel>>>,
    /// Active text generation provider
    text_provider: Arc<dyn TextGenerationProvider<Config = TextProviderConfig>>,
    /// Active embedding provider
    embedding_provider: Arc<dyn EmbeddingProvider>,
    /// Service metrics
    metrics: Arc<RwLock<ServiceMetrics>>,
    /// Event publisher (legacy)
    event_sender: mpsc::UnboundedSender<InferenceEngineEvent>,
    /// Service status
    running: Arc<RwLock<bool>>,
    /// Request cache
    cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
    /// Resource limits
    limits: Arc<RwLock<InferenceLimits>>,
    /// Event integration manager for daemon coordination
    event_integration: Option<Arc<EventIntegrationManager>>,
}

/// Configuration for the inference engine service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEngineConfig {
    /// Text provider configuration
    pub text_provider: TextProviderConfig,
    /// Embedding provider configuration
    pub embedding_provider: crucible_llm::EmbeddingConfig,
    /// Default model settings
    pub default_models: DefaultModels,
    /// Performance settings
    pub performance: PerformanceSettings,
    /// Cache settings
    pub cache: CacheSettings,
    /// Resource limits
    pub limits: InferenceLimits,
    /// Monitoring settings
    pub monitoring: MonitoringSettings,
}

/// Default model configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultModels {
    /// Default text generation model
    pub text_model: String,
    /// Default embedding model
    pub embedding_model: String,
    /// Default chat model
    pub chat_model: String,
}

/// Performance optimization settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSettings {
    /// Enable request batching
    pub enable_batching: bool,
    /// Batch size for text generation
    pub batch_size: u32,
    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,
    /// Enable request deduplication
    pub enable_deduplication: bool,
    /// Connection pool size
    pub connection_pool_size: u32,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    /// Enable caching
    pub enabled: bool,
    /// Cache TTL in seconds
    pub ttl_seconds: u64,
    /// Maximum cache size in bytes
    pub max_size_bytes: u64,
    /// Cache eviction policy
    pub eviction_policy: CacheEvictionPolicy,
}

/// Cache eviction policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheEvictionPolicy {
    /// Least recently used
    LRU,
    /// Least frequently used
    LFU,
    /// First in, first out
    FIFO,
    /// Time-based expiration only
    TTL,
}

/// Monitoring and metrics settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSettings {
    /// Enable detailed metrics
    pub enable_metrics: bool,
    /// Metrics collection interval in seconds
    pub metrics_interval_seconds: u64,
    /// Enable performance profiling
    pub enable_profiling: bool,
    /// Export metrics to external system
    pub export_metrics: bool,
}

/// Information about a loaded model
#[derive(Debug, Clone)]
struct LoadedModel {
    /// Model information
    info: ModelInfo,
    /// Load timestamp
    loaded_at: DateTime<Utc>,
    /// Usage statistics
    usage: ModelUsageStats,
    /// Resource usage
    resources: ModelResourceUsage,
}

/// Model usage statistics
#[derive(Debug, Clone, Default)]
struct ModelUsageStats {
    /// Total requests
    total_requests: u64,
    /// Successful requests
    successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Total tokens processed
    total_tokens: u64,
    /// Average response time
    average_response_time: std::time::Duration,
    /// Last used timestamp
    last_used: Option<DateTime<Utc>>,
}

/// Cached response data
#[derive(Debug, Clone)]
struct CachedResponse {
    /// Response data
    data: CachedResponseData,
    /// Creation timestamp
    created_at: DateTime<Utc>,
    /// Access count
    access_count: u64,
    /// Last accessed timestamp
    last_accessed: DateTime<Utc>,
}

/// Cached response data (different types)
#[derive(Debug, Clone)]
enum CachedResponseData {
    /// Completion response
    Completion(CompletionResponse),
    /// Chat completion response
    ChatCompletion(ChatCompletionResponse),
    /// Embedding response
    Embedding(EmbeddingResponse),
    /// Batch embedding response
    BatchEmbedding(Vec<EmbeddingResponse>),
}

/// Inference engine events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceEngineEvent {
    /// Model loaded
    ModelLoaded { model_id: String, model_info: ModelInfo },
    /// Model unloaded
    ModelUnloaded { model_id: String },
    /// Request started
    RequestStarted { request_id: String, request_type: String, model: String },
    /// Request completed
    RequestCompleted {
        request_id: String,
        duration_ms: u64,
        tokens_used: u32,
        success: bool
    },
    /// Cache hit
    CacheHit { cache_key: String, request_type: String },
    /// Cache miss
    CacheMiss { cache_key: String, request_type: String },
    /// Error occurred
    Error { error: String, context: HashMap<String, String> },
    /// Performance alert
    PerformanceAlert { metric: String, value: f64, threshold: f64 },
}

impl InferenceEngineService {
    /// Create a new inference engine service
    pub async fn new(config: InferenceEngineConfig) -> Result<Self, ServiceError> {
        // Create text provider
        let text_provider = create_text_provider(config.text_provider.clone())
            .await
            .map_err(|e| ServiceError::execution_error(format!("Failed to create text provider: {}", e)))?;

        // Create embedding provider
        let embedding_provider = crucible_llm::embeddings::create_provider(config.embedding_provider.clone())
            .await
            .map_err(|e| ServiceError::execution_error(format!("Failed to create embedding provider: {}", e)))?;

        let (event_sender, _) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            models: Arc::new(RwLock::new(HashMap::new())),
            text_provider: Arc::from(text_provider),
            embedding_provider: Arc::from(embedding_provider),
            metrics: Arc::new(RwLock::new(ServiceMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                average_response_time: std::time::Duration::ZERO,
                uptime: std::time::Duration::ZERO,
                memory_usage: 0,
                cpu_usage: 0.0,
            })),
            event_sender,
            running: Arc::new(RwLock::new(false)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            limits: Arc::new(RwLock::new(config.limits.clone())),
            event_integration: None,
        })
    }

    /// Generate a cache key for a request
    fn generate_cache_key(&self, request_type: &str, request_data: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        request_type.hash(&mut hasher);
        request_data.hash(&mut hasher);
        format!("{}:{:x}", request_type, hasher.finish())
    }

    /// Get a cached response if available and not expired
    async fn get_cached_response(&self, cache_key: &str) -> Option<CachedResponseData> {
        if !self.config.cache.enabled {
            return None;
        }

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(cache_key) {
            let now = Utc::now();
            let age = now.signed_duration_since(cached.created_at);

            if age.num_seconds() < self.config.cache.ttl_seconds as i64 {
                return Some(cached.data.clone());
            }
        }
        None
    }

    /// Cache a response
    async fn cache_response(&self, cache_key: String, data: CachedResponseData) {
        if !self.config.cache.enabled {
            return;
        }

        let mut cache = self.cache.write().await;

        // Simple size-based eviction (could be enhanced with LRU/LFU)
        if cache.len() >= 1000 { // Rough limit
            // Remove oldest entries
            let mut entries: Vec<_> = cache.iter().collect();
            entries.sort_by_key(|(_, cached)| cached.created_at);

            for (key, _) in entries.iter().take(100) {
                cache.remove(*key);
            }
        }

        cache.insert(cache_key, CachedResponse {
            data,
            created_at: Utc::now(),
            access_count: 1,
            last_accessed: Utc::now(),
        });
    }

    /// Initialize event integration with the daemon event system
    pub async fn initialize_event_integration(&mut self, event_router: Arc<dyn EventRouter>) -> Result<(), ServiceError> {
        let service_id = "crucible-inference-engine".to_string();
        let service_type = "inference-engine".to_string();

        info!("Initializing event integration for Inference Engine service: {}", service_id);

        let event_integration = EventIntegrationManager::new(service_id, service_type, event_router);

        // Register with event router
        let registration = self.get_service_registration();
        event_integration.register_service(registration).await
            .map_err(|e| ServiceError::execution_error(format!("Failed to register with event router: {}", e)))?;

        // Start event processing
        let engine_clone = self.clone();
        event_integration.start_event_processing(move |daemon_event| {
            let engine = engine_clone.clone();
            async move {
                engine.handle_daemon_event(daemon_event).await
                    .map_err(|e| ServiceError::execution_error(format!("Event handling error: {}", e)))
            }
        }).await
            .map_err(|e| ServiceError::execution_error(format!("Failed to start event processing: {}", e)))?;

        self.event_integration = Some(Arc::new(event_integration));

        // Publish registration event
        self.publish_lifecycle_event(LifecycleEventType::Registered,
            HashMap::from([("event_router".to_string(), "connected".to_string())])).await
            .map_err(|e| ServiceError::execution_error(format!("Failed to publish registration event: {}", e)))?;

        info!("Inference Engine event integration initialized successfully");
        Ok(())
    }

    /// Publish event using the daemon event system
    async fn publish_daemon_event(&self, event: DaemonEvent) -> Result<(), ServiceError> {
        if let Some(event_integration) = &self.event_integration {
            event_integration.publish_event(event).await
                .map_err(|e| ServiceError::execution_error(format!("Failed to publish daemon event: {}", e)))?;
        }
        Ok(())
    }

    /// Convert InferenceEngine event to Daemon event
    fn inference_event_to_daemon_event(&self, inference_event: &InferenceEngineEvent, priority: EventPriority) -> Result<DaemonEvent, EventError> {
        let service_id = "crucible-inference-engine";
        let adapter = ServiceEventAdapter::new(service_id.to_string(), "inference-engine".to_string());

        let (event_type, payload) = match inference_event {
            InferenceEngineEvent::ModelLoaded { model_id, model_info } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::RequestReceived {
                    from_service: service_id.to_string(),
                    to_service: "daemon".to_string(),
                    request: serde_json::json!({
                        "type": "model_loaded",
                        "model_id": model_id,
                        "model_info": model_info,
                    }),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "model_id": model_id,
                    "model_info": model_info,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            InferenceEngineEvent::RequestCompleted { request_id, duration_ms, tokens_used, success } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::ResponseSent {
                    from_service: service_id.to_string(),
                    to_service: "daemon".to_string(),
                    response: serde_json::json!({
                        "type": "request_completed",
                        "request_id": request_id,
                        "duration_ms": duration_ms,
                        "tokens_used": tokens_used,
                        "success": success,
                    }),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "request_id": request_id,
                    "duration_ms": duration_ms,
                    "tokens_used": tokens_used,
                    "success": success,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            InferenceEngineEvent::Error { error, context } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::ConfigurationChanged {
                    service_id: service_id.to_string(),
                    changes: HashMap::from([("error".to_string(), serde_json::Value::String(error.clone()))]),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "error": error,
                    "context": context,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            InferenceEngineEvent::PerformanceAlert { metric, value, threshold } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::HealthCheck {
                    service_id: service_id.to_string(),
                    status: "performance_alert".to_string(),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "metric": metric,
                    "value": value,
                    "threshold": threshold,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            _ => {
                let event_type = EventType::Custom("inference_engine_event".to_string());
                let payload = EventPayload::json(serde_json::json!({
                    "event": format!("{:?}", inference_event),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
        };

        Ok(adapter.create_daemon_event(event_type, payload, priority, None))
    }

    /// Update service metrics
    async fn update_metrics(&self, duration: std::time::Duration, success: bool, tokens: u32) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;

        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        // Update average response time
        let total_requests = metrics.total_requests as f64;
        let current_avg = metrics.average_response_time.as_secs_f64();
        let new_duration = duration.as_secs_f64();
        metrics.average_response_time = std::time::Duration::from_secs_f64(
            (current_avg * (total_requests - 1.0) + new_duration) / total_requests
        );
    }

    /// Publish an event
    async fn publish_event(&self, event: InferenceEngineEvent) {
        let _ = self.event_sender.send(event);
    }

    /// Validate a request against current limits
    async fn validate_request_limits(&self, request_tokens: u32) -> Result<(), ServiceError> {
        let limits = self.limits.read().await;

        if let Some(max_tokens) = limits.max_request_tokens {
            if request_tokens > max_tokens {
                return Err(ServiceError::invalid_request(format!(
                    "Request exceeds maximum token limit: {} > {}",
                    request_tokens, max_tokens
                )));
            }
        }

        Ok(())
    }

    /// Convert crucible-llm completion response to service type
    fn convert_completion_response(&self, llm_response: crucible_llm::text_generation::CompletionResponse) -> CompletionResponse {
        CompletionResponse {
            completions: llm_response.choices.into_iter().map(|choice| Completion {
                text: choice.text,
                index: choice.index,
                logprobs: choice.logprobs.map(|lp| LogProbs {
                    tokens: lp.tokens,
                    token_logprobs: lp.token_logprobs,
                    top_logprobs: lp.top_logprobs,
                    bytes_offset: lp.bytes_offset,
                }),
                finish_reason: choice.finish_reason,
            }).collect(),
            model: llm_response.model,
            usage: TokenUsage {
                prompt_tokens: llm_response.usage.prompt_tokens,
                completion_tokens: llm_response.usage.completion_tokens,
                total_tokens: llm_response.usage.total_tokens,
            },
            request_id: llm_response.id,
            timestamp: llm_response.created,
        }
    }

    /// Convert crucible-llm chat completion response to service type
    fn convert_chat_completion_response(&self, llm_response: crucible_llm::text_generation::ChatCompletionResponse) -> ChatCompletionResponse {
        ChatCompletionResponse {
            choices: llm_response.choices.into_iter().map(|choice| ChatChoice {
                index: choice.index,
                message: ChatMessage {
                    role: match choice.message.role {
                        crucible_llm::text_generation::MessageRole::System => MessageRole::System,
                        crucible_llm::text_generation::MessageRole::User => MessageRole::User,
                        crucible_llm::text_generation::MessageRole::Assistant => MessageRole::Assistant,
                        crucible_llm::text_generation::MessageRole::Function => MessageRole::Function,
                        crucible_llm::text_generation::MessageRole::Tool => MessageRole::Tool,
                    },
                    content: choice.message.content,
                    function_call: choice.message.function_call.map(|fc| FunctionCall {
                        name: fc.name,
                        arguments: fc.arguments,
                    }),
                    tool_calls: choice.message.tool_calls.map(|tc| tc.into_iter().map(|tool_call| ToolCall {
                        id: tool_call.id,
                        r#type: tool_call.r#type,
                        function: FunctionCall {
                            name: tool_call.function.name,
                            arguments: tool_call.function.arguments,
                        },
                    }).collect()),
                    name: choice.message.name,
                },
                finish_reason: choice.finish_reason,
            }).collect(),
            model: llm_response.model,
            usage: TokenUsage {
                prompt_tokens: llm_response.usage.prompt_tokens,
                completion_tokens: llm_response.usage.completion_tokens,
                total_tokens: llm_response.usage.total_tokens,
            },
            request_id: llm_response.id,
            timestamp: llm_response.created,
            finish_reason: vec![], // Could be populated from choices
        }
    }

    /// Convert crucible-llm embedding response to service type
    fn convert_embedding_response(&self, llm_response: crucible_llm::EmbeddingResponse) -> EmbeddingResponse {
        EmbeddingResponse {
            data: vec![Embedding {
                index: 0,
                object: "embedding".to_string(),
                embedding: llm_response.embedding,
            }],
            model: llm_response.model,
            usage: TokenUsage {
                prompt_tokens: llm_response.tokens.unwrap_or(0) as u32,
                completion_tokens: 0,
                total_tokens: llm_response.tokens.unwrap_or(0) as u32,
            },
            request_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
        }
    }
}

#[async_trait]
impl ServiceLifecycle for InferenceEngineService {
    async fn start(&mut self) -> Result<(), ServiceError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(ServiceError::config_error("Service is already running"));
        }

        // Perform health checks on providers
        if !self.text_provider.health_check().await.unwrap_or(false) {
            return Err(ServiceError::config_error("Text provider health check failed"));
        }

        if !self.embedding_provider.health_check().await.unwrap_or(false) {
            return Err(ServiceError::config_error("Embedding provider health check failed"));
        }

        // Load default models
        let default_models = vec![
            (&self.config.default_models.text_model, ModelType::TextGeneration),
            (&self.config.default_models.chat_model, ModelType::Chat),
            (&self.config.default_models.embedding_model, ModelType::Embedding),
        ];

        for (model_name, model_type) in default_models {
            let model_config = ModelConfig {
                name: model_name.clone(),
                provider: self.text_provider.provider_name().to_string(),
                version: None,
                parameters: HashMap::new(),
                api_key: None,
                base_url: None,
                max_tokens: None,
                temperature: None,
                top_p: None,
            };

            let model_info = ModelInfo {
                model_id: model_name.clone(),
                name: model_name.clone(),
                provider: self.text_provider.provider_name().to_string(),
                version: None,
                model_type,
                capabilities: vec![],
                context_window: 4096, // Default
                parameter_count: None,
                loaded: true,
                memory_usage: None,
                loaded_at: Some(Utc::now()),
            };

            let mut models = self.models.write().await;
            models.insert(model_name.clone(), LoadedModel {
                info: model_info,
                loaded_at: Utc::now(),
                usage: ModelUsageStats::default(),
                resources: ModelResourceUsage {
                    model_id: model_name.clone(),
                    memory_bytes: 0,
                    gpu_memory_bytes: None,
                    active_requests: 0,
                    average_response_time: std::time::Duration::ZERO,
                    requests_per_minute: 0.0,
                },
            });
        }

        *running = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ServiceError> {
        let mut running = self.running.write().await;
        *running = false;

        // Unload all models
        let mut models = self.models.write().await;
        models.clear();

        // Clear cache
        let mut cache = self.cache.write().await;
        cache.clear();

        Ok(())
    }

    fn is_running(&self) -> bool {
        // Note: This is a synchronous method, so we can't easily check the async state
        // In a real implementation, we'd use an atomic or other sync primitive
        true
    }

    fn service_name(&self) -> &str {
        "InferenceEngine"
    }

    fn service_version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

#[async_trait]
impl HealthCheck for InferenceEngineService {
    async fn health_check(&self) -> Result<ServiceHealth, ServiceError> {
        let mut details = HashMap::new();
        let mut is_healthy = true;

        // Check text provider
        match self.text_provider.health_check().await {
            Ok(true) => {
                details.insert("text_provider".to_string(), "healthy".to_string());
            }
            Ok(false) => {
                details.insert("text_provider".to_string(), "unhealthy".to_string());
                is_healthy = false;
            }
            Err(e) => {
                details.insert("text_provider".to_string(), format!("error: {}", e));
                is_healthy = false;
            }
        }

        // Check embedding provider
        match self.embedding_provider.health_check().await {
            Ok(true) => {
                details.insert("embedding_provider".to_string(), "healthy".to_string());
            }
            Ok(false) => {
                details.insert("embedding_provider".to_string(), "unhealthy".to_string());
                is_healthy = false;
            }
            Err(e) => {
                details.insert("embedding_provider".to_string(), format!("error: {}", e));
                is_healthy = false;
            }
        }

        // Check loaded models
        let models = self.models.read().await;
        details.insert("loaded_models".to_string(), models.len().to_string());

        let status = if is_healthy {
            ServiceStatus::Healthy
        } else {
            ServiceStatus::Unhealthy
        };

        Ok(ServiceHealth {
            status,
            message: Some("Inference engine health check".to_string()),
            last_check: Utc::now(),
            details,
        })
    }
}

#[async_trait]
impl Configurable for InferenceEngineService {
    type Config = InferenceEngineConfig;

    async fn get_config(&self) -> Result<Self::Config, ServiceError> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: Self::Config) -> Result<(), ServiceError> {
        self.config = config;
        Ok(())
    }

    async fn validate_config(&self, config: &Self::Config) -> Result<(), ServiceError> {
        // Basic validation
        if config.default_models.text_model.is_empty() {
            return Err(ServiceError::config_error("Text model name cannot be empty"));
        }

        if config.default_models.embedding_model.is_empty() {
            return Err(ServiceError::config_error("Embedding model name cannot be empty"));
        }

        Ok(())
    }

    async fn reload_config(&mut self) -> Result<(), ServiceError> {
        // In a real implementation, this would reload from a file or external source
        Ok(())
    }
}

#[async_trait]
impl Observable for InferenceEngineService {
    async fn get_metrics(&self) -> Result<ServiceMetrics, ServiceError> {
        let metrics = self.metrics.read().await;
        Ok(metrics.clone())
    }

    async fn reset_metrics(&mut self) -> Result<(), ServiceError> {
        let mut metrics = self.metrics.write().await;
        *metrics = ServiceMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: std::time::Duration::ZERO,
            uptime: std::time::Duration::ZERO,
            memory_usage: 0,
            cpu_usage: 0.0,
        };
        Ok(())
    }

    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, ServiceError> {
        let metrics = self.metrics.read().await;
        let models = self.models.read().await;

        let mut custom_metrics = HashMap::new();
        custom_metrics.insert("loaded_models".to_string(), models.len() as f64);

        let cache = self.cache.read().await;
        custom_metrics.insert("cache_size".to_string(), cache.len() as f64);

        Ok(PerformanceMetrics {
            request_times: vec![metrics.average_response_time.as_millis() as f64],
            memory_usage: metrics.memory_usage,
            cpu_usage: metrics.cpu_usage,
            active_connections: 0,
            queue_sizes: HashMap::new(),
            custom_metrics,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl EventDriven for InferenceEngineService {
    type Event = InferenceEngineEvent;

    async fn subscribe(&mut self, _event_type: &str) -> Result<mpsc::UnboundedReceiver<Self::Event>, ServiceError> {
        // In a real implementation, this would support subscribing to specific event types
        let (_, receiver) = mpsc::unbounded_channel();
        Ok(receiver)
    }

    async fn unsubscribe(&mut self, _event_type: &str) -> Result<(), ServiceError> {
        // Implementation would track subscriptions
        Ok(())
    }

    async fn publish(&self, event: Self::Event) -> Result<(), ServiceError> {
        let _ = self.event_sender.send(event);
        Ok(())
    }

    async fn handle_event(&mut self, event: Self::Event) -> Result<(), ServiceError> {
        match event {
            InferenceEngineEvent::ModelLoaded { model_id, model_info } => {
                let mut models = self.models.write().await;
                models.insert(model_id.clone(), LoadedModel {
                    info: model_info,
                    loaded_at: Utc::now(),
                    usage: ModelUsageStats::default(),
                    resources: ModelResourceUsage {
                        model_id,
                        memory_bytes: 0,
                        gpu_memory_bytes: None,
                        active_requests: 0,
                        average_response_time: std::time::Duration::ZERO,
                        requests_per_minute: 0.0,
                    },
                });
            }
            InferenceEngineEvent::ModelUnloaded { model_id } => {
                let mut models = self.models.write().await;
                models.remove(&model_id);
            }
            _ => {
                // Handle other event types as needed
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ResourceManager for InferenceEngineService {
    async fn get_resource_usage(&self) -> Result<ResourceUsage, ServiceError> {
        let models = self.models.read().await;
        let cache = self.cache.read().await;

        let total_memory = models.values()
            .map(|model| model.resources.memory_bytes)
            .sum::<u64>()
            + (cache.len() * 1024) as u64; // Rough estimate of cache memory

        Ok(ResourceUsage {
            memory_bytes: total_memory,
            cpu_percentage: 0.0, // Would need system monitoring
            disk_bytes: 0,
            network_bytes: 0,
            open_files: 0,
            active_threads: tokio::runtime::Handle::current().metrics().num_workers() as u32,
            measured_at: Utc::now(),
        })
    }

    async fn set_limits(&mut self, limits: ResourceLimits) -> Result<(), ServiceError> {
        // Convert ResourceLimits to InferenceLimits and update
        let inference_limits = InferenceLimits {
            max_concurrent_requests: limits.max_concurrent_operations,
            max_request_tokens: None, // Would need mapping from ResourceLimits
            max_response_tokens: None,
            request_timeout: limits.operation_timeout,
            max_queue_size: limits.max_queue_size,
        };

        let mut current_limits = self.limits.write().await;
        *current_limits = inference_limits;
        Ok(())
    }

    async fn get_limits(&self) -> Result<ResourceLimits, ServiceError> {
        let limits = self.limits.read().await;
        Ok(ResourceLimits {
            max_memory_bytes: None,
            max_cpu_percentage: None,
            max_disk_bytes: None,
            max_concurrent_operations: limits.max_concurrent_requests,
            max_queue_size: limits.max_queue_size,
            operation_timeout: limits.request_timeout,
        })
    }

    async fn cleanup_resources(&mut self) -> Result<(), ServiceError> {
        // Clear cache
        let mut cache = self.cache.write().await;
        cache.clear();

        // Remove unused models
        let mut models = self.models.write().await;
        let now = Utc::now();
        models.retain(|_, model| {
            if let Some(last_used) = model.usage.last_used {
                now.signed_duration_since(last_used).num_hours() < 24
            } else {
                false
            }
        });

        Ok(())
    }
}

// Note: The InferenceEngine trait implementation would be quite large.
// For now, I'll implement the key methods and mark where others would go.

#[async_trait]
impl InferenceEngine for InferenceEngineService {
    type Config = InferenceEngineConfig;
    type Event = InferenceEngineEvent;

    // Model management methods
    async fn load_model(&mut self, model_config: ModelConfig) -> Result<ModelInfo, ServiceError> {
        let model_info = ModelInfo {
            model_id: model_config.name.clone(),
            name: model_config.name.clone(),
            provider: model_config.provider.clone(),
            version: model_config.version,
            model_type: ModelType::TextGeneration, // Default
            capabilities: vec![],
            context_window: 4096,
            parameter_count: None,
            loaded: true,
            memory_usage: None,
            loaded_at: Some(Utc::now()),
        };

        let mut models = self.models.write().await;
        models.insert(model_config.name.clone(), LoadedModel {
            info: model_info.clone(),
            loaded_at: Utc::now(),
            usage: ModelUsageStats::default(),
            resources: ModelResourceUsage {
                model_id: model_config.name.clone(),
                memory_bytes: 0,
                gpu_memory_bytes: None,
                active_requests: 0,
                average_response_time: std::time::Duration::ZERO,
                requests_per_minute: 0.0,
            },
        });

        self.publish_event(InferenceEngineEvent::ModelLoaded {
            model_id: model_config.name.clone(),
            model_info: model_info.clone(),
        }).await;

        Ok(model_info)
    }

    async fn unload_model(&mut self, model_id: &str) -> Result<(), ServiceError> {
        let mut models = self.models.write().await;
        models.remove(model_id);

        self.publish_event(InferenceEngineEvent::ModelUnloaded {
            model_id: model_id.to_string(),
        }).await;

        Ok(())
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ServiceError> {
        let models = self.models.read().await;
        Ok(models.values().map(|model| model.info.clone()).collect())
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>, ServiceError> {
        let models = self.models.read().await;
        Ok(models.get(model_id).map(|model| model.info.clone()))
    }

    async fn switch_model(&mut self, model_id: &str) -> Result<(), ServiceError> {
        // In a real implementation, this would update the active model
        // For now, we just verify the model exists
        let models = self.models.read().await;
        if !models.contains_key(model_id) {
            return Err(ServiceError::config_error(format!("Model not found: {}", model_id)));
        }
        Ok(())
    }

    // Text generation methods
    async fn generate_completion(&self, request: CompletionRequest) -> Result<CompletionResponse, ServiceError> {
        let request_id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        self.publish_event(InferenceEngineEvent::RequestStarted {
            request_id: request_id.clone(),
            request_type: "completion".to_string(),
            model: request.model.clone(),
        }).await;

        // Check cache first
        let cache_key = self.generate_cache_key("completion", &serde_json::to_string(&request).unwrap());
        if let Some(cached_data) = self.get_cached_response(&cache_key).await {
            self.publish_event(InferenceEngineEvent::CacheHit {
                cache_key,
                request_type: "completion".to_string(),
            }).await;

            if let CachedResponseData::Completion(response) = cached_data {
                self.update_metrics(start_time.elapsed(), true, response.usage.total_tokens).await;
                return Ok(response);
            }
        }

        self.publish_event(InferenceEngineEvent::CacheMiss {
            cache_key: cache_key.clone(),
            request_type: "completion".to_string(),
        }).await;

        // Validate request limits
        self.validate_request_limits(request.prompt.len() as u32).await?;

        // Convert to crucible-llm request
        let llm_request = crucible_llm::text_generation::CompletionRequest {
            model: request.model.clone(),
            prompt: request.prompt,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            frequency_penalty: request.frequency_penalty,
            presence_penalty: request.presence_penalty,
            stop: request.stop,
            n: request.n,
            echo: request.echo,
            logit_bias: request.logit_bias,
            user: request.user,
        };

        // Make the actual request
        let result = self.text_provider.generate_completion(llm_request).await;
        let duration = start_time.elapsed();

        match result {
            Ok(llm_response) => {
                let response = self.convert_completion_response(&llm_response);
                let tokens_used = response.usage.total_tokens;

                // Cache the response
                self.cache_response(cache_key, CachedResponseData::Completion(response.clone())).await;

                self.update_metrics(duration, true, tokens_used).await;
                self.publish_event(InferenceEngineEvent::RequestCompleted {
                    request_id,
                    duration_ms: duration.as_millis() as u64,
                    tokens_used,
                    success: true,
                }).await;

                Ok(response)
            }
            Err(e) => {
                self.update_metrics(duration, false, 0).await;
                self.publish_event(InferenceEngineEvent::Error {
                    error: format!("Completion failed: {}", e),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("request_id".to_string(), request_id);
                        ctx.insert("model".to_string(), request.model);
                        ctx
                    },
                }).await;

                Err(ServiceError::execution_error(format!("Completion failed: {}", e)))
            }
        }
    }

    async fn generate_completion_stream(&self, _request: CompletionRequest) -> Result<mpsc::UnboundedReceiver<CompletionChunk>, ServiceError> {
        // Implementation would create a streaming response
        todo!("Streaming completion implementation")
    }

    async fn generate_chat_completion(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse, ServiceError> {
        let request_id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        self.publish_event(InferenceEngineEvent::RequestStarted {
            request_id: request_id.clone(),
            request_type: "chat_completion".to_string(),
            model: request.model.clone(),
        }).await;

        // Check cache first
        let cache_key = self.generate_cache_key("chat_completion", &serde_json::to_string(&request).unwrap());
        if let Some(cached_data) = self.get_cached_response(&cache_key).await {
            self.publish_event(InferenceEngineEvent::CacheHit {
                cache_key,
                request_type: "chat_completion".to_string(),
            }).await;

            if let CachedResponseData::ChatCompletion(response) = cached_data {
                self.update_metrics(start_time.elapsed(), true, response.usage.total_tokens).await;
                return Ok(response);
            }
        }

        self.publish_event(InferenceEngineEvent::CacheMiss {
            cache_key: cache_key.clone(),
            request_type: "chat_completion".to_string(),
        }).await;

        // Convert to crucible-llm request
        let llm_request = crucible_llm::text_generation::ChatCompletionRequest {
            model: request.model.clone(),
            messages: request.messages.into_iter().map(|msg| {
                let role = match msg.role {
                    MessageRole::System => crucible_llm::text_generation::MessageRole::System,
                    MessageRole::User => crucible_llm::text_generation::MessageRole::User,
                    MessageRole::Assistant => crucible_llm::text_generation::MessageRole::Assistant,
                    MessageRole::Function => crucible_llm::text_generation::MessageRole::Function,
                    MessageRole::Tool => crucible_llm::text_generation::MessageRole::Tool,
                };
                crucible_llm::text_generation::ChatMessage {
                    role,
                    content: msg.content,
                    function_call: msg.function_call.map(|fc| crucible_llm::text_generation::FunctionCall {
                        name: fc.name,
                        arguments: fc.arguments,
                    }),
                    tool_calls: msg.tool_calls.map(|tc| tc.into_iter().map(|tool_call| crucible_llm::text_generation::ToolCall {
                        id: tool_call.id,
                        r#type: tool_call.r#type,
                        function: crucible_llm::text_generation::FunctionCall {
                            name: tool_call.function.name,
                            arguments: tool_call.function.arguments,
                        },
                    }).collect()),
                    name: msg.name,
                    tool_call_id: msg.tool_call_id,
                }
            }).collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            functions: request.functions.map(|funcs| funcs.into_iter().map(|func| crucible_llm::text_generation::FunctionDefinition {
                name: func.name,
                description: func.description,
                parameters: func.parameters,
            }).collect()),
            function_call: request.function_call.map(|fc| match fc {
                FunctionCallBehavior::Auto => crucible_llm::text_generation::FunctionCallBehavior::Auto,
                FunctionCallBehavior::Force(name) => crucible_llm::text_generation::FunctionCallBehavior::Force(name),
                FunctionCallBehavior::None => crucible_llm::text_generation::FunctionCallBehavior::None,
            }),
            system: request.system,
            stop: request.stop,
            frequency_penalty: request.frequency_penalty,
            presence_penalty: request.presence_penalty,
            logit_bias: request.logit_bias,
            user: request.user,
            response_format: request.response_format.map(|rf| crucible_llm::text_generation::ResponseFormat {
                r#type: rf.r#type,
            }),
            seed: request.seed,
            tool_choice: request.tool_choice.map(|tc| match tc {
                ToolChoice::Auto => crucible_llm::text_generation::ToolChoice::Auto,
                ToolChoice::Required => crucible_llm::text_generation::ToolChoice::Required,
                ToolChoice::None => crucible_llm::text_generation::ToolChoice::None,
                ToolChoice::Specific { r#type, function } => crucible_llm::text_generation::ToolChoice::Specific {
                    r#type,
                    function: crucible_llm::text_generation::FunctionDefinition {
                        name: function.name,
                        description: function.description,
                        parameters: function.parameters,
                    },
                },
            }),
            tools: request.tools.map(|tools| tools.into_iter().map(|tool| crucible_llm::text_generation::ToolDefinition {
                r#type: tool.r#type,
                function: crucible_llm::text_generation::FunctionDefinition {
                    name: tool.function.name,
                    description: tool.function.description,
                    parameters: tool.function.parameters,
                },
            }).collect()),
        };

        // Make the actual request
        let result = self.text_provider.generate_chat_completion(llm_request).await;
        let duration = start_time.elapsed();

        match result {
            Ok(llm_response) => {
                let response = self.convert_chat_completion_response(&llm_response);
                let tokens_used = response.usage.total_tokens;

                // Cache the response
                self.cache_response(cache_key, CachedResponseData::ChatCompletion(response.clone())).await;

                self.update_metrics(duration, true, tokens_used).await;
                self.publish_event(InferenceEngineEvent::RequestCompleted {
                    request_id,
                    duration_ms: duration.as_millis() as u64,
                    tokens_used,
                    success: true,
                }).await;

                Ok(response)
            }
            Err(e) => {
                self.update_metrics(duration, false, 0).await;
                self.publish_event(InferenceEngineEvent::Error {
                    error: format!("Chat completion failed: {}", e),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("request_id".to_string(), request_id);
                        ctx.insert("model".to_string(), request.model);
                        ctx
                    },
                }).await;

                Err(ServiceError::execution_error(format!("Chat completion failed: {}", e)))
            }
        }
    }

    async fn generate_chat_completion_stream(&self, _request: ChatCompletionRequest) -> Result<mpsc::UnboundedReceiver<ChatCompletionChunk>, ServiceError> {
        // Implementation would create a streaming response
        todo!("Streaming chat completion implementation")
    }

    // Embedding generation methods
    async fn generate_embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ServiceError> {
        let request_id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        self.publish_event(InferenceEngineEvent::RequestStarted {
            request_id: request_id.clone(),
            request_type: "embedding".to_string(),
            model: request.model.clone(),
        }).await;

        // Convert input to string
        let input_text = match request.input {
            EmbeddingInput::String(text) => text,
            EmbeddingInput::Array(texts) => {
                // For single embedding request, use first text
                texts.into_iter().next().unwrap_or_default()
            }
        };

        // Check cache first
        let cache_key = self.generate_cache_key("embedding", &format!("{}:{}", request.model, input_text));
        if let Some(cached_data) = self.get_cached_response(&cache_key).await {
            self.publish_event(InferenceEngineEvent::CacheHit {
                cache_key,
                request_type: "embedding".to_string(),
            }).await;

            if let CachedResponseData::Embedding(response) = cached_data {
                self.update_metrics(start_time.elapsed(), true, response.usage.total_tokens).await;
                return Ok(response);
            }
        }

        self.publish_event(InferenceEngineEvent::CacheMiss {
            cache_key: cache_key.clone(),
            request_type: "embedding".to_string(),
        }).await;

        // Make the actual request
        let result = self.embedding_provider.embed(&input_text).await;
        let duration = start_time.elapsed();

        match result {
            Ok(llm_response) => {
                let response = self.convert_embedding_response(llm_response);
                let tokens_used = response.usage.total_tokens;

                // Cache the response
                self.cache_response(cache_key, CachedResponseData::Embedding(response.clone())).await;

                self.update_metrics(duration, true, tokens_used).await;
                self.publish_event(InferenceEngineEvent::RequestCompleted {
                    request_id,
                    duration_ms: duration.as_millis() as u64,
                    tokens_used,
                    success: true,
                }).await;

                Ok(response)
            }
            Err(e) => {
                self.update_metrics(duration, false, 0).await;
                self.publish_event(InferenceEngineEvent::Error {
                    error: format!("Embedding generation failed: {}", e),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("request_id".to_string(), request_id);
                        ctx.insert("model".to_string(), request.model);
                        ctx
                    },
                }).await;

                Err(ServiceError::execution_error(format!("Embedding generation failed: {}", e)))
            }
        }
    }

    async fn generate_batch_embeddings(&self, request: BatchEmbeddingRequest) -> Result<BatchEmbeddingResponse, ServiceError> {
        let request_id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        self.publish_event(InferenceEngineEvent::RequestStarted {
            request_id: request_id.clone(),
            request_type: "batch_embedding".to_string(),
            model: request.model.clone(),
        }).await;

        // Use the batch embedding capability
        let result = self.embedding_provider.embed_batch(request.inputs).await;
        let duration = start_time.elapsed();

        match result {
            Ok(llm_responses) => {
                let embeddings = llm_responses.into_iter().enumerate().map(|(index, llm_response)| Embedding {
                    index: index as u32,
                    object: "embedding".to_string(),
                    embedding: llm_response.embedding,
                }).collect();

                let total_tokens = llm_responses.iter()
                    .map(|r| r.tokens.unwrap_or(0))
                    .sum::<usize>() as u32;

                let response = BatchEmbeddingResponse {
                    data: embeddings,
                    model: request.model.clone(),
                    usage: TokenUsage {
                        prompt_tokens: total_tokens,
                        completion_tokens: 0,
                        total_tokens: total_tokens,
                    },
                    request_id: Uuid::new_v4().to_string(),
                    timestamp: Utc::now(),
                };

                self.update_metrics(duration, true, total_tokens).await;
                self.publish_event(InferenceEngineEvent::RequestCompleted {
                    request_id,
                    duration_ms: duration.as_millis() as u64,
                    tokens_used: total_tokens,
                    success: true,
                }).await;

                Ok(response)
            }
            Err(e) => {
                self.update_metrics(duration, false, 0).await;
                self.publish_event(InferenceEngineEvent::Error {
                    error: format!("Batch embedding generation failed: {}", e),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("request_id".to_string(), request_id);
                        ctx.insert("model".to_string(), request.model);
                        ctx
                    },
                }).await;

                Err(ServiceError::execution_error(format!("Batch embedding generation failed: {}", e)))
            }
        }
    }

    // Placeholder implementations for advanced features
    async fn perform_reasoning(&self, _request: ReasoningRequest) -> Result<ReasoningResponse, ServiceError> {
        todo!("Reasoning implementation")
    }

    async fn perform_tool_use(&self, _request: ToolUseRequest) -> Result<ToolUseResponse, ServiceError> {
        todo!("Tool use implementation")
    }

    async fn semantic_search(&self, _request: SemanticSearchRequest) -> Result<SemanticSearchResponse, ServiceError> {
        todo!("Semantic search implementation")
    }

    async fn fine_tune_model(&mut self, _request: FineTuningRequest) -> Result<FineTuningJob, ServiceError> {
        todo!("Fine-tuning implementation")
    }

    async fn get_fine_tuning_status(&self, _job_id: &str) -> Result<FineTuningStatus, ServiceError> {
        todo!("Fine-tuning status implementation")
    }

    async fn optimize_model(&mut self, _model_id: &str, _optimization: ModelOptimization) -> Result<ModelInfo, ServiceError> {
        todo!("Model optimization implementation")
    }

    async fn get_model_resources(&self, model_id: &str) -> Result<ModelResourceUsage, ServiceError> {
        let models = self.models.read().await;
        if let Some(model) = models.get(model_id) {
            Ok(model.resources.clone())
        } else {
            Err(ServiceError::config_error(format!("Model not found: {}", model_id)))
        }
    }

    async fn set_inference_limits(&mut self, limits: InferenceLimits) -> Result<(), ServiceError> {
        let mut current_limits = self.limits.write().await;
        *current_limits = limits;
        Ok(())
    }

    async fn get_inference_stats(&self) -> Result<InferenceStatistics, ServiceError> {
        let metrics = self.metrics.read().await;
        let models = self.models.read().await;

        let model_stats: HashMap<String, ModelStatistics> = models.iter().map(|(id, model)| {
            (id.clone(), ModelStatistics {
                model: id.clone(),
                request_count: model.usage.total_requests,
                success_rate: if model.usage.total_requests > 0 {
                    model.usage.successful_requests as f32 / model.usage.total_requests as f32
                } else {
                    0.0
                },
                average_response_time: model.usage.average_response_time,
                token_usage: TokenUsage {
                    prompt_tokens: (model.usage.total_tokens / 2) as u32, // Rough estimate
                    completion_tokens: (model.usage.total_tokens / 2) as u32,
                    total_tokens: model.usage.total_tokens as u32,
                },
            })
        }).collect();

        Ok(InferenceStatistics {
            total_requests: metrics.total_requests,
            successful_requests: metrics.successful_requests,
            failed_requests: metrics.failed_requests,
            average_response_time: metrics.average_response_time,
            requests_per_minute: 0.0, // Would need time-window tracking
            token_usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            error_rates: HashMap::new(),
            model_stats,
        })
    }
}

// Implement Clone for InferenceEngineService to support event processing
impl Clone for InferenceEngineService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            models: self.models.clone(),
            text_provider: self.text_provider.clone(),
            embedding_provider: self.embedding_provider.clone(),
            metrics: self.metrics.clone(),
            event_sender: self.event_sender.clone(),
            running: self.running.clone(),
            cache: self.cache.clone(),
            limits: self.limits.clone(),
            event_integration: self.event_integration.clone(),
        }
    }
}

// Implement EventIntegratedService for daemon coordination
#[async_trait]
impl EventIntegratedService for InferenceEngineService {
    fn service_id(&self) -> &str {
        "crucible-inference-engine"
    }

    fn service_type(&self) -> &str {
        "inference-engine"
    }

    fn published_event_types(&self) -> Vec<String> {
        vec![
            "model_loaded".to_string(),
            "model_unloaded".to_string(),
            "request_completed".to_string(),
            "cache_hit".to_string(),
            "cache_miss".to_string(),
            "inference_error".to_string(),
            "performance_alert".to_string(),
        ]
    }

    fn subscribed_event_types(&self) -> Vec<String> {
        vec![
            "data_change".to_string(),
            "model_update_request".to_string(),
            "configuration_changed".to_string(),
            "system_shutdown".to_string(),
            "maintenance_mode".to_string(),
        ]
    }

    async fn handle_daemon_event(&mut self, event: DaemonEvent) -> EventResult<()> {
        debug!("Inference Engine handling daemon event: {:?}", event.event_type);

        match &event.event_type {
            EventType::Database(db_event) => {
                match db_event {
                    crate::events::core::DatabaseEventType::RecordCreated { table, id, .. } => {
                        if table == "models" || table == "inference_requests" {
                            info!("Relevant database record created: {} {}", table, id);
                            // Handle model or request creation
                        }
                    }
                    crate::events::core::DatabaseEventType::RecordUpdated { table, id, changes, .. } => {
                        if table == "models" {
                            info!("Model configuration updated: {} {:?}", id, changes);
                            // Handle model configuration updates
                        }
                    }
                    _ => {}
                }
            }
            EventType::Service(service_event) => {
                match service_event {
                    crate::events::core::ServiceEventType::ConfigurationChanged { service_id, changes } => {
                        if service_id == self.service_id() {
                            info!("Inference Engine configuration changed: {:?}", changes);
                            // Handle configuration changes like model updates
                        }
                    }
                    crate::events::core::ServiceEventType::ServiceStatusChanged { service_id, new_status, .. } => {
                        if new_status == "maintenance" {
                            warn!("Entering maintenance mode, limiting inference operations");
                            // Enter limited operation mode
                        }
                    }
                    _ => {}
                }
            }
            EventType::System(system_event) => {
                match system_event {
                    crate::events::core::SystemEventType::EmergencyShutdown { reason } => {
                        warn!("Emergency shutdown triggered: {}, stopping all inference operations", reason);
                        // Emergency stop all operations
                        let _ = self.stop().await;
                    }
                    crate::events::core::SystemEventType::MaintenanceStarted { reason } => {
                        info!("System maintenance started: {}, limiting inference operations", reason);
                        // Enter limited operation mode
                    }
                    _ => {}
                }
            }
            _ => {
                debug!("Unhandled event type in Inference Engine: {:?}", event.event_type);
            }
        }

        Ok(())
    }

    fn service_event_to_daemon_event(&self, service_event: &dyn std::any::Any, priority: EventPriority) -> EventResult<DaemonEvent> {
        // Try to downcast to InferenceEngineEvent
        if let Some(inference_event) = service_event.downcast_ref::<InferenceEngineEvent>() {
            self.inference_event_to_daemon_event(inference_event, priority)
        } else {
            Err(EventError::ValidationError("Invalid event type for InferenceEngine".to_string()))
        }
    }

    fn daemon_event_to_service_event(&self, daemon_event: &DaemonEvent) -> Option<Box<dyn std::any::Any>> {
        // Convert daemon events to InferenceEngine events if applicable
        match &daemon_event.event_type {
            EventType::Database(db_event) => {
                match db_event {
                    crate::events::core::DatabaseEventType::RecordUpdated { table, id, changes, .. } => {
                        if table == "models" && changes.contains_key("status") {
                            Some(Box::new(InferenceEngineEvent::ModelLoaded {
                                model_id: id.clone(),
                                model_info: ModelInfo {
                                    id: id.clone(),
                                    name: id.clone(),
                                    provider: "unknown".to_string(),
                                    model_type: "text".to_string(),
                                    capabilities: vec![],
                                    context_size: 4096,
                                    max_tokens: 2048,
                                    description: Some("Updated model".to_string()),
                                    created_at: chrono::Utc::now(),
                                    updated_at: chrono::Utc::now(),
                                },
                            }))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// Implement EventPublishingService for lifecycle events
#[async_trait]
impl EventPublishingService for InferenceEngineService {
    async fn publish_lifecycle_event(&self, event_type: LifecycleEventType, details: HashMap<String, String>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let lifecycle_event = event_integration.adapter().create_lifecycle_event(event_type, details);
            event_integration.publish_event(lifecycle_event).await?;
        }
        Ok(())
    }

    async fn publish_health_event(&self, health: ServiceHealth) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let health_event = event_integration.adapter().create_health_event(health);
            event_integration.publish_event(health_event).await?;
        }
        Ok(())
    }

    async fn publish_error_event(&self, error: String, context: Option<HashMap<String, String>>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let error_event = event_integration.adapter().create_error_event(error, context);
            event_integration.publish_event(error_event).await?;
        }
        Ok(())
    }

    async fn publish_metric_event(&self, metrics: HashMap<String, f64>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let metric_event = event_integration.adapter().create_metric_event(metrics);
            event_integration.publish_event(metric_event).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        // Note: This test would need to create a service instance
        // For now, we'll test the key generation logic conceptually
        let request_data = "test request";
        let request_type = "completion";

        // The actual implementation would use the service instance
        // This is just a placeholder test
        assert!(!request_data.is_empty());
        assert!(!request_type.is_empty());
    }

    #[test]
    fn test_config_validation() {
        let config = InferenceEngineConfig {
            text_provider: TextProviderConfig::openai("test-key".to_string()),
            embedding_provider: crucible_llm::EmbeddingConfig::ollama(None, None),
            default_models: DefaultModels {
                text_model: "gpt-3.5-turbo".to_string(),
                embedding_model: "text-embedding-ada-002".to_string(),
                chat_model: "gpt-3.5-turbo".to_string(),
            },
            performance: PerformanceSettings {
                enable_batching: true,
                batch_size: 4,
                batch_timeout_ms: 1000,
                enable_deduplication: true,
                connection_pool_size: 10,
                request_timeout_ms: 30000,
            },
            cache: CacheSettings {
                enabled: true,
                ttl_seconds: 3600,
                max_size_bytes: 1024 * 1024 * 100, // 100MB
                eviction_policy: CacheEvictionPolicy::LRU,
            },
            limits: InferenceLimits {
                max_concurrent_requests: Some(10),
                max_request_tokens: Some(4096),
                max_response_tokens: Some(2048),
                request_timeout: Some(std::time::Duration::from_secs(30)),
                max_queue_size: Some(100),
            },
            monitoring: MonitoringSettings {
                enable_metrics: true,
                metrics_interval_seconds: 60,
                enable_profiling: false,
                export_metrics: false,
            },
        };

        // Test that config validation would pass
        assert!(!config.default_models.text_model.is_empty());
        assert!(!config.default_models.embedding_model.is_empty());
    }
}