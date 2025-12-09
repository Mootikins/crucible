//! Integration tests for the event handler system

use crucible_rune::{EnrichedRecipe, EventHandler, EventHandlerConfig};
use tempfile::TempDir;

fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// Test the full event processing flow with a real script
#[tokio::test]
async fn test_recipe_categorization_with_script() {
    // Create temp directory with event handler
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // Write categorizer script
    let script = r#"
use crucible::categorize_by_name;

pub fn on_recipe_discovered(recipe) {
    let name = recipe["name"];
    let category = categorize_by_name(name);
    #{ category: category }
}
"#;
    std::fs::write(event_dir.join("categorizer.rn"), script).unwrap();

    // Create handler
    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    // Test various recipe names
    let test_cases = vec![
        ("test", "testing"),
        ("test-crate", "testing"),
        ("build", "build"),
        ("release", "build"),
        ("deploy-prod", "deploy"),
        ("clean", "maintenance"),
        ("fmt", "quality"),
        ("clippy", "quality"),
        ("docs", "documentation"),
        ("ci", "ci"),
        ("web", "web"),
        ("mcp", "mcp"),
        ("bench", "benchmarks"),
        ("default", "default"),
        ("random", "other"),
    ];

    for (name, expected_category) in test_cases {
        let recipe = EnrichedRecipe::from_recipe(
            name.to_string(),
            Some(format!("Recipe: {}", name)),
            vec![],
            false,
        );

        let enriched = handler.process_event(recipe).await.unwrap();
        assert_eq!(
            enriched.category,
            Some(expected_category.to_string()),
            "Recipe '{}' should be categorized as '{}'",
            name,
            expected_category
        );
    }
}

/// Test that multiple handlers can chain enrichments
#[tokio::test]
async fn test_multiple_handlers_chain() {
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // First handler: adds category
    let script1 = r#"
pub fn on_recipe_discovered(recipe) {
    #{ category: "from-handler-1" }
}
"#;
    std::fs::write(event_dir.join("01_categorizer.rn"), script1).unwrap();

    // Second handler: adds tags
    let script2 = r#"
pub fn on_recipe_discovered(recipe) {
    #{ tags: ["from-handler-2"] }
}
"#;
    std::fs::write(event_dir.join("02_tagger.rn"), script2).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    let recipe = EnrichedRecipe::from_recipe("test".to_string(), None, vec![], false);

    let enriched = handler.process_event(recipe).await.unwrap();

    // Both enrichments should be applied
    assert_eq!(enriched.category, Some("from-handler-1".to_string()));
    assert_eq!(enriched.tags, vec!["from-handler-2"]);
}

/// Test handler that returns nothing (no enrichment)
#[tokio::test]
async fn test_handler_returns_null() {
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // Handler returns nothing for private recipes
    let script = r#"
pub fn on_recipe_discovered(recipe) {
    if recipe["private"] {
        ()  // Return unit (null) - no enrichment
    } else {
        #{ category: "public" }
    }
}
"#;
    std::fs::write(event_dir.join("conditional.rn"), script).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    // Public recipe gets enriched
    let public_recipe = EnrichedRecipe::from_recipe("public-task".to_string(), None, vec![], false);
    let enriched = handler.process_event(public_recipe).await.unwrap();
    assert_eq!(enriched.category, Some("public".to_string()));

    // Private recipe doesn't get enriched
    let private_recipe = EnrichedRecipe::from_recipe("_private".to_string(), None, vec![], true);
    let enriched = handler.process_event(private_recipe).await.unwrap();
    assert!(enriched.category.is_none());
}

/// Test batch processing of multiple recipes
#[tokio::test]
async fn test_process_recipes_batch() {
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    let script = r#"
use crucible::categorize_by_name;

pub fn on_recipe_discovered(recipe) {
    #{ category: categorize_by_name(recipe["name"]) }
}
"#;
    std::fs::write(event_dir.join("categorizer.rn"), script).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    let recipes = vec![
        EnrichedRecipe::from_recipe("test".to_string(), None, vec![], false),
        EnrichedRecipe::from_recipe("build".to_string(), None, vec![], false),
        EnrichedRecipe::from_recipe("deploy".to_string(), None, vec![], false),
    ];

    let enriched = handler.process_recipes(recipes).await.unwrap();

    assert_eq!(enriched.len(), 3);
    assert_eq!(enriched[0].category, Some("testing".to_string()));
    assert_eq!(enriched[1].category, Some("build".to_string()));
    assert_eq!(enriched[2].category, Some("deploy".to_string()));
}

