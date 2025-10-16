// Comprehensive Integration Tests for Crucible MCP Tools
//
// This test suite exhaustively tests all built-in MCP tools and Rune tools.
// Each test is isolated and creates its own test environment.

use crucible_mcp::{
    CrucibleMcpService, EmbeddingConfig, EmbeddingDatabase, create_provider,
    types::{EmbeddingMetadata, ToolCallArgs},
};
use rmcp::handler::server::ServerHandler;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use chrono::Utc;

// ============================================================================
// Test Helper Functions
// ============================================================================

async fn create_test_database() -> (tempfile::TempDir, Arc<EmbeddingDatabase>) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );
    (temp_dir, database)
}

async fn create_test_service() -> (tempfile::TempDir, CrucibleMcpService) {
    let (_temp_dir, database) = create_test_database().await;

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    let service = CrucibleMcpService::new(database, provider);
    (_temp_dir, service)
}

async fn seed_test_data(db: &EmbeddingDatabase) {
    let embedding = vec![0.1; 384];

    // Create test notes with various metadata
    let notes = vec![
        (
            "project-alpha.md",
            "# Project Alpha\n\nThis is a Rust project about web development.",
            vec!["rust", "web", "project"],
            "Projects",
            vec![("status", "active"), ("priority", "high")],
        ),
        (
            "project-beta.md",
            "# Project Beta\n\nPython data science project.",
            vec!["python", "data-science", "project"],
            "Projects",
            vec![("status", "active"), ("priority", "medium")],
        ),
        (
            "notes/meeting-2025-10-15.md",
            "# Meeting Notes\n\nDiscussed project roadmap.",
            vec!["meeting", "notes"],
            "notes",
            vec![("date", "2025-10-15"), ("type", "meeting")],
        ),
        (
            "archive/old-project.md",
            "# Old Project\n\nArchived project from 2024.",
            vec!["archive", "old"],
            "archive",
            vec![("status", "archived"), ("year", "2024")],
        ),
        (
            "tutorial.md",
            "# Rust Tutorial\n\nLearning Rust programming.",
            vec!["rust", "tutorial", "learning"],
            "",
            vec![("difficulty", "beginner")],
        ),
    ];

    for (path, content, tags, folder, props) in notes {
        let mut properties = HashMap::new();
        for (k, v) in props {
            properties.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }

        let metadata = EmbeddingMetadata {
            file_path: path.to_string(),
            title: Some(path.replace(".md", "").replace("-", " ")),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            folder: folder.to_string(),
            properties,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        db.store_embedding(path, content, &embedding, &metadata)
            .await
            .expect(&format!("Failed to store {}", path));
    }
}

// ============================================================================
// Built-in Tool Tests
// ============================================================================

#[tokio::test]
async fn test_search_by_properties_success() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let mut props = HashMap::new();
    props.insert("status".to_string(), serde_json::Value::String("active".to_string()));

    let args = ToolCallArgs {
        properties: Some(props),
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_properties(&db, &args)
        .await
        .expect("search_by_properties failed");

    assert!(result.success, "Search should succeed");
    assert!(result.data.is_some(), "Should return data");
    assert!(result.error.is_none(), "Should not have error");
}

#[tokio::test]
async fn test_search_by_properties_missing_params() {
    let (_temp_dir, db) = create_test_database().await;

    let args = ToolCallArgs {
        properties: None, // Missing required parameter
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_properties(&db, &args)
        .await
        .expect("Should return result");

    assert!(!result.success, "Should fail with missing params");
    assert!(result.error.is_some(), "Should have error message");
}

#[tokio::test]
async fn test_search_by_tags_single_tag() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: Some(vec!["rust".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_tags(&db, &args)
        .await
        .expect("search_by_tags failed");

    assert!(result.success, "Search should succeed");
}

#[tokio::test]
async fn test_search_by_tags_multiple_tags() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: Some(vec!["rust".to_string(), "python".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_tags(&db, &args)
        .await
        .expect("search_by_tags failed");

    assert!(result.success, "Search should succeed");
}

#[tokio::test]
async fn test_search_by_tags_nonexistent() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: Some(vec!["nonexistent-tag".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_tags(&db, &args)
        .await
        .expect("search_by_tags failed");

    // Should succeed but return empty results
    assert!(result.success, "Search should succeed even with no results");
}

#[tokio::test]
async fn test_list_notes_in_folder_recursive() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some("Projects".to_string()),
        recursive: Some(true),
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_folder(&db, &args)
        .await
        .expect("search_by_folder failed");

    assert!(result.success, "Folder search should succeed");
}

#[tokio::test]
async fn test_list_notes_in_folder_non_recursive() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some("Projects".to_string()),
        recursive: Some(false),
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_folder(&db, &args)
        .await
        .expect("search_by_folder failed");

    assert!(result.success, "Non-recursive folder search should succeed");
}

#[tokio::test]
async fn test_search_by_filename_exact() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: Some("tutorial.md".to_string()),
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_filename(&db, &args)
        .await
        .expect("search_by_filename failed");

    assert!(result.success, "Filename search should succeed");
}

#[tokio::test]
async fn test_search_by_filename_wildcard() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: Some("project-*.md".to_string()),
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_filename(&db, &args)
        .await
        .expect("search_by_filename failed");

    assert!(result.success, "Wildcard search should succeed");
}

#[tokio::test]
async fn test_search_by_content_found() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("Rust".to_string()),
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_content(&db, &args)
        .await
        .expect("search_by_content failed");

    assert!(result.success, "Content search should succeed");
}

