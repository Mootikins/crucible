use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Performance tracking system for agents
#[derive(Debug)]
pub struct AgentPerformanceTracker {
    /// Performance history for each agent
    performance_history: HashMap<Uuid, Vec<PerformanceRecord>>,
    /// Aggregated performance metrics
    aggregated_metrics: HashMap<Uuid, AggregatedMetrics>,
    /// Learning model weights for prediction
    prediction_weights: PredictionWeights,
}

/// Individual performance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecord {
    /// When the performance was recorded
    pub timestamp: DateTime<Utc>,
    /// Agent ID
    pub agent_id: Uuid,
    /// Task type/category
    pub task_type: String,
    /// Whether the task was completed successfully
    pub success: bool,
    /// Time taken to complete the task (ms)
    pub completion_time_ms: u64,
    /// User satisfaction score (0-1)
    pub user_satisfaction: Option<f32>,
    /// Complexity of the task (1-10)
    pub task_complexity: u8,
    /// Tools used during the task
    pub tools_used: Vec<String>,
    /// Context of the task
    pub context: String,
    /// Collaboration involved
    pub collaboration_involved: bool,
}

/// Aggregated performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    /// Total number of tasks
    pub total_tasks: usize,
    /// Success rate (0-1)
    pub success_rate: f32,
    /// Average completion time (ms)
    pub avg_completion_time_ms: f64,
    /// Average user satisfaction
    pub avg_user_satisfaction: f32,
    /// Performance by task type
    pub performance_by_task_type: HashMap<String, TaskTypeMetrics>,
    /// Tool efficiency metrics
    pub tool_efficiency: HashMap<String, f32>,
    /// Collaboration success rate
    pub collaboration_success_rate: f32,
    /// Performance trend (improving, stable, declining)
    pub performance_trend: PerformanceTrend,
    /// Specialization score (0-1)
    pub specialization_score: f32,
    /// Reliability score (0-1)
    pub reliability_score: f32,
}

/// Metrics specific to a task type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeMetrics {
    /// Number of tasks of this type
    pub count: usize,
    /// Success rate for this task type
    pub success_rate: f32,
    /// Average completion time for this task type
    pub avg_completion_time_ms: f64,
    /// Proficiency score for this task type (0-1)
    pub proficiency_score: f32,
}

/// Performance trend over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceTrend {
    /// Performance is improving
    Improving,
    /// Performance is stable
    Stable,
    /// Performance is declining
    Declining,
    /// Insufficient data to determine trend
    Unknown,
}

/// Weights for performance prediction model
#[derive(Debug, Clone)]
pub struct PredictionWeights {
    /// Weight for task complexity
    pub complexity_weight: f32,
    /// Weight for task type match
    pub task_type_weight: f32,
    /// Weight for tool availability
    pub tool_weight: f32,
    /// Weight for collaboration history
    pub collaboration_weight: f32,
    /// Weight for recency (more recent performance has higher weight)
    pub recency_weight: f32,
}

impl Default for PredictionWeights {
    fn default() -> Self {
        Self {
            complexity_weight: 0.2,
            task_type_weight: 0.3,
            tool_weight: 0.2,
            collaboration_weight: 0.1,
            recency_weight: 0.2,
        }
    }
}

impl AgentPerformanceTracker {
    /// Create a new performance tracker
    pub fn new() -> Self {
        Self {
            performance_history: HashMap::new(),
            aggregated_metrics: HashMap::new(),
            prediction_weights: PredictionWeights::default(),
        }
    }

    /// Record performance for an agent
    pub fn record_performance(&mut self, agent_id: &Uuid, success: bool,
                            completion_time_ms: u64, user_satisfaction: Option<f32>) {
        let record = PerformanceRecord {
            timestamp: Utc::now(),
            agent_id: *agent_id,
            task_type: "general".to_string(), // Default, can be enhanced
            success,
            completion_time_ms,
            user_satisfaction,
            task_complexity: 5, // Default, can be enhanced
            tools_used: Vec::new(),
            context: String::new(),
            collaboration_involved: false,
        };

        self.performance_history
            .entry(*agent_id)
            .or_insert_with(Vec::new)
            .push(record);

        // Update aggregated metrics
        self.update_aggregated_metrics(agent_id);
    }

    /// Record detailed performance for an agent
    pub fn record_detailed_performance(&mut self, record: PerformanceRecord) {
        let agent_id = record.agent_id;

        self.performance_history
            .entry(agent_id)
            .or_insert_with(Vec::new)
            .push(record);

        // Update aggregated metrics
        self.update_aggregated_metrics(&agent_id);
    }

