//! Integration tests for ExtendedMcpServer
//!
//! Tests tool aggregation, routing, and proper MCP exposure
//! for all three tool sources: Kiln, Just, and Rune.

use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_rune::RuneDiscoveryConfig;
use crucible_tools::ExtendedMcpServer;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

// =============================================================================
// Mock implementations
// =============================================================================

struct MockKnowledgeRepository;
struct MockEmbeddingProvider;

#[async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }
}

fn create_mocks() -> (Arc<dyn KnowledgeRepository>, Arc<dyn EmbeddingProvider>) {
    (
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
    )
}

// =============================================================================
// Kiln tool tests (baseline 12 tools)
// =============================================================================

#[tokio::test]
async fn test_kiln_tools_always_present() {
    let temp = TempDir::new().unwrap();
    let (knowledge_repo, embedding_provider) = create_mocks();

    let server = ExtendedMcpServer::kiln_only(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    let tools = server.list_all_tools().await;
    assert_eq!(tools.len(), 12, "Should have exactly 12 kiln tools");

    // Verify all expected tool names
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // NoteTools (6)
    assert!(tool_names.contains(&"create_note"));
    assert!(tool_names.contains(&"read_note"));
    assert!(tool_names.contains(&"read_metadata"));
    assert!(tool_names.contains(&"update_note"));
    assert!(tool_names.contains(&"delete_note"));
    assert!(tool_names.contains(&"list_notes"));

    // SearchTools (3)
    assert!(tool_names.contains(&"semantic_search"));
    assert!(tool_names.contains(&"text_search"));
    assert!(tool_names.contains(&"property_search"));

    // KilnTools (3)
    assert!(tool_names.contains(&"get_kiln_info"));
    assert!(tool_names.contains(&"get_kiln_roots"));
    assert!(tool_names.contains(&"get_kiln_stats"));
}

#[tokio::test]
async fn test_kiln_tool_schemas() {
    let temp = TempDir::new().unwrap();
    let (knowledge_repo, embedding_provider) = create_mocks();

    let server = ExtendedMcpServer::kiln_only(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    let tools = server.list_all_tools().await;

    // Check create_note has required schema
    let create_note = tools.iter().find(|t| t.name.as_ref() == "create_note").unwrap();
    assert!(create_note.description.is_some());
    assert!(!create_note.input_schema.is_empty());

    // Check semantic_search has query parameter
    let semantic = tools.iter().find(|t| t.name.as_ref() == "semantic_search").unwrap();
    assert!(semantic.description.is_some());
}

// =============================================================================
// Rune tool integration tests
// =============================================================================

#[tokio::test]
async fn test_rune_tools_discovered() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");
    fs::create_dir_all(&runes_dir).unwrap();

    // Create multi-tool Rune file
    fs::write(
        runes_dir.join("tools.rn"),
        r#"
#[tool(desc = "Greet someone")]
#[param(name = "name", type = "string", desc = "Name to greet")]
pub fn greet(name) {
    Ok(format!("Hello, {}!", name))
}

#[tool(desc = "Add two numbers")]
#[param(name = "a", type = "integer", desc = "First number")]
#[param(name = "b", type = "integer", desc = "Second number")]
pub fn add(a, b) {
    Ok(a + b)
}
"#,
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // Should have 12 kiln + 2 rune tools
    assert_eq!(tools.len(), 14, "Should have 12 kiln + 2 rune tools");

    // Rune tools have rune_ prefix
    assert!(tool_names.contains(&"rune_greet"));
    assert!(tool_names.contains(&"rune_add"));
}

#[tokio::test]
async fn test_rune_tool_schema_preserved() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");
    fs::create_dir_all(&runes_dir).unwrap();

    fs::write(
        runes_dir.join("schema_test.rn"),
        r#"
#[tool(desc = "Test tool with various params", tags = ["test", "schema"])]
#[param(name = "text", type = "string", desc = "Text input")]
#[param(name = "count", type = "integer", desc = "Count value")]
#[param(name = "enabled", type = "boolean", desc = "Enable flag")]
#[param(name = "optional_val", type = "string", desc = "Optional", required = false)]
pub fn schema_test(text, count, enabled, optional_val) {}
"#,
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let test_tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "rune_schema_test")
        .unwrap();

    // Verify description
    assert_eq!(
        test_tool.description.as_ref().map(|s| s.as_ref()),
        Some("Test tool with various params")
    );

    // Verify schema has properties
    let schema = &test_tool.input_schema;
    assert!(schema.contains_key("properties"));
    assert!(schema.contains_key("required"));
}

#[tokio::test]
async fn test_mixed_legacy_and_multi_tool_rune() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");
    fs::create_dir_all(&runes_dir).unwrap();

    // Legacy single-tool format
    fs::write(
        runes_dir.join("legacy.rn"),
        r#"//! Legacy format tool
//! @param input string The input text

pub fn main(input) {
    Ok(input)
}
"#,
    )
    .unwrap();

    // Multi-tool format
    fs::write(
        runes_dir.join("modern.rn"),
        r#"
#[tool(desc = "Modern tool A")]
pub fn tool_a() {}

#[tool(desc = "Modern tool B")]
pub fn tool_b() {}
"#,
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // Should have: 12 kiln + 1 legacy + 2 modern = 15
    assert_eq!(tools.len(), 15);

    assert!(tool_names.contains(&"rune_legacy"));
    assert!(tool_names.contains(&"rune_tool_a"));
    assert!(tool_names.contains(&"rune_tool_b"));
}

// =============================================================================
// Directory overlay tests (global + kiln)
// =============================================================================

#[tokio::test]
async fn test_multiple_rune_directories() {
    let temp = TempDir::new().unwrap();

    // Global runes directory
    let global_dir = temp.path().join("global_runes");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("global_tool.rn"),
        "//! Global tool\npub fn main() {}",
    )
    .unwrap();

    // Kiln-specific runes directory
    let kiln_dir = temp.path().join("kiln_runes");
    fs::create_dir_all(&kiln_dir).unwrap();
    fs::write(
        kiln_dir.join("kiln_tool.rn"),
        "//! Kiln-specific tool\npub fn main() {}",
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![global_dir, kiln_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // 12 kiln + 2 rune (one from each directory)
    assert_eq!(tools.len(), 14);
    assert!(tool_names.contains(&"rune_global_tool"));
    assert!(tool_names.contains(&"rune_kiln_tool"));
}

#[tokio::test]
async fn test_kiln_overlay_wins_on_duplicate_names() {
    let temp = TempDir::new().unwrap();

    // Global directory with "shared" tool
    let global_dir = temp.path().join("global");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("shared.rn"),
        "//! Global version\npub fn main() {}",
    )
    .unwrap();

    // Kiln directory with same name "shared" tool
    let kiln_dir = temp.path().join("kiln");
    fs::create_dir_all(&kiln_dir).unwrap();
    fs::write(
        kiln_dir.join("shared.rn"),
        "//! Kiln version\npub fn main() {}",
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    // Global first, kiln second - kiln should override
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![global_dir, kiln_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let shared_tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "rune_shared")
        .unwrap();

    // Kiln version should win (processed second)
    assert_eq!(
        shared_tool.description.as_ref().map(|s| s.as_ref()),
        Some("Kiln version")
    );
}

// =============================================================================
// Tool routing tests
// =============================================================================

#[test]
fn test_tool_prefix_routing() {
    // Just tools
    assert!(ExtendedMcpServer::is_just_tool("just_build"));
    assert!(ExtendedMcpServer::is_just_tool("just_test_all"));
    assert!(!ExtendedMcpServer::is_just_tool("rune_tool"));
    assert!(!ExtendedMcpServer::is_just_tool("create_note"));

    // Rune tools
    assert!(ExtendedMcpServer::is_rune_tool("rune_greet"));
    assert!(ExtendedMcpServer::is_rune_tool("rune_complex_tool_name"));
    assert!(!ExtendedMcpServer::is_rune_tool("just_build"));
    assert!(!ExtendedMcpServer::is_rune_tool("semantic_search"));

    // Kiln tools (neither prefix)
    assert!(!ExtendedMcpServer::is_just_tool("create_note"));
    assert!(!ExtendedMcpServer::is_rune_tool("create_note"));
}

// =============================================================================
// Refresh/hot-reload behavior tests
// =============================================================================

#[tokio::test]
async fn test_rune_refresh_discovers_new_tools() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");
    fs::create_dir_all(&runes_dir).unwrap();

    // Start with one tool
    fs::write(
        runes_dir.join("initial.rn"),
        "//! Initial tool\npub fn main() {}",
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir.clone()],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    // Initial count: 12 kiln + 1 rune
    assert_eq!(server.tool_count().await, 13);

    // Add another tool
    fs::write(
        runes_dir.join("added.rn"),
        "//! Added tool\npub fn main() {}",
    )
    .unwrap();

    // Refresh rune tools
    let new_count = server.refresh_rune().await.unwrap();
    assert_eq!(new_count, 2, "Should now have 2 rune tools");

    // Total should be 12 kiln + 2 rune
    assert_eq!(server.tool_count().await, 14);
}

// =============================================================================
// Empty/missing directory tests
// =============================================================================

#[tokio::test]
async fn test_nonexistent_rune_directory_handled() {
    let temp = TempDir::new().unwrap();
    let (knowledge_repo, embedding_provider) = create_mocks();

    // Point to nonexistent directory
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![temp.path().join("nonexistent_dir")],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    // Should still have 12 kiln tools, 0 rune
    assert_eq!(server.tool_count().await, 12);
}

#[tokio::test]
async fn test_empty_rune_directory_handled() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");
    fs::create_dir_all(&runes_dir).unwrap();
    // Directory exists but is empty

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir],
        extensions: vec!["rn".to_string()],
        recursive: false,
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    // Should still have 12 kiln tools, 0 rune
    assert_eq!(server.tool_count().await, 12);
}

