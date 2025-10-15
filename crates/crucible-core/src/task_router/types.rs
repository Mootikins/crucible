use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// User request to be processed by the task routing system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRequest {
    /// Unique request ID
    pub id: Uuid,
    /// User who made the request
    pub user_id: String,
    /// Request content/prompt
    pub content: String,
    /// Request type (chat, command, complex_task)
    pub request_type: RequestType,
    /// Priority level
    pub priority: TaskPriority,
    /// Request context
    pub context: RequestContext,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Expected completion time (optional)
    pub deadline: Option<DateTime<Utc>>,
}

/// Type of user request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    /// Simple chat/conversation
    Chat,
    /// Direct command execution
    Command,
    /// Complex multi-step task
    ComplexTask,
    /// Collaboration request
    Collaboration,
    /// Research query
    Research,
    /// Code generation/modification
    CodeGeneration,
    /// Analysis request
    Analysis,
}

/// Request context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Conversation history (if applicable)
    pub conversation_history: Vec<String>,
    /// Previous interactions
    pub previous_results: Vec<TaskResult>,
    /// Available tools and resources
    pub available_tools: Vec<String>,
    /// User preferences
    pub user_preferences: HashMap<String, String>,
    /// Session context
    pub session_context: HashMap<String, String>,
}

/// Task priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
    Emergency = 5,
}

/// Result of task analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    /// Original request
    pub request_id: Uuid,
    /// Breakdown into subtasks
    pub subtasks: Vec<Subtask>,
    /// Required capabilities
    pub required_capabilities: Vec<String>,
    /// Estimated complexity
    pub complexity: TaskComplexity,
    /// Estimated duration in minutes
    pub estimated_duration_minutes: u32,
    /// Dependencies between subtasks
    pub dependencies: Vec<TaskDependency>,
    /// Recommended execution strategy
    pub execution_strategy: ExecutionStrategy,
    /// Analysis confidence (0-1)
    pub confidence: f32,
    /// Analysis timestamp
    pub timestamp: DateTime<Utc>,
}

/// Individual subtask identified during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    /// Unique subtask ID
    pub id: Uuid,
    /// Subtask description
    pub description: String,
    /// Subtask type
    pub subtask_type: SubtaskType,
    /// Required capabilities for this subtask
    pub required_capabilities: Vec<String>,
    /// Required tools
    pub required_tools: Vec<String>,
    /// Estimated duration in minutes
    pub estimated_duration_minutes: u32,
    /// Priority relative to other subtasks
    pub priority: TaskPriority,
    /// Whether this can be executed in parallel
    pub can_parallelize: bool,
    /// Input data required
    pub input_requirements: Vec<String>,
    /// Expected output
    pub expected_output: String,
}

/// Type of subtask
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubtaskType {
    /// Information gathering
    Research,
    /// Data analysis
    Analysis,
    /// Code generation
    CodeGeneration,
    /// Code review
    CodeReview,
    /// Writing/documentation
    Writing,
    /// Decision making
    Decision,
    /// Validation/testing
    Validation,
    /// Coordination/communication
    Coordination,
    /// File operations
    FileOperation,
    /// API calls/integration
    Integration,
}

/// Task complexity assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComplexity {
    /// Overall complexity score (1-10)
    pub score: u8,
    /// Number of distinct skills required
    pub skill_diversity: u8,
    /// Coordination complexity
    pub coordination_complexity: u8,
    /// Technical difficulty
    pub technical_difficulty: u8,
    /// Ambiguity level
    pub ambiguity_level: u8,
}

/// Dependency between subtasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependency {
    /// Dependent subtask ID
    pub dependent_id: Uuid,
    /// Prerequisite subtask ID
    pub prerequisite_id: Uuid,
    /// Dependency type
    pub dependency_type: DependencyType,
    /// Description of dependency
    pub description: String,
}

