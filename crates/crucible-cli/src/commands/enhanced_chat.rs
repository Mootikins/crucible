use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::agents::backend::{Backend, ChatParams, Message};
use crate::agents::backend::ollama::OllamaBackend;
use crate::agents::card::AgentCard;
use crate::agents::registry::AgentRegistry as CardRegistry;
use crate::config::CliConfig;

// Import core agent types
use crucible_core::agent::{
    AgentDefinition, AgentRegistry, AgentQuery, AgentMatch,
    CapabilityMatcher, AgentStatus, SkillLevel, Personality, Verbosity,
    Capability, Skill
};

// Import our new modules
use super::performance_tracker::AgentPerformanceTracker;
use super::collaboration_manager::{CollaborationManager};

/// Enhanced chat message with agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedChatMessage {
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    /// Message role (user, assistant, system, tool)
    pub role: String,
    /// Message content
    pub content: String,
    /// Agent that generated this message
    pub agent_id: Option<Uuid>,
    /// Agent name at time of generation
    pub agent_name: Option<String>,
    /// Tool calls made by this message (for assistant messages)
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call results (for tool messages)
    pub tool_results: Option<Vec<ToolResult>>,
    /// Confidence score for this response
    pub confidence: Option<f32>,
    /// Processing time in milliseconds
    pub processing_time_ms: Option<u64>,
}

/// Tool call representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: serde_json::Value,
    /// Call ID for tracking
    pub call_id: String,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool name
    pub name: String,
    /// Call ID matching the tool call
    pub call_id: String,
    /// Result of the tool call
    pub result: String,
    /// Whether the call was successful
    pub success: bool,
}

/// Enhanced conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConversationHistory {
    /// Messages in chronological order
    pub messages: Vec<EnhancedChatMessage>,
    /// Conversation title (auto-generated or user-provided)
    pub title: Option<String>,
    /// Primary agent used for this conversation
    pub primary_agent_id: Uuid,
    /// Agent switches during conversation
    pub agent_switches: Vec<AgentSwitch>,
    /// Task suggestions made during conversation
    pub task_suggestions: Vec<TaskSuggestion>,
    /// Performance metrics
    pub performance_metrics: ConversationMetrics,
    /// Collaboration sessions
    pub collaboration_sessions: Vec<CollaborationSession>,
}

/// Agent switch event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSwitch {
    /// When the switch occurred
    pub timestamp: DateTime<Utc>,
    /// Previous agent ID
    pub from_agent_id: Uuid,
    /// New agent ID
    pub to_agent_id: Uuid,
    /// Reason for the switch
    pub reason: AgentSwitchReason,
    /// Context at time of switch
    pub context: String,
}

/// Reason for agent switch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentSwitchReason {
    /// User requested switch
    UserRequested,
    /// Automatic capability-based switch
    CapabilityMatch,
    /// Performance-based switch
    PerformanceOptimization,
    /// Tool requirement switch
    ToolRequirement,
    /// Collaboration initiation
    Collaboration,
}

/// Task suggestion for the user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSuggestion {
    /// Unique ID for this suggestion
    pub id: Uuid,
    /// When the suggestion was made
    pub timestamp: DateTime<Utc>,
    /// Suggested task description
    pub task_description: String,
    /// Agent that would be best for this task
    pub suggested_agent_id: Uuid,
    /// Suggested agent name
    pub suggested_agent_name: String,
    /// Confidence in this suggestion (0-1)
    pub confidence: f32,
    /// Why this suggestion was made
    pub reasoning: String,
    /// Whether the user accepted this suggestion
    pub accepted: Option<bool>,
}

/// Conversation performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationMetrics {
    /// Total messages exchanged
    pub total_messages: usize,
    /// Average response time
    pub avg_response_time_ms: u64,
    /// Agent switch count
    pub agent_switches: usize,
    /// Tool usage count
    pub tool_usage_count: usize,
    /// User satisfaction score (if provided)
    pub user_satisfaction: Option<f32>,
    /// Task completion rate
    pub task_completion_rate: f32,
}

// Import collaboration types from collaboration_manager
pub use super::collaboration_manager::{
    CollaborationSession, CollaborationState, CollaborationRole
};

