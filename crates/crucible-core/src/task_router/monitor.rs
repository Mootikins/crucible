use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use super::types::*;
use crate::agent::AgentStatus;

/// Performance Monitor - tracks task execution metrics and optimizes routing decisions
#[derive(Debug)]
pub struct PerformanceMonitor {
    /// Real-time metrics
    realtime_metrics: RealtimeMetrics,
    /// Historical performance data
    historical_data: HistoricalData,
    /// Performance alerts
    alerts: AlertManager,
    /// Optimization engine
    optimizer: OptimizationEngine,
    /// Configuration
    config: MonitoringConfig,
}

/// Real-time metrics tracking
#[derive(Debug)]
struct RealtimeMetrics {
    /// Currently executing tasks
    active_tasks: HashMap<Uuid, ActiveTaskMetrics>,
    /// Recent completions (last 100)
    recent_completions: VecDeque<TaskCompletionMetrics>,
    /// System resource usage
    resource_usage: ResourceUsage,
    /// Agent performance snapshot
    agent_performance: HashMap<Uuid, AgentRealtimePerformance>,
    /// Queue metrics
    queue_metrics: QueueMetrics,
}

/// Metrics for active tasks
#[derive(Debug, Clone)]
struct ActiveTaskMetrics {
    /// Task ID
    task_id: Uuid,
    /// Agent ID
    agent_id: Uuid,
    /// Start time
    start_time: DateTime<Utc>,
    /// Estimated completion time
    estimated_completion: DateTime<Utc>,
    /// Progress percentage (0-100)
    progress: u8,
    /// Current operation
    current_operation: String,
    /// Resource usage so far
    resource_usage: TaskResourceUsage,
    /// Checkpoints for progress tracking
    checkpoints: Vec<TaskCheckpoint>,
}

/// Resource usage for a task
#[derive(Debug, Clone, Default)]
struct TaskResourceUsage {
    /// CPU time in milliseconds
    cpu_time_ms: u64,
    /// Memory allocated in MB
    memory_mb: u64,
    /// Network requests made
    network_requests: u32,
    /// Tokens processed (for LLM tasks)
    tokens_processed: u32,
    /// Tool calls made
    tool_calls: u32,
}

/// Task checkpoint for progress tracking
#[derive(Debug, Clone)]
struct TaskCheckpoint {
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Progress percentage
    progress: u8,
    /// Operation at checkpoint
    operation: String,
    /// Metrics at checkpoint
    metrics: HashMap<String, f64>,
}

/// Metrics for completed tasks
#[derive(Debug, Clone)]
struct TaskCompletionMetrics {
    /// Task ID
    task_id: Uuid,
    /// Agent ID
    agent_id: Uuid,
    /// Start time
    start_time: DateTime<Utc>,
    /// End time
    end_time: DateTime<Utc>,
    /// Success status
    success: bool,
    /// Total execution time in milliseconds
    execution_time_ms: u64,
    /// Wait time in milliseconds
    wait_time_ms: u64,
    /// Resource usage
    resource_usage: TaskResourceUsage,
    /// Quality metrics
    quality_metrics: QualityMetrics,
    /// Error information (if any)
    error_info: Option<String>,
}

/// Quality metrics for completed tasks
#[derive(Debug, Clone)]
struct QualityMetrics {
    /// Result confidence score
    confidence_score: f32,
    /// User satisfaction (if available)
    user_satisfaction: Option<f32>,
    /// Validation score
    validation_score: f32,
    /// Completeness score
    completeness_score: f32,
}

/// System resource usage
#[derive(Debug, Clone, Default)]
struct ResourceUsage {
    /// CPU usage percentage
    cpu_percent: f32,
    /// Memory usage percentage
    memory_percent: f32,
    /// Disk usage percentage
    disk_percent: f32,
    /// Network usage in bytes per second
    network_bytes_per_sec: u64,
    /// Active connections
    active_connections: u32,
}

/// Agent real-time performance
#[derive(Debug, Clone)]
struct AgentRealtimePerformance {
    /// Agent ID
    agent_id: Uuid,
    /// Number of active tasks
    active_tasks: u8,
    /// Maximum concurrent tasks
    max_concurrent: u8,
    /// Current load percentage
    load_percentage: f32,
    /// Average response time for recent tasks
    avg_response_time_ms: u64,
    /// Success rate for recent tasks
    recent_success_rate: f32,
    /// Last activity timestamp
    last_activity: DateTime<Utc>,
    /// Current status
    status: AgentStatus,
}

