use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::enhanced_chat::{
    EnhancedChatMessage, EnhancedConversationHistory, EnhancedAgentRegistry,
    ToolCall, ToolResult, AgentSwitch, AgentSwitchReason, TaskSuggestion,
    CollaborationSession, CollaborationState
};
use super::performance_tracker::PerformanceRecord;
use super::collaboration_manager::{
    CollaborationManager, CollaborationMessage, CollaborationMessageType,
    CollaborationParticipant, CollaborationRole, ParticipantStatus
};
use crate::agents::backend::{Backend, ChatParams, Message};
use crate::config::CliConfig;
use crate::cli::AgentCommands;
use std::path::PathBuf;

/// Enhanced chat session with advanced agent management
pub struct EnhancedChatSession {
    /// Configuration
    config: CliConfig,
    /// Enhanced agent registry
    agent_registry: EnhancedAgentRegistry,
    /// Current conversation history
    history: EnhancedConversationHistory,
    /// Current active agent ID
    current_agent_id: Uuid,
    /// Backend for generating responses
    backend: Option<Box<dyn Backend>>,
    /// Task suggestion engine
    task_suggester: TaskSuggestionEngine,
    /// Agent switch predictor
    switch_predictor: AgentSwitchPredictor,
    /// Performance tracking enabled
    performance_tracking: bool,
}

/// Engine for suggesting tasks and agents
#[derive(Debug)]
pub struct TaskSuggestionEngine {
    /// Suggestion history
    suggestion_history: Vec<TaskSuggestion>,
    /// Learning from user interactions
    user_preference_learning: UserPreferenceLearning,
}

/// Learning system for user preferences
#[derive(Debug)]
pub struct UserPreferenceLearning {
    /// Accepted suggestions by category
    accepted_suggestions: HashMap<String, usize>,
    /// Rejected suggestions by category
    rejected_suggestions: HashMap<String, usize>,
    /// Preferred agent patterns
    preferred_agents: HashMap<String, Uuid>,
}

/// Predictor for agent switches
#[derive(Debug)]
pub struct AgentSwitchPredictor {
    /// Switch history
    switch_history: Vec<AgentSwitch>,
    /// Context analysis weights
    context_weights: ContextWeights,
}

/// Weights for context analysis
#[derive(Debug, Clone)]
pub struct ContextWeights {
    /// Weight for capability mismatch
    pub capability_weight: f32,
    /// Weight for tool requirement
    pub tool_weight: f32,
    /// Weight for performance issues
    pub performance_weight: f32,
    /// Weight for user feedback
    pub feedback_weight: f32,
}

impl Default for ContextWeights {
    fn default() -> Self {
        Self {
            capability_weight: 0.4,
            tool_weight: 0.3,
            performance_weight: 0.2,
            feedback_weight: 0.1,
        }
    }
}

impl EnhancedChatSession {
    /// Create a new enhanced chat session
    pub async fn new(
        config: CliConfig,
        agent_name: &str,
        model: Option<String>,
        performance_tracking: bool,
    ) -> Result<Self> {
        let mut agent_registry = EnhancedAgentRegistry::new();

        // Add kiln paths for agent discovery
        if let Ok(kiln_path) = config.kiln_path_str() {
            agent_registry.add_vault_path(std::path::Path::new(&kiln_path));
        }

        // Load agents
        let loaded_count = agent_registry.load_agents().await
            .context("Failed to load agents")?;

        if loaded_count == 0 {
            warn!("No agents loaded. Please check your agent configuration.");
        }

        // Get the requested agent or default to first available
        let current_agent = agent_registry.get_enhanced_agent_by_name(agent_name)
            .or_else(|| agent_registry.list_enhanced_agents().first())
            .ok_or_else(|| anyhow::anyhow!("No agents available"))?;

        let current_agent_id = current_agent.id();

        let history = EnhancedConversationHistory::new(current_agent_id);

        // Initialize backend
        let backend = Some(Box::new(crate::agents::backend::ollama::OllamaBackend::new(
            config.ollama_endpoint()
        )) as Box<dyn Backend>);

        Ok(Self {
            config,
            agent_registry,
            history,
            current_agent_id,
            backend,
            task_suggester: TaskSuggestionEngine::new(),
            switch_predictor: AgentSwitchPredictor::new(),
            performance_tracking,
        })
    }

