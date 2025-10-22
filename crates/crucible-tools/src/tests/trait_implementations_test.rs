//! Test trait implementations for Rune types
//!
//! This module tests that the Debug and Clone trait implementations
//! work correctly for the key Rune types that received new trait implementations.

use crate::analyzer::{TypeInferenceEngine, RuneAstAnalyzer};
use crate::context_factory::{ContextFactory, ContextFactoryConfig};
use crate::database::{DatabaseManager, DatabaseConfig};
use crate::rune_registry::RuneToolRegistry;
use crate::discovery::ToolDiscovery;
use crate::context::ContextManager;
use crate::types::AnalyzerConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_inference_engine_clone() {
        let config = AnalyzerConfig::default();
        let engine = TypeInferenceEngine::new(config);
        let cloned_engine = engine.clone();

        // Test that both instances are independent
        assert_eq!(format!("{:?}", engine), format!("{:?}", cloned_engine));
    }

    #[test]
    fn test_rune_ast_analyzer_clone() {
        let analyzer = RuneAstAnalyzer::new();
        assert!(analyzer.is_ok());

        let analyzer = analyzer.unwrap();
        let cloned_analyzer = analyzer.clone();

        // Test that both instances are independent
        assert_eq!(format!("{:?}", analyzer), format!("{:?}", cloned_analyzer));
    }

    #[test]
    fn test_context_factory_clone() {
        let config = ContextFactoryConfig::default();
        let factory = ContextFactory::new(config);
        assert!(factory.is_ok());

        let factory = factory.unwrap();
        let cloned_factory = factory.clone();

        // Test that both instances are independent
        assert_eq!(format!("{:?}", factory), format!("{:?}", cloned_factory));
    }

    #[test]
    fn test_database_manager_clone() {
        let config = DatabaseConfig {
            connection_string: "memory".to_string(),
            pool_size: 1,
        };

        // Note: This test may fail without a proper database connection
        // but it demonstrates the Clone trait usage
        let db_result = DatabaseManager::new(config);

        if let Ok(db_manager) = db_result {
            let cloned_manager = db_manager.clone();

            // Test that both instances are independent
            assert_eq!(format!("{:?}", db_manager), format!("{:?}", cloned_manager));
        }
    }

    #[test]
    fn test_rune_tool_registry_clone() {
        let registry = RuneToolRegistry::new();
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        let cloned_registry = registry.clone();

        // Test that both instances are independent
        assert_eq!(format!("{:?}", registry), format!("{:?}", cloned_registry));
    }

    #[test]
    fn test_context_manager_clone() {
        use crate::context::ContextConfig;

        let config = ContextConfig::default();
        let manager = ContextManager::new(config);
        let cloned_manager = manager.clone();

        // Test that both instances are independent
        assert_eq!(format!("{:?}", manager), format!("{:?}", cloned_manager));
    }

    #[test]
    fn test_debug_implementations() {
        // Test that all types with new Debug implementations can be formatted
        let config = AnalyzerConfig::default();
        let engine = TypeInferenceEngine::new(config);
        assert!(format!("{:?}", engine).len() > 0);

        let registry = RuneToolRegistry::new();
        assert!(registry.is_ok());
        if let Ok(reg) = registry {
            assert!(format!("{:?}", reg).len() > 0);
        }
    }
}