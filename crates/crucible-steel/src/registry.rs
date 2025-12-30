//! Steel tool registry
//!
//! Discovers and manages Steel tools from directories.
//! Uses comment annotations (@tool, @param) for schema extraction.

use crate::error::SteelError;
use crate::executor::SteelExecutor;
use crate::types::{SteelTool, ToolParam, ToolResult};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Registry of discovered Steel tools
pub struct SteelToolRegistry {
    /// Discovered tools by name
    tools: HashMap<String, SteelTool>,
    /// Source code by tool name (for execution)
    sources: HashMap<String, String>,
}

impl SteelToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Result<Self, SteelError> {
        // Verify Steel works by creating a test executor
        let _executor = SteelExecutor::new()?;

        Ok(Self {
            tools: HashMap::new(),
            sources: HashMap::new(),
        })
    }

    /// Discover tools from a directory
    ///
    /// Looks for .scm and .steel files with @tool annotations.
    pub async fn discover_from(&mut self, dir: impl AsRef<Path>) -> Result<usize, SteelError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            debug!("Tool directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| SteelError::Execution(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| SteelError::Execution(e.to_string()))?
        {
            let path = entry.path();

            // Check for .scm or .steel extension
            let is_steel = path
                .extension()
                .map(|e| e == "scm" || e == "steel")
                .unwrap_or(false);

            if !is_steel {
                continue;
            }

            match self.discover_tool(&path).await {
                Ok(Some(tool)) => {
                    info!(
                        "Discovered Steel tool: {} ({} params)",
                        tool.name,
                        tool.params.len()
                    );
                    self.tools.insert(tool.name.clone(), tool);
                    count += 1;
                }
                Ok(None) => {
                    debug!("No tool definition in: {}", path.display());
                }
                Err(e) => {
                    warn!("Failed to discover tool in {}: {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Discover a tool from a single file
    async fn discover_tool(&mut self, path: &Path) -> Result<Option<SteelTool>, SteelError> {
        let source = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| SteelError::Execution(e.to_string()))?;
        let source_path = path.to_string_lossy().to_string();

        // Parse annotations from comments
        let tool = parse_tool_annotations(&source, &source_path)?;

        if let Some(ref t) = tool {
            // Store source for later execution
            self.sources.insert(t.name.clone(), source);
        }

        Ok(tool)
    }

    /// List all discovered tools
    pub fn list_tools(&self) -> Vec<&SteelTool> {
        self.tools.values().collect()
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&SteelTool> {
        self.tools.get(name)
    }

    /// Execute a tool by name
    pub async fn execute(&self, name: &str, args: JsonValue) -> Result<ToolResult, SteelError> {
        let source = self
            .sources
            .get(name)
            .ok_or_else(|| SteelError::FunctionNotFound(name.to_string()))?;

        // Create fresh executor for this execution
        let executor = SteelExecutor::new()?;

        // Load the tool source
        executor.execute_source(source).await?;

        // Call the handler function with args
        let result = executor.call_function(name, vec![args]).await;

        match result {
            Ok(content) => Ok(ToolResult::ok(content)),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

/// Parse @tool and @param annotations from Steel source comments
fn parse_tool_annotations(source: &str, source_path: &str) -> Result<Option<SteelTool>, SteelError> {
    let mut description = String::new();
    let mut params = Vec::new();
    let mut is_tool = false;
    let mut function_name = None;

    for line in source.lines() {
        let trimmed = line.trim();

        // Doc comment (;;; Description)
        if trimmed.starts_with(";;;") {
            description = trimmed.trim_start_matches(";;;").trim().to_string();
        }
        // Annotation comment (;; @tool, ;; @param)
        else if trimmed.starts_with(";;") {
            let content = trimmed.trim_start_matches(";;").trim();

            if content.starts_with("@tool") {
                is_tool = true;
            } else if content.starts_with("@param") {
                // @param name type Description
                let rest = content.trim_start_matches("@param").trim();
                let parts: Vec<&str> = rest.splitn(3, ' ').collect();

                if parts.len() >= 2 {
                    params.push(ToolParam {
                        name: parts[0].to_string(),
                        param_type: parts[1].to_string(),
                        description: parts.get(2).unwrap_or(&"").to_string(),
                        required: true,
                    });
                }
            }
        }
        // Function definition
        else if (trimmed.starts_with("(define ") || trimmed.starts_with("(define/contract "))
            && is_tool
        {
            // Extract function name: (define (name ...) or (define/contract (name ...)
            let after_define = if trimmed.starts_with("(define/contract") {
                trimmed.trim_start_matches("(define/contract").trim()
            } else {
                trimmed.trim_start_matches("(define").trim()
            };

            // Handle both (define (name args) ...) and (define name ...)
            if after_define.starts_with('(') {
                // (define (name arg1 arg2) ...)
                let inner = after_define.trim_start_matches('(');
                if let Some(name_end) = inner.find(|c: char| c.is_whitespace() || c == ')') {
                    function_name = Some(inner[..name_end].to_string());
                }
            } else {
                // (define name value)
                if let Some(name_end) = after_define.find(|c: char| c.is_whitespace()) {
                    function_name = Some(after_define[..name_end].to_string());
                }
            }
            break;
        }
    }

    if is_tool {
        let name = function_name.unwrap_or_else(|| "handler".to_string());
        Ok(Some(SteelTool {
            name,
            description,
            params,
            source_path: source_path.to_string(),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_annotations() {
        let source = r#"
;;; Search for notes
;; @tool
;; @param query string The search query
;; @param limit number Maximum results
(define (handler args)
  (list))
"#;

        let tool = parse_tool_annotations(source, "test.scm").unwrap().unwrap();
        assert_eq!(tool.name, "handler");
        assert_eq!(tool.description, "Search for notes");
        assert_eq!(tool.params.len(), 2);
        assert_eq!(tool.params[0].name, "query");
        assert_eq!(tool.params[0].param_type, "string");
        assert_eq!(tool.params[1].name, "limit");
    }

    #[test]
    fn test_parse_contract_function() {
        let source = r#"
;;; Divide safely
;; @tool
;; @param x number Dividend
;; @param y number Divisor
(define/contract (safe-divide args)
  (->/c hash? number?)
  (/ (hash-ref args 'x) (hash-ref args 'y)))
"#;

        let tool = parse_tool_annotations(source, "test.scm").unwrap().unwrap();
        assert_eq!(tool.name, "safe-divide");
    }
}
