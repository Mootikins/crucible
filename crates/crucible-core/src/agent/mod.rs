pub mod loader;
pub mod matcher;
pub mod types;

pub use loader::AgentLoader;
pub use matcher::CapabilityMatcher;
pub use types::*;

use anyhow::Result;
use std::collections::HashMap;

/// Registry for managing loaded agents
#[derive(Debug)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentDefinition>,
    loader: AgentLoader,
    matcher: CapabilityMatcher,
}

impl AgentRegistry {
    /// Create a new agent registry
    pub fn new(loader: AgentLoader) -> Self {
        let matcher = CapabilityMatcher::new();
        Self {
            agents: HashMap::new(),
            loader,
            matcher,
        }
    }

    /// Load all agents from a directory
    pub fn load_agents_from_directory(&mut self, dir_path: &str) -> Result<usize> {
        let agents = self.loader.load_from_directory(dir_path)?;
        let count = agents.len();

        for agent in agents {
            self.agents.insert(agent.name.clone(), agent);
        }

        Ok(count)
    }

    /// Load a single agent from a file
    pub fn load_agent_from_file(&mut self, file_path: &str) -> Result<()> {
        let agent = self.loader.load_from_file(file_path)?;
        self.agents.insert(agent.name.clone(), agent);
        Ok(())
    }

    /// Get an agent by name
    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    /// Get an agent by ID
    pub fn get_agent_by_id(&self, id: &uuid::Uuid) -> Option<&AgentDefinition> {
        self.agents.values().find(|agent| &agent.id == id)
    }

    /// List all agent names
    pub fn list_agents(&self) -> Vec<&String> {
        self.agents.keys().collect()
    }

    /// Find agents matching a query
    pub fn find_agents(&self, query: &AgentQuery) -> Vec<AgentMatch> {
        self.matcher.find_matching_agents(&self.agents, query)
    }

    /// Get agents by capability
    pub fn get_agents_by_capability(&self, capability: &str) -> Vec<&AgentDefinition> {
        self.agents
            .values()
            .filter(|agent| agent.capabilities.iter().any(|cap| cap.name == capability))
            .collect()
    }

    /// Get agents by tag
    pub fn get_agents_by_tag(&self, tag: &str) -> Vec<&AgentDefinition> {
        self.agents
            .values()
            .filter(|agent| agent.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get agents by skill
    pub fn get_agents_by_skill(&self, skill: &str) -> Vec<&AgentDefinition> {
        self.agents
            .values()
            .filter(|agent| agent.skills.iter().any(|s| s.name == skill))
            .collect()
    }

    /// Get all agents that require specific tools
    pub fn get_agents_requiring_tools(&self, tools: &[String]) -> Vec<&AgentDefinition> {
        self.agents
            .values()
            .filter(|agent| {
                tools.iter().any(|tool| agent.required_tools.contains(tool))
            })
            .collect()
    }

    /// Get the total number of registered agents
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Check if an agent exists
    pub fn has_agent(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// Remove an agent by name
    pub fn remove_agent(&mut self, name: &str) -> Option<AgentDefinition> {
        self.agents.remove(name)
    }

    /// Clear all agents
    pub fn clear(&mut self) {
        self.agents.clear();
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new(AgentLoader::new())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_test;