/// Tests for AST analyzer module discovery
///
/// Test-Driven Development approach: Write failing tests first, then implement functionality.

use super::*;
use crate::rune_tools::ast_analyzer::{RuneAstAnalyzer, DiscoveredModule, AsyncFunctionInfo};
use anyhow::Result;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_discover_simple_module() -> Result<()> {
    // Test basic module discovery with one async function
    let source = r#"
        pub mod file_operations {
            pub async fn create_file(args) {
                #{ success: true, file: args.path }
            }
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let modules = analyzer.analyze_modules(&unit)?;

    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].name, "file_operations");
    assert_eq!(modules[0].path, vec!["file_operations"]);
    assert_eq!(modules[0].functions.len(), 1);
    assert_eq!(modules[0].functions[0].name, "create_file");

    Ok(())
}

#[tokio::test]
async fn test_discover_multiple_modules() -> Result<()> {
    // Test discovery of multiple modules with different functions
    let source = r#"
        pub mod file {
            pub async fn create(args) { #{ success: true } }
            pub async fn delete(args) { #{ success: true } }
        }

        pub mod search {
            pub async fn query(args) { #{ success: true } }
        }

        pub mod ui {
            pub async fn format_results(args) { #{ success: true } }
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let modules = analyzer.analyze_modules(&unit)?;

    assert_eq!(modules.len(), 3);

    let module_names: Vec<_> = modules.iter().map(|m| &m.name).collect();
    assert!(module_names.contains(&"file"));
    assert!(module_names.contains(&"search"));
    assert!(module_names.contains(&"ui"));

    // Verify function counts
    let file_module = modules.iter().find(|m| m.name == "file").unwrap();
    assert_eq!(file_module.functions.len(), 2);

    let search_module = modules.iter().find(|m| m.name == "search").unwrap();
    assert_eq!(search_module.functions.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_discover_nested_modules() -> Result<()> {
    // Test discovery of nested module structures
    let source = r#"
        pub mod advanced {
            pub mod file {
                pub async fn create_with_template(args) { #{ success: true } }
                pub async fn move_file(args) { #{ success: true } }
            }

            pub mod search {
                pub async fn complex_query(args) { #{ success: true } }
            }
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let modules = analyzer.analyze_modules(&unit)?;

    assert_eq!(modules.len(), 2);

    // Check nested module paths
    let file_module = modules.iter().find(|m| m.name == "file").unwrap();
    assert_eq!(file_module.path, vec!["advanced", "file"]);
    assert_eq!(file_module.functions.len(), 2);

    let search_module = modules.iter().find(|m| m.name == "search").unwrap();
    assert_eq!(search_module.path, vec!["advanced", "search"]);
    assert_eq!(search_module.functions.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_ignore_non_async_functions() -> Result<()> {
    // Test that only async functions are discovered
    let source = r#"
        pub mod mixed {
            pub async fn async_function(args) { #{ success: true } }
            pub fn sync_function(args) { #{ success: false } }
            pub const CONSTANT = 42;

            pub struct SomeStruct {
                field: String
            }

            impl SomeStruct {
                pub fn new() { Self { field: "test".to_string() } }
            }
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let modules = analyzer.analyze_modules(&unit)?;

    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].functions.len(), 1);
    assert_eq!(modules[0].functions[0].name, "async_function");

    Ok(())
}

#[tokio::test]
async fn test_discover_no_modules() -> Result<()> {
    // Test that files without modules return empty list
    let source = r#"
        pub async fn simple_function(args) { #{ success: true } }

        pub fn another_function() { /* sync function */ }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let modules = analyzer.analyze_modules(&unit)?;

    assert_eq!(modules.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_extract_functions_from_module() -> Result<()> {
    // Test extracting all functions from a specific module
    let source = r#"
        pub mod file_operations {
            pub async fn create_file(args) { #{ success: true, path: args.path } }
            pub async fn delete_file(args) { #{ success: true, path: args.path } }
            pub async fn copy_file(args) { #{ success: true, from: args.from, to: args.to } }
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let unit = compile_source(source, &context)?;
    let analyzer = RuneAstAnalyzer::new();

    let module_path = vec!["file_operations"];
    let functions = analyzer.extract_functions_in_module(&unit, &module_path)?;

    assert_eq!(functions.len(), 3);

    let function_names: Vec<_> = functions.iter().map(|f| &f.name).collect();
    assert!(function_names.contains(&"create_file"));
    assert!(function_names.contains(&"delete_file"));
    assert!(function_names.contains(&"copy_file"));

    // Verify function metadata
    let create_func = functions.iter().find(|f| f.name == "create_file").unwrap();
    assert!(create_func.is_async);
    assert!(create_func.is_public);

    Ok(())
}

#[tokio::test]
async fn test_error_handling_invalid_module() -> Result<()> {
    // Test error handling for malformed source code
    let source = r#"
        // This should fail to compile
        pub mod invalid {
            pub async fn broken_function(args) {
                // Missing closing brace
        "#;

    let context = rune::Context::with_default_modules()?;
    let result = compile_source(source, &context);

    // Should fail to compile
    assert!(result.is_err());

    Ok(())
}

/// Helper function to compile Rune source for testing
fn compile_source(source: &str, context: &rune::Context) -> Result<Arc<rune::Unit>> {
    let source_obj = rune::Source::memory(source)?;
    let mut sources = rune::Sources::new();
    sources.insert(source_obj)?;

    let mut diagnostics = rune::Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(context)
        .with_diagnostics(&mut diagnostics)
        .build();

    // Report warnings but don't fail on them
    if !diagnostics.is_empty() {
        let mut writer = rune::termcolor::StandardStream::stderr(rune::termcolor::ColorChoice::Always);
        diagnostics.emit(&mut writer, &sources)?;
    }

    let unit = result?;
    Ok(Arc::new(unit))
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_discovery_workflow() -> Result<()> {
        // Test the complete discovery workflow with a realistic example
        let source = r#"
        // Simple direct tools (backwards compatible)
        pub async fn search_files(args) {
            #{ success: true, results: ["file1.md", "file2.md"] }
        }

        // Organized tools in modules
        pub mod file {
            pub async fn create(args) { #{ success: true, file: args.path } }
            pub async fn delete(args) { #{ success: true, file: args.path } }
            pub async fn rename(args) { #{ success: true, from: args.from, to: args.to } }
        }

        pub mod search {
            pub async fn by_content(args) { #{ success: true, query: args.query } }
            pub async fn by_name(args) { #{ success: true, pattern: args.pattern } }
        }

        pub mod ui {
            pub async fn format_results(args) { #{ success: true, formatted: args.results } }
            pub async fn get_suggestions(args) { #{ success: true, suggestions: args.query } }
        }

        // Consumer-specific helpers
        pub mod agent_helpers {
            pub async fn optimize_for_agents(args) { #{ success: true, optimized: args } }
        }
        "#;

        let context = rune::Context::with_default_modules()?;
        let unit = compile_source(source, &context)?;
        let analyzer = RuneAstAnalyzer::new();

        // Discover all modules
        let modules = analyzer.analyze_modules(&unit)?;

        // Should discover 4 modules: file, search, ui, agent_helpers
        assert_eq!(modules.len(), 4);

        // Verify module names
        let module_names: Vec<_> = modules.iter().map(|m| &m.name).collect();
        assert!(module_names.contains(&"file"));
        assert!(module_names.contains(&"search"));
        assert!(module_names.contains(&"ui"));
        assert!(module_names.contains(&"agent_helpers"));

        // Count total functions
        let total_functions: usize = modules.iter().map(|m| m.functions.len()).sum();
        assert_eq!(total_functions, 6); // 3 + 2 + 2 + 1

        // Verify nested module structure (if any)
        for module in &modules {
            for function in &module.functions {
                assert!(function.is_async);
                assert!(function.is_public);
            }
        }

        Ok(())
    }
}