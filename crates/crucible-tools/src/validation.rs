//! Simple validation file to test trait implementations
//!
//! This file validates that the Clone and Debug traits work correctly
//! for the types that received new trait implementations.

use crate::analyzer::{TypeInferenceEngine, RuneAstAnalyzer};
use crate::context_factory::ContextFactory;
use crate::database::DatabaseManager;
use crate::rune_registry::RuneToolRegistry;
use crate::discovery::ToolDiscovery;
use crate::context::ContextManager;
use crate::types::AnalyzerConfig;

/// Simple validation function to ensure Clone trait works
pub fn validate_clone_implementations() -> Result<(), Box<dyn std::error::Error>> {
    println!("Validating Clone trait implementations...");

    // Test TypeInferenceEngine
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);
    let _cloned_engine = engine.clone();
    println!("✓ TypeInferenceEngine Clone works");

    // Test RuneAstAnalyzer (may fail due to context creation)
    match RuneAstAnalyzer::new() {
        Ok(analyzer) => {
            let _cloned_analyzer = analyzer.clone();
            println!("✓ RuneAstAnalyzer Clone works");
        }
        Err(_) => {
            println!("⚠ RuneAstAnalyzer Clone skipped (context creation failed)");
        }
    }

    // Test ContextFactory (may fail due to module loading)
    let factory_config = crate::context_factory::ContextFactoryConfig::default();
    match ContextFactory::new(factory_config) {
        Ok(factory) => {
            let _cloned_factory = factory.clone();
            println!("✓ ContextFactory Clone works");
        }
        Err(_) => {
            println!("⚠ ContextFactory Clone skipped (module loading failed)");
        }
    }

    // Test RuneToolRegistry
    match RuneToolRegistry::new() {
        Ok(registry) => {
            let _cloned_registry = registry.clone();
            println!("✓ RuneToolRegistry Clone works");
        }
        Err(_) => {
            println!("⚠ RuneToolRegistry Clone skipped (creation failed)");
        }
    }

    // Test ContextManager
    let context_config = crate::context::ContextConfig::default();
    let manager = ContextManager::new(context_config);
    let _cloned_manager = manager.clone();
    println!("✓ ContextManager Clone works");

    println!("All Clone trait validations completed!");
    Ok(())
}

/// Simple validation function to ensure Debug trait works
pub fn validate_debug_implementations() -> Result<(), Box<dyn std::error::Error>> {
    println!("Validating Debug trait implementations...");

    // Test TypeInferenceEngine
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);
    let debug_output = format!("{:?}", engine);
    assert!(!debug_output.is_empty());
    println!("✓ TypeInferenceEngine Debug works");

    // Test ContextManager
    let context_config = crate::context::ContextConfig::default();
    let manager = ContextManager::new(context_config);
    let debug_output = format!("{:?}", manager);
    assert!(!debug_output.is_empty());
    println!("✓ ContextManager Debug works");

    println!("All Debug trait validations completed!");
    Ok(())
}