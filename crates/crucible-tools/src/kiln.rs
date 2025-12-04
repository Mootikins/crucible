//! Kiln operations tools
//!
//! This module provides kiln-specific tools like roots and statistics.

use rmcp::{model::CallToolResult, tool, tool_router};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct KilnTools {
    kiln_path: String,
}

impl KilnTools {
    #[allow(missing_docs)]
    pub fn new(kiln_path: String) -> Self {
        Self { kiln_path }
    }
}

#[tool_router]
impl KilnTools {
    #[tool(description = "Get comprehensive kiln information including root path and statistics")]
    pub async fn get_kiln_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // Get canonical path for URI
        let canonical_path = std::path::Path::new(&self.kiln_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(&self.kiln_path));

        let uri = format!("file://{}", canonical_path.display());

        // Calculate statistics
        let mut total_files = 0;
        let mut total_size = 0;
        let mut md_files = 0;

        if let Ok(entries) = std::fs::read_dir(&self.kiln_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_files += 1;
                        total_size += metadata.len();

                        if entry.path().extension().map_or(false, |ext| ext == "md") {
                            md_files += 1;
                        }
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "root": {
                    "uri": uri,
                    "name": "Kiln Root",
                    "path": self.kiln_path
                },
                "stats": {
                    "total_files": total_files,
                    "markdown_files": md_files,
                    "total_size_bytes": total_size
                }
            }),
        )?]))
    }

    #[tool(description = "Get kiln roots information")]
    pub async fn get_kiln_roots(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let roots = vec![serde_json::json!({
            "uri": format!("file://{}", std::path::Path::new(&self.kiln_path).canonicalize().unwrap_or_else(|_| std::path::PathBuf::from(&self.kiln_path)).display()),
            "name": "Kiln Root"
        })];

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "roots": roots
            }),
        )?]))
    }

    #[tool(description = "Get kiln statistics")]
    pub async fn get_kiln_stats(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut total_files = 0;
        let mut total_size = 0;
        let mut md_files = 0;

        if let Ok(entries) = std::fs::read_dir(&self.kiln_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_files += 1;
                        total_size += metadata.len();

                        if entry.path().extension().map_or(false, |ext| ext == "md") {
                            md_files += 1;
                        }
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "total_files": total_files,
                "markdown_files": md_files,
                "total_size_bytes": total_size,
                "kiln_path": self.kiln_path
            }),
        )?]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_kiln_tools_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path);
        assert_eq!(kiln_tools.kiln_path, temp_dir.path().to_string_lossy());
    }

    #[tokio::test]
    async fn test_get_kiln_roots() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path);

        let result = kiln_tools.get_kiln_roots().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());

        // Check response structure
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert!(parsed["roots"].is_array());
            let roots = parsed["roots"].as_array().unwrap();
            assert_eq!(roots.len(), 1);

            let root = &roots[0];
            assert!(root["uri"].as_str().unwrap().starts_with("file://"));
            assert_eq!(root["name"], "Kiln Root");
        }
    }

    #[tokio::test]
    async fn test_get_kiln_stats_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        let result = kiln_tools.get_kiln_stats().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_files"], 0);
            assert_eq!(parsed["markdown_files"], 0);
            assert_eq!(parsed["total_size_bytes"], 0);
            assert_eq!(parsed["kiln_path"], kiln_path);
        }
    }

    #[tokio::test]
    async fn test_get_kiln_stats_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        // Create some test files
        std::fs::write(temp_dir.path().join("test1.md"), "# Test Note 1").unwrap();
        std::fs::write(temp_dir.path().join("test2.md"), "# Test Note 2").unwrap();
        std::fs::write(temp_dir.path().join("ignore.txt"), "Ignore me").unwrap();

        let result = kiln_tools.get_kiln_stats().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
            assert_eq!(parsed["total_files"], 3);
            assert_eq!(parsed["markdown_files"], 2); // Only .md files
            assert!(parsed["total_size_bytes"].as_u64().unwrap() > 0);
            assert_eq!(parsed["kiln_path"], kiln_path);
        }
    }

    #[tokio::test]
    async fn test_get_kiln_info_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check root information
            assert!(parsed["root"]["uri"]
                .as_str()
                .unwrap()
                .starts_with("file://"));
            assert_eq!(parsed["root"]["name"], "Kiln Root");
            assert_eq!(parsed["root"]["path"], kiln_path);

            // Check statistics
            assert_eq!(parsed["stats"]["total_files"], 0);
            assert_eq!(parsed["stats"]["markdown_files"], 0);
            assert_eq!(parsed["stats"]["total_size_bytes"], 0);
        }
    }

    #[tokio::test]
    async fn test_get_kiln_info_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        // Create some test files
        std::fs::write(temp_dir.path().join("test1.md"), "# Test Note 1").unwrap();
        std::fs::write(
            temp_dir.path().join("test2.md"),
            "# Test Note 2\nWith more content.",
        )
        .unwrap();
        std::fs::write(temp_dir.path().join("ignore.txt"), "Ignore me").unwrap();

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check root information
            assert!(parsed["root"]["uri"]
                .as_str()
                .unwrap()
                .starts_with("file://"));
            assert_eq!(parsed["root"]["name"], "Kiln Root");
            assert_eq!(parsed["root"]["path"], kiln_path);

            // Check statistics
            assert_eq!(parsed["stats"]["total_files"], 3);
            assert_eq!(parsed["stats"]["markdown_files"], 2);
            assert!(parsed["stats"]["total_size_bytes"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn test_tool_router_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let _kiln_tools = KilnTools::new(kiln_path);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = KilnTools::tool_router();
    }
}