    /// Start the enhanced interactive chat session
    pub async fn start(&mut self, start_message: Option<String>) -> Result<()> {
        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id)
            .unwrap(); // Should always exist

        println!("🤖 Enhanced Crucible Chat - Agent: {}", current_agent.name());
        println!("🎯 Performance tracking: {}",
                if self.performance_tracking { "enabled" } else { "disabled" });
        println!("Type 'help' for commands, 'quit' or Ctrl+C to exit.\n");

        // Add system message to history
        let system_message = EnhancedChatMessage {
            timestamp: Utc::now(),
            role: "system".to_string(),
            content: current_agent.definition.system_prompt.clone(),
            agent_id: Some(current_agent.id()),
            agent_name: Some(current_agent.name().to_string()),
            tool_calls: None,
            tool_results: None,
            confidence: Some(1.0),
            processing_time_ms: None,
        };
        self.history.add_message(system_message);

        // Process start message if provided
        if let Some(message) = start_message {
            self.process_user_message(&message).await?;
        }

        // Main REPL loop
        loop {
            let input = self.read_user_input()?;

            match input.trim() {
                "quit" | "exit" | ":q" => break,
                "help" | ":h" => self.show_help(),
                "clear" | ":c" => {
                    self.history.messages.clear();
                    println!("Conversation history cleared.");
                },
                "agents" => {
                    self.show_available_agents();
                },
                "suggest" => {
                    self.show_task_suggestions().await?;
                },
                "rankings" => {
                    self.show_agent_rankings();
                },
                "collaborate" => {
                    self.start_collaboration_mode().await?;
                },
                "switch" => {
                    self.suggest_agent_switch().await?;
                },
                "performance" => {
                    self.show_performance_insights();
                },
                cmd if cmd.starts_with(":agent ") => {
                    let new_agent = cmd.strip_prefix(":agent ").unwrap().trim();
                    self.switch_to_agent(new_agent, AgentSwitchReason::UserRequested).await?;
                },
                cmd if cmd.starts_with(":collaborate ") => {
                    let task = cmd.strip_prefix(":collaborate ").unwrap().trim();
                    self.start_collaboration_for_task(task).await?;
                },
                message if !message.trim().is_empty() => {
                    if let Err(e) = self.process_user_message(message).await {
                        error!("Error processing message: {}", e);
                        println!("❌ Error: {}", e);
                    }
                },
                _ => continue,
            }
        }

        // Save conversation before exiting
        if let Err(e) = self.save_history() {
            warn!("Failed to save conversation history: {}", e);
        }

