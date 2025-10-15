use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

use super::types::*;

/// Task Queue Manager - manages task prioritization, queuing, and scheduling
#[derive(Debug)]
pub struct TaskQueueManager {
    /// Priority queues for different priority levels
    priority_queues: HashMap<TaskPriority, Arc<Mutex<BinaryHeap<PriorityTask>>>>,
    /// Tasks waiting for dependencies
    dependency_waitlist: Arc<RwLock<HashMap<Uuid, WaitingTask>>>,
    /// Currently executing tasks
    executing_tasks: Arc<RwLock<HashMap<Uuid, ExecutingTask>>>,
    /// Queue statistics
    stats: Arc<RwLock<QueueStatistics>>,
    /// Configuration
    config: QueueConfig,
    /// Task completion notifications
    completion_callbacks: Arc<RwLock<HashMap<Uuid, Vec<CompletionCallback>>>>,
}

/// Task with priority information for heap ordering
#[derive(Debug, Clone)]
struct PriorityTask {
    /// Task ID
    id: Uuid,
    /// Priority level
    priority: TaskPriority,
    /// Creation timestamp
    created_at: DateTime<Utc>,
    /// Estimated duration
    estimated_duration_ms: u64,
    /// Deadline (if any)
    deadline: Option<DateTime<Utc>>,
    /// Number of retries
    retries: u8,
    /// The actual queued task
    task: QueuedTask,
}

/// Task waiting for dependencies to be resolved
#[derive(Debug, Clone)]
struct WaitingTask {
    /// Task being waited for
    task: PriorityTask,
    /// Dependencies that must complete first
    waiting_for: HashSet<Uuid>,
    /// When this task started waiting
    wait_start_time: DateTime<Utc>,
}

/// Currently executing task
#[derive(Debug, Clone)]
struct ExecutingTask {
    /// Task being executed
    task: PriorityTask,
    /// Agent executing the task
    agent_id: Uuid,
    /// Start time
    start_time: DateTime<Utc>,
    /// Checkpoint data (for recovery)
    checkpoint: Option<TaskCheckpoint>,
}