    /// Update aggregated metrics for an agent
    fn update_aggregated_metrics(&mut self, agent_id: &Uuid) {
        let records = match self.performance_history.get(agent_id) {
            Some(records) => records,
            None => return,
        };

        if records.is_empty() {
            return;
        }

        let total_tasks = records.len();
        let successful_tasks = records.iter().filter(|r| r.success).count();
        let success_rate = successful_tasks as f32 / total_tasks as f32;

        let total_time: u64 = records.iter().map(|r| r.completion_time_ms).sum();
        let avg_completion_time_ms = total_time as f64 / total_tasks as f64;

        let satisfaction_scores: Vec<f32> = records.iter()
            .filter_map(|r| r.user_satisfaction)
            .collect();
        let avg_user_satisfaction = if satisfaction_scores.is_empty() {
            0.0
        } else {
            satisfaction_scores.iter().sum::<f32>() / satisfaction_scores.len() as f32
        };

        // Calculate performance by task type
        let mut performance_by_task_type = HashMap::new();
        let mut task_type_counts = HashMap::new();

        for record in records {
            let task_type = &record.task_type;
            let count = task_type_counts.entry(task_type.clone()).or_insert(0);
            *count += 1;

            let metrics = performance_by_task_type.entry(task_type.clone())
                .or_insert_with(|| TaskTypeMetrics {
                    count: 0,
                    success_rate: 0.0,
                    avg_completion_time_ms: 0.0,
                    proficiency_score: 0.0,
                });

            metrics.count += 1;
        }

        // Calculate task type metrics
        for (task_type, metrics) in &mut performance_by_task_type {
            let type_records: Vec<&PerformanceRecord> = records.iter()
                .filter(|r| &r.task_type == task_type)
                .collect();

            if !type_records.is_empty() {
                let successful = type_records.iter().filter(|r| r.success).count();
                metrics.success_rate = successful as f32 / type_records.len() as f32;

                let total_type_time: u64 = type_records.iter().map(|r| r.completion_time_ms).sum();
                metrics.avg_completion_time_ms = total_type_time as f64 / type_records.len() as f64;

                // Proficiency score based on success rate and speed
                let avg_complexity = type_records.iter().map(|r| r.task_complexity as f32).sum::<f32>()
                    / type_records.len() as f32;
                let speed_factor = (10000.0 / metrics.avg_completion_time_ms as f32).min(1.0);
                metrics.proficiency_score = (metrics.success_rate * 0.7 + speed_factor * 0.3) *
                    (avg_complexity / 10.0);
            }
        }

        // Calculate collaboration success rate
        let collaboration_records: Vec<&PerformanceRecord> = records.iter()
            .filter(|r| r.collaboration_involved)
            .collect();
        let collaboration_success_rate = if collaboration_records.is_empty() {
            0.0
        } else {
            let successful_collabs = collaboration_records.iter().filter(|r| r.success).count();
            successful_collabs as f32 / collaboration_records.len() as f32
        };

        // Determine performance trend
        let performance_trend = self.calculate_performance_trend(records);

        // Calculate specialization score
        let specialization_score = self.calculate_specialization_score(&performance_by_task_type);

        // Calculate reliability score
        let reliability_score = self.calculate_reliability_score(records);

        let metrics = AggregatedMetrics {
            total_tasks,
            success_rate,
            avg_completion_time_ms,
            avg_user_satisfaction,
            performance_by_task_type,
            tool_efficiency: HashMap::new(), // TODO: Implement tool efficiency tracking
            collaboration_success_rate,
            performance_trend,
            specialization_score,
            reliability_score,
        };

        self.aggregated_metrics.insert(*agent_id, metrics);
    }

    /// Calculate performance trend over time
    fn calculate_performance_trend(&self, records: &[PerformanceRecord]) -> PerformanceTrend {
        if records.len() < 5 {
            return PerformanceTrend::Unknown;
        }

        // Split records into first half and second half
        let mid_point = records.len() / 2;
        let first_half = &records[..mid_point];
        let second_half = &records[mid_point..];

        let first_half_success = first_half.iter().filter(|r| r.success).count() as f32 / first_half.len() as f32;
        let second_half_success = second_half.iter().filter(|r| r.success).count() as f32 / second_half.len() as f32;

        let difference = second_half_success - first_half_success;

        if difference > 0.1 {
            PerformanceTrend::Improving
        } else if difference < -0.1 {
            PerformanceTrend::Declining
        } else {
            PerformanceTrend::Stable
        }
    }

