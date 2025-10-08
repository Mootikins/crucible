//! Unit tests for HTTP client methods

use super::mock_server::MockObsidianServer;
use super::test_data::{TestFileBuilder, TestFixtures};
use crucible_mcp::obsidian_client::ObsidianClient;

#[tokio::test]
async fn test_client_creation() {
    let result = ObsidianClient::new();
    assert!(result.is_ok(), "Should create client successfully");
}

#[tokio::test]
async fn test_client_with_custom_port() {
    let result = ObsidianClient::with_port(8080);
    assert!(result.is_ok(), "Should create client with custom port");
}

#[tokio::test]
async fn test_list_files_empty() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_list_files_mock(vec![]);

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let files = client.list_files().await.unwrap();

    assert_eq!(files.len(), 0, "Should return empty list");
}

#[tokio::test]
async fn test_list_files_with_data() {
    let mut mock = MockObsidianServer::new().await;
    let vault = TestFixtures::sample_vault();
    let file_infos: Vec<_> = vault.iter().map(|f| f.build_file_info()).collect();

    let _m = mock.setup_list_files_mock(file_infos.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let files = client.list_files().await.unwrap();

    assert_eq!(files.len(), vault.len(), "Should return all files");
    assert_eq!(files[0].path, "projects/project-alpha.md");
}

#[tokio::test]
async fn test_get_file_content() {
    let mut mock = MockObsidianServer::new().await;
    let file = TestFileBuilder::new("test.md").with_content("Test content");

    let _m = mock.setup_get_file_mock(&file.path, &file.build_content());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let content = client.get_file("test.md").await.unwrap();

    assert!(content.contains("Test content"));
}

#[tokio::test]
async fn test_get_metadata() {
    let mut mock = MockObsidianServer::new().await;
    let file = TestFileBuilder::new("test.md")
        .with_tags(vec!["test", "example"])
        .with_property("key", serde_json::json!("value"));

    let _m = mock.setup_get_metadata_mock(&file.path, file.build_metadata());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let metadata = client.get_metadata("test.md").await.unwrap();

    assert_eq!(metadata.path, "test.md");
    assert_eq!(metadata.tags.len(), 2);
    assert!(metadata.properties.contains_key("key"));
}

#[tokio::test]
async fn test_search_by_tags() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![
        TestFileBuilder::new("file1.md")
            .with_tags(vec!["ai"])
            .build_file_info(),
        TestFileBuilder::new("file2.md")
            .with_tags(vec!["ai"])
            .build_file_info(),
    ];

    let _m = mock.setup_search_by_tags_mock(&["ai"], files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_tags(&[String::from("ai")]).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_search_by_folder() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![
        TestFileBuilder::new("projects/file1.md").build_file_info(),
        TestFileBuilder::new("projects/file2.md").build_file_info(),
    ];

    let _m = mock.setup_search_by_folder_mock("projects", false, files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_folder("projects", false).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_search_by_content() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![TestFileBuilder::new("ai.md")
        .with_content("Machine learning and AI")
        .build_file_info()];

    let _m = mock.setup_search_by_content_mock("machine learning", files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_content("machine learning").await.unwrap();

    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_update_properties() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_update_properties_mock("test.md", true);

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let mut props = std::collections::HashMap::new();
    props.insert("status".to_string(), serde_json::json!("completed"));

    let result = client.update_properties("test.md", &props).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_get_embedding_settings() {
    let mut mock = MockObsidianServer::new().await;
    let settings = TestFixtures::embedding_settings();

    let _m = mock.setup_get_settings_mock(settings.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_embedding_settings().await.unwrap();

    assert_eq!(result.provider, "ollama");
    assert_eq!(result.model, "nomic-embed-text");
}

#[tokio::test]
async fn test_list_embedding_models() {
    let mut mock = MockObsidianServer::new().await;
    let models = TestFixtures::embedding_models();

    let _m = mock.setup_list_models_mock(models.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_embedding_models().await.unwrap();

    assert_eq!(result.len(), 3);
    assert!(result.contains(&"nomic-embed-text".to_string()));
}