impl EnhancedConversationHistory {
    /// Create a new enhanced conversation history
    pub fn new(primary_agent_id: Uuid) -> Self {
        Self {
            messages: Vec::new(),
            title: None,
            primary_agent_id,
            agent_switches: Vec::new(),
            task_suggestions: Vec::new(),
            performance_metrics: ConversationMetrics::default(),
            collaboration_sessions: Vec::new(),
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: EnhancedChatMessage) {
        self.messages.push(message);
        self.performance_metrics.total_messages = self.messages.len();
    }

    /// Record an agent switch
    pub fn record_agent_switch(&mut self, switch: AgentSwitch) {
        self.agent_switches.push(switch);
        self.performance_metrics.agent_switches = self.agent_switches.len();
    }

    /// Add a task suggestion
    pub fn add_task_suggestion(&mut self, suggestion: TaskSuggestion) {
        self.task_suggestions.push(suggestion);
    }

    /// Get the last N messages
    pub fn get_last_messages(&self, n: usize) -> &[EnhancedChatMessage] {
        let start = if self.messages.len() >= n {
            self.messages.len() - n
        } else {
            0
        };
        &self.messages[start..]
    }

    /// Get current active agent (last agent that was switched to)
    pub fn get_current_agent_id(&self) -> Uuid {
        self.agent_switches
            .last()
            .map(|switch| switch.to_agent_id)
            .unwrap_or(self.primary_agent_id)
    }

    /// Update performance metrics
    pub fn update_metrics(&mut self) {
        if self.messages.is_empty() {
            return;
        }

        let response_times: Vec<u64> = self.messages
            .iter()
            .filter_map(|msg| msg.processing_time_ms)
            .collect();

        if !response_times.is_empty() {
            self.performance_metrics.avg_response_time_ms =
                response_times.iter().sum::<u64>() / response_times.len() as u64;
        }

        let tool_usage = self.messages
            .iter()
            .filter(|msg| msg.tool_calls.is_some() || msg.tool_results.is_some())
            .count();
        self.performance_metrics.tool_usage_count = tool_usage;
    }

    /// Save conversation to file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize enhanced conversation history")?;
        std::fs::write(path, json)
            .context("Failed to write enhanced conversation history to file")?;
        Ok(())
    }

    /// Load conversation from file
    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .context("Failed to read enhanced conversation history from file")?;
        serde_json::from_str(&json)
            .context("Failed to deserialize enhanced conversation history")
    }

    /// Generate a title from the first user message
    pub fn generate_title(&mut self) {
        if let Some(first_user_msg) = self.messages.iter()
            .find(|msg| msg.role == "user") {
            let title = first_user_msg.content
                .lines()
                .next()
                .unwrap_or("Untitled Conversation")
                .chars()
                .take(50)
                .collect::<String>();
            self.title = Some(title);
        }
    }
}

/// Enhanced AI Agent with advanced capabilities
#[derive(Debug, Clone)]
pub struct EnhancedAgent {
    /// Agent definition
    pub definition: AgentDefinition,
    /// Agent card for backend configuration
    pub card: AgentCard,
    /// Performance metrics
    pub performance_metrics: AgentPerformanceMetrics,
    /// Collaboration preferences
    pub collaboration_preferences: CollaborationPreferences,
}

/// Agent performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPerformanceMetrics {
    /// Total tasks completed
    pub tasks_completed: usize,
    /// Average task completion time
    pub avg_completion_time_ms: u64,
    /// Success rate (0-1)
    pub success_rate: f32,
    /// User satisfaction score (0-1)
    pub user_satisfaction: f32,
    /// Tool usage efficiency
    pub tool_efficiency: f32,
    /// Collaboration success rate
    pub collaboration_success_rate: f32,
    /// Specialization score (how well it performs in its specialized areas)
    pub specialization_score: f32,
    /// Last time metrics were updated
    pub last_updated: Option<DateTime<Utc>>,
}

/// Collaboration preferences for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationPreferences {
    /// Preferred collaboration partners (agent IDs)
    pub preferred_partners: Vec<Uuid>,
    /// Collaboration styles this agent works well with
    pub compatible_styles: Vec<String>,
    /// Roles this agent prefers in collaborations
    pub preferred_roles: Vec<CollaborationRole>,
    /// Maximum concurrent collaborations
    pub max_concurrent_collaborations: usize,
}

/// Role an agent can take in collaboration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborationRole {
    /// Lead the collaboration
    Lead,
    /// Provide specialized expertise
    Specialist,
    /// Review and validate work
    Reviewer,
    /// Support role
    Support,
}

