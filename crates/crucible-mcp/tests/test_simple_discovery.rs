/// Simple test to debug module discovery

use anyhow::Result;
use std::sync::Arc;
use crucible_mcp::rune_tools::RuneAstAnalyzer;

/// Helper function to load and compile a Rune source file
fn load_and_compile_rune_file(file_path: &str) -> Result<Arc<rune::Unit>> {
    let source = std::fs::read_to_string(file_path)?;
    let context = rune::Context::with_default_modules()?;
    let source_obj = rune::Source::memory(&source)?;
    let mut sources = rune::Sources::new();
    sources.insert(source_obj)?;

    let mut diagnostics = rune::Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
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

#[test]
fn test_simple_discovery() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;

    // Test with simple modules file
    let unit = load_and_compile_rune_file("test_data/simple_modules.rn")?;
    let modules = analyzer.analyze_modules(&unit)?;

    println!("Found {} modules:", modules.len());
    for module in &modules {
        println!("  - Module: {}", module.name);
        println!("    Functions: {}", module.functions.len());
        for function in &module.functions {
            println!("      - {}", function.name);
        }
    }

    // Check what we actually found
    assert!(!modules.is_empty(), "Should discover at least one module");

    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    println!("Module names: {:?}", module_names);

    Ok(())
}