//! Integration tests for documentation example plugins
//!
//! These tests verify that the example plugins in docs/plugins/ work correctly.
//! They serve as both tests and documentation for plugin development patterns.

use crucible_rune::{RuneDiscoveryConfig, RuneToolRegistry, ToolDiscovery};
use std::fs;
use std::path::PathBuf;

/// Get the path to the docs/plugins directory
fn docs_plugins_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs")
        .join("plugins")
}

// =============================================================================
// Categorizer Plugin Tests
// =============================================================================

#[test]
fn test_categorizer_plugin_parses() {
    let plugins_path = docs_plugins_path();
    let categorizer = plugins_path.join("categorizer.rn");

    assert!(
        categorizer.exists(),
        "categorizer.rn should exist at {:?}",
        categorizer
    );

    let content = fs::read_to_string(&categorizer).unwrap();

    // Should have hook attribute
    assert!(content.contains("#[hook("), "Should have hook attribute");
    assert!(
        content.contains("tool:discovered"),
        "Should hook tool:discovered event"
    );

    // Should use categorize_by_name from crucible module
    assert!(
        content.contains("crucible::categorize_by_name")
            || content.contains("use crucible::categorize_by_name"),
        "Should use crucible::categorize_by_name"
    );

    // Should use regex for pattern matching
    assert!(
        content.contains("regex::") || content.contains("use regex::"),
        "Should use regex module"
    );
}

#[test]
fn test_categorizer_plugin_has_tool() {
    let plugins_path = docs_plugins_path();
    let categorizer = plugins_path.join("categorizer.rn");
    let content = fs::read_to_string(&categorizer).unwrap();

    // Should expose a tool for getting statistics
    assert!(
        content.contains("#[tool("),
        "Should have tool attribute for stats"
    );
    assert!(
        content.contains("category_stats") || content.contains("get_stats"),
        "Should have a stats tool function"
    );
}