impl EnhancedAgent {
    /// Create an enhanced agent from definition and card
    pub fn new(definition: AgentDefinition, card: AgentCard) -> Self {
        Self {
            definition,
            card,
            performance_metrics: AgentPerformanceMetrics::default(),
            collaboration_preferences: CollaborationPreferences {
                preferred_partners: Vec::new(),
                compatible_styles: vec!["professional".to_string(), "collaborative".to_string()],
                preferred_roles: vec![CollaborationRole::Specialist],
                max_concurrent_collaborations: 3,
            },
        }
    }

    /// Get agent name
    pub fn name(&self) -> &str {
        &self.definition.name
    }

    /// Get agent ID
    pub fn id(&self) -> Uuid {
        self.definition.id
    }

    /// Check if agent has a specific capability
    pub fn has_capability(&self, capability: &str) -> bool {
        self.definition.capabilities.iter().any(|cap| cap.name == capability)
    }

    /// Get agent's skill level in a capability
    pub fn get_skill_level(&self, capability: &str) -> Option<&SkillLevel> {
        self.definition.capabilities.iter()
            .find(|cap| cap.name == capability)
            .map(|cap| &cap.skill_level)
    }

    /// Update performance metrics
    pub fn update_performance(&mut self, task_completed: bool, completion_time_ms: u64, user_satisfaction: Option<f32>) {
        self.performance_metrics.tasks_completed += 1;

        // Update average completion time
        let total_time = self.performance_metrics.avg_completion_time_ms *
            (self.performance_metrics.tasks_completed - 1) as u64 + completion_time_ms;
        self.performance_metrics.avg_completion_time_ms = total_time / self.performance_metrics.tasks_completed as u64;

        // Update success rate
        if task_completed {
            let successes = (self.performance_metrics.success_rate * (self.performance_metrics.tasks_completed - 1) as f32 + 1.0);
            self.performance_metrics.success_rate = successes / self.performance_metrics.tasks_completed as f32;
        } else {
            let successes = self.performance_metrics.success_rate * (self.performance_metrics.tasks_completed - 1) as f32;
            self.performance_metrics.success_rate = successes / self.performance_metrics.tasks_completed as f32;
        }

        // Update user satisfaction if provided
        if let Some(satisfaction) = user_satisfaction {
            let total_satisfaction = self.performance_metrics.user_satisfaction *
                (self.performance_metrics.tasks_completed - 1) as f32 + satisfaction;
            self.performance_metrics.user_satisfaction = total_satisfaction / self.performance_metrics.tasks_completed as f32;
        }

        self.performance_metrics.last_updated = Some(Utc::now());
    }

    /// Get collaboration compatibility score with another agent
    pub fn get_compatibility_score(&self, other: &EnhancedAgent) -> f32 {
        let mut score = 0.5; // Base score

        // Check for preferred partner status
        if self.collaboration_preferences.preferred_partners.contains(&other.id()) {
            score += 0.3;
        }

        // Check for complementary capabilities
        let complementary_caps = other.definition.capabilities.iter()
            .filter(|cap| !self.has_capability(&cap.name))
            .count();
        score += (complementary_caps as f32 / other.definition.capabilities.len() as f32) * 0.2;

        // Check for shared tools (can be both positive and negative)
        let shared_tools = other.definition.required_tools.iter()
            .filter(|tool| self.definition.required_tools.contains(*tool))
            .count();
        if shared_tools > 0 {
            score += 0.1;
        }

        score.min(1.0)
    }
}

/// Enhanced agent registry that combines both systems
#[derive(Debug)]
pub struct EnhancedAgentRegistry {
    /// Core agent registry with advanced definitions
    core_registry: AgentRegistry,
    /// Simple agent card registry
    card_registry: CardRegistry,
    /// Enhanced agents (combination of both)
    enhanced_agents: HashMap<Uuid, EnhancedAgent>,
    /// Capability matcher for intelligent suggestions
    capability_matcher: CapabilityMatcher,
    /// Performance tracker for learning
    performance_tracker: AgentPerformanceTracker,
    /// Collaboration manager
    collaboration_manager: CollaborationManager,
}

impl EnhancedAgentRegistry {
    /// Create a new enhanced agent registry
    pub fn new() -> Self {
        Self {
            core_registry: AgentRegistry::default(),
            card_registry: CardRegistry::new(),
            enhanced_agents: HashMap::new(),
            capability_matcher: CapabilityMatcher::new(),
            performance_tracker: AgentPerformanceTracker::new(),
            collaboration_manager: CollaborationManager::new(),
        }
    }

