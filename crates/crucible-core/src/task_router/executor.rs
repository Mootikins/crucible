use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex, Semaphore};
use uuid::Uuid;

use super::types::*;
use crate::agent::{AgentDefinition, AgentRegistry};

/// Execution Engine - coordinates task execution across multiple agents
#[derive(Debug)]
pub struct ExecutionEngine {
    /// Available agents
    agents: Arc<RwLock<AgentRegistry>>,
    /// Currently executing tasks
    active_tasks: Arc<RwLock<HashMap<Uuid, ActiveTask>>>,
    /// Agent capacity management
    agent_capacity: Arc<RwLock<HashMap<Uuid, AgentCapacity>>>,
    /// Execution semaphore for concurrency control
    execution_semaphore: Arc<Semaphore>,
    /// Task execution statistics
    execution_stats: Arc<RwLock<ExecutionStatistics>>,
    /// Configuration
    config: ExecutionConfig,
    /// Collaboration coordinator
    collaboration_coordinator: Arc<CollaborationCoordinator>,
}

/// Currently active task execution
#[derive(Debug, Clone)]
struct ActiveTask {
    /// Task ID
    task_id: Uuid,
    /// Assigned agent
    agent_id: Uuid,
    /// Task status
    status: TaskExecutionStatus,
    /// Start time
    start_time: chrono::DateTime<chrono::Utc>,
    /// Current progress (0-100)
    progress: u8,
    /// Checkpoint data
    checkpoint: Option<TaskCheckpoint>,
    /// Execution log
    execution_log: Vec<ExecutionLogEntry>,
    /// Estimated completion time
    estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
    /// Current operation
    current_operation: Option<String>,
}

