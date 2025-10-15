#[cfg(test)]
mod tests {
    use crate::agent::{AgentLoader, AgentRegistry, AgentQuery, AgentStatus, SkillLevel};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_agent_file(temp_dir: &TempDir, filename: &str, content: &str) -> String {
        let file_path = temp_dir.path().join(filename);
        fs::write(&file_path, content).unwrap();
        file_path.to_string_lossy().to_string()
    }

    fn get_sample_agent_frontmatter() -> &'static str {
        r#"---
name: "Test Agent"
version: "1.0.0"
description: "A test agent for unit testing"

capabilities:
  - name: "Testing"
    description: "Ability to run and write tests"
    skill_level: "Intermediate"
    required_tools: []

required_tools:
  - "search_by_content"

tags:
  - "test"
  - "sample"

personality:
  tone: "friendly"
  style: "casual"
  verbosity: "Moderate"
  traits:
    - "helpful"
  preferences: {}

skills:
  - name: "Testing"
    category: "development"
    proficiency: 7
    experience_years: 3.0
    certifications: []

status: "Active"
author: "Test Author"
---

# System Prompt

You are a test agent used for unit testing the agent system.

## Purpose

This agent is used to verify that the agent loading and parsing functionality works correctly.

## Capabilities

- Basic testing functionality
- Simple task execution
- Test validation

## System Behavior

This agent follows simple patterns and provides predictable responses for testing purposes.
"#
    }

    #[test]
    fn test_agent_loader_parse_valid_agent() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut loader = AgentLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(result.is_ok(), "Failed to load valid agent file: {:?}", result.err());

        let agent = result.unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.version, "1.0.0");
        assert_eq!(agent.description, "A test agent for unit testing");
        assert_eq!(agent.required_tools, vec!["search_by_content"]);
        assert_eq!(agent.tags, vec!["test", "sample"]);
        assert_eq!(agent.capabilities.len(), 1);
        assert_eq!(agent.capabilities[0].name, "Testing");
        assert_eq!(agent.skills.len(), 1);
        assert_eq!(agent.skills[0].name, "Testing");
        assert_eq!(agent.status, AgentStatus::Active);
        assert_eq!(agent.author, Some("Test Author".to_string()));
        assert!(agent.system_prompt.contains("test agent used for unit testing"));
    }

    #[test]
    fn test_agent_loader_parse_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_content = r#"---
name: "Test Agent"
version: 1.0.0
description: "Invalid YAML - version should be quoted"

capabilities:
  - name: "Testing"
    description: "Invalid frontmatter"
    skill_level: "Intermediate"
    required_tools: []

# Missing closing ---
"#;

        let file_path = create_test_agent_file(&temp_dir, "invalid_agent.md", invalid_content);

        let mut loader = AgentLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(result.is_err(), "Should have failed to parse invalid YAML");
    }

    #[test]
    fn test_agent_loader_no_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let no_frontmatter_content = r#"# Just a markdown file

This file has no YAML frontmatter, so it should fail to load.

## Content

