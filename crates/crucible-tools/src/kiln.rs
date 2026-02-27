//! Kiln operations tools
//!
//! This module provides kiln-specific tools like roots and statistics.

#![allow(missing_docs)]

use rmcp::{model::CallToolResult, tool, tool_router};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct KilnTools {
    kiln_path: String,
}

impl KilnTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String) -> Self {
        Self { kiln_path }
    }
}

#[tool_router]
impl KilnTools {
    #[tool(description = "Get comprehensive kiln information")]
    pub async fn get_kiln_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // Extract kiln name from path
        let name = std::path::Path::new(&self.kiln_path)
            .file_name()
            .map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );

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

                        if entry.path().extension().is_some_and(|ext| ext == "md") {
                            md_files += 1;
                        }
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "name": name,
                "total_files": total_files,
                "markdown_files": md_files,
                "total_size_bytes": total_size
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

            // Check flat structure
            assert!(parsed["name"].as_str().is_some());
            assert_eq!(parsed["total_files"], 0);
            assert_eq!(parsed["markdown_files"], 0);
            assert_eq!(parsed["total_size_bytes"], 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
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

            // Check flat structure
            assert!(parsed["name"].as_str().is_some());
            assert_eq!(parsed["total_files"], 3);
            assert_eq!(parsed["markdown_files"], 2);
            assert!(parsed["total_size_bytes"].as_u64().unwrap() > 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
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
