use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::types::*;
use crate::agent::{AgentDefinition, AgentRegistry, AgentQuery, AgentMatch, AgentStatus};

/// Intelligent routing algorithm for optimal agent selection
#[derive(Debug)]
pub struct IntelligentRouter {
    /// Agent registry for finding available agents
    agent_registry: Arc<RwLock<AgentRegistry>>,
    /// Historical routing performance data
    routing_history: Arc<RwLock<Vec<RoutingRecord>>>,
    /// Agent performance metrics
    agent_performance: Arc<RwLock<HashMap<Uuid, AgentPerformanceData>>>,
    /// Routing strategy configuration
    strategy_config: RoutingStrategyConfig,
    /// Learning parameters
    learning_params: LearningParameters,
}

/// Historical routing record
#[derive(Debug, Clone)]
struct RoutingRecord {
    timestamp: chrono::DateTime<chrono::Utc>,
    subtask_type: String,
    required_capabilities: Vec<String>,
    selected_agent_id: Uuid,
    success: bool,
    execution_time_ms: u64,
    user_satisfaction: Option<f32>,
    alternative_agents: Vec<(Uuid, f32)>, // (agent_id, score)
}

/// Performance data for an agent
#[derive(Debug, Clone, Default)]
struct AgentPerformanceData {
    total_tasks: u64,
    successful_tasks: u64,
    avg_execution_time_ms: f64,
    user_satisfaction_scores: Vec<f32>,
    capability_performance: HashMap<String, CapabilityPerformance>,
    last_used: Option<chrono::DateTime<chrono::Utc>>,
    current_load: u8, // Number of active tasks (0-10)
}

/// Performance for specific capabilities
#[derive(Debug, Clone, Default)]
struct CapabilityPerformance {
    tasks_completed: u64,
    success_rate: f32,
    avg_execution_time_ms: f64,
}

/// Routing strategy configuration
#[derive(Debug, Clone)]
struct RoutingStrategyConfig {
    /// Weight for capability matching (0-1)
    capability_weight: f32,
    /// Weight for performance history (0-1)
    performance_weight: f32,
    /// Weight for current load balancing (0-1)
    load_balance_weight: f32,
    /// Weight for specialization (0-1)
    specialization_weight: f32,
    /// Minimum performance score threshold
    min_performance_threshold: f32,
    /// Maximum load per agent
    max_agent_load: u8,
}

/// Learning parameters for adaptive routing
#[derive(Debug, Clone)]
struct LearningParameters {
    /// Learning rate for performance updates
    learning_rate: f32,
    /// Decay factor for historical data
    decay_factor: f32,
    /// Exploration rate for trying new agents
    exploration_rate: f32,
}

impl IntelligentRouter {
    /// Create a new intelligent router
    pub fn new() -> Self {
        Self {
            agent_registry: Arc::new(RwLock::new(AgentRegistry::default())),
            routing_history: Arc::new(RwLock::new(Vec::new())),
            agent_performance: Arc::new(RwLock::new(HashMap::new())),
            strategy_config: RoutingStrategyConfig::default(),
            learning_params: LearningParameters::default(),
        }
    }

    /// Route tasks to appropriate agents based on analysis
    pub async fn route_tasks(&self, analysis: &TaskAnalysis) -> Result<Vec<RoutingDecision>> {
        let mut decisions = Vec::new();

        for subtask in &analysis.subtasks {
            let decision = self.route_single_subtask(subtask, &analysis.dependencies).await?;
            decisions.push(decision);
        }

        Ok(decisions)
    }

    /// Route a single subtask to the best agent
    async fn route_single_subtask(&self, subtask: &Subtask, dependencies: &[TaskDependency]) -> Result<RoutingDecision> {
        // Step 1: Find candidate agents
        let candidates = self.find_candidate_agents(subtask).await?;

        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No suitable agents found for subtask: {}", subtask.description));
        }

        // Step 2: Score and rank candidates
        let scored_candidates = self.score_candidates(subtask, candidates).await?;

        // Step 3: Select primary agent
        let (primary_agent, primary_score) = scored_candidates.first()
            .ok_or_else(|| anyhow::anyhow!("No scored candidates available"))?;

        // Step 4: Select backup agents
        let backup_agents: Vec<(Uuid, String, f32)> = scored_candidates.iter()
            .skip(1)
            .take(3) // Top 3 backups
            .map(|(agent, score, reason)| (agent.id.clone(), agent.name.clone(), *score))
            .collect();