/// Task execution status
#[derive(Debug, Clone, PartialEq)]
enum TaskExecutionStatus {
    /// Initializing task
    Initializing,
    /// Task is running
    Running,
    /// Task is paused
    Paused,
    /// Task is waiting for something
    Waiting,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Agent capacity tracking
#[derive(Debug, Clone, Default)]
struct AgentCapacity {
    /// Number of currently active tasks
    active_tasks: u8,
    /// Maximum concurrent tasks allowed
    max_concurrent: u8,
    /// Agent availability status
    availability: AgentAvailability,
    /// Last activity timestamp
    last_activity: chrono::DateTime<chrono::Utc>,
    /// Current load percentage (0-100)
    current_load: f32,
}

/// Agent availability status
#[derive(Debug, Clone, PartialEq, Default)]
enum AgentAvailability {
    /// Available for new tasks
    #[default]
    Available,
    /// Busy but can accept urgent tasks
    Busy,
    /// At full capacity
    Full,
    /// Unavailable (offline or error)
    Unavailable,
}

/// Execution statistics
#[derive(Debug, Clone, Default)]
struct ExecutionStatistics {
    /// Total tasks executed
    total_executed: u64,
    /// Successful executions
    successful_executions: u64,
    /// Failed executions
    failed_executions: u64,
    /// Average execution time in milliseconds
    avg_execution_time_ms: f64,
    /// Agent performance data
    agent_performance: HashMap<Uuid, AgentPerformance>,
    /// Task type performance data
    task_type_performance: HashMap<String, TaskTypePerformance>,
}

/// Performance data for an agent
#[derive(Debug, Clone, Default)]
struct AgentPerformance {
    /// Tasks completed
    tasks_completed: u64,
    /// Success rate
    success_rate: f32,
    /// Average execution time
    avg_execution_time_ms: f64,
    /// Specialization scores by capability
    specialization_scores: HashMap<String, f32>,
    /// Last updated
    last_updated: chrono::DateTime<chrono::Utc>,
}

/// Performance data for task types
#[derive(Debug, Clone, Default)]
struct TaskTypePerformance {
    /// Total executions
    total_executions: u64,
    /// Success rate
    success_rate: f32,
    /// Average execution time
    avg_execution_time_ms: f64,
    /// Best performing agents
    best_agents: Vec<(Uuid, f32)>, // (agent_id, success_rate)
}

/// Execution configuration
#[derive(Debug, Clone)]
struct ExecutionConfig {
    /// Maximum concurrent executions
    max_concurrent_executions: usize,
    /// Task timeout in milliseconds
    task_timeout_ms: u64,
    /// Heartbeat interval in seconds
    heartbeat_interval_secs: u64,
    /// Progress reporting interval in seconds
    progress_report_interval_secs: u64,
    /// Maximum retry attempts
    max_retries: u8,
    /// Enable task checkpoints
    enable_checkpoints: bool,
    /// Checkpoint interval in seconds
    checkpoint_interval_secs: u64,
}

/// Execution log entry
#[derive(Debug, Clone)]
struct ExecutionLogEntry {
    /// Timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Log level
    level: LogLevel,
    /// Message
    message: String,
    /// Additional data
    data: Option<serde_json::Value>,
}

/// Log level
#[derive(Debug, Clone)]
enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Task checkpoint
#[derive(Debug, Clone)]
struct TaskCheckpoint {
    /// Checkpoint timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Progress percentage
    progress: u8,
    /// Checkpoint data
    data: serde_json::Value,
    /// Last completed operation
    last_operation: String,
}

/// Collaboration coordinator for multi-agent tasks
#[derive(Debug)]
struct CollaborationCoordinator {
    /// Active collaboration sessions
    active_sessions: Arc<RwLock<HashMap<Uuid, CollaborationSession>>>,
    /// Agent communication channels
    communication_channels: HashMap<Uuid, AgentChannel>,
}

/// Active collaboration session
#[derive(Debug, Clone)]
struct CollaborationSession {
    /// Session ID
    session_id: Uuid,
    /// Participating agents
    participants: Vec<CollaborationParticipant>,
    /// Current state
    state: CollaborationState,
    /// Shared context
    shared_context: HashMap<String, serde_json::Value>,
    /// Communication history
    communication_history: Vec<CollaborationMessage>,
}

/// Collaboration participant
#[derive(Debug, Clone)]
struct CollaborationParticipant {
    /// Agent ID
    agent_id: Uuid,
    /// Role in collaboration
    role: CollaborationRole,
    /// Current status
    status: ParticipantStatus,
    /// Assigned tasks
    assigned_tasks: Vec<Uuid>,
}

/// Agent communication channel
#[derive(Debug)]
struct AgentChannel {
    /// Channel ID
    channel_id: Uuid,
    /// Agent ID
    agent_id: Uuid,
    /// Message queue
    message_queue: Arc<Mutex<Vec<CollaborationMessage>>>,
}

/// Collaboration role
#[derive(Debug, Clone)]
enum CollaborationRole {
    Coordinator,
    Specialist,
    Reviewer,
    Implementer,
}

/// Collaboration state
#[derive(Debug, Clone)]
enum CollaborationState {
    Initializing,
    Active,
    Coordinating,
    Reviewing,
    Finalizing,
    Completed,
    Failed,
}

/// Participant status
#[derive(Debug, Clone)]
enum ParticipantStatus {
    Active,
    Waiting,
    Completed,
    Blocked,
}

/// Collaboration message
#[derive(Debug, Clone)]
struct CollaborationMessage {
    /// Message ID
    message_id: Uuid,
    /// Sender
    sender_id: Uuid,
    /// Recipient (None for broadcast)
    recipient_id: Option<Uuid>,
    /// Message type
    message_type: CollaborationMessageType,
    /// Content
    content: String,
    /// Timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Collaboration message type
#[derive(Debug, Clone)]
enum CollaborationMessageType {
    Data,
    Request,
    Response,
    Status,
    Error,
}

impl ExecutionEngine {
    /// Create a new execution engine
    pub fn new() -> Self {
        let config = ExecutionConfig::default();

        Self {
            agents: Arc::new(RwLock::new(AgentRegistry::default())),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            agent_capacity: Arc::new(RwLock::new(HashMap::new())),
            execution_semaphore: Arc::new(Semaphore::new(config.max_concurrent_executions)),
            execution_stats: Arc::new(RwLock::new(ExecutionStatistics::default())),
            config,
            collaboration_coordinator: Arc::new(CollaborationCoordinator::new()),
        }
    }

    /// Execute a queued task
    pub async fn execute_task(&self, queued_task: QueuedTask) -> Result<TaskExecutionResult> {
        let task_id = queued_task.id;

        // Acquire execution permit
        let _permit = self.execution_semaphore.acquire().await
            .map_err(|_| anyhow::anyhow!("Execution engine is at capacity"))?;

        // Initialize task execution
        let active_task = self.initialize_task_execution(&queued_task).await?;

        // Check if this requires collaboration
        if self.requires_collaboration(&queued_task).await? {
            return self.execute_collaborative_task(queued_task, active_task).await;
        }

        // Execute single-agent task
        self.execute_single_agent_task(queued_task, active_task).await
    }