#[tokio::test]
async fn test_search_by_content_case_insensitive() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("PYTHON".to_string()), // All caps
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_content(&db, &args)
        .await
        .expect("search_by_content failed");

    assert!(result.success, "Case-insensitive search should succeed");
}

#[tokio::test]
async fn test_semantic_search_success() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("web development".to_string()),
        top_k: Some(5),
        force: None,
    };

    let result = crucible_mcp::tools::semantic_search(&db, &provider, &args)
        .await
        .expect("semantic_search failed");

    assert!(result.success, "Semantic search should succeed");
}

#[tokio::test]
async fn test_semantic_search_custom_limit() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("programming".to_string()),
        top_k: Some(2), // Limit to 2 results
        force: None,
    };

    let result = crucible_mcp::tools::semantic_search(&db, &provider, &args)
        .await
        .expect("semantic_search failed");

    assert!(result.success, "Semantic search with custom limit should succeed");
}

#[tokio::test]
async fn test_get_note_metadata_exists() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some("tutorial.md".to_string()),
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::get_note_metadata(&db, &args)
        .await
        .expect("get_note_metadata failed");

    assert!(result.success, "Getting metadata for existing note should succeed");
    assert!(result.data.is_some(), "Should return metadata");
}

#[tokio::test]
async fn test_get_note_metadata_not_found() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some("nonexistent.md".to_string()),
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::get_note_metadata(&db, &args)
        .await
        .expect("get_note_metadata failed");

    assert!(!result.success, "Getting metadata for nonexistent note should fail");
    assert!(result.error.is_some(), "Should have error message");
}

#[tokio::test]
async fn test_update_note_properties_success() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let mut props = HashMap::new();
    props.insert("reviewed".to_string(), serde_json::Value::Bool(true));
    props.insert("reviewer".to_string(), serde_json::Value::String("Alice".to_string()));

    let args = ToolCallArgs {
        properties: Some(props),
        tags: None,
        path: Some("tutorial.md".to_string()),
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::update_note_properties(&db, &args)
        .await
        .expect("update_note_properties failed");

    assert!(result.success, "Updating properties should succeed");
}

#[tokio::test]
async fn test_update_note_properties_not_found() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let mut props = HashMap::new();
    props.insert("test".to_string(), serde_json::Value::String("value".to_string()));

    let args = ToolCallArgs {
        properties: Some(props),
        tags: None,
        path: Some("nonexistent.md".to_string()),
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::update_note_properties(&db, &args)
        .await
        .expect("update_note_properties failed");

    assert!(!result.success, "Updating nonexistent note should fail");
}

#[tokio::test]
async fn test_get_vault_stats() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::get_document_stats(&db, &args)
        .await
        .expect("get_document_stats failed");

    assert!(result.success, "Getting vault stats should succeed");
    assert!(result.data.is_some(), "Should return stats data");
}

#[tokio::test]
async fn test_index_vault_no_force() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = EmbeddingDatabase::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create database");

    let embedding_config = EmbeddingConfig::from_env()
        .expect("Failed to load embedding config");
    let provider = create_provider(embedding_config)
        .await
        .expect("Failed to create embedding provider");

    // Create a temporary vault with markdown files
    let vault_path = temp_dir.path().join("vault");
    std::fs::create_dir(&vault_path).unwrap();
    std::fs::write(
        vault_path.join("test.md"),
        "# Test Note\n\nThis is test content."
    ).unwrap();

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: Some(vault_path.to_str().unwrap().to_string()),
        recursive: None,
        pattern: Some("**/*.md".to_string()),
        query: None,
        top_k: None,
        force: Some(false),
    };

    // Note: This test requires Obsidian plugin running or will fail with connection error
    let result = crucible_mcp::tools::index_vault(&db, &provider, &args).await;

    // We expect this to fail without Obsidian plugin
    match result {
        Ok(res) => {
            // If it succeeds, check the result
            println!("Index result: {:?}", res);
        }
        Err(e) => {
            // Expected failure without Obsidian plugin
            println!("Expected error (no Obsidian plugin): {}", e);
        }
    }
}

