use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use crucible_mcp::{McpServer, create_provider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::agents::backend::{Backend, ChatParams, Message};
use crate::agents::backend::ollama::OllamaBackend;
use crate::config::CliConfig;

/// Chat message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    /// Message role (user, assistant, system, tool)
    pub role: String,
    /// Message content
    pub content: String,
    /// Tool calls made by this message (for assistant messages)
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call results (for tool messages)
    pub tool_results: Option<Vec<ToolResult>>,
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

/// Conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHistory {
    /// Messages in chronological order
    pub messages: Vec<ChatMessage>,
    /// Conversation title (auto-generated or user-provided)
    pub title: Option<String>,
    /// Agent used for this conversation
    pub agent: String,
    /// Model used for this conversation
    pub model: String,
}

impl ConversationHistory {
    /// Create a new conversation history
    pub fn new(agent: String, model: String) -> Self {
        Self {
            messages: Vec::new(),
            title: None,
            agent,
            model,
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Get the last N messages
    pub fn get_last_messages(&self, n: usize) -> &[ChatMessage] {
        let start = if self.messages.len() >= n {
            self.messages.len() - n
        } else {
            0
        };
        &self.messages[start..]
    }

    /// Save conversation to file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize conversation history")?;
        std::fs::write(path, json)
            .context("Failed to write conversation history to file")?;
        Ok(())
    }

    /// Load conversation from file
    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .context("Failed to read conversation history from file")?;
        serde_json::from_str(&json)
            .context("Failed to deserialize conversation history")
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

/// AI Agent configuration and behavior
#[derive(Debug, Clone)]
pub struct Agent {
    /// Agent name
    pub name: String,
    /// System prompt for this agent
    pub system_prompt: String,
    /// Model to use for this agent
    pub model: Option<String>,
    /// Temperature for this agent
    pub temperature: Option<f32>,
    /// Maximum tokens for this agent
    pub max_tokens: Option<u32>,
    /// Whether this agent can use tools
    pub can_use_tools: bool,
}

impl Agent {
    /// Create a new agent
    pub fn new(name: String, system_prompt: String) -> Self {
        Self {
            name,
            system_prompt,
            model: None,
            temperature: None,
            max_tokens: None,
            can_use_tools: true,
        }
    }

    /// Set model for this agent
    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    /// Set temperature for this agent
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set max tokens for this agent
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set whether this agent can use tools
    pub fn with_tools(mut self, can_use_tools: bool) -> Self {
        self.can_use_tools = can_use_tools;
        self
    }
}

/// Agent registry for managing different agents
#[derive(Debug)]
pub struct AgentRegistry {
    agents: HashMap<String, Agent>,
}

impl AgentRegistry {
    /// Create a new agent registry
    pub fn new() -> Self {
        let mut agents = HashMap::new();

        // Default agent
        agents.insert("default".to_string(), Agent::new(
            "default".to_string(),
            "You are a helpful assistant with access to a knowledge management system. You can search through notes, create new ones, and help organize information. Be concise and helpful in your responses.".to_string(),
        ));

        // Research agent
        agents.insert("researcher".to_string(), Agent::new(
            "researcher".to_string(),
            "You are a research assistant with deep expertise in finding and synthesizing information. You have access to a comprehensive knowledge base and excel at finding connections between different pieces of information. Always cite your sources and provide detailed, well-structured responses.".to_string(),
        ));

        // Writing agent
        agents.insert("writer".to_string(), Agent::new(
            "writer".to_string(),
            "You are a writing assistant focused on clear, effective communication. You help draft, edit, and improve written content while maintaining the author's voice and intent. You have access to reference materials and can help structure documents properly.".to_string(),
        ));

        Self { agents }
    }

    /// Get an agent by name
    pub fn get(&self, name: &str) -> Option<&Agent> {
        self.agents.get(name)
    }

    /// Register a new agent
    pub fn register(&mut self, agent: Agent) {
        self.agents.insert(agent.name.clone(), agent);
    }

    /// List all available agents
    pub fn list_agents(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Chat session manager
pub struct ChatSession {
    /// Configuration
    config: CliConfig,
    /// Current agent
    agent: Agent,
    /// Conversation history
    history: ConversationHistory,
    /// MCP server for tool access
    mcp_server: Option<McpServer>,
    /// Agent registry
    agent_registry: AgentRegistry,
    /// LLM backend for generating responses
    backend: Option<Arc<dyn Backend>>,
}

impl ChatSession {
    /// Create a new chat session
    pub async fn new(
        config: CliConfig,
        agent_name: String,
        model: Option<String>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Self> {
        let agent_registry = AgentRegistry::new();

        let mut agent = agent_registry.get(&agent_name)
            .cloned()
            .unwrap_or_else(|| {
                warn!("Unknown agent '{}', using default agent", agent_name);
                agent_registry.get("default").unwrap().clone()
            });

        // Override agent settings with command line arguments
        if let Some(model) = model {
            agent.model = Some(model);
        }
        if let Some(temperature) = temperature {
            agent.temperature = Some(temperature);
        }
        if let Some(max_tokens) = max_tokens {
            agent.max_tokens = Some(max_tokens);
        }

        let model_name = agent.model
            .clone()
            .unwrap_or_else(|| config.chat_model());

        let history = ConversationHistory::new(agent.name.clone(), model_name);

        // Initialize MCP server if tools are enabled
        let mcp_server = if agent.can_use_tools {
            let embedding_config = config.to_embedding_config()?;
            let provider = create_provider(embedding_config).await?;
            let db_path = config.database_path_str()?;
            let server = McpServer::new(&db_path, provider).await?;
            server.start().await?;
            Some(server)
        } else {
            None
        };

        // Initialize LLM backend (Ollama for now)
        let backend = Some(Arc::new(OllamaBackend::new(config.ollama_endpoint())) as Arc<dyn Backend>);

        Ok(Self {
            config,
            agent,
            history,
            mcp_server,
            agent_registry,
            backend,
        })
    }

    /// Start the interactive chat session
    pub async fn start(&mut self, start_message: Option<String>) -> Result<()> {
        println!("🤖 Crucible Chat - Agent: {}", self.agent.name);
        println!("Type 'help' for commands, 'quit' or Ctrl+C to exit.\n");

        // Add system message to history
        let system_message = ChatMessage {
            timestamp: Utc::now(),
            role: "system".to_string(),
            content: self.agent.system_prompt.clone(),
            tool_calls: None,
            tool_results: None,
        };
        self.history.add_message(system_message);

        // If there's a start message, process it
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
                    let agents = self.agent_registry.list_agents();
                    println!("🤖 Available agents:");
                    for agent in agents {
                        if let Some(agent_info) = self.agent_registry.get(agent) {
                            let tools_status = if agent_info.can_use_tools { "🔧" } else { "💬" };
                            println!("  {} {} - {}", tools_status, agent_info.name,
                                agent_info.system_prompt.lines().next().unwrap_or("No description"));
                        }
                    }
                },
                "models" => {
                    if let Some(backend) = &self.backend {
                        match backend.list_models().await {
                            Ok(models) => {
                                println!("📋 Available models:");
                                for model in models {
                                    println!("  - {}", model.name);
                                    if let Some(details) = model.details {
                                        let info = vec![
                                            details.family.map(|f| format!("family: {}", f)),
                                            details.parameter_size.map(|s| format!("size: {}", s)),
                                            details.quantization_level.map(|q| format!("quant: {}", q)),
                                        ].into_iter().flatten().collect::<Vec<_>>().join(", ");
                                        if !info.is_empty() {
                                            println!("    {}", info);
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                println!("❌ Failed to list models: {}", e);
                            }
                        }
                    } else {
                        println!("❌ No backend available");
                    }
                },
                "save" => {
                    if let Err(e) = self.save_history() {
                        error!("Failed to save history: {}", e);
                    } else {
                        println!("💾 Conversation history saved.");
                    }
                },
                "tools" => {
                    if self.agent.can_use_tools {
                        let tools = McpServer::get_tools();
                        println!("🔧 Available MCP tools:");
                        for tool in tools {
                            println!("  - {}: {}", tool.name, tool.description);
                        }
                    } else {
                        println!("🚫 Tools are disabled for current agent '{}'.", self.agent.name);
                    }
                },
                "history" => {
                    self.show_history_summary();
                },
                cmd if cmd.starts_with(":agent ") => {
                    let new_agent = cmd.strip_prefix(":agent ").unwrap().trim();
                    if let Some(agent) = self.agent_registry.get(new_agent) {
                        self.agent = agent.clone();
                        println!("🤖 Switched to agent: {}", self.agent.name);
                    } else {
                        println!("❌ Unknown agent: {}", new_agent);
                    }
                },
                cmd if cmd.starts_with(":model ") => {
                    let new_model = cmd.strip_prefix(":model ").unwrap().trim();
                    self.history.model = new_model.to_string();
                    println!("📋 Switched to model: {}", new_model);
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

    /// Read user input with a prompt
    fn read_user_input(&self) -> Result<String> {
        print!("❯ ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input)
    }

    /// Process a user message and get a response
    async fn process_user_message(&mut self, message: &str) -> Result<()> {
        // Add user message to history
        let user_message = ChatMessage {
            timestamp: Utc::now(),
            role: "user".to_string(),
            content: message.to_string(),
            tool_calls: None,
            tool_results: None,
        };
        self.history.add_message(user_message);

        // Generate response (placeholder for now)
        println!("🤖 Processing...");

        // TODO: Implement actual LLM integration here
        let response = self.generate_response(message).await?;

        // Add assistant response to history
        let assistant_message = ChatMessage {
            timestamp: Utc::now(),
            role: "assistant".to_string(),
            content: response.clone(),
            tool_calls: None,
            tool_results: None,
        };
        self.history.add_message(assistant_message);

        println!("🤖 {}", response);
        Ok(())
    }

    /// Generate a response using the configured backend
    async fn generate_response(&self, _message: &str) -> Result<String> {
        let backend = match &self.backend {
            Some(backend) => backend,
            None => return Ok("Backend not available".to_string()),
        };

        // Convert conversation history to backend messages
        let mut backend_messages = Vec::new();

        // Add system message if not present
        if !self.history.messages.iter().any(|msg| msg.role == "system") {
            backend_messages.push(Message::system(&self.agent.system_prompt));
        }

        // Add last 10 messages (excluding system messages which we add above)
        for chat_msg in self.history.get_last_messages(10) {
            if chat_msg.role == "user" {
                backend_messages.push(Message::user(&chat_msg.content));
            } else if chat_msg.role == "assistant" {
                backend_messages.push(Message::assistant(&chat_msg.content));
            }
            // Skip system and tool messages for now
        }

        // Prepare chat parameters
        let params = ChatParams {
            model: self.history.model.clone(),
            temperature: self.agent.temperature.or(Some(self.config.temperature())),
            max_tokens: self.agent.max_tokens.or(Some(self.config.max_tokens())),
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

    /// Show conversation history summary
    fn show_history_summary(&self) {
        println!("📚 Conversation Summary:");
        println!("  Agent: {}", self.agent.name);
        println!("  Model: {}", self.history.model);
        println!("  Messages: {}", self.history.messages.len());

        if let Some(title) = &self.history.title {
            println!("  Title: {}", title);
        }

        let user_messages = self.history.messages.iter()
            .filter(|msg| msg.role == "user")
            .count();
        let assistant_messages = self.history.messages.iter()
            .filter(|msg| msg.role == "assistant")
            .count();

        println!("  User messages: {}", user_messages);
        println!("  Assistant messages: {}", assistant_messages);

        if !self.history.messages.is_empty() {
            let first_message = &self.history.messages[0];
            println!("  Started: {}", first_message.timestamp.format("%Y-%m-%d %H:%M:%S"));

            if self.history.messages.len() > 1 {
                let last_message = &self.history.messages[self.history.messages.len() - 1];
                println!("  Last activity: {}", last_message.timestamp.format("%Y-%m-%d %H:%M:%S"));
            }
        }
    }

    /// Show help information
    fn show_help(&self) {
        println!("🤖 Crucible Chat Commands:");
        println!("  help, :h         - Show this help message");
        println!("  quit, exit       - Exit the chat");
        println!("  clear, :c        - Clear conversation history");
        println!("  agents           - List available agents");
        println!("  models           - List available models");
        println!("  save             - Save conversation history");
        println!("  tools            - List available MCP tools");
        println!("  :agent <name>    - Switch to a different agent");
        println!("  :model <name>    - Switch to a different model");
        println!("  history          - Show conversation summary");
        println!();
        println!("💡 Tips:");
        println!("  - Use Tab completion for commands (in future versions)");
        println!("  - Conversation history is automatically saved");
        println!("  - MCP tools allow searching and editing your vault");
        println!("  - Different agents have different system prompts and capabilities");
    }

    /// Save conversation history
    fn save_history(&self) -> Result<()> {
        // Generate title if not set
        let mut history = self.history.clone();
        if history.title.is_none() {
            history.generate_title();
        }

        // Create filename from title and timestamp
        let title = history.title.as_deref().unwrap_or("chat");
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}.json", title, timestamp);

        let history_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".crucible")
            .join("chat_history")
            .join(filename);

        // Create directory if it doesn't exist
        if let Some(parent) = history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        history.save_to_file(&history_path)?;
        println!("Conversation saved to: {}", history_path.display());
        Ok(())
    }
}

/// Load conversation history from file
pub async fn load_conversation_history(path: &PathBuf) -> Result<ConversationHistory> {
    ConversationHistory::load_from_file(path)
}

/// Execute the chat command
pub async fn execute(
    config: CliConfig,
    agent: String,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    _streaming: bool,
    start_message: Option<String>,
    history_file: Option<PathBuf>,
) -> Result<()> {
    info!("Starting chat session with agent: {}", agent);

    let mut session = ChatSession::new(
        config,
        agent,
        model,
        temperature,
        max_tokens,
    ).await?;

    // Load history if provided
    if let Some(history_path) = history_file {
        match load_conversation_history(&history_path).await {
            Ok(history) => {
                session.history = history;
                println!("Loaded conversation history from: {}", history_path.display());
            },
            Err(e) => {
                warn!("Failed to load conversation history: {}", e);
            }
        }
    }

    session.start(start_message).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_agent_registry() {
        let registry = AgentRegistry::new();

        // Should have default agents
        assert!(registry.get("default").is_some());
        assert!(registry.get("researcher").is_some());
        assert!(registry.get("writer").is_some());

        // Should not have unknown agent
        assert!(registry.get("unknown").is_none());
    }

    #[test]
    fn test_conversation_history() {
        let mut history = ConversationHistory::new("default".to_string(), "test-model".to_string());

        let message = ChatMessage {
            timestamp: Utc::now(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            tool_calls: None,
            tool_results: None,
        };

        history.add_message(message);
        assert_eq!(history.messages.len(), 1);

        let last = history.get_last_messages(1);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].content, "Hello");
    }

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new("test".to_string(), "Test prompt".to_string())
            .with_model("test-model".to_string())
            .with_temperature(0.5)
            .with_max_tokens(1000)
            .with_tools(false);

        assert_eq!(agent.name, "test");
        assert_eq!(agent.system_prompt, "Test prompt");
        assert_eq!(agent.model, Some("test-model".to_string()));
        assert_eq!(agent.temperature, Some(0.5));
        assert_eq!(agent.max_tokens, Some(1000));
        assert!(!agent.can_use_tools);
    }

    #[tokio::test]
    async fn test_conversation_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_chat.json");

        let mut original = ConversationHistory::new("default".to_string(), "test-model".to_string());
        original.generate_title();

        let message = ChatMessage {
            timestamp: Utc::now(),
            role: "user".to_string(),
            content: "Test message".to_string(),
            tool_calls: None,
            tool_results: None,
        };
        original.add_message(message);

        original.save_to_file(&file_path).unwrap();
        let loaded = ConversationHistory::load_from_file(&file_path).unwrap();

        assert_eq!(loaded.agent, original.agent);
        assert_eq!(loaded.model, original.model);
        assert_eq!(loaded.messages.len(), original.messages.len());
        assert_eq!(loaded.messages[0].content, "Test message");
    }
}