    /// Initialize task execution
    async fn initialize_task_execution(&self, queued_task: &QueuedTask) -> Result<ActiveTask> {
        let agent_id = queued_task.routing.assigned_agent_id;

        // Check agent availability
        if !self.is_agent_available(&agent_id).await? {
            return Err(anyhow::anyhow!("Agent {} is not available", agent_id));
        }

        // Update agent capacity
        {
            let mut capacity = self.agent_capacity.write().await;
            let agent_cap = capacity.entry(agent_id).or_insert_with(AgentCapacity::default);
            agent_cap.active_tasks += 1;
            agent_cap.last_activity = Utc::now();
            agent_cap.current_load = (agent_cap.active_tasks as f32 / agent_cap.max_concurrent as f32) * 100.0;
        }

        let active_task = ActiveTask {
            task_id: queued_task.id,
            agent_id,
            status: TaskExecutionStatus::Initializing,
            start_time: Utc::now(),
            progress: 0,
            checkpoint: None,
            execution_log: vec![],
            estimated_completion: Some(Utc::now() + chrono::Duration::milliseconds(queued_task.routing.estimated_execution_time_ms as i64)),
            current_operation: Some("Initializing task".to_string()),
        };

        // Add to active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(queued_task.id, active_task.clone());
        }

        tracing::info!("Task {} initialized for execution by agent {}", queued_task.id, agent_id);
        Ok(active_task)
    }

    /// Check if agent is available
    async fn is_agent_available(&self, agent_id: &Uuid) -> Result<bool> {
        let capacity = self.agent_capacity.read().await;

        match capacity.get(agent_id) {
            Some(cap) => {
                Ok(cap.active_tasks < cap.max_concurrent &&
                   cap.availability != AgentAvailability::Unavailable)
            }
            None => {
                // New agent, assume available with default capacity
                Ok(true)
            }
        }
    }

    /// Check if task requires collaboration
    async fn requires_collaboration(&self, queued_task: &QueuedTask) -> Result<bool> {
        // Simple heuristic: if task has complex requirements or multiple capabilities
        let complex_requirements = queued_task.subtask.required_capabilities.len() > 2;
        let long_duration = queued_task.routing.estimated_execution_time_ms > 300000; // 5 minutes

        Ok(complex_requirements || long_duration)
    }

    /// Execute single-agent task
    async fn execute_single_agent_task(&self, queued_task: QueuedTask,
                                     mut active_task: ActiveTask) -> Result<TaskExecutionResult> {
        let task_id = queued_task.id;
        let agent_id = queued_task.routing.assigned_agent_id;
        let start_time = Utc::now();

        // Update task status to running
        {
            let mut active_tasks = self.active_tasks.write().await;
            if let Some(task) = active_tasks.get_mut(&task_id) {
                task.status = TaskExecutionStatus::Running;
                task.current_operation = Some("Executing task".to_string());
            }
        }

        // Simulate task execution with progress updates
        let execution_result = self.simulate_task_execution(&queued_task, &mut active_task).await;

        // Record final result
        let end_time = Utc::now();
        let execution_time_ms = end_time.signed_duration_since(start_time).num_milliseconds() as u64;

        let result = match execution_result {
            Ok(content) => {
                let success = true;
                self.record_task_completion(&task_id, &agent_id, success, execution_time_ms).await;

                TaskExecutionResult {
                    task_id,
                    executing_agent_id: agent_id,
                    success,
                    result_content: content,
                    metrics: ExecutionMetrics {
                        start_time,
                        end_time,
                        execution_time_ms,
                        cpu_usage_percent: None,
                        memory_usage_mb: None,
                        tool_calls_count: 0,
                        tokens_processed: None,
                        confidence_score: 0.8,
                    },
                    artifacts: vec![],
                    error: None,
                    agent_feedback: Some("Task completed successfully".to_string()),
                }
            }
            Err(error) => {
                let success = false;
                self.record_task_completion(&task_id, &agent_id, success, execution_time_ms).await;

                TaskExecutionResult {
                    task_id,
                    executing_agent_id: agent_id,
                    success,
                    result_content: String::new(),
                    metrics: ExecutionMetrics {
                        start_time,
                        end_time,
                        execution_time_ms,
                        cpu_usage_percent: None,
                        memory_usage_mb: None,
                        tool_calls_count: 0,
                        tokens_processed: None,
                        confidence_score: 0.0,
                    },
                    artifacts: vec![],
                    error: Some(TaskError {
                        error_type: ErrorType::AgentExecution,
                        message: error.to_string(),
                        stack_trace: None,
                        context: HashMap::new(),
                        recoverable: false,
                    }),
                    agent_feedback: None,
                }
            }
        };

        // Clean up active task
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.remove(&task_id);
        }