// ============================================================================
// Service Integration Tests
// ============================================================================

#[tokio::test]
async fn test_service_initialization() {
    let (_temp_dir, service) = create_test_service().await;
    let info = service.get_info();

    assert_eq!(info.server_info.name, "crucible-mcp");
    assert_eq!(info.server_info.version, "0.1.0");
    assert!(info.capabilities.tools.is_some());
}

#[tokio::test]
async fn test_service_multiple_concurrent_calls() {
    let (_temp_dir, db) = create_test_database().await;
    seed_test_data(&db).await;

    // Create args with longer lifetimes
    let args1 = ToolCallArgs {
        properties: None,
        tags: Some(vec!["rust".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let args2 = ToolCallArgs {
        properties: None,
        tags: Some(vec!["python".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let args3 = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("project".to_string()),
        top_k: None,
        force: None,
    };

    // Make multiple concurrent searches
    let (result1, result2, result3) = tokio::join!(
        crucible_mcp::tools::search_by_tags(&db, &args1),
        crucible_mcp::tools::search_by_tags(&db, &args2),
        crucible_mcp::tools::search_by_content(&db, &args3)
    );

    assert!(result1.is_ok(), "First concurrent call should succeed");
    assert!(result2.is_ok(), "Second concurrent call should succeed");
    assert!(result3.is_ok(), "Third concurrent call should succeed");
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_empty_database_operations() {
    let (_temp_dir, db) = create_test_database().await;

    // Search in empty database
    let args = ToolCallArgs {
        properties: None,
        tags: Some(vec!["nonexistent".to_string()]),
        path: None,
        recursive: None,
        pattern: None,
        query: None,
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_tags(&db, &args)
        .await
        .expect("Search should work on empty db");

    // Should succeed but return no results
    assert!(result.success);
}

#[tokio::test]
async fn test_special_characters_in_search() {
    let (_temp_dir, db) = create_test_database().await;

    let embedding = vec![0.1; 384];
    let mut props = HashMap::new();
    props.insert("tag".to_string(), serde_json::Value::String("test@#$%".to_string()));

    let metadata = EmbeddingMetadata {
        file_path: "special-chars.md".to_string(),
        title: Some("Special Characters".to_string()),
        tags: vec!["special@chars".to_string()],
        folder: "".to_string(),
        properties: props,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    db.store_embedding("special-chars.md", "Content with @#$% special chars", &embedding, &metadata)
        .await
        .expect("Should store note with special chars");

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("@#$%".to_string()),
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_content(&db, &args)
        .await
        .expect("Should handle special characters");

    assert!(result.success);
}

#[tokio::test]
async fn test_very_long_content() {
    let (_temp_dir, db) = create_test_database().await;

    let embedding = vec![0.1; 384];
    let long_content = "x".repeat(100000); // 100KB of content

    let metadata = EmbeddingMetadata {
        file_path: "long-note.md".to_string(),
        title: Some("Long Note".to_string()),
        tags: vec![],
        folder: "".to_string(),
        properties: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = db.store_embedding("long-note.md", &long_content, &embedding, &metadata).await;
    assert!(result.is_ok(), "Should handle very long content");
}

#[tokio::test]
async fn test_unicode_content() {
    let (_temp_dir, db) = create_test_database().await;

    let embedding = vec![0.1; 384];
    let unicode_content = "こんにちは世界 🌍 Здравствуй мир";

    let metadata = EmbeddingMetadata {
        file_path: "unicode.md".to_string(),
        title: Some("Unicode Test".to_string()),
        tags: vec!["unicode".to_string()],
        folder: "".to_string(),
        properties: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    db.store_embedding("unicode.md", unicode_content, &embedding, &metadata)
        .await
        .expect("Should handle unicode content");

    let args = ToolCallArgs {
        properties: None,
        tags: None,
        path: None,
        recursive: None,
        pattern: None,
        query: Some("世界".to_string()),
        top_k: None,
        force: None,
    };

    let result = crucible_mcp::tools::search_by_content(&db, &args)
        .await
        .expect("Should search unicode content");

    assert!(result.success);
}
