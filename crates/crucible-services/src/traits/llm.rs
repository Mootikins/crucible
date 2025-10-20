use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::{ServiceInfo, ServiceHealth, ServiceMetrics, ServiceDependency};
use crate::types::llm::*;
use std::collections::HashMap;
use uuid::Uuid;
use super::BaseService;

/// Trait for LLM (Large Language Model) services
#[async_trait]
pub trait LLMService: BaseService + Send + Sync {
    // === Model Management ===

    /// List available models
    async fn list_models(&self) -> ServiceResult<Vec<ModelInfo>>;

    /// Get model information
    async fn get_model(&self, model_name: &str) -> ServiceResult<Option<ModelInfo>>;

    /// Load a model
    async fn load_model(&self, model_name: &str) -> ServiceResult<bool>;

    /// Unload a model
    async fn unload_model(&self, model_name: &str) -> ServiceResult<bool>;

    /// Get loaded models
    async fn get_loaded_models(&self) -> ServiceResult<Vec<String>>;

    /// Get model capabilities
    async fn get_model_capabilities(&self, model_name: &str) -> ServiceResult<Vec<ModelCapability>>;

    /// Validate model availability
    async fn validate_model(&self, model_name: &str) -> ServiceResult<ModelValidationResult>;

    // === Text Generation ===

    /// Generate text completion
    async fn complete(&self, request: CompletionRequest) -> ServiceResult<CompletionResponse>;

    /// Generate chat completion
    async fn chat_complete(&self, request: CompletionRequest) -> ServiceResult<CompletionResponse>;

    /// Stream text completion
    async fn complete_stream(&self, request: CompletionRequest) -> ServiceResult<Box<dyn CompletionStream>>;

    /// Stream chat completion
    async fn chat_complete_stream(&self, request: CompletionRequest) -> ServiceResult<Box<dyn CompletionStream>>;

    /// Generate multiple completions
    async fn complete_multiple(&self, request: CompletionRequest, count: u32) -> ServiceResult<Vec<CompletionResponse>>;

    // === Embeddings ===

    /// Generate text embeddings
    async fn embed(&self, request: EmbeddingRequest) -> ServiceResult<EmbeddingResponse>;

    /// Generate single text embedding
    async fn embed_single(&self, model: &str, text: &str) -> ServiceResult<Vec<f32>>;

    /// Batch embed multiple texts
    async fn embed_batch(&self, model: &str, texts: Vec<String>) -> ServiceResult<Vec<Vec<f32>>>;

    // === Function Calling ===

    /// Execute function calling
    async fn call_function(&self, request: FunctionCallRequest) -> ServiceResult<FunctionCallResponse>;

    /// Extract function schema from text
    async fn extract_function_schema(&self, model: &str, text: &str) -> ServiceResult<FunctionSchema>;

    // === Advanced Features ===

    /// Generate with retrieval-augmented generation (RAG)
    async fn generate_with_rag(&self, request: RAGRequest) -> ServiceResult<RAGResponse>;

    /// Generate with chain-of-thought reasoning
    async fn generate_with_cot(&self, request: CoTRequest) -> ServiceResult<CoTResponse>;

    /// Generate with few-shot examples
    async fn generate_few_shot(&self, request: FewShotRequest) -> ServiceResult<CompletionResponse>;

    /// Generate self-consistency samples
    async fn generate_self_consistency(&self, request: SelfConsistencyRequest) -> ServiceResult<SelfConsistencyResponse>;

    // === Model Fine-tuning ===

    /// Start fine-tuning job
    async fn start_fine_tuning(&self, request: FineTuningRequest) -> ServiceResult<String>;

    /// Get fine-tuning job status
    async fn get_fine_tuning_status(&self, job_id: &str) -> ServiceResult<FineTuningStatus>;

    /// List fine-tuning jobs
    async fn list_fine_tuning_jobs(&self) -> ServiceResult<Vec<FineTuningJob>>;

    /// Cancel fine-tuning job
    async fn cancel_fine_tuning(&self, job_id: &str) -> ServiceResult<bool>;

    /// Deploy fine-tuned model
    async fn deploy_fine_tuned_model(&self, job_id: &str, deployment_name: &str) -> ServiceResult<bool>;

    // === Content Moderation ===

