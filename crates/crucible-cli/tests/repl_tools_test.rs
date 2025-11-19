use crucible_cli::commands::repl::tools::{UnifiedToolRegistry, ToolStatus};
use crucible_tools::types::ToolConfigContext;
use crucible_core::traits::{KnowledgeRepository, NoteMetadata};
use crucible_core::parser::ParsedNote;
use crucible_core::types::SearchResult;
use crucible_core::Result as CoreResult;
use async_trait::async_trait;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

struct MockKnowledgeRepository {
    root_path: std::path::PathBuf,
}

impl MockKnowledgeRepository {
    fn new(root_path: std::path::PathBuf) -> Self {
        Self { root_path }
    }
}

#[async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(&self, name: &str) -> CoreResult<Option<ParsedNote>> {
        // Simple mock: try to read file from root_path
        let path = self.root_path.join(name);
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap();
            // Create a dummy ParsedNote
            let mut note = ParsedNote::default();
            note.path = path;
            note.content = crucible_core::parser::NoteContent::new().with_plain_text(content);
            Ok(Some(note))
        } else {
            Ok(None)
        }
    }

    async fn list_notes(&self, _path_filter: Option<&str>) -> CoreResult<Vec<NoteMetadata>> {
        // Simple mock: list files in root_path
        let mut notes = Vec::new();
        for entry in fs::read_dir(&self.root_path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                notes.push(NoteMetadata {
                    name: path.file_name().unwrap().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    title: Some(path.file_stem().unwrap().to_string_lossy().to_string()),
                    tags: vec![],
                    created_at: None,
                    updated_at: None,
                });
            }
        }
        Ok(notes)
    }

    async fn search_vectors(&self, _vector: Vec<f32>) -> CoreResult<Vec<SearchResult>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn test_unified_tool_registry_initialization() {
    let temp_dir = TempDir::new().unwrap();
    let context = ToolConfigContext::new().with_kiln_path(temp_dir.path().to_path_buf());
    
    let registry = UnifiedToolRegistry::new(temp_dir.path().to_path_buf(), context)
        .await
        .expect("Failed to create registry");

    let tools = registry.list_tools().await;
    assert!(tools.contains(&"read_note".to_string()));
    assert!(tools.contains(&"list_notes".to_string()));
    assert!(tools.contains(&"search_notes".to_string()));
}

#[tokio::test]
async fn test_read_note_tool_execution() {
    let temp_dir = TempDir::new().unwrap();
    let note_path = temp_dir.path().join("test_note.md");
    fs::write(&note_path, "# Test Note\nContent").unwrap();

    let mock_repo = Arc::new(MockKnowledgeRepository::new(temp_dir.path().to_path_buf()));
    let context = ToolConfigContext::new()
        .with_kiln_path(temp_dir.path().to_path_buf())
        .with_knowledge_repo(mock_repo);

    let registry = UnifiedToolRegistry::new(temp_dir.path().to_path_buf(), context)
        .await
        .expect("Failed to create registry");

    // Test execution with name parameter
    let result = registry.execute_tool("read_note", &["test_note.md".to_string()])
        .await
        .expect("Failed to execute tool");

    match result.status {
        ToolStatus::Success => {
            assert!(result.output.contains("Test Note"));
        },
        ToolStatus::Error(e) => {
            panic!("Tool execution failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_list_notes_tool_execution() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("note1.md"), "# Note 1").unwrap();
    fs::write(temp_dir.path().join("note2.md"), "# Note 2").unwrap();

    let mock_repo = Arc::new(MockKnowledgeRepository::new(temp_dir.path().to_path_buf()));
    let context = ToolConfigContext::new()
        .with_kiln_path(temp_dir.path().to_path_buf())
        .with_knowledge_repo(mock_repo);

    let registry = UnifiedToolRegistry::new(temp_dir.path().to_path_buf(), context)
        .await
        .expect("Failed to create registry");

    let result = registry.execute_tool("list_notes", &[])
        .await
        .expect("Failed to execute tool");

    match result.status {
        ToolStatus::Success => {
            assert!(result.output.contains("note1.md"));
            assert!(result.output.contains("note2.md"));
        },
        ToolStatus::Error(e) => {
            panic!("Tool execution failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_permission_denial() {
    use crucible_tools::permission::{PermissionManager, Permission};
    use std::sync::Arc;

    let temp_dir = TempDir::new().unwrap();
    
    // Create a permission manager that only allows listing notes, not reading
    let pm = Arc::new(PermissionManager::with_permissions(vec![
        Permission::ExecuteTool("list_notes".to_string())
    ]));

    let mock_repo = Arc::new(MockKnowledgeRepository::new(temp_dir.path().to_path_buf()));
    let context = ToolConfigContext::new()
        .with_kiln_path(temp_dir.path().to_path_buf())
        .with_knowledge_repo(mock_repo)
        .with_permission_manager(pm);

    let registry = UnifiedToolRegistry::new(temp_dir.path().to_path_buf(), context)
        .await
        .expect("Failed to create registry");

    // Try to execute read_note (should fail)
    let result = registry.execute_tool("read_note", &["test.md".to_string()])
        .await
        .expect("Failed to execute tool");

    match result.status {
        ToolStatus::Success => {
            panic!("Tool execution should have failed due to permission denial");
        },
        ToolStatus::Error(e) => {
            assert!(e.contains("Permission denied"), "Error should be permission denied, got: {}", e);
        }
    }
}

#[tokio::test]
async fn test_get_tool_schema() {
    let temp_dir = TempDir::new().unwrap();
    let mock_repo = Arc::new(MockKnowledgeRepository::new(temp_dir.path().to_path_buf()));
    let context = ToolConfigContext::new()
        .with_kiln_path(temp_dir.path().to_path_buf())
        .with_knowledge_repo(mock_repo);

    let registry = UnifiedToolRegistry::new(temp_dir.path().to_path_buf(), context)
        .await
        .expect("Failed to create registry");

    // Test existing tool schema
    let schema = registry.get_tool_schema("read_note")
        .await
        .expect("Failed to get schema")
        .expect("Schema should exist for read_note");

    assert_eq!(schema.name, "read_note");
    assert!(schema.description.contains("Read the content"));
    assert!(schema.input_schema.get("required").is_some());

    // Test non-existent tool schema
    let schema = registry.get_tool_schema("non_existent_tool")
        .await
        .expect("Failed to get schema");

    assert!(schema.is_none(), "Schema should be None for non-existent tool");
}