        println!("👋 Goodbye!");
        Ok(())
    }

    /// Process a user message with enhanced features
    async fn process_user_message(&mut self, message: &str) -> Result<()> {
        let start_time = Instant::now();

        // Add user message to history
        let user_message = EnhancedChatMessage {
            timestamp: Utc::now(),
            role: "user".to_string(),
            content: message.to_string(),
            agent_id: None,
            agent_name: None,
            tool_calls: None,
            tool_results: None,
            confidence: None,
            processing_time_ms: None,
        };
        self.history.add_message(user_message);

        // Analyze message for potential agent switch suggestions
        if let Some((suggested_agent_id, reason)) = self.analyze_for_agent_switch(message) {
            println!("💡 Suggestion: Consider switching to agent for better assistance");
            println!("   Reason: {}", reason);

            // Store suggestion
            let suggestion = TaskSuggestion {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                task_description: format!("Continue with: {}", message),
                suggested_agent_id,
                suggested_agent_name: self.agent_registry.get_enhanced_agent(&suggested_agent_id)
                    .map(|agent| agent.name().to_string())
                    .unwrap_or_else(|| "Unknown".to_string()),
                confidence: 0.8,
                reasoning: reason,
                accepted: None,
            };
            self.history.add_task_suggestion(suggestion);
        }

        // Generate and process task suggestions
        let suggestions = self.task_suggester.generate_suggestions(
            message, &self.agent_registry, &self.history
        );

        if !suggestions.is_empty() {
            println!("🎯 Task suggestions:");
            for (i, suggestion) in suggestions.iter().take(3).enumerate() {
                println!("   {}. {} (agent: {}, confidence: {:.1}%)",
                        i + 1, suggestion.task_description,
                        suggestion.suggested_agent_name,
                        suggestion.confidence * 100.0);
            }
            println!("   Use ':accept {}' to accept a suggestion", 1);
        }

        // Generate response using current agent
        let response = self.generate_response(message).await?;
        let processing_time = start_time.elapsed().as_millis() as u64;

        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();

        // Add assistant response to history
        let assistant_message = EnhancedChatMessage {
            timestamp: Utc::now(),
            role: "assistant".to_string(),
            content: response.clone(),
            agent_id: Some(current_agent.id()),
            agent_name: Some(current_agent.name().to_string()),
            tool_calls: None,
            tool_results: None,
            confidence: Some(0.9), // Could be calculated based on model confidence
            processing_time_ms: Some(processing_time),
        };
        self.history.add_message(assistant_message);

        // Update conversation metrics
        self.history.update_metrics();

        // Track performance if enabled
        if self.performance_tracking {
            self.track_interaction_performance(processing_time).await;
        }

        println!("🤖 {}", response);

        Ok(())
    }

    /// Generate response using current agent and backend
    async fn generate_response(&self, message: &str) -> Result<String> {
        let backend = match &self.backend {
            Some(backend) => backend,
            None => return Ok("Backend not available".to_string()),
        };

        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();

        // Convert conversation history to backend messages
        let mut backend_messages = Vec::new();

        // Add system message
        backend_messages.push(Message::system(&current_agent.definition.system_prompt));

        // Add recent conversation history (last 10 messages)
        for chat_msg in self.history.get_last_messages(10) {
            match chat_msg.role.as_str() {
                "user" => backend_messages.push(Message::user(&chat_msg.content)),
                "assistant" => backend_messages.push(Message::assistant(&chat_msg.content)),
                _ => continue, // Skip system and tool messages for now
            }
        }

        // Prepare chat parameters
        let params = ChatParams {
            model: current_agent.card.backend.model().unwrap_or_else(|| self.config.chat_model()),
            temperature: current_agent.card.temperature.or(Some(self.config.temperature())),
            max_tokens: current_agent.card.max_tokens.or(Some(self.config.max_tokens())),
        };

        // Generate response using the backend
        match backend.chat(backend_messages, &params).await {
            Ok(response) => Ok(response),
            Err(e) => {
                error!("Failed to generate response: {}", e);
                Ok(format!("Sorry, I encountered an error: {}", e))
            }
        }
    }

    /// Analyze message for potential agent switch suggestions
    fn analyze_for_agent_switch(&self, message: &str) -> Option<(Uuid, String)> {
        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();

        // Get recent context from conversation
        let recent_context: String = self.history.get_last_messages(5)
            .iter()
            .filter_map(|msg| {
                if msg.role == "user" || msg.role == "assistant" {
                    Some(msg.content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let full_context = format!("{} {}", recent_context, message);

        // Use the agent registry to suggest switches
        self.agent_registry.suggest_agent_switch(&self.current_agent_id, &full_context)
    }

    /// Switch to a different agent
    async fn switch_to_agent(&mut self, agent_name: &str, reason: AgentSwitchReason) -> Result<()> {
        let new_agent = self.agent_registry.get_enhanced_agent_by_name(agent_name)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_name))?;

        let old_agent_id = self.current_agent_id;
        self.current_agent_id = new_agent.id();

        // Record the switch
        let switch = AgentSwitch {
            timestamp: Utc::now(),
            from_agent_id: old_agent_id,
            to_agent_id: new_agent.id(),
            reason: reason.clone(),
            context: self.history.get_last_messages(3)
                .iter()
                .map(|msg| msg.content.clone())
                .collect::<Vec<_>>()
                .join(" "),
        };

        self.history.record_agent_switch(switch);

        let old_agent_name = self.agent_registry.get_enhanced_agent(&old_agent_id)
            .map(|agent| agent.name().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        println!("🔄 Switched from {} to {} (reason: {:?})",
                old_agent_name, new_agent.name(), reason);

        // Add system message about the switch
        let switch_message = EnhancedChatMessage {
            timestamp: Utc::now(),
            role: "system".to_string(),
            content: format!("Switched to agent: {}", new_agent.name()),
            agent_id: Some(new_agent.id()),
            agent_name: Some(new_agent.name().to_string()),
            tool_calls: None,
            tool_results: None,
            confidence: Some(1.0),
            processing_time_ms: None,
        };
        self.history.add_message(switch_message);

        Ok(())
    }

    /// Suggest agent switch to user
    async fn suggest_agent_switch(&mut self) -> Result<()> {
        let recent_context: String = self.history.get_last_messages(5)
            .iter()
            .filter_map(|msg| {
                if msg.role == "user" || msg.role == "assistant" {
                    Some(msg.content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        if let Some((suggested_agent_id, reason)) = self.analyze_for_agent_switch(&recent_context) {
            let suggested_agent = self.agent_registry.get_enhanced_agent(&suggested_agent_id).unwrap();

            println!("💡 Agent Switch Suggestion:");
            println!("   Current agent: {}",
                    self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap().name());
            println!("   Suggested agent: {}", suggested_agent.name());
            println!("   Reason: {}", reason);
            println!("   Use ':agent {}' to switch", suggested_agent.name());
        } else {
            println!("ℹ️  No agent switch suggestions at this time.");
            println!("   Current agent is well-suited for the current conversation.");
        }

        Ok(())
    }

    /// Start collaboration mode
    async fn start_collaboration_mode(&mut self) -> Result<()> {
        println!("🤝 Collaboration Mode");
        println!("Enter a task description for multi-agent collaboration:");

        let task = self.read_user_input()?;
        self.start_collaboration_for_task(&task).await
    }

    /// Start collaboration for a specific task
    async fn start_collaboration_for_task(&mut self, task: &str) -> Result<()> {
        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();
        let available_agents: Vec<(Uuid, &str)> = self.agent_registry.list_enhanced_agents()
            .iter()
            .map(|agent| (agent.id(), agent.name()))
            .filter(|(id, _)| *id != current_agent.id())
            .collect();

        let suggestions = self.agent_registry.get_collaboration_manager()
            .suggest_collaboration_partners(&current_agent.id(), task, &available_agents);

        if suggestions.is_empty() {
            println!("ℹ️  No suitable collaboration partners found for this task.");
            return Ok(());
        }

        println!("🤝 Suggested collaboration partners for task: {}", task);
        for (i, (agent_id, compatibility)) in suggestions.iter().take(3).enumerate() {
            if let Some(agent) = self.agent_registry.get_enhanced_agent(agent_id) {
                println!("   {}. {} (compatibility: {:.1}%)",
                        i + 1, agent.name(), compatibility * 100.0);
            }
        }

        println!("   Use ':collaborate with <agent_name>' to start collaboration");

        Ok(())
    }

    /// Show available agents with enhanced information
    fn show_available_agents(&self) {
        let agents = self.agent_registry.list_enhanced_agents();

        println!("🤖 Available Agents:");
        for agent in agents {
            let current_marker = if agent.id() == self.current_agent_id { " →" } else { "  " };

            println!("{}{} - {}", current_marker, agent.name(), agent.definition.description);
            println!("     Capabilities: {}", agent.definition.capabilities
                    .iter().map(|cap| &cap.name).collect::<Vec<_>>().join(", "));

            if let Some(metrics) = self.agent_registry.get_performance_tracker()
                .get_metrics(&agent.id()) {
                println!("     Performance: {:.1}% success rate, {:.1} avg satisfaction",
                        metrics.success_rate * 100.0, metrics.avg_user_satisfaction);
            }
        }
    }

    /// Show task suggestions
    async fn show_task_suggestions(&self) -> Result<()> {
        let recent_context: String = self.history.get_last_messages(3)
            .iter()
            .filter_map(|msg| {
                if msg.role == "user" {
                    Some(msg.content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let suggestions = self.task_suggester.generate_suggestions(
            &recent_context, &self.agent_registry, &self.history
        );

        if suggestions.is_empty() {
            println!("ℹ️  No task suggestions at this time.");
        } else {
            println!("🎯 Task Suggestions:");
            for (i, suggestion) in suggestions.iter().enumerate() {
                println!("   {}. {} (agent: {}, confidence: {:.1}%)",
                        i + 1, suggestion.task_description,
                        suggestion.suggested_agent_name,
                        suggestion.confidence * 100.0);
            }
        }

        Ok(())
    }

    /// Show agent rankings
    fn show_agent_rankings(&self) {
        let rankings = self.agent_registry.get_agent_ranking();

        println!("🏆 Agent Rankings (by overall performance):");
        for (i, (id, name, score)) in rankings.iter().enumerate() {
            let current_marker = if *id == self.current_agent_id { " →" } else { "  " };
            println!("{}{}. {} (score: {:.1})", current_marker, i + 1, name, score);
        }
    }

    /// Show performance insights
    fn show_performance_insights(&self) {
        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();

        if let Some(insights) = self.agent_registry.get_performance_tracker()
            .get_learning_insights(&current_agent.id()) {

            println!("📊 Performance Insights for {}:", current_agent.name());

            if !insights.strengths.is_empty() {
                println!("   Strengths:");
                for strength in &insights.strengths {
                    println!("     ✓ {}", strength);
                }
            }

            if !insights.improvement_areas.is_empty() {
                println!("   Areas for Improvement:");
                for area in &insights.improvement_areas {
                    println!("     • {}", area);
                }
            }

            if !insights.training_recommendations.is_empty() {
                println!("   Training Recommendations:");
                for rec in &insights.training_recommendations {
                    println!("     → {}", rec);
                }
            }

            println!("   Performance Trend: {:?}", insights.performance_trend);
            println!("   Specialization Score: {:.1}", insights.specialization_score);
            println!("   Reliability Score: {:.1}", insights.reliability_score);
        } else {
            println!("ℹ️  No performance data available for current agent.");
        }
    }

    /// Track interaction performance
    async fn track_interaction_performance(&mut self, processing_time_ms: u64) {
        // This would be enhanced with actual success metrics and user feedback
        let success = true; // Placeholder - would be determined by actual outcomes
        let user_satisfaction = None; // Would be collected from user feedback

        self.agent_registry.update_agent_performance(
            &self.current_agent_id, success, processing_time_ms, user_satisfaction
        );
    }

    /// Read user input with enhanced prompt
    fn read_user_input(&self) -> Result<String> {
        let current_agent = self.agent_registry.get_enhanced_agent(&self.current_agent_id).unwrap();
        print!("❯ [{}] ", current_agent.name());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input)
    }

    /// Show enhanced help information
    fn show_help(&self) {
        println!("🤖 Enhanced Crucible Chat Commands:");
        println!("  help, :h              - Show this help message");
        println!("  quit, exit            - Exit the chat");
        println!("  clear, :c             - Clear conversation history");
        println!("  agents                - List available agents with performance");
        println!("  suggest               - Get task suggestions");
        println!("  rankings              - Show agent performance rankings");
        println!("  collaborate           - Start collaboration mode");
        println!("  switch                - Suggest agent switch");
        println!("  performance           - Show current agent performance insights");
        println!("  :agent <name>         - Switch to a specific agent");
        println!("  :collaborate <task>   - Start collaboration for task");
        println!();
        println!("💡 Enhanced Features:");
        println!("  - Intelligent agent suggestions based on context");
        println!("  - Performance tracking and learning");
        println!("  - Multi-agent collaboration workflows");
        println!("  - Dynamic agent switching");
        println!("  - Task capability matching");
        println!("  - Conversation context preservation");
    }

    /// Save conversation history with enhanced metadata
    fn save_history(&self) -> Result<()> {
        // Generate title if not set
        let mut history = self.history.clone();
        if history.title.is_none() {
            history.generate_title();
        }

        // Create filename from title and timestamp
        let title = history.title.as_deref().unwrap_or("enhanced_chat");
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("enhanced_{}_{}.json", title, timestamp);

        let history_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".crucible")
            .join("enhanced_chat_history")
            .join(filename);

        // Create directory if it doesn't exist
        if let Some(parent) = history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        history.save_to_file(&history_path)?;
        println!("💾 Enhanced conversation saved to: {}", history_path.display());
        Ok(())
    }
}

impl TaskSuggestionEngine {
    /// Create a new task suggestion engine
    pub fn new() -> Self {
        Self {
            suggestion_history: Vec::new(),
            user_preference_learning: UserPreferenceLearning::new(),
        }
    }

    /// Generate task suggestions based on context
    pub fn generate_suggestions(&mut self, context: &str,
                              agent_registry: &EnhancedAgentRegistry,
                              history: &EnhancedConversationHistory) -> Vec<TaskSuggestion> {
        let mut suggestions = Vec::new();

        // Analyze context for task types
        let task_types = self.analyze_context_for_tasks(context);

        for task_type in task_types {
            // Find best agents for this task type
            let matches = agent_registry.find_best_agents_for_task(
                &format!("{} task: {}", task_type, context),
                &[task_type.clone()]
            );

            if let Some(best_match) = matches.first() {
                let suggestion = TaskSuggestion {
                    id: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    task_description: format!("{} related to: {}", task_type, context),
                    suggested_agent_id: best_match.agent.id,
                    suggested_agent_name: best_match.agent.name.clone(),
                    confidence: (best_match.score as f32 / 100.0).min(1.0),
                    reasoning: format!("High match score: {} for {} capability",
                                     best_match.score, task_type),
                    accepted: None,
                };

                suggestions.push(suggestion);
            }
        }

        // Store suggestions for learning
        self.suggestion_history.extend(suggestions.clone());

        suggestions
    }

    /// Analyze context to identify potential task types
    fn analyze_context_for_tasks(&self, context: &str) -> Vec<String> {
        let mut task_types = Vec::new();
        let context_lower = context.to_lowercase();

        // Simple keyword-based task detection
        if context_lower.contains("code") || context_lower.contains("program") {
            task_types.push("coding".to_string());
        }
        if context_lower.contains("write") || context_lower.contains("document") {
            task_types.push("writing".to_string());
        }
        if context_lower.contains("research") || context_lower.contains("investigate") {
            task_types.push("research".to_string());
        }
        if context_lower.contains("analyze") || context_lower.contains("data") {
            task_types.push("analysis".to_string());
        }
        if context_lower.contains("design") || context_lower.contains("create") {
            task_types.push("design".to_string());
        }
        if context_lower.contains("review") || context_lower.contains("check") {
            task_types.push("review".to_string());
        }

        task_types
    }
}

impl UserPreferenceLearning {
    /// Create new user preference learning system
    pub fn new() -> Self {
        Self {
            accepted_suggestions: HashMap::new(),
            rejected_suggestions: HashMap::new(),
            preferred_agents: HashMap::new(),
        }
    }
}

impl AgentSwitchPredictor {
    /// Create new agent switch predictor
    pub fn new() -> Self {
        Self {
            switch_history: Vec::new(),
            context_weights: ContextWeights::default(),
        }
    }
}

/// Execute the enhanced chat command
pub async fn execute(
    config: CliConfig,
    agent: String,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    performance_tracking: bool,
    start_message: Option<String>,
    history_file: Option<PathBuf>,
) -> Result<()> {
    let mut session = EnhancedChatSession::new(
        config,
        &agent,
        model,
        performance_tracking,
    ).await?;

    // Load history if provided
    if let Some(history_path) = history_file {
        match EnhancedConversationHistory::load_from_file(&history_path) {
            Ok(history) => {
                session.history = history;
                println!("Loaded enhanced conversation history from: {}", history_path.display());
            },
            Err(e) => {
                tracing::warn!("Failed to load enhanced conversation history: {}", e);
            }
        }
    }

    session.start(start_message).await
}