        // Update agent capacity
        {
            let mut capacity = self.agent_capacity.write().await;
            if let Some(agent_cap) = capacity.get_mut(&agent_id) {
                agent_cap.active_tasks = agent_cap.active_tasks.saturating_sub(1);
                agent_cap.current_load = (agent_cap.active_tasks as f32 / agent_cap.max_concurrent as f32) * 100.0;
                agent_cap.last_activity = Utc::now();
            }
        }

        Ok(result)
    }

    /// Simulate task execution with progress updates
    async fn simulate_task_execution(&self, queued_task: &QueuedTask,
                                   active_task: &mut ActiveTask) -> Result<String> {
        let duration_ms = queued_task.routing.estimated_execution_time_ms;
        let steps = 10;
        let step_duration = duration_ms / steps;

        for step in 1..=steps {
            // Update progress
            active_task.progress = (step * 100 / steps) as u8;
            active_task.current_operation = Some(format!("Executing step {} of {}", step, steps));

            // Log progress
            self.log_task_progress(&active_task.task_id, step, steps).await;

            // Create checkpoint if enabled and this is a checkpoint step
            if self.config.enable_checkpoints && step % 3 == 0 {
                self.create_task_checkpoint(active_task).await;
            }

            // Simulate work
            tokio::time::sleep(tokio::time::Duration::from_millis(step_duration)).await;

            // Check for cancellation
            if self.is_task_cancelled(&active_task.task_id).await? {
                return Err(anyhow::anyhow!("Task was cancelled"));
            }
        }

        // Generate result based on task type
        let result = self.generate_task_result(&queued_task.subtask).await;
        Ok(result)
    }

    /// Log task progress
    async fn log_task_progress(&self, task_id: &Uuid, current_step: usize, total_steps: usize) {
        let log_entry = ExecutionLogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: format!("Progress: {}/{} steps completed", current_step, total_steps),
            data: Some(serde_json::json!({
                "current_step": current_step,
                "total_steps": total_steps,
                "progress_percent": (current_step * 100) / total_steps
            })),
        };

