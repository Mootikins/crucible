pub mod search;
pub mod fuzzy;
pub mod semantic;
pub mod note;
pub mod stats;
pub mod test_tools;
pub mod rune;
pub mod config;
pub mod chat;
pub mod repl;
pub mod service;
pub mod daemon;
pub mod migration;
// pub mod enhanced_chat; // Temporarily disabled due to compilation issues
// pub mod enhanced_chat_session; // Temporarily disabled due to compilation issues
// pub mod performance_tracker; // Temporarily disabled due to compilation issues
// pub mod collaboration_manager; // Temporarily disabled due to compilation issues
// pub mod agent_management; // Temporarily disabled due to compilation issues

// Re-export for convenience
pub use chat::{execute as execute_chat, ChatSession, ConversationHistory};
// pub use enhanced_chat::{
//     EnhancedChatMessage, EnhancedConversationHistory, EnhancedAgentRegistry,
//     EnhancedAgent, AgentPerformanceMetrics, TaskSuggestion, AgentSwitch
// };
// pub use enhanced_chat_session::EnhancedChatSession;
// pub use performance_tracker::{
//     AgentPerformanceTracker, PerformanceRecord, LearningInsights, SystemStats
// };
// pub use collaboration_manager::{
//     CollaborationManager, CollaborationSession, WorkflowTemplate,
//     CollaborationStats
// };
