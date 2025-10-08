//! Mock Obsidian HTTP server for integration testing

use mockito::{Matcher, Mock, Server, ServerGuard};
use serde_json::json;
use std::collections::HashMap;

pub struct MockObsidianServer {
    server: ServerGuard,
}

impl MockObsidianServer {
    pub async fn new() -> Self {
        let server = Server::new_async().await;
        Self { server }
    }

    pub fn url(&self) -> String {
        self.server.url()
    }

    pub fn port(&self) -> u16 {
        self.url()
            .trim_start_matches("http://127.0.0.1:")
            .trim_start_matches("http://localhost:")
            .split('/')
            .next()
            .unwrap_or("0")
            .parse()
            .expect("Invalid port")
    }

    pub fn setup_list_files_mock(&mut self, files: Vec<serde_json::Value>) -> Mock {
        self.server
            .mock("GET", "/api/files")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "files": files }).to_string())
            .create()
    }

    pub fn setup_get_file_mock(&mut self, path: &str, content: &str) -> Mock {
        let encoded_path = urlencoding::encode(path);
        self.server
            .mock("GET", format!("/api/file/{}", encoded_path).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({"content": content, "path": path}).to_string())
            .create()
    }

    pub fn setup_get_metadata_mock(&mut self, path: &str, metadata: serde_json::Value) -> Mock {
        let encoded_path = urlencoding::encode(path);
        self.server
            .mock(
                "GET",
                format!("/api/file/{}/metadata", encoded_path).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(metadata.to_string())
            .create()
    }

    pub fn setup_update_properties_mock(&mut self, path: &str, success: bool) -> Mock {
        let encoded_path = urlencoding::encode(path);
        self.server
            .mock(
                "PUT",
                format!("/api/file/{}/properties", encoded_path).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "success": success }).to_string())
            .create()
    }

    pub fn setup_search_by_tags_mock(
        &mut self,
        tags: &[&str],
        files: Vec<serde_json::Value>,
    ) -> Mock {
        let tags_param = tags.join(",");

        self.server
            .mock("GET", "/api/search/tags")
            .match_query(Matcher::UrlEncoded("tags".into(), tags_param))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "files": files }).to_string())
            .create()
    }

    pub fn setup_search_by_properties_mock(
        &mut self,
        properties: HashMap<String, serde_json::Value>,
        files: Vec<serde_json::Value>,
    ) -> Mock {
        let mut query_params = vec![];
        for (key, value) in properties.iter() {
            let value_string = if let Some(s) = value.as_str() {
                s.to_string()
            } else {
                value.to_string().trim_matches('"').to_string()
            };
            query_params.push(Matcher::UrlEncoded(
                format!("properties[{}]", key),
                value_string,
            ));
        }

        self.server
            .mock("GET", "/api/search/properties")
            .match_query(Matcher::AllOf(query_params))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "files": files }).to_string())
            .create()
    }

    pub fn setup_search_by_folder_mock(
        &mut self,
        folder: &str,
        recursive: bool,
        files: Vec<serde_json::Value>,
    ) -> Mock {
        self.server
            .mock("GET", "/api/search/folder")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("path".into(), folder.to_string()),
                Matcher::UrlEncoded("recursive".into(), recursive.to_string()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "files": files }).to_string())
            .create()
    }

    pub fn setup_search_by_content_mock(
        &mut self,
        query: &str,
        files: Vec<serde_json::Value>,
    ) -> Mock {
        self.server
            .mock("GET", "/api/search/content")
            .match_query(Matcher::UrlEncoded("query".into(), query.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "files": files }).to_string())
            .create()
    }

    pub fn setup_get_settings_mock(&mut self, settings: serde_json::Value) -> Mock {
        self.server
            .mock("GET", "/api/settings/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(settings.to_string())
            .create()
    }

    pub fn setup_update_settings_mock(&mut self, success: bool) -> Mock {
        self.server
            .mock("PUT", "/api/settings/embeddings")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "success": success }).to_string())
            .create()
    }

    pub fn setup_list_models_mock(&mut self, models: Vec<String>) -> Mock {
        self.server
            .mock("GET", "/api/settings/embeddings/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({ "models": models }).to_string())
            .create()
    }

    pub fn setup_timeout_mock(&mut self, path: &str) -> Mock {
        self.server
            .mock("GET", path)
            .with_status(504)
            .with_header("content-type", "application/json")
            .with_body(json!({ "error": "Gateway Timeout" }).to_string())
            .create()
    }

    pub fn setup_not_found_mock(&mut self, path: &str) -> Mock {
        self.server
            .mock("GET", path)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(json!({ "error": "Not found" }).to_string())
            .create()
    }

    pub fn setup_server_error_mock(&mut self, path: &str) -> Mock {
        self.server
            .mock("GET", path)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(json!({ "error": "Internal server error" }).to_string())
            .create()
    }

    pub fn setup_rate_limit_mock(&mut self, path: &str) -> Mock {
        self.server
            .mock("GET", path)
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_header("retry-after", "1")
            .with_body(json!({ "error": "Too many requests" }).to_string())
            .create()
    }

    pub fn setup_invalid_json_mock(&mut self, path: &str) -> Mock {
        self.server
            .mock("GET", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("invalid json")
            .create()
    }
}
