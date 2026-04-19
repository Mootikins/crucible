use super::super::*;

#[test]
fn test_openai_compatible_parses_data_models_prefers_id() {
    let payload = serde_json::json!({
        "data": [
            { "id": "gpt-4o", "name": "ignored-name" },
            { "name": "fallback-name" },
            { "id": "gpt-4o-mini" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec![
            "gpt-4o".to_string(),
            "fallback-name".to_string(),
            "gpt-4o-mini".to_string()
        ]
    );
}

#[test]
fn test_openai_compatible_parses_models_fallback_shape() {
    let payload = serde_json::json!({
        "models": [
            { "name": "llama-3.1-70b" },
            { "id": "deepseek-chat" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec!["llama-3.1-70b".to_string(), "deepseek-chat".to_string()]
    );
}

#[test]
fn test_openai_compatible_missing_both_keys_errors() {
    // Neither 'data' nor 'models' key → error
    let payload = serde_json::json!({
        "other_key": []
    });

    let result =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string());

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_http_includes_auth_header_and_trims_endpoint() {
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "name": "gpt-4.1-mini" }
            ]
        }),
        Some("test-key"),
    )
    .await;

    let models =
        crate::provider::model_listing::openai_compat::list_models(&(endpoint + "/"), "test-key")
            .await
            .unwrap();
    server.await.unwrap();

    assert_eq!(
        models,
        vec!["gpt-4o".to_string(), "gpt-4.1-mini".to_string()]
    );
}

#[tokio::test]
async fn test_openai_compatible_non_success_status_returns_error() {
    let (endpoint, server) = start_mock_openai_models_server(
        503,
        serde_json::json!({ "error": "service unavailable" }),
        None,
    )
    .await;

    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;
    server.await.unwrap();

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_connection_failure_returns_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let endpoint = format!("http://{}", addr);
    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;

    assert!(result.is_err());
}
