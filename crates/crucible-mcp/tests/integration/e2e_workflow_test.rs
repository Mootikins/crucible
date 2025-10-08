//! End-to-end workflow tests

use super::mock_server::MockObsidianServer;
use super::test_data::{TestFileBuilder, TestFixtures};
use crucible_mcp::obsidian_client::ObsidianClient;
use std::collections::HashMap;

#[tokio::test]
async fn test_e2e_vault_indexing() {
    let mut mock = MockObsidianServer::new().await;
    let vault = TestFixtures::sample_vault();
    let file_infos: Vec<_> = vault.iter().map(|f| f.build_file_info()).collect();

    let _m1 = mock.setup_list_files_mock(file_infos.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let files = client.list_files().await.unwrap();

    assert_eq!(files.len(), vault.len());
}

#[tokio::test]
async fn test_e2e_search_and_update() {
    let mut mock = MockObsidianServer::new().await;

    let files = vec![TestFileBuilder::new("project1.md")
        .with_tags(vec!["project"])
        .build_file_info()];

    let _m1 = mock.setup_search_by_tags_mock(&["project"], files.clone());
    let _m2 = mock.setup_update_properties_mock("project1.md", true);

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let results = client
        .search_by_tags(&[String::from("project")])
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    let mut props = HashMap::new();
    props.insert("reviewed".to_string(), serde_json::json!(true));
    let update_result = client
        .update_properties(&results[0].path, &props)
        .await
        .unwrap();
    assert!(update_result.success);
}

#[tokio::test]
async fn test_e2e_embedding_configuration() {
    let mut mock = MockObsidianServer::new().await;

    let _m1 = mock.setup_get_settings_mock(TestFixtures::embedding_settings());
    let _m2 = mock.setup_list_models_mock(TestFixtures::embedding_models());
    let _m3 = mock.setup_update_settings_mock(true);

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let settings = client.get_embedding_settings().await.unwrap();
    assert_eq!(settings.provider, "ollama");

    let models = client.list_embedding_models().await.unwrap();
    assert!(!models.is_empty());
}

#[tokio::test]
async fn test_e2e_folder_content_retrieval() {
    let mut mock = MockObsidianServer::new().await;

    let files = vec![
        TestFileBuilder::new("projects/doc1.md").build_file_info(),
        TestFileBuilder::new("projects/doc2.md").build_file_info(),
    ];

    let _m1 = mock.setup_search_by_folder_mock("projects", true, files.clone());

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let folder_files = client.search_by_folder("projects", true).await.unwrap();

    assert_eq!(folder_files.len(), 2);
}