Some content here.
"#;

        let file_path = create_test_agent_file(&temp_dir, "no_frontmatter.md", no_frontmatter_content);

        let mut loader = AgentLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(result.is_err(), "Should have failed to load file without frontmatter");
    }

    #[test]
    fn test_agent_loader_load_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple agent files
        let agent1_content = get_sample_agent_frontmatter()
            .replace("Test Agent", "Agent 1")
            .replace("test agent", "agent 1");

        let agent2_content = get_sample_agent_frontmatter()
            .replace("Test Agent", "Agent 2")
            .replace("test agent", "agent 2");

        create_test_agent_file(&temp_dir, "agent1.md", &agent1_content);
        create_test_agent_file(&temp_dir, "agent2.md", &agent2_content);

        // Create a non-md file that should be ignored
        create_test_agent_file(&temp_dir, "ignore.txt", "This should be ignored");

        let mut loader = AgentLoader::new();
        let result = loader.load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(result.is_ok(), "Failed to load directory: {:?}", result.err());
        let agents = result.unwrap();
        assert_eq!(agents.len(), 2, "Should have loaded exactly 2 agents");

        let agent_names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        assert!(agent_names.contains(&"Agent 1".to_string()));
        assert!(agent_names.contains(&"Agent 2".to_string()));
    }

    #[test]
    fn test_agent_registry() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentRegistry::new(AgentLoader::new());

        // Test loading single agent
        let result = registry.load_agent_from_file(&file_path);
        assert!(result.is_ok(), "Failed to load agent into registry");

        // Test getting agent
        let agent = registry.get_agent("Test Agent");
        assert!(agent.is_some(), "Agent not found in registry");
        assert_eq!(agent.unwrap().name, "Test Agent");

        // Test listing agents
        let agent_names = registry.list_agents();
        assert_eq!(agent_names.len(), 1);
        assert_eq!(*agent_names[0], "Test Agent");

        // Test checking if agent exists
        assert!(registry.has_agent("Test Agent"));
        assert!(!registry.has_agent("Nonexistent Agent"));

        // Test getting agents by tag
        let tagged_agents = registry.get_agents_by_tag("test");
        assert_eq!(tagged_agents.len(), 1);

        // Test getting agents by capability
        let capable_agents = registry.get_agents_by_capability("Testing");
        assert_eq!(capable_agents.len(), 1);

        // Test getting agents by skill
        let skilled_agents = registry.get_agents_by_skill("Testing");
        assert_eq!(skilled_agents.len(), 1);

        // Test getting agents requiring tools
        let tool_agents = registry.get_agents_requiring_tools(&vec!["search_by_content".to_string()]);
        assert_eq!(tool_agents.len(), 1);

        // Test count
        assert_eq!(registry.count(), 1);

        // Test removal
        let removed_agent = registry.remove_agent("Test Agent");
        assert!(removed_agent.is_some());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_capability_matching() {
        let temp_dir = TempDir::new().unwrap();

        // Create a test agent
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentRegistry::new(AgentLoader::new());
        registry.load_agent_from_file(&file_path).unwrap();

        // Test exact capability match
        let query = AgentQuery {
            capabilities: vec!["Testing".to_string()],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: None,
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].agent.name, "Test Agent");
        assert!(matches[0].score > 0);
        assert!(matches[0].matched_criteria.iter().any(|c| c.contains("capabilities")));

        // Test tag match
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec!["test".to_string()],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: None,
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);

        // Test tool requirement match
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec![],
            skills: vec![],
            required_tools: vec!["search_by_content".to_string()],
            min_skill_level: None,
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);

        // Test text search
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: None,
            status: None,
            text_search: Some("test agent".to_string()),
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);

        // Test no matches
        let query = AgentQuery {
            capabilities: vec!["Nonexistent Capability".to_string()],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: None,
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_skill_levels() {
        let temp_dir = TempDir::new().unwrap();

        // Create an agent with advanced capabilities
        let content = get_sample_agent_frontmatter()
            .replace("skill_level: \"Intermediate\"", "skill_level: \"Advanced\"");

        let file_path = create_test_agent_file(&temp_dir, "advanced_agent.md", &content);

        let mut registry = AgentRegistry::new(AgentLoader::new());
        registry.load_agent_from_file(&file_path).unwrap();

        // Test minimum skill level filtering
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: Some(SkillLevel::Advanced),
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);

        // Test with lower minimum level (should still match)
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: Some(SkillLevel::Beginner),
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 1);

        // Test with higher minimum level (should not match)
        let query = AgentQuery {
            capabilities: vec![],
            tags: vec![],
            skills: vec![],
            required_tools: vec![],
            min_skill_level: Some(SkillLevel::Expert),
            status: None,
            text_search: None,
        };

        let matches = registry.find_agents(&query);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_agent_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut loader = AgentLoader::new();

        // Test valid semantic version
        let valid_version_content = get_sample_agent_frontmatter()
            .replace("version: \"1.0.0\"", "version: \"2.1.3\"");

        let file_path = create_test_agent_file(&temp_dir, "valid_version.md", &valid_version_content);
        let result = loader.load_from_file(&file_path);
        assert!(result.is_ok());

        // Test invalid semantic version
        let invalid_version_content = get_sample_agent_frontmatter()
            .replace("version: \"1.0.0\"", "version: \"v1.0\"");

        let file_path = create_test_agent_file(&temp_dir, "invalid_version.md", &invalid_version_content);
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err());

        // Test invalid skill proficiency
        let invalid_skill_content = get_sample_agent_frontmatter()
            .replace("proficiency: 7", "proficiency: 11"); // Over 10

        let file_path = create_test_agent_file(&temp_dir, "invalid_skill.md", &invalid_skill_content);
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_cache() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "cached_agent.md", content);

        let mut loader = AgentLoader::new();

        // First load should parse the file
        let result1 = loader.load_from_file(&file_path);
        assert!(result1.is_ok());
        assert_eq!(loader.cache_stats(), 1);

        // Second load should use cache
        let result2 = loader.load_from_file(&file_path);
        assert!(result2.is_ok());
        assert_eq!(loader.cache_stats(), 1); // Still 1 entry in cache

        // Results should be identical
        assert_eq!(result1.unwrap().id, result2.unwrap().id);

        // Clear cache
        loader.clear_cache();
        assert_eq!(loader.cache_stats(), 0);
    }
}