use crate::agents::card::AgentCard;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentCard>,
    vault_paths: Vec<PathBuf>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            vault_paths: Vec::new(),
        }
    }

    pub fn add_vault_path<P: AsRef<Path>>(&mut self, path: P) {
        self.vault_paths.push(path.as_ref().to_path_buf());
    }

    pub fn load_agents(&mut self) -> Result<()> {
        self.agents.clear();

        for vault_path in &self.vault_paths.clone() {
            if let Err(e) = self.load_agents_from_path(vault_path) {
                eprintln!("Warning: Failed to load agents from {}: {}", vault_path.display(), e);
            }
        }

        Ok(())
    }

    fn load_agents_from_path<P: AsRef<Path>>(&mut self, vault_path: P) -> Result<()> {
        let path = vault_path.as_ref();

        if !path.exists() {
            return Ok(());
        }

        self.load_agents_from_dir(path)
    }

    fn load_agents_from_dir(&mut self, dir: &Path) -> Result<()> {
        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

        for entry in entries {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();

            if path.is_dir() {
                self.load_agents_from_dir(&path)?;
            } else if let Some(extension) = path.extension() {
                if extension == "md" {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(card) = AgentCard::from_str(&content) {
                            let name = card.name.clone();
                            self.agents.insert(name, card);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_agent(&self, name: &str) -> Option<&AgentCard> {
        self.agents.get(name)
    }

    pub fn list_agents(&self) -> Vec<&AgentCard> {
        self.agents.values().collect()
    }

    pub fn find_agents_by_tag(&self, tag: &str) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|agent| agent.tags.contains(&tag.to_string()))
            .collect()
    }

    pub fn find_agents_by_capability(&self, capability: &str) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|agent| agent.capabilities.contains(&capability.to_string()))
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use crate::agents::card::{AgentCard, BackendConfig};

    fn create_test_agent_card(name: &str, system_prompt: &str) -> AgentCard {
        AgentCard {
            name: name.to_string(),
            capabilities: vec!["chat".to_string()],
            tags: vec!["test".to_string()],
            backend: BackendConfig::Ollama {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
            },
            temperature: Some(0.7),
            max_tokens: Some(1000),
            owner: "local".to_string(),
            shareable: true,
            system_prompt: system_prompt.to_string(),
        }
    }

    fn create_test_markdown_file(path: &Path, card: &AgentCard) -> Result<()> {
        let yaml_content = serde_yaml::to_string(card)?;
        let content = format!("---\n{}---\n\n{}\n", yaml_content, card.system_prompt);
        fs::write(path, content)?;
        Ok(())
    }

    #[test]
    fn test_new_registry() {
        let registry = AgentRegistry::new();
        assert!(registry.list_agents().is_empty());
        assert!(registry.vault_paths.is_empty());
    }

    #[test]
    fn test_add_vault_path() {
        let mut registry = AgentRegistry::new();
        registry.add_vault_path("/test/path");
        assert_eq!(registry.vault_paths.len(), 1);
        assert_eq!(registry.vault_paths[0], PathBuf::from("/test/path"));
    }

    #[test]
    fn test_load_agents_from_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir.path());

        let agent1 = create_test_agent_card("test-agent-1", "You are a helpful assistant.");
        let agent2 = create_test_agent_card("test-agent-2", "You are a creative writer.");

        create_test_markdown_file(&temp_dir.path().join("agent1.md"), &agent1)?;
        create_test_markdown_file(&temp_dir.path().join("agent2.md"), &agent2)?;

        let non_agent_file = temp_dir.path().join("notes.txt");
        fs::write(&non_agent_file, "This is not an agent file")?;

        registry.load_agents()?;

        let agents = registry.list_agents();
        assert_eq!(agents.len(), 2);

        assert!(registry.get_agent("test-agent-1").is_some());
        assert!(registry.get_agent("test-agent-2").is_some());
        assert!(registry.get_agent("nonexistent").is_none());

        let retrieved_agent = registry.get_agent("test-agent-1").unwrap();
        assert_eq!(retrieved_agent.name, "test-agent-1");
        assert_eq!(retrieved_agent.system_prompt, "You are a helpful assistant.");

        Ok(())
    }

    #[test]
    fn test_load_agents_from_nested_directories() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir.path());

        let nested_dir = temp_dir.path().join("nested");
        fs::create_dir(&nested_dir)?;

        let agent = create_test_agent_card("nested-agent", "You are in a nested directory.");
        create_test_markdown_file(&nested_dir.join("agent.md"), &agent)?;

        registry.load_agents()?;

        let agents = registry.list_agents();
        assert_eq!(agents.len(), 1);
        assert!(registry.get_agent("nested-agent").is_some());

        Ok(())
    }

    #[test]
    fn test_load_agents_handles_invalid_yaml() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir.path());

        let invalid_file = temp_dir.path().join("invalid.md");
        fs::write(&invalid_file, "---\ninvalid: yaml: content: [\n---\nContent")?;

        let valid_agent = create_test_agent_card("valid-agent", "You are valid.");
        create_test_markdown_file(&temp_dir.path().join("valid.md"), &valid_agent)?;

        registry.load_agents()?;

        let agents = registry.list_agents();
        assert_eq!(agents.len(), 1);
        assert!(registry.get_agent("valid-agent").is_some());
        assert!(registry.get_agent("invalid-agent").is_none());

        Ok(())
    }

    #[test]
    fn test_load_agents_from_nonexistent_directory() -> Result<()> {
        let mut registry = AgentRegistry::new();
        registry.add_vault_path("/nonexistent/path");

        registry.load_agents()?;

        assert!(registry.list_agents().is_empty());

        Ok(())
    }

    #[test]
    fn test_find_agents_by_tag() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir.path());

        let agent1 = AgentCard {
            name: "agent1".to_string(),
            capabilities: vec!["chat".to_string()],
            tags: vec!["research".to_string(), "writing".to_string()],
            backend: BackendConfig::Ollama {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
            },
            temperature: Some(0.7),
            max_tokens: Some(1000),
            owner: "local".to_string(),
            shareable: true,
            system_prompt: "Research assistant".to_string(),
        };

        let agent2 = AgentCard {
            name: "agent2".to_string(),
            capabilities: vec!["chat".to_string()],
            tags: vec!["coding".to_string()],
            backend: BackendConfig::Ollama {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
            },
            temperature: Some(0.7),
            max_tokens: Some(1000),
            owner: "local".to_string(),
            shareable: true,
            system_prompt: "Coding assistant".to_string(),
        };

        create_test_markdown_file(&temp_dir.path().join("agent1.md"), &agent1)?;
        create_test_markdown_file(&temp_dir.path().join("agent2.md"), &agent2)?;

        registry.load_agents()?;

        let research_agents = registry.find_agents_by_tag("research");
        assert_eq!(research_agents.len(), 1);
        assert_eq!(research_agents[0].name, "agent1");

        let coding_agents = registry.find_agents_by_tag("coding");
        assert_eq!(coding_agents.len(), 1);
        assert_eq!(coding_agents[0].name, "agent2");

        let nonexistent_agents = registry.find_agents_by_tag("nonexistent");
        assert!(nonexistent_agents.is_empty());

        Ok(())
    }

    #[test]
    fn test_find_agents_by_capability() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir.path());

        let agent1 = AgentCard {
            name: "agent1".to_string(),
            capabilities: vec!["chat".to_string(), "analysis".to_string()],
            tags: vec!["test".to_string()],
            backend: BackendConfig::Ollama {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
            },
            temperature: Some(0.7),
            max_tokens: Some(1000),
            owner: "local".to_string(),
            shareable: true,
            system_prompt: "Analysis assistant".to_string(),
        };

        let agent2 = AgentCard {
            name: "agent2".to_string(),
            capabilities: vec!["image_generation".to_string()],
            tags: vec!["test".to_string()],
            backend: BackendConfig::Ollama {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
            },
            temperature: Some(0.7),
            max_tokens: Some(1000),
            owner: "local".to_string(),
            shareable: true,
            system_prompt: "Image generator".to_string(),
        };

        create_test_markdown_file(&temp_dir.path().join("agent1.md"), &agent1)?;
        create_test_markdown_file(&temp_dir.path().join("agent2.md"), &agent2)?;

        registry.load_agents()?;

        let chat_agents = registry.find_agents_by_capability("chat");
        assert_eq!(chat_agents.len(), 1);
        assert_eq!(chat_agents[0].name, "agent1");

        let analysis_agents = registry.find_agents_by_capability("analysis");
        assert_eq!(analysis_agents.len(), 1);
        assert_eq!(analysis_agents[0].name, "agent1");

        let image_agents = registry.find_agents_by_capability("image_generation");
        assert_eq!(image_agents.len(), 1);
        assert_eq!(image_agents[0].name, "agent2");

        let nonexistent_agents = registry.find_agents_by_capability("nonexistent");
        assert!(nonexistent_agents.is_empty());

        Ok(())
    }

    #[test]
    fn test_multiple_vault_paths() -> Result<()> {
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir1.path());
        registry.add_vault_path(temp_dir2.path());

        let agent1 = create_test_agent_card("agent1", "From directory 1");
        let agent2 = create_test_agent_card("agent2", "From directory 2");

        create_test_markdown_file(&temp_dir1.path().join("agent1.md"), &agent1)?;
        create_test_markdown_file(&temp_dir2.path().join("agent2.md"), &agent2)?;

        registry.load_agents()?;

        let agents = registry.list_agents();
        assert_eq!(agents.len(), 2);
        assert!(registry.get_agent("agent1").is_some());
        assert!(registry.get_agent("agent2").is_some());

        Ok(())
    }

    #[test]
    fn test_load_agents_clears_existing() -> Result<()> {
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;
        let mut registry = AgentRegistry::new();
        registry.add_vault_path(temp_dir1.path());

        let agent1 = create_test_agent_card("agent1", "First load");
        create_test_markdown_file(&temp_dir1.path().join("agent1.md"), &agent1)?;

        registry.load_agents()?;
        assert_eq!(registry.list_agents().len(), 1);

        registry.add_vault_path(temp_dir2.path());
        let agent2 = create_test_agent_card("agent2", "Second load");
        create_test_markdown_file(&temp_dir2.path().join("agent2.md"), &agent2)?;

        registry.load_agents()?;
        assert_eq!(registry.list_agents().len(), 2);

        Ok(())
    }

    #[test]
    fn test_default() {
        let registry = AgentRegistry::default();
        assert!(registry.list_agents().is_empty());
        assert!(registry.vault_paths.is_empty());
    }
}