/// Type of dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    /// Must complete before dependent can start
    FinishToStart,
    /// Can start when prerequisite is partially complete
    PartialDependency,
    /// Output of prerequisite is input to dependent
    DataDependency,
    /// Resource dependency (shared tool/agent)
    ResourceDependency,
}

/// Recommended execution strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// Single agent handles everything
    SingleAgent,
    /// Multiple agents in sequence
    SequentialMultiAgent,
    /// Parallel execution where possible
    ParallelExecution,
    /// Collaborative approach with coordination
    Collaborative,
    /// Hybrid approach based on subtask analysis
    Hybrid,
}

/// Routing decision for a subtask
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Subtask being routed
    pub subtask_id: Uuid,
    /// Chosen agent
    pub assigned_agent_id: Uuid,
    /// Agent name
    pub assigned_agent_name: String,
    /// Routing confidence (0-1)
    pub confidence: f32,
    /// Routing reason
    pub routing_reason: RoutingReason,
    /// Estimated execution time
    pub estimated_execution_time_ms: u64,
    /// Required resources
    pub required_resources: Vec<String>,
    /// Backup agents (if primary fails)
    pub backup_agents: Vec<(Uuid, String, f32)>,
}

/// Reason for routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingReason {
    /// Best capability match
    CapabilityMatch,
    /// Highest performance rating
    PerformanceRating,
    /// Specialized expertise
    Specialization,
    /// Availability and load balancing
    LoadBalancing,
    /// Previous successful collaborations
    CollaborationHistory,
    /// User preference
    UserPreference,
    /// Cost optimization
    CostOptimization,
}

/// Task queued for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedTask {
    /// Task ID
    pub id: Uuid,
    /// Associated subtask
    pub subtask: Subtask,
    /// Routing decision
    pub routing: RoutingDecision,
    /// Queue position
    pub queue_position: usize,
    /// Queued timestamp
    pub queued_at: DateTime<Utc>,
    /// Estimated start time
    pub estimated_start_time: Option<DateTime<Utc>>,
    /// Task status
    pub status: QueuedTaskStatus,
}

/// Status of a queued task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueuedTaskStatus {
    /// Waiting in queue
    Queued,
    /// Assigned to agent, waiting to start
    Assigned,
    /// Currently executing
    Executing,
    /// Paused
    Paused,
    /// Cancelled
    Cancelled,
}

/// Result of task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    /// Task ID
    pub task_id: Uuid,
    /// Agent that executed the task
    pub executing_agent_id: Uuid,
    /// Execution success status
    pub success: bool,
    /// Result content
    pub result_content: String,
    /// Execution metrics
    pub metrics: ExecutionMetrics,
    /// Artifacts produced
    pub artifacts: Vec<TaskArtifact>,
    /// Error information (if failed)
    pub error: Option<TaskError>,
    /// Agent feedback
    pub agent_feedback: Option<String>,
}

/// Execution metrics for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: DateTime<Utc>,
    /// Total execution time in milliseconds
    pub execution_time_ms: u64,
    /// CPU usage (if available)
    pub cpu_usage_percent: Option<f32>,
    /// Memory usage (if available)
    pub memory_usage_mb: Option<f32>,
    /// Number of tool calls made
    pub tool_calls_count: usize,
    /// Tokens processed (for LLM tasks)
    pub tokens_processed: Option<u32>,
    /// Agent confidence in result
    pub confidence_score: f32,
}

/// Artifact produced during task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskArtifact {
    /// Artifact name
    pub name: String,
    /// Artifact type
    pub artifact_type: ArtifactType,
    /// Content or reference
    pub content: String,
    /// File path (if saved to disk)
    pub file_path: Option<String>,
    /// Size in bytes (if applicable)
    pub size_bytes: Option<u64>,
}

/// Type of artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    Text,
    Code,
    Image,
    Document,
    Data,
    Model,
    Configuration,
    Log,
}