    /// Moderate content
    async fn moderate_content(&self, request: ModerationRequest) -> ServiceResult<ModerationResponse>;

    /// Check content safety
    async fn check_content_safety(&self, model: &str, content: &str) -> ServiceResult<Vec<SafetyCheck>>;

    // === Token Management ===

    /// Count tokens
    async fn count_tokens(&self, model: &str, text: &str) -> ServiceResult<TokenCount>;

    /// Count tokens for messages
    async fn count_message_tokens(&self, model: &str, messages: &[ChatMessage]) -> ServiceResult<TokenCount>;

    /// Estimate token usage for completion
    async fn estimate_usage(&self, request: &CompletionRequest) -> ServiceResult<TokenUsage>;

    // === Performance and Monitoring ===

    /// Get model performance metrics
    async fn get_model_metrics(&self, model_name: &str) -> ServiceResult<ModelMetrics>;

    /// Get service performance metrics
    async fn get_service_metrics(&self) -> ServiceResult<LLMServiceMetrics>;

    /// Benchmark model performance
    async fn benchmark_model(&self, model: &str, benchmark: ModelBenchmark) -> ServiceResult<BenchmarkResult>;

    // === Advanced Operations ===

    /// Generate structured output
    async fn generate_structured(&self, request: StructuredGenerationRequest) -> ServiceResult<StructuredGenerationResponse>;

    /// Perform semantic search
    async fn semantic_search(&self, request: SemanticSearchRequest) -> ServiceResult<Vec<SemanticSearchResult>>;

    /// Classify text
    async fn classify_text(&self, request: TextClassificationRequest) -> ServiceResult<TextClassificationResponse>;

    /// Extract entities
    async fn extract_entities(&self, request: EntityExtractionRequest) -> ServiceResult<EntityExtractionResponse>;

    /// Summarize text
    async fn summarize_text(&self, request: SummarizationRequest) -> ServiceResult<SummarizationResponse>;

    /// Translate text
    async fn translate_text(&self, request: TranslationRequest) -> ServiceResult<TranslationResponse>;

    // === Configuration and Management ===

    /// Update model configuration
    async fn update_model_config(&self, model_name: &str, config: ModelConfig) -> ServiceResult<bool>;

    /// Get model configuration
    async fn get_model_config(&self, model_name: &str) -> ServiceResult<Option<ModelConfig>>;

    /// Set rate limits
    async fn set_rate_limits(&self, model: &str, limits: RateLimits) -> ServiceResult<bool>;

    /// Get rate limits
    async fn get_rate_limits(&self, model: &str) -> ServiceResult<Option<RateLimits>>;

    // === Caching and Optimization ===

    /// Clear model cache
    async fn clear_model_cache(&self, model_name: &str) -> ServiceResult<bool>;

    /// Warm up model
    async fn warm_up_model(&self, model_name: &str, warmup_data: Vec<String>) -> ServiceResult<bool>;

    /// Get cache statistics
    async fn get_cache_stats(&self, model_name: &str) -> ServiceResult<CacheStatistics>;
}

/// Trait for streaming completion responses
#[async_trait]
pub trait CompletionStream: Send + Sync {
    /// Get the next chunk from the stream
    async fn next_chunk(&mut self) -> ServiceResult<Option<CompletionChunk>>;

    /// Check if stream is finished
    fn is_finished(&self) -> bool;

    /// Get final completion result
    async fn get_final_result(&self) -> ServiceResult<Option<CompletionResponse>>;

    /// Cancel the stream
    async fn cancel(&mut self) -> ServiceResult<()>;
}

/// Completion chunk for streaming
#[derive(Debug, Clone)]
pub struct CompletionChunk {
    /// Chunk content
    pub content: String,
    /// Chunk index
    pub index: usize,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Token usage so far
    pub usage: Option<TokenUsage>,
    /// Finish reason (if final)
    pub finish_reason: Option<FinishReason>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Model validation result
#[derive(Debug, Clone)]
pub struct ModelValidationResult {
    /// Whether model is valid
    pub valid: bool,
    /// Model availability
    pub available: bool,
    /// Validation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Model health information
    pub health: ModelHealth,
    /// Performance metrics
    pub performance: Option<ModelPerformanceMetrics>,
}

/// Model health information
#[derive(Debug, Clone)]
pub struct ModelHealth {
    /// Overall health status
    pub status: ModelHealthStatus,
    /// Last health check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// Response time in milliseconds
    pub response_time_ms: Option<u64>,
    /// Memory usage in bytes
    pub memory_usage_bytes: Option<u64>,
    /// GPU utilization percentage
    pub gpu_utilization_percent: Option<f32>,
    /// Health check messages
    pub messages: Vec<String>,
}

/// Model health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Loading,
    Error,
}