#[tokio::test]
async fn test_categorizer_plugin_discovers() {
    let plugins_path = docs_plugins_path();

    if !plugins_path.exists() {
        eprintln!(
            "Skipping test - docs/plugins not found at {:?}",
            plugins_path
        );
        return;
    }

    let config = RuneDiscoveryConfig {
        tool_directories: vec![plugins_path],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should discover tools from categorizer.rn
    let categorizer_tools: Vec<_> = tools
        .iter()
        .filter(|t| t.name == "category_stats")
        .collect();

    assert!(
        !categorizer_tools.is_empty(),
        "Should discover category_stats tool"
    );
}

// =============================================================================
// Log Tool Calls Plugin Tests
// =============================================================================

#[test]
fn test_log_tool_calls_plugin_parses() {
    let plugins_path = docs_plugins_path();
    let log_plugin = plugins_path.join("log_tool_calls.rn");

    assert!(
        log_plugin.exists(),
        "log_tool_calls.rn should exist at {:?}",
        log_plugin
    );

    let content = fs::read_to_string(&log_plugin).unwrap();

    // Should have multiple hooks
    assert!(
        content.matches("#[hook(").count() >= 3,
        "Should have at least 3 hooks (tool:after, tool:error, tool:discovered)"
    );

    // Should hook different event types
    assert!(
        content.contains("tool:after"),
        "Should hook tool:after event"
    );
    assert!(
        content.contains("tool:error"),
        "Should hook tool:error event"
    );
    assert!(
        content.contains("tool:discovered"),
        "Should hook tool:discovered event"
    );

    // Should use context for storage
    assert!(
        content.contains("ctx.set(") && content.contains("ctx.get("),
        "Should use context for state storage"
    );

    // Should emit custom events
    assert!(
        content.contains("ctx.emit_custom("),
        "Should emit custom audit events"
    );
}

#[test]
fn test_log_tool_calls_has_query_tools() {
    let plugins_path = docs_plugins_path();
    let log_plugin = plugins_path.join("log_tool_calls.rn");
    let content = fs::read_to_string(&log_plugin).unwrap();

    // Should have tools for querying audit data
    assert!(
        content.contains("audit_log") || content.contains("get_audit_log"),
        "Should have audit_log query tool"
    );
    assert!(
        content.contains("audit_stats") || content.contains("get_audit_stats"),
        "Should have audit_stats tool"
    );
}

#[tokio::test]
async fn test_log_tool_calls_plugin_discovers() {
    let plugins_path = docs_plugins_path();

    if !plugins_path.exists() {
        eprintln!(
            "Skipping test - docs/plugins not found at {:?}",
            plugins_path
        );
        return;
    }

    let config = RuneDiscoveryConfig {
        tool_directories: vec![plugins_path],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all().unwrap();

    // Should discover audit tools
    let audit_tools: Vec<_> = tools.iter().filter(|t| t.name.contains("audit")).collect();

    assert!(
        audit_tools.len() >= 2,
        "Should discover at least 2 audit tools (audit_log, audit_stats), found: {:?}",
        audit_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

// =============================================================================
// Just Plugin Tests
// =============================================================================

#[test]
fn test_just_plugin_parses() {
    let plugins_path = docs_plugins_path();
    let just_plugin = plugins_path.join("just.rn");

    assert!(
        just_plugin.exists(),
        "just.rn should exist at {:?}",
        just_plugin
    );

    let content = fs::read_to_string(&just_plugin).unwrap();

    // Should be a struct-based plugin
    assert!(
        content.contains("struct JustPlugin"),
        "Should define JustPlugin struct"
    );

    // Should have plugin attribute with watch patterns
    assert!(
        content.contains("#[plugin("),
        "Should have plugin attribute"
    );
    assert!(
        content.contains("justfile") || content.contains("*.just"),
        "Should watch justfile patterns"
    );

    // Should implement required methods
    assert!(content.contains("fn new("), "Should have new() constructor");
    assert!(content.contains("fn tools("), "Should have tools() method");
    assert!(
        content.contains("fn dispatch("),
        "Should have dispatch() method"
    );

    // Should use shell::exec
    assert!(
        content.contains("shell::exec") || content.contains("use shell::exec"),
        "Should use shell::exec for running just"
    );

    // Should use oq for JSON parsing
    assert!(
        content.contains("oq::query") || content.contains("use oq"),
        "Should use oq for JSON parsing"
    );
}

// =============================================================================
// All Plugins Discovery Test
// =============================================================================

#[tokio::test]
async fn test_all_docs_plugins_discover_without_errors() {
    let plugins_path = docs_plugins_path();

    if !plugins_path.exists() {
        eprintln!(
            "Skipping test - docs/plugins not found at {:?}",
            plugins_path
        );
        return;
    }

    let config = RuneDiscoveryConfig {
        tool_directories: vec![plugins_path.clone()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let discovery = ToolDiscovery::new(config);
    let tools = discovery.discover_all();

    assert!(
        tools.is_ok(),
        "All docs plugins should parse without errors: {:?}",
        tools.err()
    );

    let tools = tools.unwrap();
    println!("Discovered {} tools from docs/plugins:", tools.len());
    for tool in &tools {
        println!("  - {} ({})", tool.name, tool.description);
    }

    // Should discover tools from at least 2 plugin files
    assert!(
        tools.len() >= 4,
        "Should discover at least 4 tools from docs/plugins, found {}",
        tools.len()
    );
}

#[tokio::test]
async fn test_docs_plugins_register_in_registry() {
    let plugins_path = docs_plugins_path();

    if !plugins_path.exists() {
        eprintln!(
            "Skipping test - docs/plugins not found at {:?}",
            plugins_path
        );
        return;
    }

    let config = RuneDiscoveryConfig {
        tool_directories: vec![plugins_path],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let registry = RuneToolRegistry::discover_from(config).await;
    assert!(
        registry.is_ok(),
        "Registry should load docs plugins: {:?}",
        registry.err()
    );

    let registry = registry.unwrap();
    let tools = registry.list_tools().await;

    println!("Registry has {} tools:", tools.len());
    for tool in &tools {
        println!("  - rune_{}", tool.name);
    }

    // Verify expected tools are present
    assert!(
        tools.iter().any(|t| t.name == "category_stats"),
        "Should have category_stats tool"
    );
    assert!(
        tools.iter().any(|t| t.name == "audit_log"),
        "Should have audit_log tool"
    );
    assert!(
        tools.iter().any(|t| t.name == "audit_stats"),
        "Should have audit_stats tool"
    );
}
