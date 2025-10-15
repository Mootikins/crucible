pub mod analyzer;
pub mod router;
pub mod queue;
pub mod executor;
pub mod aggregator;
pub mod error_handler;
pub mod monitor;
pub mod types;

#[cfg(test)]
mod tests;

pub use analyzer::TaskAnalyzer;
pub use router::IntelligentRouter;
pub use queue::TaskQueueManager;
pub use executor::ExecutionEngine;
pub use aggregator::ResultAggregator;
pub use error_handler::ErrorHandler;
pub use monitor::PerformanceMonitor;
pub use types::*;

use anyhow::Result;
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Comprehensive task routing system for agent orchestration
#[derive(Debug)]
pub struct TaskRouter {
    /// Task analysis engine
    analyzer: Arc<TaskAnalyzer>,
    /// Intelligent routing algorithm
    router: Arc<IntelligentRouter>,
    /// Task queue management
    queue_manager: Arc<RwLock<TaskQueueManager>>,
    /// Task execution engine
    executor: Arc<ExecutionEngine>,
    /// Result aggregation system
    aggregator: Arc<ResultAggregator>,
    /// Error handling and recovery
    error_handler: Arc<ErrorHandler>,
    /// Performance monitoring
    monitor: Arc<RwLock<PerformanceMonitor>>,
}

impl TaskRouter {
    /// Create a new task routing system
    pub fn new() -> Self {
        Self {
            analyzer: Arc::new(TaskAnalyzer::new()),
            router: Arc::new(IntelligentRouter::new()),
            queue_manager: Arc::new(RwLock::new(TaskQueueManager::new())),
            executor: Arc::new(ExecutionEngine::new()),
            aggregator: Arc::new(ResultAggregator::new()),
            error_handler: Arc::new(ErrorHandler::new()),
            monitor: Arc::new(RwLock::new(PerformanceMonitor::new())),
        }
    }

    /// Process a user request through the complete routing pipeline
    pub async fn process_request(&self, request: UserRequest) -> Result<TaskResult> {
        let start_time = std::time::Instant::now();

        // Step 1: Analyze the request
        let analysis = self.analyzer.analyze_request(&request).await?;

        // Step 2: Route tasks to appropriate agents
        let routing_decisions = self.router.route_tasks(&analysis).await?;

        // Step 3: Queue tasks for execution
        let queued_tasks = self.queue_tasks(routing_decisions).await?;

        // Step 4: Execute tasks
        let execution_results = self.execute_tasks(queued_tasks).await?;

        // Step 5: Aggregate results
        let final_result = self.aggregator.aggregate_results(&analysis, execution_results).await?;

        // Step 6: Record performance metrics
        let execution_time = start_time.elapsed();
        {
            let mut monitor = self.monitor.write().await;
            monitor.record_execution(&analysis, &final_result, execution_time).await?;
        }

        Ok(final_result)
    }

    /// Queue tasks for execution
    async fn queue_tasks(&self, decisions: Vec<RoutingDecision>) -> Result<Vec<QueuedTask>> {
        let mut queue_manager = self.queue_manager.write().await;
        let mut queued_tasks = Vec::new();

        for decision in decisions {
            let queued_task = queue_manager.enqueue_task(decision).await?;
            queued_tasks.push(queued_task);
        }

        Ok(queued_tasks)
    }

    /// Execute queued tasks
    async fn execute_tasks(&self, tasks: Vec<QueuedTask>) -> Result<Vec<TaskExecutionResult>> {
        let mut results = Vec::new();

        for task in tasks {
            let result = self.executor.execute_task(task).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get current system status
    pub async fn get_status(&self) -> Result<SystemStatus> {
        let queue_manager = self.queue_manager.read().await;
        let queue_stats = queue_manager.get_queue_stats().await?;
        let monitor = self.monitor.read().await;

        Ok(SystemStatus {
            queue_stats,
            performance_metrics: monitor.get_metrics().await?,
            active_tasks: self.executor.get_active_task_count().await,
            total_processed: monitor.get_total_processed().await,
        })
    }

    /// Cancel a task by ID
    pub async fn cancel_task(&self, task_id: &Uuid) -> Result<bool> {
        // Try to cancel in queue first
        {
            let mut queue_manager = self.queue_manager.write().await;
            if queue_manager.cancel_task(task_id).await? {
                return Ok(true);
            }
        }

        // If not in queue, try to cancel active execution
        self.executor.cancel_task(task_id).await
    }

    /// Get task history
    pub async fn get_task_history(&self, limit: Option<usize>) -> Result<Vec<TaskHistoryEntry>> {
        let monitor = self.monitor.read().await;
        monitor.get_task_history(limit).await
    }

    /// Get routing analytics
    pub async fn get_routing_analytics(&self) -> Result<RoutingAnalytics> {
        let monitor = self.monitor.read().await;
        monitor.get_routing_analytics().await
    }

    /// Update routing strategy based on performance
    pub async fn update_routing_strategy(&self) -> Result<()> {
        let analytics = self.get_routing_analytics().await?;
        self.router.update_strategy(&analytics).await?;
        Ok(())
    }
}

impl Default for TaskRouter {
    fn default() -> Self {
        Self::new()
    }
}