    /// Add vault path for agent card discovery
    pub fn add_vault_path<P: AsRef<std::path::Path>>(&mut self, path: P) {
        self.card_registry.add_vault_path(path);
    }

    /// Load agents from both systems
    pub async fn load_agents(&mut self) -> Result<usize> {
        // Load agent cards
        self.card_registry.load_agents()
            .context("Failed to load agent cards")?;

        // Convert agent cards to enhanced agents
        let mut loaded_count = 0;
        for card in self.card_registry.list_agents() {
            let definition = self.convert_card_to_definition(card)?;
            let enhanced_agent = EnhancedAgent::new(definition.clone(), card.clone());
            self.enhanced_agents.insert(definition.id, enhanced_agent);
            loaded_count += 1;
        }

        info!("Loaded {} enhanced agents", loaded_count);
        Ok(loaded_count)
    }

    /// Convert agent card to full agent definition
    fn convert_card_to_definition(&self, card: &AgentCard) -> Result<AgentDefinition> {
        let now = Utc::now();

        // Convert capabilities string list to Capability structs
        let capabilities = card.capabilities.iter().enumerate().map(|(i, cap_name)| {
            Capability {
                name: cap_name.clone(),
                description: format!("Capability in {}", cap_name),
                skill_level: SkillLevel::Intermediate, // Default skill level
                required_tools: Vec::new(),
            }
        }).collect();

        // Convert tags to skills (basic implementation)
        let skills = card.tags.iter().enumerate().map(|(i, tag)| {
            Skill {
                name: tag.clone(),
                category: "general".to_string(),
                proficiency: 7, // Default proficiency
                experience_years: 2.0,
                certifications: Vec::new(),
            }
        }).collect();

        Ok(AgentDefinition {
            id: Uuid::new_v4(),
            name: card.name.clone(),
            version: "1.0.0".to_string(),
            description: format!("Agent with capabilities: {}", card.capabilities.join(", ")),
            capabilities,
            required_tools: Vec::new(), // Could be extracted from backend config
            optional_tools: Vec::new(),
            tags: card.tags.clone(),
            personality: Personality {
                tone: "professional".to_string(),
                style: "helpful".to_string(),
                verbosity: Verbosity::Moderate,
                traits: vec!["helpful".to_string(), "intelligent".to_string()],
                preferences: HashMap::new(),
            },
            system_prompt: card.system_prompt.clone(),
            skills,
            config: HashMap::new(),
            dependencies: Vec::new(),
            created_at: now,
            updated_at: now,
            status: AgentStatus::Active,
            author: Some(card.owner.clone()),
            documentation_url: None,
        })
    }

    /// Get an enhanced agent by ID
    pub fn get_enhanced_agent(&self, id: &Uuid) -> Option<&EnhancedAgent> {
        self.enhanced_agents.get(id)
    }

    /// Get an enhanced agent by name
    pub fn get_enhanced_agent_by_name(&self, name: &str) -> Option<&EnhancedAgent> {
        self.enhanced_agents.values()
            .find(|agent| agent.name() == name)
    }

    /// Find best agents for a given task
    pub fn find_best_agents_for_task(&self, task_description: &str, required_capabilities: &[String]) -> Vec<AgentMatch> {
        // Create agents hashmap for matching
        let agents_map: HashMap<String, AgentDefinition> = self.enhanced_agents
            .iter()
            .map(|(id, agent)| (agent.name().to_string(), agent.definition.clone()))
            .collect();

        let query = AgentQuery {
            capabilities: required_capabilities.to_vec(),
            skills: Vec::new(),
            tags: Vec::new(),
            required_tools: Vec::new(),
            min_skill_level: Some(SkillLevel::Intermediate),
            status: Some(AgentStatus::Active),
            text_search: Some(task_description.to_string()),
        };

        let mut matches = self.capability_matcher.find_matching_agents(&agents_map, &query);

        // Sort by performance metrics
        matches.sort_by(|a, b| {
            let a_perf = self.enhanced_agents.get(&a.agent.id)
                .map(|agent| agent.performance_metrics.success_rate)
                .unwrap_or(0.0);
            let b_perf = self.enhanced_agents.get(&b.agent.id)
                .map(|agent| agent.performance_metrics.success_rate)
                .unwrap_or(0.0);

            // First sort by performance, then by match score
            b_perf.partial_cmp(&a_perf)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.score.cmp(&a.score))
        });