        // Step 5: Determine routing reason
        let routing_reason = self.determine_routing_reason(primary_agent, primary_score, subtask).await?;

        // Step 6: Estimate execution time
        let estimated_time = self.estimate_execution_time(subtask, primary_agent).await?;

        Ok(RoutingDecision {
            subtask_id: subtask.id,
            assigned_agent_id: primary_agent.id,
            assigned_agent_name: primary_agent.name.clone(),
            confidence: *primary_score,
            routing_reason,
            estimated_execution_time_ms: estimated_time,
            required_resources: subtask.required_tools.clone(),
            backup_agents,
        })
    }

    /// Find candidate agents for a subtask
    async fn find_candidate_agents(&self, subtask: &Subtask) -> Result<Vec<AgentDefinition>> {
        let agent_registry = self.agent_registry.read().await;

        let query = AgentQuery {
            capabilities: subtask.required_capabilities.clone(),
            skills: Vec::new(),
            tags: Vec::new(),
            required_tools: subtask.required_tools.clone(),
            min_skill_level: None,
            status: Some(AgentStatus::Active),
            text_search: Some(subtask.description.clone()),
        };

        let matches = agent_registry.find_agents(&query);

        Ok(matches.into_iter().map(|m| m.agent).collect())
    }

    /// Score and rank candidate agents
    async fn score_candidates(&self, subtask: &Subtask, candidates: Vec<AgentDefinition>) -> Result<Vec<(AgentDefinition, f32, String)>> {
        let agent_performance = self.agent_performance.read().await;
        let mut scored_candidates = Vec::new();

        for agent in candidates {
            let (score, reason) = self.calculate_agent_score(subtask, &agent, &agent_performance).await?;
            scored_candidates.push((agent, score, reason));
        }

        // Sort by score (descending)
        scored_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored_candidates)
    }

    /// Calculate score for an agent
    async fn calculate_agent_score(&self, subtask: &Subtask, agent: &AgentDefinition,
                                 performance_data: &HashMap<Uuid, AgentPerformanceData>) -> Result<(f32, String)> {
        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        // 1. Capability matching score
        let capability_score = self.calculate_capability_match_score(subtask, agent);
        score += capability_score * self.strategy_config.capability_weight;
        if capability_score > 0.8 {
            reasons.push("Strong capability match".to_string());
        }

        // 2. Performance history score
        let performance_score = self.calculate_performance_score(subtask, agent, performance_data).await;
        score += performance_score * self.strategy_config.performance_weight;
        if performance_score > 0.7 {
            reasons.push("Good performance history".to_string());
        }

        // 3. Load balancing score
        let load_balance_score = self.calculate_load_balance_score(agent, performance_data).await;
        score += load_balance_score * self.strategy_config.load_balance_weight;
        if load_balance_score > 0.6 {
            reasons.push("Low current load".to_string());
        }

        // 4. Specialization score
        let specialization_score = self.calculate_specialization_score(subtask, agent);
        score += specialization_score * self.strategy_config.specialization_weight;
        if specialization_score > 0.7 {
            reasons.push("Specialized expertise".to_string());
        }

        // Apply exploration factor
        if should_exploit(&self.learning_params) {
            score *= 1.0 - self.learning_params.exploration_rate;
        }

        let score = score.min(1.0).max(0.0);
        let reason = if reasons.is_empty() {
            "Basic routing decision".to_string()
        } else {
            reasons.join("; ")
        };

        Ok((score, reason))
    }

    /// Calculate capability match score
    fn calculate_capability_match_score(&self, subtask: &Subtask, agent: &AgentDefinition) -> f32 {
        if subtask.required_capabilities.is_empty() {
            return 0.5; // Neutral score if no specific requirements
        }

        let mut matched_capabilities = 0;
        let total_required = subtask.required_capabilities.len();

        for required_cap in &subtask.required_capabilities {
            if agent.capabilities.iter().any(|cap| cap.name == *required_cap) {
                matched_capabilities += 1;
            }
        }

        if matched_capabilities == 0 {
            return 0.0;
        }

        // Bonus for exact matches
        let base_score = matched_capabilities as f32 / total_required as f32;
        let bonus = if matched_capabilities == total_required {
            0.2 // Bonus for matching all capabilities
        } else {
            0.0
        };

        (base_score + bonus).min(1.0)
    }

    /// Calculate performance history score
    async fn calculate_performance_score(&self, subtask: &Subtask, agent: &AgentDefinition,
                                       performance_data: &HashMap<Uuid, AgentPerformanceData>) -> f32 {
        let agent_perf = match performance_data.get(&agent.id) {
            Some(data) => data,
            None => return 0.5, // Neutral score for new agents
        };

        if agent_perf.total_tasks == 0 {
            return 0.5;
        }

        let success_rate = agent_perf.successful_tasks as f32 / agent_perf.total_tasks as f32;

        // Consider capability-specific performance
        let capability_score = if !subtask.required_capabilities.is_empty() {
            let cap_scores: Vec<f32> = subtask.required_capabilities.iter()
                .map(|cap| {
                    agent_perf.capability_performance.get(cap)
                        .map(|cp| cp.success_rate)
                        .unwrap_or(0.5)
                })
                .collect();

            cap_scores.iter().sum::<f32>() / cap_scores.len() as f32
        } else {
            0.5
        };

        // Consider execution time (lower is better)
        let time_score = if agent_perf.avg_execution_time_ms > 0.0 {
            // Normalize to 0-1 scale (assuming 10 minutes = 600,000ms as baseline)
            (1.0 - (agent_perf.avg_execution_time_ms / 600000.0).min(1.0)).max(0.0)
        } else {
            0.5
        };

        // Weighted combination
        (success_rate * 0.5 + capability_score * 0.3 + time_score * 0.2).min(1.0)
    }

    /// Calculate load balance score
    async fn calculate_load_balance_score(&self, agent: &AgentDefinition,
                                        performance_data: &HashMap<Uuid, AgentPerformanceData>) -> f32 {
        let agent_perf = performance_data.get(&agent.id);

        let current_load = match agent_perf {
            Some(data) => data.current_load,
            None => 0,
        };

        // Higher score for lower load
        if current_load == 0 {
            1.0
        } else if current_load >= self.strategy_config.max_agent_load {
            0.0
        } else {
            1.0 - (current_load as f32 / self.strategy_config.max_agent_load as f32)
        }
    }

    /// Calculate specialization score
    fn calculate_specialization_score(&self, subtask: &Subtask, agent: &AgentDefinition) -> f32 {
        // Check if agent has specialized in the required capabilities
        let mut specialization_score = 0.0f32;
        let mut relevant_capabilities = 0;

        for required_cap in &subtask.required_capabilities {
            if let Some(capability) = agent.capabilities.iter().find(|c| c.name == *required_cap) {
                relevant_capabilities += 1;

                // Higher score for higher skill levels
                let skill_score = match capability.skill_level {
                    crate::agent::SkillLevel::Beginner => 0.3,
                    crate::agent::SkillLevel::Intermediate => 0.6,
                    crate::agent::SkillLevel::Advanced => 0.8,
                    crate::agent::SkillLevel::Expert => 1.0,
                };

                specialization_score += skill_score;
            }
        }

        if relevant_capabilities == 0 {
            return 0.0;
        }

        specialization_score / relevant_capabilities as f32
    }

    /// Determine routing reason
    async fn determine_routing_reason(&self, agent: &AgentDefinition, score: f32, subtask: &Subtask) -> Result<RoutingReason> {
        if score > 0.9 {
            Ok(RoutingReason::Specialization)
        } else if score > 0.8 {
            Ok(RoutingReason::CapabilityMatch)
        } else if score > 0.7 {
            Ok(RoutingReason::PerformanceRating)
        } else if score > 0.6 {
            Ok(RoutingReason::LoadBalancing)
        } else {
            Ok(RoutingReason::CostOptimization)
        }
    }

    /// Estimate execution time for a subtask
    async fn estimate_execution_time(&self, subtask: &Subtask, agent: &AgentDefinition) -> Result<u64> {
        let base_time = subtask.estimated_duration_minutes as u64 * 60 * 1000; // Convert to milliseconds

        // Adjust based on agent's historical performance
        let agent_performance = self.agent_performance.read().await;
        if let Some(perf) = agent_performance.get(&agent.id) {
            if perf.avg_execution_time_ms > 0.0 {
                // Use historical average as adjustment factor
                let historical_factor = perf.avg_execution_time_ms / (base_time as f64);
                let adjusted_time = (base_time as f64 * historical_factor) as u64;
                return Ok(adjusted_time);
            }
        }

        Ok(base_time)
    }

    /// Update routing strategy based on analytics
    pub async fn update_strategy(&self, analytics: &RoutingAnalytics) -> Result<()> {
        let mut config = self.strategy_config.clone();

        // Adjust weights based on performance
        if analytics.routing_accuracy < 0.7 {
            // Increase capability weight if accuracy is low
            config.capability_weight = (config.capability_weight + 0.1).min(0.5);
        }

        if analytics.routing_accuracy > 0.9 {
            // We can afford to explore more
            config.performance_weight = (config.performance_weight - 0.05).max(0.1);
        }

        // Note: In a real implementation, we would update the config
        // For now, we just log the recommendation
        tracing::info!("Routing strategy update recommended: {:?}", config);

        Ok(())
    }

    /// Record routing decision and outcome
    pub async fn record_routing_outcome(&self, decision: &RoutingDecision, result: &TaskExecutionResult) -> Result<()> {
        // Update agent performance data
        let mut agent_performance = self.agent_performance.write().await;
        let perf_data = agent_performance.entry(decision.assigned_agent_id)
            .or_insert_with(AgentPerformanceData::default);

        perf_data.total_tasks += 1;
        if result.success {
            perf_data.successful_tasks += 1;
        }

        // Update execution time
        let new_time = result.metrics.execution_time_ms as f64;
        if perf_data.avg_execution_time_ms == 0.0 {
            perf_data.avg_execution_time_ms = new_time;
        } else {
            perf_data.avg_execution_time_ms =
                perf_data.avg_execution_time_ms * 0.8 + new_time * 0.2; // Weighted average
        }

        // Update satisfaction score
        if let Some(confidence) = result.metrics.confidence_score {
            perf_data.user_satisfaction_scores.push(confidence);
            // Keep only recent scores
            if perf_data.user_satisfaction_scores.len() > 100 {
                perf_data.user_satisfaction_scores.remove(0);
            }
        }

        perf_data.last_used = Some(Utc::now());

        // Record in history
        let mut history = self.routing_history.write().await;
        history.push(RoutingRecord {
            timestamp: Utc::now(),
            subtask_type: "unknown".to_string(), // Would need to be passed in
            required_capabilities: Vec::new(),   // Would need to be passed in
            selected_agent_id: decision.assigned_agent_id,
            success: result.success,
            execution_time_ms: result.metrics.execution_time_ms,
            user_satisfaction: result.metrics.confidence_score,
            alternative_agents: decision.backup_agents.iter()
                .map(|(id, _, score)| (*id, *score))
                .collect(),
        });

        // Keep history size manageable
        if history.len() > 10000 {
            history.remove(0);
        }

        Ok(())
    }

    /// Set agent registry
    pub async fn set_agent_registry(&self, registry: AgentRegistry) {
        let mut agent_registry = self.agent_registry.write().await;
        *agent_registry = registry;
    }

    /// Get routing analytics
    pub async fn get_analytics(&self) -> Result<RoutingAnalytics> {
        let history = self.routing_history.read().await;
        let agent_performance = self.agent_performance.read().await;

        let total_decisions = history.len() as u64;
        let successful_decisions = history.iter().filter(|r| r.success).count() as u64;
        let routing_accuracy = if total_decisions > 0 {
            successful_decisions as f32 / total_decisions as f32
        } else {
            0.0
        };

        // Calculate top agents
        let mut agent_counts: HashMap<String, u64> = HashMap::new();
        for record in history.iter() {
            // Would need agent name mapping here
            let agent_id = record.selected_agent_id.to_string();
            *agent_counts.entry(agent_id).or_insert(0) += 1;
        }

        let mut top_agents: Vec<_> = agent_counts.into_iter().collect();
        top_agents.sort_by(|a, b| b.1.cmp(&a.1));
        top_agents.truncate(10);

        Ok(RoutingAnalytics {
            total_decisions,
            routing_accuracy,
            top_agents,
            routing_patterns: HashMap::new(), // Would need more complex analysis
            performance_trends: Vec::new(),   // Would need time-series analysis
            optimization_recommendations: vec![
                "Consider monitoring agent performance metrics".to_string(),
                "Adjust routing weights based on success rates".to_string(),
            ],
        })
    }
}

/// Decide whether to exploit (use best known) or explore (try new agents)
fn should_exploit(params: &LearningParameters) -> bool {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen::<f32>() > params.exploration_rate
}

impl Default for RoutingStrategyConfig {
    fn default() -> Self {
        Self {
            capability_weight: 0.4,
            performance_weight: 0.3,
            load_balance_weight: 0.2,
            specialization_weight: 0.1,
            min_performance_threshold: 0.6,
            max_agent_load: 5,
        }
    }
}

impl Default for LearningParameters {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            decay_factor: 0.95,
            exploration_rate: 0.1,
        }
    }
}

impl Default for IntelligentRouter {
    fn default() -> Self {
        Self::new()
    }
}