/// Model performance metrics
#[derive(Debug, Clone)]
pub struct ModelPerformanceMetrics {
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Throughput (tokens per second)
    pub throughput_tps: f64,
    /// Error rate
    pub error_rate: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// GPU memory usage in bytes
    pub gpu_memory_usage_bytes: Option<u64>,
    /// CPU usage percentage
    pub cpu_usage_percent: f32,
    /// GPU usage percentage
    pub gpu_usage_percent: Option<f32>,
}

/// Function calling request
#[derive(Debug, Clone)]
pub struct FunctionCallRequest {
    /// Model to use
    pub model: String,
    /// Messages for the conversation
    pub messages: Vec<ChatMessage>,
    /// Available functions
    pub functions: Vec<FunctionDefinition>,
    /// Function call behavior
    pub function_call: FunctionCallBehavior,
    /// Generation parameters
    pub parameters: GenerationParameters,
}

/// Function definition
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters schema
    pub parameters: serde_json::Value,
    /// Whether function is required
    pub required: bool,
}

/// Function call behavior
#[derive(Debug, Clone)]
pub enum FunctionCallBehavior {
    /// Let the model decide
    Auto,
    /// Force calling a specific function
    Force(String),
    /// No function calling
    None,
}

/// Function call response
#[derive(Debug, Clone)]
pub struct FunctionCallResponse {
    /// Model used
    pub model: String,
    /// Generated message
    pub message: ChatMessage,
    /// Function calls made
    pub function_calls: Vec<FunctionCall>,
    /// Token usage
    pub usage: TokenUsage,
    /// Response metadata
    pub metadata: HashMap<String, String>,
}

/// Function schema
#[derive(Debug, Clone)]
pub struct FunctionSchema {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters
    pub parameters: FunctionParameters,
    /// Examples
    pub examples: Vec<FunctionExample>,
}

/// Function parameters
#[derive(Debug, Clone)]
pub struct FunctionParameters {
    /// Parameter type
    pub param_type: String,
    /// Required parameters
    pub required: Vec<String>,
    /// Parameter properties
    pub properties: HashMap<String, ParameterProperty>,
}

/// Parameter property
#[derive(Debug, Clone)]
pub struct ParameterProperty {
    /// Property type
    pub param_type: String,
    /// Property description
    pub description: Option<String>,
    /// Enum values (if applicable)
    pub enum_values: Option<Vec<String>>,
    /// Default value
    pub default_value: Option<serde_json::Value>,
}

/// Function example
#[derive(Debug, Clone)]
pub struct FunctionExample {
    /// Example input
    pub input: serde_json::Value,
    /// Example output
    pub output: serde_json::Value,
    /// Example description
    pub description: Option<String>,
}

/// RAG (Retrieval-Augmented Generation) request
#[derive(Debug, Clone)]
pub struct RAGRequest {
    /// Model to use
    pub model: String,
    /// Query/question
    pub query: String,
    /// Retrieved context documents
    pub context: Vec<RAGContext>,
    /// Generation parameters
    pub parameters: GenerationParameters,
    /// RAG configuration
    pub config: RAGConfig,
}

/// RAG context document
#[derive(Debug, Clone)]
pub struct RAGContext {
    /// Document content
    pub content: String,
    /// Document source
    pub source: String,
    /// Relevance score
    pub relevance_score: f32,
    /// Document metadata
    pub metadata: HashMap<String, String>,
}

/// RAG configuration
#[derive(Debug, Clone)]
pub struct RAGConfig {
    /// Maximum context length
    pub max_context_length: usize,
    /// Context ordering strategy
    pub context_ordering: ContextOrdering,
    /// Include citations
    pub include_citations: bool,
    /// Citation format
    pub citation_format: CitationFormat,
}

/// Context ordering strategies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextOrdering {
    /// Order by relevance score
    ByRelevance,
    /// Order by recency
    ByRecency,
    /// Original order
    Original,
}

