//! Query block syntax extension for Crucible
//!
//! This module implements support for query blocks like:
//! ```query:SQL SELECT * FROM documents WHERE tags CONTAINS 'project'```

use super::error::{ParseError, ParseErrorType};
use super::extensions::SyntaxExtension;
use async_trait::async_trait;
use crate::parser::types::{CodeBlock, NoteContent};
use regex::Regex;
use std::sync::Arc;

/// Query block syntax extension
pub struct QueryBlockExtension;

impl QueryBlockExtension {
    /// Create a new query block extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for QueryBlockExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for QueryBlockExtension {
    fn name(&self) -> &'static str {
        "query-blocks"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports query blocks using ```query:language syntax for embedded database queries"
    }

    fn can_handle(&self, content: &str) -> bool {
        content.contains("```query")
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();
        let mut line_offset = 0;

        // Regex to match query blocks: ```query:language\ncode\n```
        let re = match Regex::new(r"```query:([^\n]*)\n([\s\S]*?)```") {
            Ok(re) => re,
            Err(e) => {
                errors.push(ParseError::error(
                    format!("Failed to compile query block regex: {}", e),
                    ParseErrorType::SyntaxError,
                    0,
                    0,
                    0,
                ));
                return errors;
            }
        };

        for cap in re.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let query_type = cap.get(1).unwrap().as_str().trim();
            let query_content = cap.get(2).unwrap().as_str().trim_end();

            // Count lines up to this match to calculate line offset
            line_offset += content[..full_match.start()].matches('\n').count();

            // Validate query type
            if !self.is_valid_query_type(query_type) {
                errors.push(ParseError::warning(
                    format!("Unknown query type: '{}'", query_type),
                    ParseErrorType::SyntaxError,
                    line_offset,
                    0,
                    full_match.start(),
                ));
            }

            // Create a code block representing the query
            let code_block = CodeBlock::new(
                Some(format!("query:{}", query_type)),
                query_content.to_string(),
                full_match.start(),
            );

            doc_content.add_code_block(code_block);
        }

        errors
    }

    fn priority(&self) -> u8 {
        75 // Higher priority than basic parsing
    }
}

impl QueryBlockExtension {
    /// Check if a query type is valid
    fn is_valid_query_type(&self, query_type: &str) -> bool {
        matches!(
            query_type.to_lowercase().as_str(),
            "sql" | "surrealql" | "sparql" | "graphql" | "query"
        )
    }
}

/// Factory function to create the query block extension
pub fn create_query_block_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(QueryBlockExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_query_block_detection() {
        let extension = QueryBlockExtension::new();

        assert!(extension.can_handle("```query:SQL SELECT * FROM table"));
        assert!(extension.can_handle("Some text\n```query:sparql\nINSERT { test: true }\n```"));
        assert!(!extension.can_handle("```sql\nSELECT * FROM table\n```"));
    }

    #[tokio::test]
    async fn test_query_block_parsing() {
        let extension = QueryBlockExtension::new();
        let content = "Some content\n```query:SQL\nSELECT * FROM documents\n```";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.code_blocks.len(), 1);

        let code_block = &doc_content.code_blocks[0];
        assert_eq!(code_block.language, Some("query:SQL".to_string()));
        assert_eq!(code_block.content, "SELECT * FROM documents");
    }

    #[tokio::test]
    async fn test_invalid_query_type() {
        let extension = QueryBlockExtension::new();
        let content = "```query:unknown\nSELECT * FROM table\n```";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ParseErrorType::SyntaxError);
        assert!(errors[0].message.contains("Unknown query type"));

        // Should still parse the code block even with invalid type
        assert_eq!(doc_content.code_blocks.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_query_blocks() {
        let extension = QueryBlockExtension::new();
        let content = r#"
First query:
```query:SQL
SELECT * FROM users
```

Second query:
```query:surrealql
SELECT * FROM documents WHERE tags CONTAINS 'project'
```
        "#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.code_blocks.len(), 2);
    }

    #[tokio::test]
    async fn test_extension_metadata() {
        let extension = QueryBlockExtension::new();

        assert_eq!(extension.name(), "query-blocks");
        assert_eq!(extension.version(), "1.0.0");
        assert!(extension.description().contains("query blocks"));
        assert_eq!(extension.priority(), 75);
        assert!(extension.is_enabled());
    }
}
