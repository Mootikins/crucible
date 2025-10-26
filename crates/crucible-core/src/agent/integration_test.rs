#[cfg(test)]
mod integration_tests {
    use crate::agent::{AgentLoader, AgentQuery, AgentRegistry, SkillLevel};

    #[test]
    fn test_load_example_agents() {
        let mut registry = AgentRegistry::new(AgentLoader::new());

        // Try to load from the agents directory
        let agents_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/agents");

        if std::path::Path::new(agents_dir).exists() {
            let result = registry.load_agents_from_directory(agents_dir);
            if let Ok(count) = result {
                println!("Successfully loaded {} example agents", count);

                // Test that we can find agents by capability
                let backend_query = AgentQuery {
                    capabilities: vec!["Rust Development".to_string()],
                    tags: vec!["backend".to_string()],
                    skills: vec![],
                    required_tools: vec![],
                    min_skill_level: Some(SkillLevel::Advanced),
                    status: None,
                    text_search: None,
                };

                let matches = registry.find_agents(&backend_query);
                assert!(!matches.is_empty(), "Should find backend agents");

                // Test that we can find frontend agents
                let frontend_agents = registry.get_agents_by_tag("frontend");
                assert!(!frontend_agents.is_empty(), "Should find frontend agents");

                // Test that we can find the orchestrator
                let orchestrator_agents = registry.get_agents_by_tag("orchestration");
                assert!(
                    !orchestrator_agents.is_empty(),
                    "Should find orchestrator agents"
                );
            }
        }
    }
}
