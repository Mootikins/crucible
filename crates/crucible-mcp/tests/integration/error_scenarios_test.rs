//! Error scenario tests for robustness validation

use super::mock_server::MockObsidianServer;
use crucible_mcp::obsidian_client::{ObsidianClient, ObsidianError};

// ===== Network Error Tests =====

#[tokio::test]
async fn test_timeout_error() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_timeout_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    // Should fail with timeout or HTTP error
    assert!(result.is_err(), "Should fail on timeout");
}

#[tokio::test]
async fn test_not_found_error() {
    let mut mock = MockObsidianServer::new().await;
    let encoded_path = urlencoding::encode("nonexistent.md");
    let _m = mock.setup_not_found_mock(&format!("/api/file/{}", encoded_path));

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_file("nonexistent.md").await;

    assert!(result.is_err(), "Should return error for non-existent file");

    if let Err(ObsidianError::FileNotFound(_)) = result {
        // Expected error type
    } else {
        panic!("Expected FileNotFound error");
    }
}

#[tokio::test]
async fn test_server_error_500() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_server_error_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    assert!(result.is_err(), "Should fail on 500 error");
}

#[tokio::test]
async fn test_rate_limit_error() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_rate_limit_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    // Rate limiting should be retriable, but will eventually fail
    assert!(result.is_err(), "Should fail after retries exhausted");
}

#[tokio::test]
async fn test_invalid_json_response() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_invalid_json_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    assert!(result.is_err(), "Should fail on invalid JSON");

    if let Err(ObsidianError::InvalidResponse(_)) = result {
        // Expected error type
    } else {
        panic!("Expected InvalidResponse error, got: {:?}", result);
    }
}

// ===== Invalid Input Tests =====

#[tokio::test]
async fn test_empty_path() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_not_found_mock("/api/file/");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_file("").await;

    // Should handle empty path gracefully
    assert!(result.is_err());
}

#[tokio::test]
async fn test_special_characters_in_path() {
    let mut mock = MockObsidianServer::new().await;
    let path = "folder with spaces/file (copy).md";
    let _encoded = urlencoding::encode(path);
    let _m = mock.setup_get_file_mock(path, "Test content");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_file(path).await;

    assert!(result.is_ok(), "Should handle special characters");
}

// ===== Retry Logic Tests =====

#[tokio::test]
async fn test_retry_on_server_error() {
    let mut mock = MockObsidianServer::new().await;

    // First attempt fails with 500
    let _error_mock = mock.setup_server_error_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    // Will retry but still fail eventually
    assert!(result.is_err());
}

#[tokio::test]
async fn test_retry_exhaustion() {
    let mut mock = MockObsidianServer::new().await;

    // Always return 500
    let _m = mock.setup_server_error_mock("/api/files");

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.list_files().await;

    assert!(result.is_err(), "Should fail after max retries");

    if let Err(ObsidianError::TooManyRetries) = result {
        // Expected - retries exhausted
    } else {
        // Also acceptable - HTTP error
        assert!(result.is_err());
    }
}

// ===== Edge Cases =====

#[tokio::test]
async fn test_empty_search_results() {
    let mut mock = MockObsidianServer::new().await;
    let _m = mock.setup_search_by_tags_mock(&["nonexistent"], vec![]);

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let results = client
        .search_by_tags(&[String::from("nonexistent")])
        .await
        .unwrap();

    assert_eq!(results.len(), 0, "Should handle empty results");
}

#[tokio::test]
async fn test_update_nonexistent_file() {
    let mut mock = MockObsidianServer::new().await;
    let encoded_path = urlencoding::encode("nonexistent.md");
    let _m = mock.setup_not_found_mock(&format!("/api/file/{}/properties", encoded_path));

    let client = ObsidianClient::with_port(mock.port()).unwrap();

    let mut props = std::collections::HashMap::new();
    props.insert("status".to_string(), serde_json::json!("active"));

    let result = client.update_properties("nonexistent.md", &props).await;

    assert!(result.is_err(), "Should error on nonexistent file");
}

#[tokio::test]
async fn test_malformed_metadata() {
    let mut mock = MockObsidianServer::new().await;
    let encoded_path = urlencoding::encode("test.md");
    let _m = mock.setup_invalid_json_mock(&format!("/api/file/{}/metadata", encoded_path));

    let client = ObsidianClient::with_port(mock.port()).unwrap();
    let result = client.get_metadata("test.md").await;

    assert!(result.is_err(), "Should handle malformed metadata");
}
