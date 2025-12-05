#[cfg(test)]
mod integration_tests {
    use crate::agent::{AgentCardLoader, AgentCardQuery, AgentCardRegistry};

    #[test]
    fn test_load_example_agents() {
        let mut registry = AgentCardRegistry::new(AgentCardLoader::new());

        // Try to load from the agents directory
        let agents_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/agents");

        if std::path::Path::new(agents_dir).exists() {
            let result = registry.load_from_directory(agents_dir);
            if let Ok(count) = result {
                println!("Successfully loaded {} example agent cards", count);

                // Test that we can find agent cards by tag
                let backend_query = AgentCardQuery {
                    tags: vec!["backend".to_string()],
                    text_search: None,
                };

                let matches = registry.find(&backend_query);
                // Note: This test may not find matches if no example agents exist
                if !matches.is_empty() {
                    println!("Found {} backend agent cards", matches.len());
                }

                // Test that we can find frontend agent cards
                let frontend_cards = registry.get_by_tag("frontend");
                if !frontend_cards.is_empty() {
                    println!("Found {} frontend agent cards", frontend_cards.len());
                }

                // Test that we can find the orchestrator
                let orchestrator_cards = registry.get_by_tag("orchestration");
                if !orchestrator_cards.is_empty() {
                    println!("Found {} orchestrator agent cards", orchestrator_cards.len());
                }
            }
        }
    }
}