    /// Calculate specialization score
    fn calculate_specialization_score(&self, performance_by_task_type: &HashMap<String, TaskTypeMetrics>) -> f32 {
        if performance_by_task_type.is_empty() {
            return 0.0;
        }

        // Specialization score is higher if agent performs well in specific areas
        let total_score: f32 = performance_by_task_type.values()
            .map(|metrics| metrics.proficiency_score)
            .sum();

        let avg_score = total_score / performance_by_task_type.len() as f32;

        // Bonus for having specialized areas (high proficiency in specific types)
        let specialized_areas = performance_by_task_type.values()
            .filter(|metrics| metrics.proficiency_score > 0.8)
            .count();

        let specialization_bonus = (specialized_areas as f32 / performance_by_task_type.len() as f32) * 0.2;

        (avg_score + specialization_bonus).min(1.0)
    }

    /// Calculate reliability score
    fn calculate_reliability_score(&self, records: &[PerformanceRecord]) -> f32 {
        if records.is_empty() {
            return 0.0;
        }

        // Reliability is based on consistency and success rate
        let success_rate = records.iter().filter(|r| r.success).count() as f32 / records.len() as f32;

        // Calculate variance in completion times
        let completion_times: Vec<u64> = records.iter().map(|r| r.completion_time_ms).collect();
        let mean_time = completion_times.iter().sum::<u64>() as f64 / completion_times.len() as f64;
        let variance = completion_times.iter()
            .map(|&time| (time as f64 - mean_time).powi(2))
            .sum::<f64>() / completion_times.len() as f64;
        let std_dev = variance.sqrt();

        // Lower variance = higher reliability
        let consistency_score = 1.0 - (std_dev / mean_time).min(1.0);

        (success_rate * 0.7 + consistency_score * 0.3).min(1.0)
    }

    /// Get performance metrics for an agent
    pub fn get_metrics(&self, agent_id: &Uuid) -> Option<&AggregatedMetrics> {
        self.aggregated_metrics.get(agent_id)
    }

    /// Get performance prediction for an agent on a specific task
    pub fn predict_performance(&self, agent_id: &Uuid, task_type: &str, complexity: u8) -> Option<f32> {
        let metrics = match self.aggregated_metrics.get(agent_id) {
            Some(metrics) => metrics,
            None => return None,
        };

        let task_metrics = match metrics.performance_by_task_type.get(task_type) {
            Some(task_metrics) => task_metrics,
            None => return Some(0.5), // Default prediction for unknown task types
        };

        let base_score = task_metrics.proficiency_score;

        // Adjust for complexity
        let complexity_factor = 1.0 - ((complexity as f32 - 5.0) / 10.0).abs() * 0.3;

        // Adjust for trend
        let trend_factor = match metrics.performance_trend {
            PerformanceTrend::Improving => 1.1,
            PerformanceTrend::Stable => 1.0,
            PerformanceTrend::Declining => 0.9,
            PerformanceTrend::Unknown => 1.0,
        };

        // Adjust for reliability
        let reliability_factor = metrics.reliability_score;

        let predicted_score = base_score * complexity_factor * trend_factor * reliability_factor;
        Some(predicted_score.min(1.0))
    }

    /// Get top performing agents for a specific task type
    pub fn get_top_performers_for_task(&self, task_type: &str, limit: usize) -> Vec<(Uuid, f32)> {
        let mut performers = Vec::new();

        for (agent_id, metrics) in &self.aggregated_metrics {
            if let Some(task_metrics) = metrics.performance_by_task_type.get(task_type) {
                performers.push((*agent_id, task_metrics.proficiency_score));
            }
        }

        performers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        performers.truncate(limit);
        performers
    }

    /// Get learning insights for an agent
    pub fn get_learning_insights(&self, agent_id: &Uuid) -> Option<LearningInsights> {
        let metrics = match self.aggregated_metrics.get(agent_id) {
            Some(metrics) => metrics,
            None => return None,
        };

        let records = match self.performance_history.get(agent_id) {
            Some(records) => records,
            None => return None,
        };

        // Identify areas for improvement
        let improvement_areas = self.identify_improvement_areas(metrics);

        // Identify strengths
        let strengths = self.identify_strengths(metrics);

        // Recommend training focus
        let training_recommendations = self.recommend_training_focus(metrics);

        Some(LearningInsights {
            improvement_areas,
            strengths,
            training_recommendations,
            performance_trend: metrics.performance_trend.clone(),
            specialization_score: metrics.specialization_score,
            reliability_score: metrics.reliability_score,
        })
    }