/// Citation formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationFormat {
    /// Numeric citations [1], [2]
    Numeric,
    /// Author-year citations (Smith, 2023)
    AuthorYear,
    /// Inline citations (Source: doc1)
    Inline,
}

/// RAG response
#[derive(Debug, Clone)]
pub struct RAGResponse {
    /// Generated answer
    pub answer: String,
    /// Model used
    pub model: String,
    /// Citations used
    pub citations: Vec<Citation>,
    /// Token usage
    pub usage: TokenUsage,
    /// Response metadata
    pub metadata: HashMap<String, String>,
}

/// Citation
#[derive(Debug, Clone)]
pub struct Citation {
    /// Citation identifier
    pub id: String,
    /// Source document
    pub source: String,
    /// Relevant snippet
    pub snippet: String,
    /// Citation position in answer
    pub position: usize,
}

/// Chain-of-Thought request
#[derive(Debug, Clone)]
pub struct CoTRequest {
    /// Model to use
    pub model: String,
    /// Problem/question
    pub problem: String,
    /// CoT strategy
    pub strategy: CoTStrategy,
    /// Generation parameters
    pub parameters: GenerationParameters,
}

/// Chain-of-Thought strategies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoTStrategy {
    /// Step-by-step reasoning
    StepByStep,
    /// Decompose into sub-problems
    Decomposition,
    /// Self-questioning
    SelfQuestioning,
    /// Analogy-based reasoning
    Analogy,
}

/// CoT response
#[derive(Debug, Clone)]
pub struct CoTResponse {
    /// Final answer
    pub answer: String,
    /// Reasoning steps
    pub reasoning_steps: Vec<ReasoningStep>,
    /// Model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
    /// Response metadata
    pub metadata: HashMap<String, String>,
}

/// Reasoning step
#[derive(Debug, Clone)]
pub struct ReasoningStep {
    /// Step number
    pub step_number: u32,
    /// Step description
    pub description: String,
    /// Step content
    pub content: String,
    /// Step confidence
    pub confidence: Option<f32>,
}

/// Few-shot request
#[derive(Debug, Clone)]
pub struct FewShotRequest {
    /// Model to use
    pub model: String,
    /// Examples to include
    pub examples: Vec<FewShotExample>,
    /// Test prompt
    pub test_prompt: String,
    /// Generation parameters
    pub parameters: GenerationParameters,
}

/// Few-shot example
#[derive(Debug, Clone)]
pub struct FewShotExample {
    /// Input example
    pub input: String,
    /// Output example
    pub output: String,
    /// Example explanation
    pub explanation: Option<String>,
}

/// Self-consistency request
#[derive(Debug, Clone)]
pub struct SelfConsistencyRequest {
    /// Model to use
    pub model: String,
    /// Problem/question
    pub problem: String,
    /// Number of samples to generate
    pub num_samples: u32,
    /// Aggregation method
    pub aggregation_method: AggregationMethod,
    /// Generation parameters
    pub parameters: GenerationParameters,
}

/// Aggregation methods for self-consistency
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregationMethod {
    /// Majority vote
    MajorityVote,
    /// Weighted vote by confidence
    WeightedVote,
    /// Most common answer
    MostCommon,
}

/// Self-consistency response
#[derive(Debug, Clone)]
pub struct SelfConsistencyResponse {
    /// Final consensus answer
    pub consensus_answer: String,
    /// Individual samples
    pub samples: Vec<SelfConsistencySample>,
    /// Agreement level (0-1)
    pub agreement_level: f32,
    /// Model used
    pub model: String,
    /// Total token usage
    pub usage: TokenUsage,
}

/// Self-consistency sample
#[derive(Debug, Clone)]
pub struct SelfConsistencySample {
    /// Sample answer
    pub answer: String,
    /// Sample reasoning
    pub reasoning: Option<String>,
    /// Sample confidence
    pub confidence: f32,
}

/// Fine-tuning request
#[derive(Debug, Clone)]
pub struct FineTuningRequest {
    /// Base model to fine-tune
    pub base_model: String,
    /// Training dataset
    pub training_data: Vec<FineTuningExample>,
    /// Validation dataset
    pub validation_data: Option<Vec<FineTuningExample>>,
    /// Fine-tuning configuration
    pub config: FineTuningConfig,
    /// Output model name
    pub output_model: String,
}

