#[cfg(test)]
mod agent_tests {
    use crate::agent::{AgentCardLoader, AgentCardQuery, AgentCardRegistry};
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
tags:
  - "test"
  - "sample"
mcp_servers:
  - "crucible"
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

        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(
            result.is_ok(),
            "Failed to load valid agent file: {:?}",
            result.err()
        );

        let card = result.unwrap();
        assert_eq!(card.name, "Test Agent");
        assert_eq!(card.version, "1.0.0");
        assert_eq!(card.description, "A test agent for unit testing");
        assert_eq!(card.tags, vec!["test", "sample"]);
        assert_eq!(card.mcp_servers, vec!["crucible"]);
        assert!(card
            .system_prompt
            .contains("test agent used for unit testing"));
    }

    #[test]
    fn test_agent_loader_parse_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_content = r#"---
name: "Test Agent"
version: 1.0.0
description: "Invalid YAML - version should be quoted"
tags: []
# Missing closing ---
"#;

        let file_path = create_test_agent_file(&temp_dir, "invalid_agent.md", invalid_content);

        let mut loader = AgentCardLoader::new();
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

        let file_path =
            create_test_agent_file(&temp_dir, "no_frontmatter.md", no_frontmatter_content);

        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(
            result.is_err(),
            "Should have failed to load file without frontmatter"
        );
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

        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(
            result.is_ok(),
            "Failed to load directory: {:?}",
            result.err()
        );
        let cards = result.unwrap();
        assert_eq!(cards.len(), 2, "Should have loaded exactly 2 agent cards");

        let card_names: Vec<String> = cards.iter().map(|c| c.name.clone()).collect();
        assert!(card_names.contains(&"Agent 1".to_string()));
        assert!(card_names.contains(&"Agent 2".to_string()));
    }

    #[test]
    fn test_agent_registry() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());

        // Test loading single agent card
        let result = registry.load_from_file(&file_path);
        assert!(result.is_ok(), "Failed to load agent card into registry");

        // Test getting agent card
        let card = registry.get("Test Agent");
        assert!(card.is_some(), "Agent card not found in registry");
        assert_eq!(card.unwrap().name, "Test Agent");

        // Test listing agent cards
        let card_names = registry.list();
        assert_eq!(card_names.len(), 1);
        assert_eq!(*card_names[0], "Test Agent");

        // Test checking if agent card exists
        assert!(registry.has("Test Agent"));
        assert!(!registry.has("Nonexistent Agent"));

        // Test getting agent cards by tag
        let tagged_cards = registry.get_by_tag("test");
        assert_eq!(tagged_cards.len(), 1);

        // Test count
        assert_eq!(registry.count(), 1);

        // Test removal
        let removed_card = registry.remove("Test Agent");
        assert!(removed_card.is_some());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_tag_matching() {
        let temp_dir = TempDir::new().unwrap();

        // Create a test agent card
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());
        registry.load_from_file(&file_path).unwrap();

        // Test tag match
        let query = AgentCardQuery {
            tags: vec!["test".to_string()],
            text_search: None,
        };

        let matches = registry.find(&query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].card.name, "Test Agent");
        assert!(matches[0].score > 0);
        assert!(matches[0]
            .matched_criteria
            .iter()
            .any(|c| c.contains("tags")));

        // Test text search
        let query = AgentCardQuery {
            tags: vec![],
            text_search: Some("test agent".to_string()),
        };

        let matches = registry.find(&query);
        assert_eq!(matches.len(), 1);

        // Test no matches
        let query = AgentCardQuery {
            tags: vec!["nonexistent".to_string()],
            text_search: None,
        };

        let matches = registry.find(&query);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_agent_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut loader = AgentCardLoader::new();

        // Test valid semantic version
        let valid_version_content =
            get_sample_agent_frontmatter().replace("version: \"1.0.0\"", "version: \"2.1.3\"");

        let file_path =
            create_test_agent_file(&temp_dir, "valid_version.md", &valid_version_content);
        let result = loader.load_from_file(&file_path);
        assert!(result.is_ok());

        // Test invalid semantic version
        let invalid_version_content =
            get_sample_agent_frontmatter().replace("version: \"1.0.0\"", "version: \"v1.0\"");

        let file_path =
            create_test_agent_file(&temp_dir, "invalid_version.md", &invalid_version_content);
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_cache() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "cached_agent.md", content);

        let mut loader = AgentCardLoader::new();

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

    #[test]
    fn test_loader_nonexistent_directory() {
        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_directory("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err(), "Should fail for nonexistent directory");
    }

    #[test]
    fn test_loader_default() {
        let loader = AgentCardLoader::default();
        assert_eq!(loader.cache_stats(), 0);
    }

    #[test]
    fn test_validation_empty_name() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_content =
            get_sample_agent_frontmatter().replace("name: \"Test Agent\"", "name: \"\"");

        let file_path = create_test_agent_file(&temp_dir, "empty_name.md", &invalid_content);
        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err(), "Should fail for empty name");
    }

    #[test]
    fn test_validation_empty_description() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_content = get_sample_agent_frontmatter().replace(
            "description: \"A test agent for unit testing\"",
            "description: \"\"",
        );

        let file_path = create_test_agent_file(&temp_dir, "empty_desc.md", &invalid_content);
        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err(), "Should fail for empty description");
    }

    #[test]
    fn test_validation_empty_system_prompt() {
        let temp_dir = TempDir::new().unwrap();
        // Create agent card with only frontmatter, no markdown content
        let invalid_content = r#"---
name: "Test Agent"
version: "1.0.0"
description: "A test agent"
tags: []
---
"#;

        let file_path = create_test_agent_file(&temp_dir, "empty_prompt.md", invalid_content);
        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);
        assert!(result.is_err(), "Should fail for empty system prompt");
    }

    #[test]
    fn test_registry_get_agent_by_id() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());
        registry.load_from_file(&file_path).unwrap();

        let card = registry.get("Test Agent").unwrap();
        let card_id = card.id;

        // Test getting agent card by ID
        let found_card = registry.get_by_id(&card_id);
        assert!(found_card.is_some());
        assert_eq!(found_card.unwrap().id, card_id);

        // Test with non-existent ID
        let random_id = uuid::Uuid::new_v4();
        let not_found = registry.get_by_id(&random_id);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_registry_clear() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());
        registry.load_from_file(&file_path).unwrap();
        assert_eq!(registry.count(), 1);

        // Clear all agent cards
        registry.clear();
        assert_eq!(registry.count(), 0);
        assert!(!registry.has("Test Agent"));
    }

    #[test]
    fn test_registry_default() {
        let registry = AgentCardRegistry::default();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_matcher_with_custom_weights() {
        use crate::agent::matcher::{AgentCardMatcher, MatchingWeights};

        let custom_weights = MatchingWeights {
            tag_match: 50,
            text_match: 15,
        };

        let matcher = AgentCardMatcher::with_weights(custom_weights);

        // Verify matcher was created (implicit test via successful construction)
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());
        registry.load_from_file(&file_path).unwrap();

        // Test that custom weights affect scoring
        let query = AgentCardQuery {
            tags: vec!["test".to_string()],
            text_search: None,
        };

        let cards: std::collections::HashMap<String, crate::agent::AgentCard> = registry
            .list()
            .iter()
            .map(|name| {
                let card = registry.get(name).unwrap();
                ((**name).clone(), card.clone())
            })
            .collect();

        let matches = matcher.find_matching(&cards, &query);
        assert!(!matches.is_empty());
        // With custom weight of 50 for tag_match, score should be 50
        assert_eq!(matches[0].score, 50);
    }

    #[test]
    fn test_text_search_in_name_and_tags() {
        let temp_dir = TempDir::new().unwrap();
        let content = get_sample_agent_frontmatter();
        let file_path = create_test_agent_file(&temp_dir, "test_agent.md", content);

        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());
        registry.load_from_file(&file_path).unwrap();

        // Test search in name
        let query = AgentCardQuery {
            tags: vec![],
            text_search: Some("Test Agent".to_string()),
        };

        let matches = registry.find(&query);
        assert_eq!(matches.len(), 1);

        // Test search in tag
        let query_tag = AgentCardQuery {
            tags: vec![],
            text_search: Some("test".to_string()),
        };

        let matches_tag = registry.find(&query_tag);
        assert_eq!(matches_tag.len(), 1);

        // Test search with no matches
        let query_no_match = AgentCardQuery {
            tags: vec![],
            text_search: Some("xyznonexistent".to_string()),
        };

        let matches_none = registry.find(&query_no_match);
        assert_eq!(matches_none.len(), 0);
    }

    #[test]
    fn test_minimal_agent_card() {
        let temp_dir = TempDir::new().unwrap();
        // Minimal agent card - only required fields
        let minimal_content = r#"---
name: "Minimal Agent"
version: "1.0.0"
description: "A minimal agent card"
---

# System Prompt

This is a minimal agent with just required fields.
"#;

        let file_path = create_test_agent_file(&temp_dir, "minimal_agent.md", minimal_content);
        let mut loader = AgentCardLoader::new();
        let result = loader.load_from_file(&file_path);

        assert!(result.is_ok(), "Should load minimal agent card");
        let card = result.unwrap();
        assert_eq!(card.name, "Minimal Agent");
        assert!(card.tags.is_empty());
        assert!(card.mcp_servers.is_empty());
    }
}
