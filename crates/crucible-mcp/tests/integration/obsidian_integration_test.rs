//! Integration tests for Obsidian API endpoints

use super::mock_server::MockObsidianServer;
use super::test_data::{TestFileBuilder, TestFixtures};
use crucible_mcp::obsidian_client::ObsidianClient;
use std::collections::HashMap;

#[tokio::test]
async fn test_list_files_integration() {
    let mut mock = MockObsidianServer::new().await;
    let vault = TestFixtures::sample_vault();
    let file_infos: Vec<_> = vault.iter().map(|f| f.build_file_info()).collect();

    let _m = mock.setup_list_files_mock(file_infos.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let files = client.list_files().await.unwrap();

    assert_eq!(files.len(), vault.len(), "Should return all vault files");
}

#[tokio::test]
async fn test_get_file_integration() {
    let mut mock = MockObsidianServer::new().await;
    let file = TestFileBuilder::new("test.md").with_content("# Test\n\nContent");

    let _m = mock.setup_get_file_mock(&file.path, &file.build_content());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let content = client.get_file("test.md").await.unwrap();

    assert!(content.contains("Test"));
}

#[tokio::test]
async fn test_search_by_tags_integration() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![TestFileBuilder::new("ai.md").build_file_info()];

    let _m = mock.setup_search_by_tags_mock(&["ai"], files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_tags(&[String::from("ai")]).await.unwrap();

    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_search_by_folder_integration() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![
        TestFileBuilder::new("projects/p1.md").build_file_info(),
        TestFileBuilder::new("projects/p2.md").build_file_info(),
    ];

    let _m = mock.setup_search_by_folder_mock("projects", false, files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_folder("projects", false).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_search_by_properties_integration() {
    let mut mock = MockObsidianServer::new().await;
    let mut props = HashMap::new();
    props.insert("status".to_string(), serde_json::json!("active"));

    let files = vec![TestFileBuilder::new("active.md").build_file_info()];
    let _m = mock.setup_search_by_properties_mock(props.clone(), files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let search_props: HashMap<String, String> = props
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();

    let results = client.search_by_properties(&search_props).await.unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_update_properties_integration() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_update_properties_mock("test.md", true);

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let mut props = HashMap::new();
    props.insert("status".to_string(), serde_json::json!("completed"));

    let result = client.update_properties("test.md", &props).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_get_embedding_settings_integration() {
    let mut mock = MockObsidianServer::new().await;
    let settings = TestFixtures::embedding_settings();

    let _m = mock.setup_get_settings_mock(settings.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_embedding_settings().await.unwrap();

    assert_eq!(result.provider, "ollama");
    assert_eq!(result.model, "nomic-embed-text");
}

#[tokio::test]
async fn test_update_embedding_settings_integration() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_update_settings_mock(true);

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let settings = crucible_mcp::obsidian_client::EmbeddingSettings {
        provider: "ollama".to_string(),
        api_url: "http://localhost:11434".to_string(),
        api_key: None,
        model: "nomic-embed-text".to_string(),
    };

    let result = client.update_embedding_settings(&settings).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_list_embedding_models_integration() {
    let mut mock = MockObsidianServer::new().await;
    let models = TestFixtures::embedding_models();

    let _m = mock.setup_list_models_mock(models.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.list_embedding_models().await.unwrap();

    assert_eq!(results.len(), 3);
    assert!(results.contains(&"nomic-embed-text".to_string()));
}

#[tokio::test]
async fn test_get_metadata_integration() {
    let mut mock = MockObsidianServer::new().await;
    let file = TestFileBuilder::new("test.md")
        .with_tags(vec!["test", "example"])
        .with_property("key", serde_json::json!("value"));

    let _m = mock.setup_get_metadata_mock(&file.path, file.build_metadata());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let metadata = client.get_metadata("test.md").await.unwrap();

    assert_eq!(metadata.path, "test.md");
    assert_eq!(metadata.tags.len(), 2);
}

#[tokio::test]
async fn test_search_by_content_integration() {
    let mut mock = MockObsidianServer::new().await;
    let files = vec![TestFileBuilder::new("ml.md").build_file_info()];

    let _m = mock.setup_search_by_content_mock("machine learning", files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client.search_by_content("machine learning").await.unwrap();

    assert_eq!(results.len(), 1);
}