// =============================================================================
// Recursive discovery tests
// =============================================================================

#[tokio::test]
async fn test_recursive_rune_discovery() {
    let temp = TempDir::new().unwrap();
    let runes_dir = temp.path().join("runes");

    // Create nested structure
    fs::create_dir_all(runes_dir.join("notes")).unwrap();
    fs::create_dir_all(runes_dir.join("utils")).unwrap();

    fs::write(
        runes_dir.join("root.rn"),
        "//! Root tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        runes_dir.join("notes").join("note_tool.rn"),
        "//! Notes tool\npub fn main() {}",
    )
    .unwrap();
    fs::write(
        runes_dir.join("utils").join("util_tool.rn"),
        "//! Utils tool\npub fn main() {}",
    )
    .unwrap();

    let (knowledge_repo, embedding_provider) = create_mocks();
    let rune_config = RuneDiscoveryConfig {
        tool_directories: vec![runes_dir],
        extensions: vec!["rn".to_string()],
        recursive: true, // Enable recursive scanning
    };

    let server = ExtendedMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        temp.path(),
        rune_config,
    )
    .await
    .unwrap();

    let tools = server.list_all_tools().await;
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // 12 kiln + 3 rune (recursive)
    assert_eq!(tools.len(), 15, "Should discover tools in subdirectories");
    assert!(tool_names.contains(&"rune_root"));
    assert!(tool_names.contains(&"rune_note_tool"));
    assert!(tool_names.contains(&"rune_util_tool"));
}