/// Checkpoint data for task recovery
#[derive(Debug, Clone)]
struct TaskCheckpoint {
    /// Checkpoint timestamp
    timestamp: DateTime<Utc>,
    /// Progress percentage (0-100)
    progress_percent: u8,
    /// Checkpoint data
    data: serde_json::Value,
    /// Last successful operation
    last_operation: String,
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
struct QueueStatistics {
    /// Total tasks queued
    total_queued: u64,
    /// Total tasks completed
    total_completed: u64,
    /// Total tasks failed
    total_failed: u64,
    /// Total wait time (cumulative)
    total_wait_time_ms: u64,
    /// Total execution time (cumulative)
    total_execution_time_ms: u64,
    /// Tasks by priority
    tasks_by_priority: HashMap<TaskPriority, u64>,
    /// Average wait time by priority
    avg_wait_time_by_priority: HashMap<TaskPriority, u64>,
    /// Queue length history (for monitoring)
    queue_length_history: Vec<(DateTime<Utc>, usize)>,
}

/// Queue configuration
#[derive(Debug, Clone)]
struct QueueConfig {
    /// Maximum concurrent tasks
    max_concurrent_tasks: usize,
    /// Maximum tasks per priority level
    max_tasks_per_priority: HashMap<TaskPriority, usize>,
    /// Task timeout in milliseconds
    task_timeout_ms: u64,
    /// Maximum retry attempts
    max_retries: u8,
    /// Queue cleanup interval in minutes
    cleanup_interval_minutes: u32,
    /// Maximum wait time for dependencies
    max_dependency_wait_time_ms: u64,
}

/// Completion callback for task dependencies
#[derive(Debug, Clone)]
struct CompletionCallback {
    /// Task waiting for completion
    waiting_task_id: Uuid,
    /// Callback function identifier
    callback_id: String,
}

impl TaskQueueManager {
    /// Create a new task queue manager
    pub fn new() -> Self {
        let config = QueueConfig::default();
        let mut priority_queues = HashMap::new();

        // Initialize priority queues
        for priority in [TaskPriority::Low, TaskPriority::Normal, TaskPriority::High,
                        TaskPriority::Critical, TaskPriority::Emergency] {
            priority_queues.insert(priority, Arc::new(Mutex::new(BinaryHeap::new())));
        }

        Self {
            priority_queues,
            dependency_waitlist: Arc::new(RwLock::new(HashMap::new())),
            executing_tasks: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(QueueStatistics::default())),
            config,
            completion_callbacks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Enqueue a task for execution
    pub async fn enqueue_task(&mut self, routing_decision: RoutingDecision) -> Result<QueuedTask> {
        let task_id = Uuid::new_v4();
        let now = Utc::now();

        let queued_task = QueuedTask {
            id: task_id,
            subtask: Subtask {
                id: task_id,
                description: format!("Task for agent: {}", routing_decision.assigned_agent_name),
                subtask_type: SubtaskType::Analysis, // Default
                required_capabilities: routing_decision.required_resources.clone(),
                required_tools: routing_decision.required_resources.clone(),
                estimated_duration_minutes: (routing_decision.estimated_execution_time_ms / 60000) as u32,
                priority: TaskPriority::Normal, // Default
                can_parallelize: true,
                input_requirements: Vec::new(),
                expected_output: "Task completion".to_string(),
            },
            routing: routing_decision.clone(),
            queue_position: 0, // Will be updated
            queued_at: now,
            estimated_start_time: Some(now + Duration::minutes(5)), // Estimate
            status: QueuedTaskStatus::Queued,
        };

        let priority_task = PriorityTask {
            id: task_id,
            priority: queued_task.subtask.priority.clone(),
            created_at: now,
            estimated_duration_ms: routing_decision.estimated_execution_time_ms,
            deadline: None, // Could be extracted from request
            retries: 0,
            task: queued_task.clone(),
        };

        // Add to appropriate priority queue
        let priority_queue = self.priority_queues.get(&priority_task.priority)
            .ok_or_else(|| anyhow::anyhow!("Invalid priority level"))?;

        {
            let mut queue = priority_queue.lock().await;
            queue.push(priority_task.clone());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_queued += 1;
            *stats.tasks_by_priority.entry(priority_task.priority.clone()).or_insert(0) += 1;

            // Record queue length
            stats.queue_length_history.push((now, self.get_total_queue_size().await));
            if stats.queue_length_history.len() > 1000 {
                stats.queue_length_history.remove(0);
            }
        }

        tracing::info!("Task {} queued for agent {}", task_id, routing_decision.assigned_agent_name);
        Ok(queued_task)
    }

    /// Get the next task to execute
    pub async fn get_next_task(&self) -> Result<Option<QueuedTask>> {
        // Check current execution limit
        {
            let executing = self.executing_tasks.read().await;
            if executing.len() >= self.config.max_concurrent_tasks {
                return Ok(None);
            }
        }

        // Check queues in priority order
        let priorities = [
            TaskPriority::Emergency,
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
        ];

        for priority in priorities {
            if let Some(task) = self.get_next_from_priority_queue(&priority).await? {
                return Ok(Some(task));
            }
        }

        Ok(None)
    }

    /// Get next task from a specific priority queue
    async fn get_next_from_priority_queue(&self, priority: &TaskPriority) -> Result<Option<QueuedTask>> {
        let priority_queue = self.priority_queues.get(priority)
            .ok_or_else(|| anyhow::anyhow!("Invalid priority level"))?;

        let priority_task = {
            let mut queue = priority_queue.lock().await;
            queue.pop()
        };

        if let Some(mut priority_task) = priority_task {
            // Update queue position (simplified)
            priority_task.task.queue_position = 1;
            priority_task.task.status = QueuedTaskStatus::Assigned;

            // Move to executing tasks
            {
                let mut executing = self.executing_tasks.write().await;
                executing.insert(
                    priority_task.id,
                    ExecutingTask {
                        task: priority_task.clone(),
                        agent_id: priority_task.task.routing.assigned_agent_id,
                        start_time: Utc::now(),
                        checkpoint: None,
                    }
                );
            }

            return Ok(Some(priority_task.task));
        }

        Ok(None)
    }

    /// Mark a task as started
    pub async fn mark_task_started(&self, task_id: &Uuid) -> Result<()> {
        let mut executing = self.executing_tasks.write().await;
        if let Some(executing_task) = executing.get_mut(task_id) {
            executing_task.start_time = Utc::now();
        }
        Ok(())
    }

    /// Mark a task as completed
    pub async fn mark_task_completed(&self, task_id: &Uuid, result: &TaskExecutionResult) -> Result<()> {
        // Remove from executing tasks
        let priority_task = {
            let mut executing = self.executing_tasks.write().await;
            executing.remove(task_id).map(|et| et.task)
        };

        if let Some(task) = priority_task {
            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.total_completed += 1;

                let execution_time = result.metrics.execution_time_ms;
                let wait_time = result.metrics.start_time.signed_duration_since(task.created_at)
                    .num_milliseconds() as u64;

                stats.total_execution_time_ms += execution_time;
                stats.total_wait_time_ms += wait_time;

                // Update average wait time by priority
                let avg_wait = stats.avg_wait_time_by_priority.entry(task.priority.clone())
                    .or_insert(0);
                *avg_wait = (*avg_wait + wait_time) / 2; // Simple moving average
            }

            // Check for tasks waiting on this completion
            self.check_dependency_completions(task_id).await?;

            // Trigger completion callbacks
            self.trigger_completion_callbacks(task_id, result).await?;

            tracing::info!("Task {} completed successfully", task_id);
        }

        Ok(())
    }