/// Fine-tuning example
#[derive(Debug, Clone)]
pub struct FineTuningExample {
    /// Input prompt
    pub prompt: String,
    /// Expected completion
    pub completion: String,
    /// Example weight
    pub weight: Option<f32>,
}

/// Fine-tuning configuration
#[derive(Debug, Clone)]
pub struct FineTuningConfig {
    /// Number of training epochs
    pub epochs: u32,
    /// Batch size
    pub batch_size: u32,
    /// Learning rate
    pub learning_rate: f32,
    /// Weight decay
    pub weight_decay: Option<f32>,
    /// Warmup steps
    pub warmup_steps: Option<u32>,
    /// Early stopping patience
    pub early_stopping_patience: Option<u32>,
}

/// Fine-tuning job
#[derive(Debug, Clone)]
pub struct FineTuningJob {
    /// Job ID
    pub job_id: String,
    /// Base model
    pub base_model: String,
    /// Output model name
    pub output_model: String,
    /// Job status
    pub status: FineTuningStatus,
    /// Progress percentage
    pub progress_percent: f32,
    /// Started timestamp
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Estimated completion timestamp
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
    /// Training metrics
    pub metrics: Option<FineTuningMetrics>,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

/// Fine-tuning status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FineTuningStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Fine-tuning metrics
#[derive(Debug, Clone)]
pub struct FineTuningMetrics {
    /// Training loss
    pub training_loss: f32,
    /// Validation loss
    pub validation_loss: Option<f32>,
    /// Current epoch
    pub current_epoch: u32,
    /// Total epochs
    pub total_epochs: u32,
    /// Samples processed
    pub samples_processed: u64,
    /// Total samples
    pub total_samples: u64,
}

/// Moderation request
#[derive(Debug, Clone)]
pub struct ModerationRequest {
    /// Model to use
    pub model: String,
    /// Content to moderate
    pub content: String,
    /// Content type
    pub content_type: ContentType,
    /// Moderation categories to check
    pub categories: Vec<ModerationCategory>,
}

/// Content types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    Text,
    Image,
    Audio,
    Video,
}

/// Moderation categories
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationCategory {
    Harassment,
    Hate,
    Violence,
    SelfHarm,
    Sexual,
    Terrorism,
    Extremism,
    Illegal,
}

/// Moderation response
#[derive(Debug, Clone)]
pub struct ModerationResponse {
    /// Content is flagged
    pub flagged: bool,
    /// Category assessments
    pub categories: HashMap<ModerationCategory, CategoryAssessment>,
    /// Overall confidence score
    pub confidence_score: f32,
    /// Moderation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Category assessment
#[derive(Debug, Clone)]
pub struct CategoryAssessment {
    /// Whether category is flagged
    pub flagged: bool,
    /// Confidence score (0-1)
    pub confidence: f32,
    /// Severity level
    pub severity: SeverityLevel,
}

/// Severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeverityLevel {
    Low,
    Medium,
    High,
    Severe,
}

/// Safety check
#[derive(Debug, Clone)]
pub struct SafetyCheck {
    /// Safety category
    pub category: ModerationCategory,
    /// Safe flag
    pub safe: bool,
    /// Confidence score
    pub confidence: f32,
    /// Explanation
    pub explanation: Option<String>,
}

/// Token count
#[derive(Debug, Clone)]
pub struct TokenCount {
    /// Number of tokens
    pub count: u32,
    /// Tokens
    pub tokens: Vec<String>,
}