    /// Identify areas for improvement
    fn identify_improvement_areas(&self, metrics: &AggregatedMetrics) -> Vec<String> {
        let mut areas = Vec::new();

        // Check task types with low performance
        for (task_type, task_metrics) in &metrics.performance_by_task_type {
            if task_metrics.proficiency_score < 0.6 {
                areas.push(format!("Improve performance in {}: current score {:.2}",
                                 task_type, task_metrics.proficiency_score));
            }
        }

        // Check overall metrics
        if metrics.success_rate < 0.8 {
            areas.push("Overall success rate needs improvement".to_string());
        }

        if metrics.avg_user_satisfaction < 0.7 {
            areas.push("User satisfaction scores are low".to_string());
        }

        if metrics.collaboration_success_rate < 0.7 {
            areas.push("Collaboration skills need improvement".to_string());
        }

        areas
    }

    /// Identify strengths
    fn identify_strengths(&self, metrics: &AggregatedMetrics) -> Vec<String> {
        let mut strengths = Vec::new();

        // Check task types with high performance
        for (task_type, task_metrics) in &metrics.performance_by_task_type {
            if task_metrics.proficiency_score > 0.8 {
                strengths.push(format!("Excellent performance in {}: score {:.2}",
                                    task_type, task_metrics.proficiency_score));
            }
        }

        // Check overall metrics
        if metrics.success_rate > 0.9 {
            strengths.push("High overall success rate".to_string());
        }

        if metrics.avg_user_satisfaction > 0.8 {
            strengths.push("High user satisfaction scores".to_string());
        }

        if metrics.reliability_score > 0.9 {
            strengths.push("Highly reliable performance".to_string());
        }

        if matches!(metrics.performance_trend, PerformanceTrend::Improving) {
            strengths.push("Consistently improving performance".to_string());
        }

        strengths
    }

    /// Recommend training focus areas
    fn recommend_training_focus(&self, metrics: &AggregatedMetrics) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Find worst performing task types
        let mut task_types: Vec<_> = metrics.performance_by_task_type.iter().collect();
        task_types.sort_by(|a, b| a.1.proficiency_score.partial_cmp(&b.1.proficiency_score).unwrap());

        for (task_type, task_metrics) in task_types.iter().take(2) {
            if task_metrics.proficiency_score < 0.7 {
                recommendations.push(format!("Focus on {} skills - current proficiency: {:.2}",
                                         task_type, task_metrics.proficiency_score));
            }
        }

        if metrics.collaboration_success_rate < 0.8 {
            recommendations.push("Practice collaborative problem-solving".to_string());
        }

        if metrics.specialization_score < 0.6 {
            recommendations.push("Develop specialized expertise in specific areas".to_string());
        }

        recommendations
    }

    /// Clear performance history for an agent
    pub fn clear_agent_history(&mut self, agent_id: &Uuid) {
        self.performance_history.remove(agent_id);
        self.aggregated_metrics.remove(agent_id);
    }

    /// Get overall system performance statistics
    pub fn get_system_stats(&self) -> SystemStats {
        let total_agents = self.aggregated_metrics.len();
        let total_tasks: usize = self.aggregated_metrics.values()
            .map(|m| m.total_tasks)
            .sum();

        let avg_success_rate = if total_agents > 0 {
            self.aggregated_metrics.values()
                .map(|m| m.success_rate)
                .sum::<f32>() / total_agents as f32
        } else {
            0.0
        };

        let avg_satisfaction = if total_agents > 0 {
            self.aggregated_metrics.values()
                .map(|m| m.avg_user_satisfaction)
                .sum::<f32>() / total_agents as f32
        } else {
            0.0
        };

        let improving_agents = self.aggregated_metrics.values()
            .filter(|m| matches!(m.performance_trend, PerformanceTrend::Improving))
            .count();

        SystemStats {
            total_agents,
            total_tasks,
            avg_success_rate,
            avg_satisfaction,
            improving_agents,
        }
    }
}

impl Default for AgentPerformanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Learning insights for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningInsights {
    /// Areas that need improvement
    pub improvement_areas: Vec<String>,
    /// Agent's strengths
    pub strengths: Vec<String>,
    /// Recommended training focus
    pub training_recommendations: Vec<String>,
    /// Current performance trend
    pub performance_trend: PerformanceTrend,
    /// Specialization score
    pub specialization_score: f32,
    /// Reliability score
    pub reliability_score: f32,
}

/// System-wide performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Total number of agents being tracked
    pub total_agents: usize,
    /// Total tasks completed across all agents
    pub total_tasks: usize,
    /// Average success rate across all agents
    pub avg_success_rate: f32,
    /// Average user satisfaction across all agents
    pub avg_satisfaction: f32,
    /// Number of agents with improving performance
    pub improving_agents: usize,
}