/// Queue metrics
#[derive(Debug, Clone)]
struct QueueMetrics {
    /// Total tasks in queue
    total_queued: usize,
    /// Tasks by priority
    tasks_by_priority: HashMap<TaskPriority, usize>,
    /// Average wait time
    avg_wait_time_ms: u64,
    /// Queue throughput (tasks per minute)
    throughput_per_minute: f32,
    /// Queue age distribution
    age_distribution: HashMap<String, usize>, // e.g., "0-1min", "1-5min", "5min+"
}

/// Historical performance data
#[derive(Debug)]
struct HistoricalData {
    /// Daily aggregated metrics
    daily_metrics: VecDeque<DailyMetrics>,
    /// Agent performance history
    agent_history: HashMap<Uuid, Vec<AgentPerformanceSnapshot>>,
    /// Task type performance
    task_type_performance: HashMap<String, TaskTypePerformanceHistory>,
    /// System performance trends
    system_trends: SystemPerformanceTrends,
}

/// Daily aggregated metrics
#[derive(Debug, Clone)]
struct DailyMetrics {
    /// Date
    date: chrono::NaiveDate,
    /// Total tasks processed
    total_tasks: u64,
    /// Success rate
    success_rate: f32,
    /// Average execution time
    avg_execution_time_ms: u64,
    /// Peak concurrent tasks
    peak_concurrent_tasks: u16,
    /// Resource usage averages
    avg_resource_usage: ResourceUsage,
    /// Top performing agents
    top_agents: Vec<(Uuid, f32)>, // (agent_id, performance_score)
}

/// Agent performance snapshot
#[derive(Debug, Clone)]
struct AgentPerformanceSnapshot {
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Agent ID
    agent_id: Uuid,
    /// Performance score
    performance_score: f32,
    /// Tasks completed
    tasks_completed: u32,
    /// Average execution time
    avg_execution_time_ms: u64,
    /// Success rate
    success_rate: f32,
    /// Specialization areas
    specialization_areas: Vec<String>,
}

/// Task type performance history
#[derive(Debug, Clone)]
struct TaskTypePerformanceHistory {
    /// Task type
    task_type: String,
    /// Performance data points
    data_points: VecDeque<TaskTypeDataPoint>,
    /// Best performing agents
    best_agents: Vec<(Uuid, f32)>,
}

/// Task type performance data point
#[derive(Debug, Clone)]
struct TaskTypeDataPoint {
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Average execution time
    avg_execution_time_ms: u64,
    /// Success rate
    success_rate: f32,
    /// Number of tasks
    task_count: u32,
}

/// System performance trends
#[derive(Debug, Clone)]
struct SystemPerformanceTrends {
    /// Performance trend (improving, stable, declining)
    performance_trend: TrendDirection,
    /// Throughput trend
    throughput_trend: TrendDirection,
    /// Error rate trend
    error_rate_trend: TrendDirection,
    /// Resource efficiency trend
    resource_efficiency_trend: TrendDirection,
}

/// Trend direction
#[derive(Debug, Clone, PartialEq)]
enum TrendDirection {
    Improving,
    Stable,
    Declining,
}

/// Alert manager for performance issues
#[derive(Debug)]
struct AlertManager {
    /// Active alerts
    active_alerts: Vec<PerformanceAlert>,
    /// Alert history
    alert_history: VecDeque<PerformanceAlert>,
    /// Alert rules
    alert_rules: Vec<AlertRule>,
    /// Notification channels
    notification_channels: Vec<NotificationChannel>,
}

/// Performance alert
#[derive(Debug, Clone)]
struct PerformanceAlert {
    /// Alert ID
    alert_id: Uuid,
    /// Alert type
    alert_type: AlertType,
    /// Severity level
    severity: AlertSeverity,
    /// Title
    title: String,
    /// Description
    description: String,
    /// Timestamp
    timestamp: DateTime<Utc>,
    /// Affected resources
    affected_resources: Vec<String>,
    /// Current value
    current_value: f64,
    /// Threshold value
    threshold_value: f64,
    /// Alert status
    status: AlertStatus,
}

/// Alert type
#[derive(Debug, Clone, PartialEq)]
enum AlertType {
    HighErrorRate,
    SlowPerformance,
    ResourceExhaustion,
    QueueBacklog,
    AgentFailure,
    SystemOverload,
}

/// Alert severity
#[derive(Debug, Clone, PartialEq)]
enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Alert status
#[derive(Debug, Clone, PartialEq)]
enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
}