/// Test that handler errors don't stop processing
#[tokio::test]
async fn test_handler_error_continues() {
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // First handler: has a bug (undefined function)
    let script1 = r#"
pub fn on_recipe_discovered(recipe) {
    undefined_function()  // This will error
}
"#;
    std::fs::write(event_dir.join("01_broken.rn"), script1).unwrap();

    // Second handler: works fine
    let script2 = r#"
pub fn on_recipe_discovered(recipe) {
    #{ category: "from-working-handler" }
}
"#;
    std::fs::write(event_dir.join("02_working.rn"), script2).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    let recipe = EnrichedRecipe::from_recipe("test".to_string(), None, vec![], false);

    // Should still complete with enrichment from working handler
    let enriched = handler.process_event(recipe).await.unwrap();
    assert_eq!(enriched.category, Some("from-working-handler".to_string()));
}

/// Test using regex module in Rune scripts
#[tokio::test]
async fn test_regex_in_script() {
    init_test_logging();
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // Script that uses regex for categorization
    // Note: Rune uses regular strings for patterns, not raw strings like Rust
    // Note: Rune uses `let` for mutable variables (mut is not supported)
    let script = r#"
use regex::{Regex, is_match};

pub fn on_recipe_discovered(recipe) {
    let name = recipe["name"];
    let tags = [];

    // Use regex for flexible pattern matching
    if is_match("test|spec|check", name) {
        tags.push("testing");
    }

    if is_match("build|release|compile", name) {
        tags.push("build");
    }

    // Use Regex object for more complex patterns
    let version_re = Regex::new("v\\d+\\.\\d+");
    if version_re.is_match(name) {
        tags.push("versioned");
    }

    // Extract numbers using find_all
    let num_re = Regex::new("\\d+");
    let numbers = num_re.find_all(name);
    if numbers.len() > 0 {
        tags.push("has-numbers");
    }

    #{ tags: tags }
}
"#;
    std::fs::write(event_dir.join("regex_categorizer.rn"), script).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    // Test various recipes
    let test_cases = vec![
        ("test-unit", vec!["testing"]),
        ("build-release", vec!["build"]),
        ("test-build", vec!["testing", "build"]),
        ("deploy-v1.0", vec!["versioned", "has-numbers"]),
        ("task123", vec!["has-numbers"]),
        ("simple-task", vec![]),
    ];

    for (name, expected_tags) in test_cases {
        let recipe = EnrichedRecipe::from_recipe(name.to_string(), None, vec![], false);
        let enriched = handler.process_event(recipe).await.unwrap();
        assert_eq!(
            enriched.tags, expected_tags,
            "Recipe '{}' should have tags {:?}, got {:?}",
            name, expected_tags, enriched.tags
        );
    }
}

/// Test regex replace functionality
#[tokio::test]
async fn test_regex_replace_in_script() {
    init_test_logging();
    let temp = TempDir::new().unwrap();
    let event_dir = temp
        .path()
        .join("runes")
        .join("events")
        .join("recipe_discovered");
    std::fs::create_dir_all(&event_dir).unwrap();

    // Script that normalizes recipe names using regex
    // Note: Rune uses regular strings, need to escape backslashes
    // Note: RecipeEnrichment uses #[serde(flatten)] for extra, so we set fields directly
    let script = r#"
use regex::{replace_all};

pub fn on_recipe_discovered(recipe) {
    let name = recipe["name"];

    // Normalize separators to hyphens
    let normalized = replace_all("[_\\s]+", name, "-");

    // Store normalized name directly (flattened into enrichment)
    #{ normalized_name: normalized }
}
"#;
    std::fs::write(event_dir.join("normalizer.rn"), script).unwrap();

    let config = EventHandlerConfig {
        base_directories: vec![temp.path().to_path_buf()],
    };
    let handler = EventHandler::new(config).unwrap();

    let recipe = EnrichedRecipe::from_recipe("my_test__task".to_string(), None, vec![], false);
    let enriched = handler.process_event(recipe).await.unwrap();

    // Due to flatten, the extra field gets the normalized_name directly
    assert_eq!(
        enriched.extra.get("normalized_name"),
        Some(&serde_json::json!("my-test-task"))
    );
}