/// Error information for failed tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskError {
    /// Error type
    pub error_type: ErrorType,
    /// Error message
    pub message: String,
    /// Stack trace (if available)
    pub stack_trace: Option<String>,
    /// Error context
    pub context: HashMap<String, String>,
    /// Whether error is recoverable
    pub recoverable: bool,
}

/// Type of error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    /// Agent execution error
    AgentExecution,
    /// Tool failure
    ToolFailure,
    /// Network error
    NetworkError,
    /// Authentication error
    AuthenticationError,
    /// Resource unavailable
    ResourceUnavailable,
    /// Timeout error
    Timeout,
    /// Invalid input
    InvalidInput,
    /// System error
    SystemError,
}

/// Final result of processed request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Original request ID
    pub request_id: Uuid,
    /// Overall success status
    pub success: bool,
    /// Primary result content
    pub content: String,
    /// Individual subtask results
    pub subtask_results: Vec<TaskExecutionResult>,
    /// Summary of execution
    pub execution_summary: ExecutionSummary,
    /// Recommendations to user
    pub recommendations: Vec<String>,
    /// Follow-up suggestions
    pub follow_up_suggestions: Vec<String>,
    /// Completion timestamp
    pub completed_at: DateTime<Utc>,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
}

/// Summary of execution process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// Total subtasks
    pub total_subtasks: usize,
    /// Successfully completed subtasks
    pub successful_subtasks: usize,
    /// Failed subtasks
    pub failed_subtasks: usize,
    /// Agents involved
    pub agents_involved: Vec<String>,
    /// Tools used
    pub tools_used: Vec<String>,
    /// Collaboration sessions
    pub collaboration_sessions: usize,
    /// Total cost (if applicable)
    pub total_cost: Option<f32>,
}

/// System status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Queue statistics
    pub queue_stats: QueueStats,
    /// Performance metrics
    pub performance_metrics: SystemPerformanceMetrics,
    /// Number of active tasks
    pub active_tasks: usize,
    /// Total tasks processed
    pub total_processed: u64,
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Tasks waiting in queue
    pub queued_tasks: usize,
    /// Tasks currently executing
    pub executing_tasks: usize,
    /// Tasks completed
    pub completed_tasks: usize,
    /// Tasks failed
    pub failed_tasks: usize,
    /// Average wait time in milliseconds
    pub avg_wait_time_ms: u64,
    /// Queue utilization percentage
    pub queue_utilization_percent: f32,
}

/// System performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPerformanceMetrics {
    /// Tasks per minute
    pub tasks_per_minute: f32,
    /// Success rate (0-1)
    pub success_rate: f32,
    /// Average task duration in milliseconds
    pub avg_task_duration_ms: u64,
    /// Agent utilization rates
    pub agent_utilization: HashMap<String, f32>,
    /// System load
    pub system_load: f32,
}

/// Task history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHistoryEntry {
    /// Request ID
    pub request_id: Uuid,
    /// User ID
    pub user_id: String,
    /// Request summary
    pub request_summary: String,
    /// Success status
    pub success: bool,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Agents involved
    pub agents_involved: Vec<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Routing analytics information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingAnalytics {
    /// Total routing decisions made
    pub total_decisions: u64,
    /// Routing accuracy (success rate of routing decisions)
    pub routing_accuracy: f32,
    /// Most commonly routed agents
    pub top_agents: Vec<(String, u64)>,
    /// Routing patterns by task type
    pub routing_patterns: HashMap<String, Vec<(String, f32)>>,
    /// Performance trends
    pub performance_trends: Vec<PerformanceTrend>,
    /// Recommendations for optimization
    pub optimization_recommendations: Vec<String>,
}

/// Performance trend data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Success rate at this point
    pub success_rate: f32,
    /// Average execution time
    pub avg_execution_time_ms: u64,
    /// Number of tasks in this period
    pub task_count: u64,
}