/// Alert rule
#[derive(Debug, Clone)]
struct AlertRule {
    /// Rule name
    name: String,
    /// Metric to monitor
    metric: String,
    /// Condition (e.g., ">", "<", "==")
    condition: String,
    /// Threshold value
    threshold: f64,
    /// Alert severity
    severity: AlertSeverity,
    /// Cooldown period in minutes
    cooldown_minutes: u32,
    /// Last triggered
    last_triggered: Option<DateTime<Utc>>,
}

/// Notification channel
#[derive(Debug, Clone)]
struct NotificationChannel {
    /// Channel name
    name: String,
    /// Channel type
    channel_type: NotificationType,
    /// Channel configuration
    config: HashMap<String, String>,
}

/// Notification type
#[derive(Debug, Clone)]
enum NotificationType {
    Email,
    Webhook,
    Slack,
    Log,
}

/// Optimization engine for performance improvements
#[derive(Debug)]
struct OptimizationEngine {
    /// Optimization suggestions
    suggestions: Vec<OptimizationSuggestion>,
    /// Applied optimizations
    applied_optimizations: Vec<AppliedOptimization>,
    /// Machine learning model for predictions
    prediction_model: Option<PredictionModel>,
    /// Optimization strategies
    strategies: Vec<OptimizationStrategy>,
}

/// Optimization suggestion
#[derive(Debug, Clone)]
struct OptimizationSuggestion {
    /// Suggestion ID
    suggestion_id: Uuid,
    /// Type of optimization
    optimization_type: OptimizationType,
    /// Title
    title: String,
    /// Description
    description: String,
    /// Expected impact
    expected_impact: ImpactLevel,
    /// Implementation effort
    implementation_effort: EffortLevel,
    /// Affected components
    affected_components: Vec<String>,
    /// Suggestion status
    status: SuggestionStatus,
}

/// Optimization type
#[derive(Debug, Clone, PartialEq)]
enum OptimizationType {
    LoadBalancing,
    ResourceAllocation,
    AgentSelection,
    QueueManagement,
    Caching,
    Parallelization,
}

/// Impact level
#[derive(Debug, Clone, PartialEq)]
enum ImpactLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Effort level
#[derive(Debug, Clone, PartialEq)]
enum EffortLevel {
    Low,
    Medium,
    High,
}

/// Suggestion status
#[derive(Debug, Clone, PartialEq)]
enum SuggestionStatus {
    Pending,
    Approved,
    Applied,
    Rejected,
}

/// Applied optimization
#[derive(Debug, Clone)]
struct AppliedOptimization {
    /// Optimization ID
    optimization_id: Uuid,
    /// Type of optimization
    optimization_type: OptimizationType,
    /// When it was applied
    applied_at: DateTime<Utc>,
    /// Before metrics
    before_metrics: HashMap<String, f64>,
    /// After metrics
    after_metrics: HashMap<String, f64>,
    /// Effectiveness score
    effectiveness_score: f32,
}

/// Simple prediction model interface
#[derive(Debug, Clone)]
struct PredictionModel {
    /// Model type
    model_type: String,
    /// Last trained
    last_trained: DateTime<Utc>,
    /// Accuracy metrics
    accuracy: f32,
}

/// Optimization strategy
#[derive(Debug, Clone)]
struct OptimizationStrategy {
    /// Strategy name
    name: String,
    /// Strategy description
    description: String,
    /// Enabled status
    enabled: bool,
    /// Parameters
    parameters: HashMap<String, String>,
}

/// Monitoring configuration
#[derive(Debug, Clone)]
struct MonitoringConfig {
    /// Metrics collection interval in seconds
    collection_interval_secs: u64,
    /// Data retention period in days
    retention_days: u32,
    /// Enable real-time monitoring
    enable_realtime: bool,
    /// Enable predictive analytics
    enable_predictions: bool,
    /// Alert checking interval in seconds
    alert_check_interval_secs: u64,
    /// Optimization check interval in minutes
    optimization_check_interval_mins: u32,
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        let config = MonitoringConfig::default();