        // Add to task execution log
        {
            let mut active_tasks = self.active_tasks.write().await;
            if let Some(task) = active_tasks.get_mut(task_id) {
                task.execution_log.push(log_entry);
            }
        }
    }

    /// Create task checkpoint
    async fn create_task_checkpoint(&self, active_task: &ActiveTask) {
        let checkpoint = TaskCheckpoint {
            timestamp: Utc::now(),
            progress: active_task.progress,
            data: serde_json::json!({
                "current_operation": active_task.current_operation,
                "execution_log_length": active_task.execution_log.len()
            }),
            last_operation: active_task.current_operation.clone().unwrap_or_default(),
        };

        // Update active task with checkpoint
        {
            let mut active_tasks = self.active_tasks.write().await;
            if let Some(task) = active_tasks.get_mut(&active_task.task_id) {
                task.checkpoint = Some(checkpoint);
            }
        }

        tracing::debug!("Created checkpoint for task {}", active_task.task_id);
    }

    /// Check if task is cancelled
    async fn is_task_cancelled(&self, task_id: &Uuid) -> Result<bool> {
        let active_tasks = self.active_tasks.read().await;
        if let Some(task) = active_tasks.get(task_id) {
            Ok(task.status == TaskExecutionStatus::Cancelled)
        } else {
            Ok(true) // Task not found, assume cancelled
        }
    }

    /// Generate task result based on subtask type
    async fn generate_task_result(&self, subtask: &Subtask) -> String {
        match subtask.subtask_type {
            SubtaskType::Research => {
                format!("Research completed for: {}. Key findings and relevant information have been gathered.",
                       subtask.description)
            }
            SubtaskType::CodeGeneration => {
                format!("Code generated for: {}. Implementation follows best practices and includes appropriate documentation.",
                       subtask.description)
            }
            SubtaskType::Analysis => {
                format!("Analysis completed for: {}. Detailed insights and recommendations have been provided.",
                       subtask.description)
            }
            SubtaskType::Writing => {
                format!("Content created for: {}. Document is well-structured and meets the specified requirements.",
                       subtask.description)
            }
            _ => {
                format!("Task completed successfully: {}", subtask.description)
            }
        }
    }

    /// Execute collaborative task
    async fn execute_collaborative_task(&self, queued_task: QueuedTask,
                                      active_task: ActiveTask) -> Result<TaskExecutionResult> {
        // For now, delegate to single-agent execution
        // In a full implementation, this would coordinate multiple agents
        tracing::info!("Collaborative task execution requested, using single-agent approach");
        self.execute_single_agent_task(queued_task, active_task).await
    }

    /// Cancel a task
    pub async fn cancel_task(&self, task_id: &Uuid) -> Result<bool> {
        let mut active_tasks = self.active_tasks.write().await;

        if let Some(task) = active_tasks.get_mut(task_id) {
            task.status = TaskExecutionStatus::Cancelled;
            tracing::info!("Task {} cancelled", task_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Record task completion in statistics
    async fn record_task_completion(&self, task_id: &Uuid, agent_id: &Uuid,
                                  success: bool, execution_time_ms: u64) {
        let mut stats = self.execution_stats.write().await;

        stats.total_executed += 1;

        if success {
            stats.successful_executions += 1;
        } else {
            stats.failed_executions += 1;
        }

        // Update agent performance
        let agent_perf = stats.agent_performance.entry(*agent_id)
            .or_insert_with(AgentPerformance::default);

        agent_perf.tasks_completed += 1;
        agent_perf.last_updated = Utc::now();

        // Update average execution time
        if agent_perf.avg_execution_time_ms == 0.0 {
            agent_perf.avg_execution_time_ms = execution_time_ms as f64;
        } else {
            agent_perf.avg_execution_time_ms =
                agent_perf.avg_execution_time_ms * 0.8 + execution_time_ms as f64 * 0.2;
        }

        // Update success rate
        agent_perf.success_rate = stats.successful_executions as f32 / stats.total_executed as f32;

        // Update overall average execution time
        if stats.avg_execution_time_ms == 0.0 {
            stats.avg_execution_time_ms = execution_time_ms as f64;
        } else {
            stats.avg_execution_time_ms =
                stats.avg_execution_time_ms * 0.9 + execution_time_ms as f64 * 0.1;
        }
    }

    /// Get active task count
    pub async fn get_active_task_count(&self) -> usize {
        self.active_tasks.read().await.len()
    }

    /// Get execution statistics
    pub async fn get_execution_stats(&self) -> Result<ExecutionStatistics> {
        Ok(self.execution_stats.read().await.clone())
    }

    /// Get task information
    pub async fn get_task_info(&self, task_id: &Uuid) -> Result<Option<TaskExecutionInfo>> {
        let active_tasks = self.active_tasks.read().await;

        if let Some(task) = active_tasks.get(task_id) {
            Ok(Some(TaskExecutionInfo {
                task_id: *task_id,
                agent_id: task.agent_id,
                status: task.status.clone(),
                progress: task.progress,
                start_time: task.start_time,
                estimated_completion: task.estimated_completion,
                current_operation: task.current_operation.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Pause task execution
    pub async fn pause_task(&self, task_id: &Uuid) -> Result<bool> {
        let mut active_tasks = self.active_tasks.write().await;

        if let Some(task) = active_tasks.get_mut(task_id) {
            if task.status == TaskExecutionStatus::Running {
                task.status = TaskExecutionStatus::Paused;
                task.current_operation = Some("Task paused".to_string());
                tracing::info!("Task {} paused", task_id);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Resume task execution
    pub async fn resume_task(&self, task_id: &Uuid) -> Result<bool> {
        let mut active_tasks = self.active_tasks.write().await;

        if let Some(task) = active_tasks.get_mut(task_id) {
            if task.status == TaskExecutionStatus::Paused {
                task.status = TaskExecutionStatus::Running;
                task.current_operation = Some("Task resumed".to_string());
                tracing::info!("Task {} resumed", task_id);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Set agent registry
    pub async fn set_agent_registry(&self, registry: AgentRegistry) {
        let mut agents = self.agents.write().await;
        *agents = registry;
    }
}

/// Task execution information for queries
#[derive(Debug, Clone)]
pub struct TaskExecutionInfo {
    /// Task ID
    pub task_id: Uuid,
    /// Agent ID
    pub agent_id: Uuid,
    /// Execution status
    pub status: TaskExecutionStatus,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Estimated completion time
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
    /// Current operation
    pub current_operation: Option<String>,
}

impl CollaborationCoordinator {
    /// Create a new collaboration coordinator
    fn new() -> Self {
        Self {
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            communication_channels: HashMap::new(),
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_executions: 10,
            task_timeout_ms: 30 * 60 * 1000, // 30 minutes
            heartbeat_interval_secs: 30,
            progress_report_interval_secs: 10,
            max_retries: 3,
            enable_checkpoints: true,
            checkpoint_interval_secs: 60,
        }
    }
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}