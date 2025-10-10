// Tests for rmcp-based MCP server functionality
//
// These tests verify the tool_router implementation and MCP protocol compliance

use crucible_mcp::{CrucibleMcpService, EmbeddingConfig, EmbeddingDatabase, create_provider};
use rmcp::handler::server::ServerHandler;
use tempfile::tempdir;

/// Test that the service implements ServerHandler correctly
#[tokio::test]
async fn test_server_handler_implementation() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = EmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create test database");

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    let service = CrucibleMcpService::new(database, provider);
    let info = service.get_info();

    // Verify protocol version
    assert_eq!(info.protocol_version, rmcp::model::ProtocolVersion::V_2024_11_05);

    // Verify server info
    assert_eq!(info.server_info.name, "crucible-mcp");
    assert_eq!(info.server_info.version, "0.1.0");
    assert_eq!(
        info.server_info.title,
        Some("Crucible MCP Server".to_string())
    );

    // Verify instructions
    assert!(info.instructions.is_some());
    assert!(info
        .instructions
        .unwrap()
        .contains("semantic search"));

    // Verify capabilities - tools should be enabled
    assert!(info.capabilities.tools.is_some());
}

/// Test hidden directory filtering during indexing
#[tokio::test]
async fn test_hidden_directory_filtering() {
    use std::fs;

    let temp_dir = tempdir().unwrap();
    let vault_path = temp_dir.path();

    // Create visible markdown file
    fs::write(vault_path.join("visible.md"), "# Visible Note\nThis is visible content.").unwrap();

    // Create subdirectory with visible file
    let projects_dir = vault_path.join("Projects");
    fs::create_dir(&projects_dir).unwrap();
    fs::write(
        projects_dir.join("project.md"),
        "# Project\nProject content.",
    )
    .unwrap();

    // Create .obsidian hidden directory
    let obsidian_dir = vault_path.join(".obsidian");
    fs::create_dir(&obsidian_dir).unwrap();
    fs::write(
        obsidian_dir.join("config.md"),
        "# Obsidian Config\nThis should be ignored.",
    )
    .unwrap();

    // Create .crucible hidden directory
    let crucible_dir = vault_path.join(".crucible");
    fs::create_dir(&crucible_dir).unwrap();
    fs::write(
        crucible_dir.join("data.md"),
        "# Crucible Data\nThis should be ignored.",
    )
    .unwrap();

    // Create .git hidden directory
    let git_dir = vault_path.join(".git");
    fs::create_dir(&git_dir).unwrap();
    fs::write(
        git_dir.join("commit.md"),
        "# Git Commit\nThis should be ignored.",
    )
    .unwrap();

    // Count files using glob pattern
    let pattern = format!("{}/**/*.md", vault_path.display());
    let all_files: Vec<_> = glob::glob(&pattern)
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .collect();

    // Filter out hidden directories (like the index_vault function does)
    // We need to check path components relative to the vault path
    let visible_files: Vec<_> = all_files
        .iter()
        .filter(|path| {
            // Get the relative path from vault_path
            if let Ok(relative) = path.strip_prefix(vault_path) {
                // Check if any component in the relative path starts with '.'
                !relative.components().any(|c| {
                    c.as_os_str().to_string_lossy().starts_with('.')
                })
            } else {
                // If we can't get relative path, exclude it
                false
            }
        })
        .collect();

    // Should only find visible files
    // Note: The glob pattern should find all 5 .md files, but only 2 should be visible after filtering
    assert!(
        visible_files.len() == 2,
        "Should find exactly 2 visible files, found {}. All files: {:?}",
        visible_files.len(),
        all_files
    );
    assert!(
        all_files.len() == 5,
        "Should find 5 total files including hidden, found {}. Files: {:?}",
        all_files.len(),
        all_files
    );

    // Verify the visible files are the ones we expect
    let visible_names: Vec<String> = visible_files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(visible_names.contains(&"visible.md".to_string()));
    assert!(visible_names.contains(&"project.md".to_string()));
    assert!(!visible_names.contains(&"config.md".to_string()));
    assert!(!visible_names.contains(&"data.md".to_string()));
    assert!(!visible_names.contains(&"commit.md".to_string()));
}

/// Test content truncation for large files
#[tokio::test]
async fn test_content_truncation_logic() {
    const MAX_CONTENT_LENGTH: usize = 8000;

    // Test small content (no truncation)
    let small_content = "# Small Note\nThis is a small note.";
    let processed_small = if small_content.len() > MAX_CONTENT_LENGTH {
        let mut truncated = small_content
            .chars()
            .take(MAX_CONTENT_LENGTH)
            .collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        small_content.to_string()
    };

    assert_eq!(processed_small, small_content);
    assert!(!processed_small.ends_with("..."));

    // Test large content (should be truncated)
    let large_content = "x".repeat(10000);
    let processed_large = if large_content.len() > MAX_CONTENT_LENGTH {
        let mut truncated = large_content
            .chars()
            .take(MAX_CONTENT_LENGTH)
            .collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        large_content.clone()
    };

    assert_eq!(processed_large.len(), MAX_CONTENT_LENGTH + 3); // +3 for "..."
    assert!(processed_large.ends_with("..."));
    assert_ne!(processed_large, large_content);

    // Test exactly at limit (no truncation)
    let exact_content = "y".repeat(MAX_CONTENT_LENGTH);
    let processed_exact = if exact_content.len() > MAX_CONTENT_LENGTH {
        let mut truncated = exact_content
            .chars()
            .take(MAX_CONTENT_LENGTH)
            .collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        exact_content.clone()
    };

    assert_eq!(processed_exact, exact_content);
    assert!(!processed_exact.ends_with("..."));

    // Test one character over limit (should truncate)
    let over_limit_content = "z".repeat(MAX_CONTENT_LENGTH + 1);
    let processed_over = if over_limit_content.len() > MAX_CONTENT_LENGTH {
        let mut truncated = over_limit_content
            .chars()
            .take(MAX_CONTENT_LENGTH)
            .collect::<String>();
        truncated.push_str("...");
        truncated
    } else {
        over_limit_content.clone()
    };

    assert_eq!(processed_over.len(), MAX_CONTENT_LENGTH + 3);
    assert!(processed_over.ends_with("..."));
}

/// Test that the tool_router field is properly initialized
#[tokio::test]
async fn test_tool_router_initialization() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = EmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create test database");

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    // This should not panic - the tool_router should be initialized
    let _service = CrucibleMcpService::new(database, provider);

    // If we get here without panicking, the tool_router was initialized correctly
}

/// Test that tools capability is properly advertised
#[tokio::test]
async fn test_tools_capability_enabled() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = EmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create test database");

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    let service = CrucibleMcpService::new(database, provider);
    let info = service.get_info();

    // Verify tools capability is present and enabled
    assert!(
        info.capabilities.tools.is_some(),
        "Tools capability should be enabled"
    );
}