        Self {
            realtime_metrics: RealtimeMetrics::new(),
            historical_data: HistoricalData::new(),
            alerts: AlertManager::new(),
            optimizer: OptimizationEngine::new(),
            config,
        }
    }

    /// Record task execution
    pub async fn record_execution(&mut self, analysis: &TaskAnalysis, result: &TaskResult,
                                execution_time: std::time::Duration) -> Result<()> {
        let timestamp = Utc::now();
        let execution_time_ms = execution_time.as_millis() as u64;

        // Create completion metrics
        let completion_metrics = TaskCompletionMetrics {
            task_id: analysis.request_id,
            agent_id: result.subtask_results.first()
                .map(|r| r.executing_agent_id)
                .unwrap_or_default(),
            start_time: timestamp - Duration::milliseconds(execution_time_ms as i64),
            end_time: timestamp,
            success: result.success,
            execution_time_ms,
            wait_time_ms: 0, // Would be calculated from queue data
            resource_usage: TaskResourceUsage::default(), // Would be collected during execution
            quality_metrics: QualityMetrics {
                confidence_score: 0.8, // Would be calculated from result
                user_satisfaction: None,
                validation_score: 1.0,
                completeness_score: 1.0,
            },
            error_info: None,
        };

        // Add to recent completions
        self.realtime_metrics.recent_completions.push_back(completion_metrics.clone());

        // Keep only recent completions
        if self.realtime_metrics.recent_completions.len() > 100 {
            self.realtime_metrics.recent_completions.pop_front();
        }

        // Update historical data
        self.update_historical_data(&completion_metrics).await;

        // Check for alerts
        self.check_alerts().await;

        // Update optimization suggestions
        self.update_optimization_suggestions().await;

        tracing::info!("Performance metrics recorded for request {}", analysis.request_id);
        Ok(())
    }

    /// Update historical data with new metrics
    async fn update_historical_data(&mut self, metrics: &TaskCompletionMetrics) {
        let today = Utc::now().date_naive();

        // Find or create today's metrics
        if self.historical_data.daily_metrics.is_empty() ||
           self.historical_data.daily_metrics.back().unwrap().date != today {

            self.historical_data.daily_metrics.push_back(DailyMetrics {
                date: today,
                total_tasks: 0,
                success_rate: 0.0,
                avg_execution_time_ms: 0,
                peak_concurrent_tasks: 0,
                avg_resource_usage: ResourceUsage::default(),
                top_agents: Vec::new(),
            });
        }

        // Update today's metrics
        if let Some(daily_metrics) = self.historical_data.daily_metrics.back_mut() {
            daily_metrics.total_tasks += 1;

            // Update success rate
            let total_tasks = daily_metrics.total_tasks as f32;
            let successful_tasks = if metrics.success { 1.0 } else { 0.0 };
            daily_metrics.success_rate = (daily_metrics.success_rate * (total_tasks - 1.0) + successful_tasks) / total_tasks;

            // Update average execution time
            daily_metrics.avg_execution_time_ms =
                ((daily_metrics.avg_execution_time_ms as f64 * (total_tasks as f64 - 1.0) + metrics.execution_time_ms as f64) / total_tasks as f64) as u64;
        }

        // Update agent history
        self.update_agent_history(metrics).await;

        // Keep only last 30 days of data
        let cutoff_date = today - Duration::days(30);
        while let Some(front) = self.historical_data.daily_metrics.front() {
            if front.date < cutoff_date {
                self.historical_data.daily_metrics.pop_front();
            } else {
                break;
            }
        }
    }

    /// Update agent performance history
    async fn update_agent_history(&mut self, metrics: &TaskCompletionMetrics) {
        let agent_id = metrics.agent_id;
        let snapshot = AgentPerformanceSnapshot {
            timestamp: metrics.end_time,
            agent_id,
            performance_score: self.calculate_agent_performance_score(metrics),
            tasks_completed: 1,
            avg_execution_time_ms: metrics.execution_time_ms,
            success_rate: if metrics.success { 1.0 } else { 0.0 },
            specialization_areas: Vec::new(), // Would be determined from task type
        };

        let agent_history = self.historical_data.agent_history
            .entry(agent_id)
            .or_insert_with(Vec::new);

        agent_history.push(snapshot);

        // Keep only last 100 snapshots per agent
        if agent_history.len() > 100 {
            agent_history.remove(0);
        }
    }

    /// Calculate agent performance score
    fn calculate_agent_performance_score(&self, metrics: &TaskCompletionMetrics) -> f32 {
        let mut score = 0.0f32;

        // Success rate component (40% weight)
        if metrics.success {
            score += 0.4;
        }

        // Execution time component (30% weight) - faster is better
        let time_score = (1.0 - (metrics.execution_time_ms as f64 / 300000.0).min(1.0)).max(0.0) as f32;
        score += time_score * 0.3;

        // Quality metrics component (30% weight)
        score += metrics.quality_metrics.confidence_score * 0.3;

        score
    }

    /// Check for performance alerts
    async fn check_alerts(&mut self) {
        // Calculate current metrics
        let recent_success_rate = self.calculate_recent_success_rate();
        let recent_avg_execution_time = self.calculate_recent_avg_execution_time();
        let queue_size = self.realtime_metrics.queue_metrics.total_queued;

        // Collect rules that need to be triggered
        let rules_to_trigger: Vec<_> = self.alerts.alert_rules.iter()
            .filter(|rule| self.should_trigger_alert(rule, recent_success_rate, recent_avg_execution_time, queue_size))
            .cloned()
            .collect();

        // Trigger alerts for collected rules
        for rule in rules_to_trigger {
            self.create_alert(&rule).await;
        }

        // Clean up old alerts
        self.cleanup_old_alerts().await;
    }

    /// Calculate recent success rate
    fn calculate_recent_success_rate(&self) -> f32 {
        if self.realtime_metrics.recent_completions.is_empty() {
            return 1.0;
        }

        let successful = self.realtime_metrics.recent_completions.iter()
            .filter(|m| m.success)
            .count();

        successful as f32 / self.realtime_metrics.recent_completions.len() as f32
    }

    /// Calculate recent average execution time
    fn calculate_recent_avg_execution_time(&self) -> u64 {
        if self.realtime_metrics.recent_completions.is_empty() {
            return 0;
        }

        let total_time: u64 = self.realtime_metrics.recent_completions.iter()
            .map(|m| m.execution_time_ms)
            .sum();

        total_time / self.realtime_metrics.recent_completions.len() as u64
    }

    /// Check if alert should be triggered
    fn should_trigger_alert(&self, rule: &AlertRule, success_rate: f32, avg_time: u64, queue_size: usize) -> bool {
        // Check cooldown period
        if let Some(last_triggered) = rule.last_triggered {
            if Utc::now() < last_triggered + Duration::minutes(rule.cooldown_minutes as i64) {
                return false;
            }
        }

        // Check threshold based on metric
        match rule.metric.as_str() {
            "success_rate" => {
                match rule.condition.as_str() {
                    "<" => success_rate < rule.threshold as f32,
                    ">" => success_rate > rule.threshold as f32,
                    _ => false,
                }
            }
            "execution_time" => {
                match rule.condition.as_str() {
                    ">" => (avg_time as f64) > rule.threshold,
                    "<" => (avg_time as f64) < rule.threshold,
                    _ => false,
                }
            }
            "queue_size" => {
                match rule.condition.as_str() {
                    ">" => (queue_size as f64) > rule.threshold,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Create and trigger alert
    async fn create_alert(&mut self, rule: &AlertRule) {
        let alert = PerformanceAlert {
            alert_id: Uuid::new_v4(),
            alert_type: self.determine_alert_type(&rule.metric),
            severity: rule.severity.clone(),
            title: format!("Performance Alert: {}", rule.name),
            description: format!("Metric '{}' has crossed threshold", rule.metric),
            timestamp: Utc::now(),
            affected_resources: vec![rule.metric.clone()],
            current_value: 0.0, // Would be the actual current value
            threshold_value: rule.threshold,
            status: AlertStatus::Active,
        };

        // Send notifications before moving the alert
        self.send_alert_notifications(&alert).await;

        self.alerts.active_alerts.push(alert.clone());
        self.alerts.alert_history.push_back(alert);

        tracing::warn!("Performance alert triggered: {}", rule.name);
    }

    /// Determine alert type from metric
    fn determine_alert_type(&self, metric: &str) -> AlertType {
        match metric {
            "success_rate" => AlertType::HighErrorRate,
            "execution_time" => AlertType::SlowPerformance,
            "queue_size" => AlertType::QueueBacklog,
            _ => AlertType::SystemOverload,
        }
    }

    /// Send alert notifications
    async fn send_alert_notifications(&self, alert: &PerformanceAlert) {
        for channel in &self.alerts.notification_channels {
            // In a real implementation, this would send actual notifications
            tracing::info!("Alert notification sent via {:?}: {}", channel.channel_type, alert.title);
        }
    }

    /// Clean up old alerts
    async fn cleanup_old_alerts(&mut self) {
        let cutoff_time = Utc::now() - Duration::hours(24);

        // Remove resolved alerts older than 24 hours
        self.alerts.active_alerts.retain(|alert| {
            alert.status != AlertStatus::Resolved || alert.timestamp > cutoff_time
        });

        // Keep only last 1000 alerts in history
        while self.alerts.alert_history.len() > 1000 {
            self.alerts.alert_history.pop_front();
        }
    }

    /// Update optimization suggestions
    async fn update_optimization_suggestions(&mut self) {
        // Analyze current performance and suggest optimizations
        let success_rate = self.calculate_recent_success_rate();
        let avg_execution_time = self.calculate_recent_avg_execution_time();

        // Suggest optimizations based on performance
        if success_rate < 0.9 {
            self.suggest_error_rate_optimization().await;
        }

        if avg_execution_time > 60000 { // 1 minute
            self.suggest_performance_optimization().await;
        }

        if self.realtime_metrics.queue_metrics.total_queued > 50 {
            self.suggest_queue_optimization().await;
        }
    }

    /// Suggest error rate optimizations
    async fn suggest_error_rate_optimization(&mut self) {
        let suggestion = OptimizationSuggestion {
            suggestion_id: Uuid::new_v4(),
            optimization_type: OptimizationType::AgentSelection,
            title: "Improve Agent Selection for Error-Prone Tasks".to_string(),
            description: "Consider using agents with better success rates for specific task types".to_string(),
            expected_impact: ImpactLevel::High,
            implementation_effort: EffortLevel::Medium,
            affected_components: vec!["IntelligentRouter".to_string()],
            status: SuggestionStatus::Pending,
        };

        if !self.optimizer.suggestions.iter().any(|s| s.title == suggestion.title) {
            self.optimizer.suggestions.push(suggestion);
        }
    }

    /// Suggest performance optimizations
    async fn suggest_performance_optimization(&mut self) {
        let suggestion = OptimizationSuggestion {
            suggestion_id: Uuid::new_v4(),
            optimization_type: OptimizationType::Parallelization,
            title: "Increase Task Parallelization".to_string(),
            description: "Break down complex tasks into smaller parallelizable subtasks".to_string(),
            expected_impact: ImpactLevel::Medium,
            implementation_effort: EffortLevel::High,
            affected_components: vec!["TaskAnalyzer".to_string(), "ExecutionEngine".to_string()],
            status: SuggestionStatus::Pending,
        };

        if !self.optimizer.suggestions.iter().any(|s| s.title == suggestion.title) {
            self.optimizer.suggestions.push(suggestion);
        }
    }

    /// Suggest queue optimizations
    async fn suggest_queue_optimization(&mut self) {
        let suggestion = OptimizationSuggestion {
            suggestion_id: Uuid::new_v4(),
            optimization_type: OptimizationType::LoadBalancing,
            title: "Optimize Load Balancing".to_string(),
            description: "Distribute tasks more evenly across available agents".to_string(),
            expected_impact: ImpactLevel::Medium,
            implementation_effort: EffortLevel::Low,
            affected_components: vec!["TaskQueueManager".to_string(), "IntelligentRouter".to_string()],
            status: SuggestionStatus::Pending,
        };

        if !self.optimizer.suggestions.iter().any(|s| s.title == suggestion.title) {
            self.optimizer.suggestions.push(suggestion);
        }
    }

    /// Get current performance metrics
    pub async fn get_metrics(&self) -> Result<SystemPerformanceMetrics> {
        let success_rate = self.calculate_recent_success_rate();
        let tasks_per_minute = self.calculate_tasks_per_minute();
        let avg_task_duration = self.calculate_recent_avg_execution_time();
        let agent_utilization = self.calculate_agent_utilization();
        let system_load = self.calculate_system_load();

        Ok(SystemPerformanceMetrics {
            tasks_per_minute,
            success_rate,
            avg_task_duration_ms: avg_task_duration,
            agent_utilization,
            system_load,
        })
    }

    /// Calculate tasks per minute
    fn calculate_tasks_per_minute(&self) -> f32 {
        if self.realtime_metrics.recent_completions.is_empty() {
            return 0.0;
        }

        // Count tasks completed in last 5 minutes
        let five_minutes_ago = Utc::now() - Duration::minutes(5);
        let recent_tasks = self.realtime_metrics.recent_completions.iter()
            .filter(|m| m.end_time > five_minutes_ago)
            .count();

        recent_tasks as f32 / 5.0
    }

    /// Calculate agent utilization
    fn calculate_agent_utilization(&self) -> HashMap<String, f32> {
        let mut utilization = HashMap::new();

        for (agent_id, perf) in &self.realtime_metrics.agent_performance {
            utilization.insert(
                agent_id.to_string(),
                perf.load_percentage
            );
        }

        utilization
    }

    /// Calculate system load
    fn calculate_system_load(&self) -> f32 {
        // Simple load calculation based on active tasks and queue size
        let active_tasks = self.realtime_metrics.active_tasks.len() as f32;
        let queued_tasks = self.realtime_metrics.queue_metrics.total_queued as f32;
        let total_capacity = 10.0; // Would be actual system capacity

        ((active_tasks + queued_tasks) / total_capacity * 100.0).min(100.0)
    }

    /// Get task history
    pub async fn get_task_history(&self, limit: Option<usize>) -> Result<Vec<TaskHistoryEntry>> {
        let limit = limit.unwrap_or(50);
        let mut history = Vec::new();

        for completion in self.realtime_metrics.recent_completions.iter().rev().take(limit) {
            history.push(TaskHistoryEntry {
                request_id: completion.task_id,
                user_id: "unknown".to_string(), // Would be from request context
                request_summary: "Task execution".to_string(), // Would be from actual request
                success: completion.success,
                execution_time_ms: completion.execution_time_ms,
                agents_involved: vec![completion.agent_id.to_string()],
                timestamp: completion.end_time,
            });
        }

        Ok(history)
    }

    /// Get routing analytics
    pub async fn get_routing_analytics(&self) -> Result<RoutingAnalytics> {
        let total_decisions = self.realtime_metrics.recent_completions.len() as u64;
        let successful_decisions = self.realtime_metrics.recent_completions.iter()
            .filter(|m| m.success)
            .count() as u64;

        let routing_accuracy = if total_decisions > 0 {
            successful_decisions as f32 / total_decisions as f32
        } else {
            0.0
        };

        // Calculate top agents
        let mut agent_counts: HashMap<String, u64> = HashMap::new();
        for completion in &self.realtime_metrics.recent_completions {
            *agent_counts.entry(completion.agent_id.to_string()).or_insert(0) += 1;
        }

        let mut top_agents: Vec<_> = agent_counts.into_iter().collect();
        top_agents.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(RoutingAnalytics {
            total_decisions,
            routing_accuracy,
            top_agents,
            routing_patterns: HashMap::new(), // Would be calculated from actual routing data
            performance_trends: Vec::new(),   // Would be calculated from historical data
            optimization_recommendations: self.optimizer.suggestions.iter()
                .filter(|s| s.status == SuggestionStatus::Pending)
                .map(|s| s.title.clone())
                .collect(),
        })
    }

    /// Get total tasks processed
    pub async fn get_total_processed(&self) -> u64 {
        self.historical_data.daily_metrics.iter()
            .map(|d| d.total_tasks)
            .sum()
    }

    /// Get active alerts
    pub fn get_active_alerts(&self) -> &[PerformanceAlert] {
        &self.alerts.active_alerts
    }

    /// Get optimization suggestions
    pub fn get_optimization_suggestions(&self) -> &[OptimizationSuggestion] {
        &self.optimizer.suggestions
    }

    /// Apply optimization suggestion
    pub async fn apply_optimization(&mut self, suggestion_id: &Uuid) -> Result<()> {
        if let Some(suggestion) = self.optimizer.suggestions.iter_mut()
            .find(|s| s.suggestion_id == *suggestion_id) {

            suggestion.status = SuggestionStatus::Applied;

            let optimization = AppliedOptimization {
                optimization_id: *suggestion_id,
                optimization_type: suggestion.optimization_type.clone(),
                applied_at: Utc::now(),
                before_metrics: HashMap::new(), // Would capture current metrics
                after_metrics: HashMap::new(),  // Would be updated later
                effectiveness_score: 0.0,       // Would be calculated later
            };

            self.optimizer.applied_optimizations.push(optimization);
            tracing::info!("Applied optimization: {}", suggestion.title);
        }

        Ok(())
    }

    /// Acknowledge alert
    pub async fn acknowledge_alert(&mut self, alert_id: &Uuid) -> Result<()> {
        if let Some(alert) = self.alerts.active_alerts.iter_mut()
            .find(|a| a.alert_id == *alert_id) {

            alert.status = AlertStatus::Acknowledged;
            tracing::info!("Alert acknowledged: {}", alert.title);
        }

        Ok(())
    }

    /// Generate performance report
    pub async fn generate_performance_report(&self, days: u32) -> Result<PerformanceReport> {
        let cutoff_date = Utc::now().date_naive() - Duration::days(days as i64);

        let relevant_metrics: Vec<_> = self.historical_data.daily_metrics.iter()
            .filter(|d| d.date >= cutoff_date)
            .collect();

        if relevant_metrics.is_empty() {
            return Err(anyhow::anyhow!("No data available for the specified period"));
        }

        let total_tasks = relevant_metrics.iter().map(|d| d.total_tasks).sum();
        let avg_success_rate = relevant_metrics.iter()
            .map(|d| d.success_rate)
            .sum::<f32>() / relevant_metrics.len() as f32;
        let avg_execution_time = relevant_metrics.iter()
            .map(|d| d.avg_execution_time_ms)
            .sum::<u64>() / relevant_metrics.len() as u64;

        Ok(PerformanceReport {
            period_start: cutoff_date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            period_end: Utc::now(),
            total_tasks,
            avg_success_rate,
            avg_execution_time_ms: avg_execution_time,
            top_agents: Vec::new(), // Would be calculated from data
            recommendations: self.optimizer.suggestions.iter()
                .filter(|s| s.status == SuggestionStatus::Pending)
                .map(|s| s.title.clone())
                .collect(),
        })
    }
}

/// Performance report
#[derive(Debug, Clone)]
pub struct PerformanceReport {
    /// Report period start
    pub period_start: DateTime<Utc>,
    /// Report period end
    pub period_end: DateTime<Utc>,
    /// Total tasks processed
    pub total_tasks: u64,
    /// Average success rate
    pub avg_success_rate: f32,
    /// Average execution time
    pub avg_execution_time_ms: u64,
    /// Top performing agents
    pub top_agents: Vec<(String, f32)>,
    /// Performance recommendations
    pub recommendations: Vec<String>,
}

impl RealtimeMetrics {
    /// Create new realtime metrics
    fn new() -> Self {
        Self {
            active_tasks: HashMap::new(),
            recent_completions: VecDeque::new(),
            resource_usage: ResourceUsage::default(),
            agent_performance: HashMap::new(),
            queue_metrics: QueueMetrics {
                total_queued: 0,
                tasks_by_priority: HashMap::new(),
                avg_wait_time_ms: 0,
                throughput_per_minute: 0.0,
                age_distribution: HashMap::new(),
            },
        }
    }
}

impl HistoricalData {
    /// Create new historical data
    fn new() -> Self {
        Self {
            daily_metrics: VecDeque::new(),
            agent_history: HashMap::new(),
            task_type_performance: HashMap::new(),
            system_trends: SystemPerformanceTrends {
                performance_trend: TrendDirection::Stable,
                throughput_trend: TrendDirection::Stable,
                error_rate_trend: TrendDirection::Stable,
                resource_efficiency_trend: TrendDirection::Stable,
            },
        }
    }
}

impl AlertManager {
    /// Create new alert manager
    fn new() -> Self {
        let mut alerts = Self {
            active_alerts: Vec::new(),
            alert_history: VecDeque::new(),
            alert_rules: Vec::new(),
            notification_channels: Vec::new(),
        };

        alerts.initialize_default_rules();
        alerts
    }

    /// Initialize default alert rules
    fn initialize_default_rules(&mut self) {
        self.alert_rules.push(AlertRule {
            name: "Low Success Rate".to_string(),
            metric: "success_rate".to_string(),
            condition: "<".to_string(),
            threshold: 0.8,
            severity: AlertSeverity::Warning,
            cooldown_minutes: 15,
            last_triggered: None,
        });

        self.alert_rules.push(AlertRule {
            name: "High Execution Time".to_string(),
            metric: "execution_time".to_string(),
            condition: ">".to_string(),
            threshold: 120000.0, // 2 minutes
            severity: AlertSeverity::Warning,
            cooldown_minutes: 10,
            last_triggered: None,
        });

        self.alert_rules.push(AlertRule {
            name: "Queue Backlog".to_string(),
            metric: "queue_size".to_string(),
            condition: ">".to_string(),
            threshold: 100.0,
            severity: AlertSeverity::Error,
            cooldown_minutes: 5,
            last_triggered: None,
        });
    }
}

impl OptimizationEngine {
    /// Create new optimization engine
    fn new() -> Self {
        Self {
            suggestions: Vec::new(),
            applied_optimizations: Vec::new(),
            prediction_model: None,
            strategies: Vec::new(),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            collection_interval_secs: 30,
            retention_days: 30,
            enable_realtime: true,
            enable_predictions: false,
            alert_check_interval_secs: 60,
            optimization_check_interval_mins: 60,
        }
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}