    /// Mark a task as failed
    pub async fn mark_task_failed(&self, task_id: &Uuid, error: &TaskError) -> Result<()> {
        // Remove from executing tasks
        let priority_task = {
            let mut executing = self.executing_tasks.write().await;
            executing.remove(task_id).map(|et| et.task)
        };

        if let Some(mut task) = priority_task {
            // Check if we should retry
            if task.retries < self.config.max_retries && error.recoverable {
                task.retries += 1;
                task.task.status = QueuedTaskStatus::Queued;

                // Re-queue the task
                let priority_queue = self.priority_queues.get(&task.priority)
                    .ok_or_else(|| anyhow::anyhow!("Invalid priority level"))?;

                {
                    let mut queue = priority_queue.lock().await;
                    queue.push(task);
                }

                tracing::info!("Task {} re-queued (attempt {})", task_id, task.retries);
            } else {
                // Mark as permanently failed
                {
                    let mut stats = self.stats.write().await;
                    stats.total_failed += 1;
                }

                tracing::error!("Task {} failed permanently: {}", task_id, error.message);
            }
        }

        Ok(())
    }

    /// Cancel a task
    pub async fn cancel_task(&mut self, task_id: &Uuid) -> Result<bool> {
        // Check if task is in a queue
        for (_, priority_queue) in &self.priority_queues {
            let mut queue = priority_queue.lock().await;

            // Find and remove the task
            let original_len = queue.len();
            queue.retain(|task| task.id != *task_id);

            if queue.len() < original_len {
                return Ok(true);
            }
        }

        // Check if task is executing
        {
            let mut executing = self.executing_tasks.write().await;
            if executing.remove(task_id).is_some() {
                tracing::info!("Task {} cancelled during execution", task_id);
                return Ok(true);
            }
        }

        // Check if task is waiting for dependencies
        {
            let mut waiting = self.dependency_waitlist.write().await;
            if waiting.remove(task_id).is_some() {
                tracing::info!("Task {} cancelled from dependency waitlist", task_id);
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Add a task to dependency waitlist
    pub async fn add_to_dependency_waitlist(&self, task: PriorityTask, dependencies: Vec<Uuid>) -> Result<()> {
        let waiting_task = WaitingTask {
            task,
            waiting_for: dependencies.into_iter().collect(),
            wait_start_time: Utc::now(),
        };

        let mut waitlist = self.dependency_waitlist.write().await;
        waitlist.insert(waiting_task.task.id, waiting_task);

        Ok(())
    }

    /// Check for tasks that can be moved from waitlist to queue
    async fn check_dependency_completions(&self, completed_task_id: &Uuid) -> Result<()> {
        let mut waitlist = self.dependency_waitlist.write().await;
        let mut ready_tasks = Vec::new();

        // Find tasks waiting on the completed task
        waitlist.retain(|task_id, waiting_task| {
            waiting_task.waiting_for.remove(completed_task_id);

            if waiting_task.waiting_for.is_empty() {
                ready_tasks.push(waiting_task.task.clone());
                false // Remove from waitlist
            } else {
                true // Keep in waitlist
            }
        });

        // Move ready tasks to appropriate queues
        for task in ready_tasks {
            let priority_queue = self.priority_queues.get(&task.priority)
                .ok_or_else(|| anyhow::anyhow!("Invalid priority level"))?;

            {
                let mut queue = priority_queue.lock().await;
                queue.push(task);
            }

            tracing::debug!("Task {} moved from waitlist to queue", task.id);
        }

        Ok(())
    }

    /// Trigger completion callbacks
    async fn trigger_completion_callbacks(&self, task_id: &Uuid, result: &TaskExecutionResult) -> Result<()> {
        let mut callbacks = self.completion_callbacks.write().await;
        if let Some(task_callbacks) = callbacks.remove(task_id) {
            for callback in task_callbacks {
                // In a real implementation, this would trigger the callback
                tracing::debug!("Triggering callback {} for task {}", callback.callback_id, task_id);
            }
        }
        Ok(())
    }

    /// Get queue statistics
    pub async fn get_queue_stats(&self) -> Result<QueueStats> {
        let stats = self.stats.read().await;
        let executing = self.executing_tasks.read().await;

        let total_queued = self.get_total_queue_size().await;
        let executing_tasks = executing.len();

        let completed_tasks = stats.total_completed as usize;
        let failed_tasks = stats.total_failed as usize;

        let avg_wait_time_ms = if stats.total_completed > 0 {
            stats.total_wait_time_ms / stats.total_completed
        } else {
            0
        };

        let queue_utilization_percent = if self.config.max_concurrent_tasks > 0 {
            (executing_tasks as f32 / self.config.max_concurrent_tasks as f32) * 100.0
        } else {
            0.0
        };

        Ok(QueueStats {
            queued_tasks: total_queued,
            executing_tasks,
            completed_tasks,
            failed_tasks,
            avg_wait_time_ms,
            queue_utilization_percent,
        })
    }

    /// Get total queue size across all priorities
    async fn get_total_queue_size(&self) -> usize {
        let mut total = 0;
        for (_, priority_queue) in &self.priority_queues {
            let queue = priority_queue.lock().await;
            total += queue.len();
        }
        total
    }

    /// Get tasks by priority breakdown
    pub async fn get_priority_breakdown(&self) -> HashMap<TaskPriority, usize> {
        let mut breakdown = HashMap::new();

        for (priority, priority_queue) in &self.priority_queues {
            let queue = priority_queue.lock().await;
            breakdown.insert(priority.clone(), queue.len());
        }

        breakdown
    }

    /// Clean up old tasks and statistics
    pub async fn cleanup(&self) -> Result<()> {
        let cutoff_time = Utc::now() - Duration::minutes(self.config.cleanup_interval_minutes as i64);

        // Clean up old statistics
        {
            let mut stats = self.stats.write().await;
            stats.queue_length_history.retain(|(timestamp, _)| *timestamp > cutoff_time);
        }

        // Clean up timed out tasks from waitlist
        {
            let mut waitlist = self.dependency_waitlist.write().await;
            let timeout_threshold = Utc::now() - Duration::milliseconds(self.config.max_dependency_wait_time_ms as i64);

            waitlist.retain(|_, waiting_task| {
                waiting_task.wait_start_time > timeout_threshold
            });
        }

        tracing::debug!("Queue cleanup completed");
        Ok(())
    }

    /// Get task information by ID
    pub async fn get_task_info(&self, task_id: &Uuid) -> Result<Option<TaskInfo>> {
        // Check executing tasks
        {
            let executing = self.executing_tasks.read().await;
            if let Some(exec_task) = executing.get(task_id) {
                return Ok(Some(TaskInfo {
                    id: *task_id,
                    status: QueuedTaskStatus::Executing,
                    priority: exec_task.task.priority.clone(),
                    agent_id: Some(exec_task.agent_id),
                    start_time: Some(exec_task.start_time),
                    queue_position: None,
                }));
            }
        }

        // Check priority queues
        for (priority, priority_queue) in &self.priority_queues {
            let queue = priority_queue.lock().await;
            for (index, task) in queue.iter().enumerate() {
                if task.id == *task_id {
                    return Ok(Some(TaskInfo {
                        id: *task_id,
                        status: task.task.status.clone(),
                        priority: priority.clone(),
                        agent_id: Some(task.task.routing.assigned_agent_id),
                        start_time: None,
                        queue_position: Some(index + 1),
                    }));
                }
            }
        }

        // Check waitlist
        {
            let waitlist = self.dependency_waitlist.read().await;
            if let Some(waiting_task) = waitlist.get(task_id) {
                return Ok(Some(TaskInfo {
                    id: *task_id,
                    status: QueuedTaskStatus::Queued,
                    priority: waiting_task.task.priority.clone(),
                    agent_id: Some(waiting_task.task.routing.assigned_agent_id),
                    start_time: None,
                    queue_position: None,
                }));
            }
        }

        Ok(None)
    }

    /// Pause/resume task execution
    pub async fn set_pause_state(&self, paused: bool) -> Result<()> {
        // This would affect the execution of new tasks
        // Implementation depends on how the execution engine works
        tracing::info!("Task queue pause state set to: {}", paused);
        Ok(())
    }
}

/// Task information for queries
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Task ID
    pub id: Uuid,
    /// Current status
    pub status: QueuedTaskStatus,
    /// Priority level
    pub priority: TaskPriority,
    /// Assigned agent (if any)
    pub agent_id: Option<Uuid>,
    /// Start time (if executing)
    pub start_time: Option<DateTime<Utc>>,
    /// Position in queue (if queued)
    pub queue_position: Option<usize>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        let mut max_tasks_per_priority = HashMap::new();
        max_tasks_per_priority.insert(TaskPriority::Emergency, 5);
        max_tasks_per_priority.insert(TaskPriority::Critical, 10);
        max_tasks_per_priority.insert(TaskPriority::High, 20);
        max_tasks_per_priority.insert(TaskPriority::Normal, 50);
        max_tasks_per_priority.insert(TaskPriority::Low, 30);

        Self {
            max_concurrent_tasks: 10,
            max_tasks_per_priority,
            task_timeout_ms: 30 * 60 * 1000, // 30 minutes
            max_retries: 3,
            cleanup_interval_minutes: 60,
            max_dependency_wait_time_ms: 60 * 60 * 1000, // 1 hour
        }
    }
}

// Implement ordering for priority queue (reverse order for max-heap)
impl std::cmp::Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First by priority (higher first)
        match other.priority.cmp(&self.priority) {
            std::cmp::Ordering::Equal => {
                // Then by creation time (earlier first)
                self.created_at.cmp(&other.created_at)
            }
            other => other,
        }
    }
}

impl std::cmp::PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::cmp::Eq for PriorityTask {}

impl Default for TaskQueueManager {
    fn default() -> Self {
        Self::new()
    }
}