        matches
    }

    /// Suggest agent switch based on context
    pub fn suggest_agent_switch(&self, current_agent_id: &Uuid, context: &str) -> Option<(Uuid, String)> {
        let current_agent = match self.enhanced_agents.get(current_agent_id) {
            Some(agent) => agent,
            None => return None,
        };

        // Analyze context to determine required capabilities
        let suggested_capabilities = self.analyze_context_for_capabilities(context);

        if suggested_capabilities.is_empty() {
            return None;
        }

        // Find better agents for these capabilities
        let matches = self.find_best_agents_for_task(context, &suggested_capabilities);

        if let Some(best_match) = matches.first() {
            // Check if the suggested agent is significantly better
            if best_match.score > 50 && best_match.agent.id != *current_agent_id {
                return Some((best_match.agent.id.clone(),
                            format!("Better match for {}: {} (score: {})",
                                   suggested_capabilities.join(", "),
                                   best_match.agent.name,
                                   best_match.score)));
            }
        }

        None
    }

    /// Analyze context to determine required capabilities
    fn analyze_context_for_capabilities(&self, context: &str) -> Vec<String> {
        let mut capabilities = Vec::new();
        let context_lower = context.to_lowercase();

        // Simple keyword-based capability detection
        if context_lower.contains("code") || context_lower.contains("programming") || context_lower.contains("rust") {
            capabilities.push("coding".to_string());
        }
        if context_lower.contains("research") || context_lower.contains("analyze") || context_lower.contains("investigate") {
            capabilities.push("research".to_string());
        }
        if context_lower.contains("write") || context_lower.contains("document") || context_lower.contains("draft") {
            capabilities.push("writing".to_string());
        }
        if context_lower.contains("data") || context_lower.contains("database") || context_lower.contains("query") {
            capabilities.push("data_analysis".to_string());
        }

        capabilities
    }

    /// Get all enhanced agents
    pub fn list_enhanced_agents(&self) -> Vec<&EnhancedAgent> {
        self.enhanced_agents.values().collect()
    }

    /// Update agent performance metrics
    pub fn update_agent_performance(&mut self, agent_id: &Uuid, task_completed: bool,
                                 completion_time_ms: u64, user_satisfaction: Option<f32>) {
        if let Some(agent) = self.enhanced_agents.get_mut(agent_id) {
            agent.update_performance(task_completed, completion_time_ms, user_satisfaction);
        }

        // Also update the performance tracker
        self.performance_tracker.record_performance(
            agent_id, task_completed, completion_time_ms, user_satisfaction
        );
    }

    /// Get agent ranking by performance
    pub fn get_agent_ranking(&self) -> Vec<(Uuid, String, f32)> {
        let mut rankings: Vec<(Uuid, String, f32)> = self.enhanced_agents
            .iter()
            .map(|(id, agent)| {
                let score = self.calculate_agent_score(agent);
                (*id, agent.name().to_string(), score)
            })
            .collect();

        rankings.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        rankings
    }

    /// Calculate overall agent score for ranking
    fn calculate_agent_score(&self, agent: &EnhancedAgent) -> f32 {
        let perf = &agent.performance_metrics;

        // Weighted score based on different metrics
        let success_weight = 0.4;
        let satisfaction_weight = 0.3;
        let efficiency_weight = 0.2;
        let specialization_weight = 0.1;

        perf.success_rate * success_weight +
        perf.user_satisfaction * satisfaction_weight +
        perf.tool_efficiency * efficiency_weight +
        perf.specialization_score * specialization_weight
    }

    /// Find collaboration partners for an agent
    pub fn find_collaboration_partners(&self, agent_id: &Uuid, task: &str) -> Vec<(Uuid, f32)> {
        let main_agent = match self.enhanced_agents.get(agent_id) {
            Some(agent) => agent,
            None => return Vec::new(),
        };

        let mut partners = Vec::new();

        for (id, agent) in &self.enhanced_agents {
            if *id == *agent_id {
                continue;
            }

            let compatibility = main_agent.get_compatibility_score(agent);
            if compatibility > 0.5 { // Only suggest compatible partners
                partners.push((*id, compatibility));
            }
        }

        // Sort by compatibility score
        partners.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        partners
    }

    /// Get performance tracker reference
    pub fn get_performance_tracker(&self) -> &AgentPerformanceTracker {
        &self.performance_tracker
    }

    /// Get collaboration manager reference
    pub fn get_collaboration_manager(&self) -> &CollaborationManager {
        &self.collaboration_manager
    }
}

impl Default for EnhancedAgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}