/// Model metrics
#[derive(Debug, Clone)]
pub struct ModelMetrics {
    /// Model name
    pub model_name: String,
    /// Total requests
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Tokens generated
    pub tokens_generated: u64,
    /// Tokens processed
    pub tokens_processed: u64,
    /// Current load (requests per second)
    pub current_load_rps: f64,
    /// Error rate
    pub error_rate: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: Option<u64>,
    /// GPU usage percentage
    pub gpu_usage_percent: Option<f32>,
    /// Metrics timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// LLM service metrics
#[derive(Debug, Clone)]
pub struct LLMServiceMetrics {
    /// Total requests across all models
    pub total_requests: u64,
    /// Total successful requests
    pub successful_requests: u64,
    /// Total failed requests
    pub failed_requests: u64,
    /// Average response time across all models
    pub avg_response_time_ms: f64,
    /// Total tokens generated
    pub total_tokens_generated: u64,
    /// Total tokens processed
    pub total_tokens_processed: u64,
    /// Active connections
    pub active_connections: u32,
    /// Queue length
    pub queue_length: u32,
    /// Service uptime in seconds
    pub uptime_seconds: u64,
    /// Individual model metrics
    pub model_metrics: HashMap<String, ModelMetrics>,
    /// Service health score (0-1)
    pub health_score: f32,
}

/// Model benchmark
#[derive(Debug, Clone)]
pub struct ModelBenchmark {
    /// Benchmark name
    pub name: String,
    /// Benchmark type
    pub benchmark_type: BenchmarkType,
    /// Test dataset
    pub test_data: Vec<BenchmarkExample>,
    /// Evaluation metrics
    pub metrics: Vec<EvaluationMetric>,
    /// Benchmark configuration
    pub config: BenchmarkConfig,
}

/// Benchmark types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchmarkType {
    /// Text generation quality
    TextGeneration,
    /// Code generation
    CodeGeneration,
    /// Question answering
    QuestionAnswering,
    /// Summarization
    Summarization,
    /// Translation
    Translation,
    /// Classification
    Classification,
}

/// Benchmark example
#[derive(Debug, Clone)]
pub struct BenchmarkExample {
    /// Input text
    pub input: String,
    /// Expected output
    pub expected_output: String,
    /// Example weight
    pub weight: Option<f32>,
    /// Example metadata
    pub metadata: HashMap<String, String>,
}

/// Evaluation metric
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationMetric {
    /// BLEU score
    BLEU,
    /// ROUGE score
    ROUGE,
    /// Exact match
    ExactMatch,
    /// Accuracy
    Accuracy,
    /// F1 score
    F1,
    /// Perplexity
    Perplexity,
    /// Latency
    Latency,
    /// Throughput
    Throughput,
}

/// Benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of samples to use
    pub sample_count: Option<usize>,
    /// Generation parameters
    pub parameters: GenerationParameters,
    /// Timeout in seconds
    pub timeout_seconds: u64,
    /// Concurrent requests
    pub concurrent_requests: u32,
}

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Benchmark name
    pub benchmark_name: String,
    /// Model name
    pub model_name: String,
    /// Overall score
    pub overall_score: f32,
    /// Individual metric scores
    pub metric_scores: HashMap<EvaluationMetric, f32>,
    /// Total samples processed
    pub total_samples: usize,
    /// Successful samples
    pub successful_samples: usize,
    /// Failed samples
    pub failed_samples: usize,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Throughput in requests per second
    pub throughput_rps: f64,
    /// Benchmark duration in seconds
    pub duration_seconds: f64,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Structured generation request
#[derive(Debug, Clone)]
pub struct StructuredGenerationRequest {
    /// Model to use
    pub model: String,
    /// Input prompt
    pub prompt: String,
    /// Output schema
    pub schema: serde_json::Value,
    /// Generation parameters
    pub parameters: GenerationParameters,
}

/// Structured generation response
#[derive(Debug, Clone)]
pub struct StructuredGenerationResponse {
    /// Generated structured data
    pub data: serde_json::Value,
    /// Model used
    pub model: String,
    /// Whether output conforms to schema
    pub valid_schema: bool,
    /// Validation errors
    pub validation_errors: Vec<String>,
    /// Token usage
    pub usage: TokenUsage,
}

/// Semantic search request
#[derive(Debug, Clone)]
pub struct SemanticSearchRequest {
    /// Model to use for embeddings
    pub model: String,
    /// Search query
    pub query: String,
    /// Documents to search
    pub documents: Vec<SemanticSearchDocument>,
    /// Number of results to return
    pub top_k: usize,
    /// Similarity threshold
    pub threshold: Option<f32>,
}

/// Semantic search document
#[derive(Debug, Clone)]
pub struct SemanticSearchDocument {
    /// Document ID
    pub id: String,
    /// Document content
    pub content: String,
    /// Document metadata
    pub metadata: HashMap<String, String>,
}

/// Semantic search result
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    /// Document ID
    pub document_id: String,
    /// Similarity score
    pub similarity_score: f32,
    /// Document content
    pub content: String,
    /// Document metadata
    pub metadata: HashMap<String, String>,
}

