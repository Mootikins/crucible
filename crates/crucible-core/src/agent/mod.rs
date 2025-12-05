//! Agent card module for defining reusable agent configurations
//!
//! Agent cards follow the "Model Card" pattern - they are metadata
//! about agents, not the agents themselves.

pub mod loader;
pub mod matcher;
pub mod types;

pub use loader::AgentCardLoader;
pub use matcher::CapabilityMatcher;
pub use types::*;

use anyhow::Result;
use std::collections::HashMap;

/// Registry for managing loaded agent cards
#[derive(Debug)]
pub struct AgentCardRegistry {
    cards: HashMap<String, AgentCard>,
    loader: AgentCardLoader,
    matcher: CapabilityMatcher,
}

impl AgentCardRegistry {
    /// Create a new agent card registry
    pub fn new(loader: AgentCardLoader) -> Self {
        let matcher = CapabilityMatcher::new();
        Self {
            cards: HashMap::new(),
            loader,
            matcher,
        }
    }

    /// Load all agent cards from a directory
    pub fn load_from_directory(&mut self, dir_path: &str) -> Result<usize> {
        let cards = self.loader.load_from_directory(dir_path)?;
        let count = cards.len();

        for card in cards {
            self.cards.insert(card.name.clone(), card);
        }

        Ok(count)
    }

    /// Load a single agent card from a file
    pub fn load_from_file(&mut self, file_path: &str) -> Result<()> {
        let card = self.loader.load_from_file(file_path)?;
        self.cards.insert(card.name.clone(), card);
        Ok(())
    }

    /// Get an agent card by name
    pub fn get(&self, name: &str) -> Option<&AgentCard> {
        self.cards.get(name)
    }

    /// Get an agent card by ID
    pub fn get_by_id(&self, id: &uuid::Uuid) -> Option<&AgentCard> {
        self.cards.values().find(|card| &card.id == id)
    }

    /// List all agent card names
    pub fn list(&self) -> Vec<&String> {
        self.cards.keys().collect()
    }

    /// Find agent cards matching a query
    pub fn find(&self, query: &AgentCardQuery) -> Vec<AgentCardMatch> {
        self.matcher.find_matching(&self.cards, query)
    }

    /// Get agent cards by capability
    pub fn get_by_capability(&self, capability: &str) -> Vec<&AgentCard> {
        self.cards
            .values()
            .filter(|card| card.capabilities.iter().any(|cap| cap.name == capability))
            .collect()
    }

    /// Get agent cards by tag
    pub fn get_by_tag(&self, tag: &str) -> Vec<&AgentCard> {
        self.cards
            .values()
            .filter(|card| card.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get agent cards by skill
    pub fn get_by_skill(&self, skill: &str) -> Vec<&AgentCard> {
        self.cards
            .values()
            .filter(|card| card.skills.iter().any(|s| s.name == skill))
            .collect()
    }

    /// Get all agent cards that require specific tools
    pub fn get_requiring_tools(&self, tools: &[String]) -> Vec<&AgentCard> {
        self.cards
            .values()
            .filter(|card| tools.iter().any(|tool| card.required_tools.contains(tool)))
            .collect()
    }

    /// Get the total number of registered agent cards
    pub fn count(&self) -> usize {
        self.cards.len()
    }

    /// Check if an agent card exists
    pub fn has(&self, name: &str) -> bool {
        self.cards.contains_key(name)
    }

    /// Remove an agent card by name
    pub fn remove(&mut self, name: &str) -> Option<AgentCard> {
        self.cards.remove(name)
    }

    /// Clear all agent cards
    pub fn clear(&mut self) {
        self.cards.clear();
    }
}

impl Default for AgentCardRegistry {
    fn default() -> Self {
        Self::new(AgentCardLoader::new())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_test;