/// Text classification request
#[derive(Debug, Clone)]
pub struct TextClassificationRequest {
    /// Model to use
    pub model: String,
    /// Text to classify
    pub text: String,
    /// Classification labels
    pub labels: Vec<String>,
    /// Multi-label classification
    pub multi_label: bool,
}

/// Text classification response
#[derive(Debug, Clone)]
pub struct TextClassificationResponse {
    /// Classification results
    pub classifications: Vec<TextClassification>,
    /// Model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
}

/// Text classification
#[derive(Debug, Clone)]
pub struct TextClassification {
    /// Label
    pub label: String,
    /// Confidence score
    pub confidence: f32,
}

/// Entity extraction request
#[derive(Debug, Clone)]
pub struct EntityExtractionRequest {
    /// Model to use
    pub model: String,
    /// Text to analyze
    pub text: String,
    /// Entity types to extract
    pub entity_types: Vec<EntityType>,
}

/// Entity types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Time,
    Money,
    Percentage,
    Email,
    Phone,
    URL,
    Custom(String),
}

/// Entity extraction response
#[derive(Debug, Clone)]
pub struct EntityExtractionResponse {
    /// Extracted entities
    pub entities: Vec<Entity>,
    /// Model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
}

/// Entity
#[derive(Debug, Clone)]
pub struct Entity {
    /// Entity text
    pub text: String,
    /// Entity type
    pub entity_type: EntityType,
    /// Start position
    pub start: usize,
    /// End position
    pub end: usize,
    /// Confidence score
    pub confidence: f32,
}

/// Summarization request
#[derive(Debug, Clone)]
pub struct SummarizationRequest {
    /// Model to use
    pub model: String,
    /// Text to summarize
    pub text: String,
    /// Summary length
    pub summary_length: SummaryLength,
    /// Summary style
    pub summary_style: SummaryStyle,
    /// Language
    pub language: Option<String>,
}

/// Summary length options
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryLength {
    Short,
    Medium,
    Long,
    Custom { max_words: u32 },
}

/// Summary styles
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryStyle {
    Bulleted,
    Paragraph,
    Executive,
    Technical,
}

/// Summarization response
#[derive(Debug, Clone)]
pub struct SummarizationResponse {
    /// Generated summary
    pub summary: String,
    /// Model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
    /// Key points extracted
    pub key_points: Option<Vec<String>>,
}

/// Translation request
#[derive(Debug, Clone)]
pub struct TranslationRequest {
    /// Model to use
    pub model: String,
    /// Text to translate
    pub text: String,
    /// Source language
    pub source_language: String,
    /// Target language
    pub target_language: String,
    /// Translation style
    pub style: Option<TranslationStyle>,
}

/// Translation styles
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationStyle {
    Formal,
    Casual,
    Technical,
    Creative,
}

/// Translation response
#[derive(Debug, Clone)]
pub struct TranslationResponse {
    /// Translated text
    pub translated_text: String,
    /// Model used
    pub model: String,
    /// Source language detected
    pub source_language_detected: Option<String>,
    /// Translation confidence
    pub confidence: Option<f32>,
    /// Token usage
    pub usage: TokenUsage,
}

/// Model configuration
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Temperature
    pub temperature: Option<f32>,
    /// Top-p sampling
    pub top_p: Option<f32>,
    /// Top-k sampling
    pub top_k: Option<u32>,
    /// Maximum tokens
    pub max_tokens: Option<u32>,
    /// Presence penalty
    pub presence_penalty: Option<f32>,
    /// Frequency penalty
    pub frequency_penalty: Option<f32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Custom configuration
    pub custom_config: HashMap<String, serde_json::Value>,
}

/// Rate limits
#[derive(Debug, Clone)]
pub struct RateLimits {
    /// Requests per minute
    pub requests_per_minute: u32,
    /// Tokens per minute
    pub tokens_per_minute: u32,
    /// Concurrent requests
    pub concurrent_requests: u32,
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStatistics {
    /// Cache size in bytes
    pub cache_size_bytes: u64,
    /// Number of cached items
    pub cached_items: u32,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate
    pub hit_rate: f64,
    /// Evictions
    pub evictions: u64,
}