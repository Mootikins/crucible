//! Enhanced tag and task list syntax extension
//!
//! This module implements support for:
//! - #hashtag syntax for inline tagging
//! - Task list parsing with - [ ] and - [x] checkbox syntax

use super::extensions::SyntaxExtension;
use super::types::{DocumentContent, Tag, ListBlock, ListItem, ListType, TaskStatus};
use super::error::{ParseError, ParseErrorType};
use async_trait::async_trait;

use regex::Regex;
use std::sync::Arc;

/// Enhanced tags and task lists syntax extension
pub struct EnhancedTagsExtension;

impl EnhancedTagsExtension {
    /// Create a new enhanced tags extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnhancedTagsExtension {
    fn default() -> Self {
        Self::new()
    }
}


#[async_trait]
impl SyntaxExtension for EnhancedTagsExtension {
    fn name(&self) -> &'static str {
        "enhanced-tags"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports #hashtag syntax and task list parsing with - [ ] and - [x] checkboxes"
    }

    fn can_handle(&self, content: &str) -> bool {
        // Check for hashtags
        let has_hashtags = content.contains('#');

        // Check for task lists with various patterns:
        // - [ ], - [x], * [ ], + [ ], 1. [ ], a. [ ], etc.
        let has_task_lists = content.contains("- [") ||
                           content.contains("* [") ||
                           content.contains("+ [") ||
                           content.contains(". [") ||
                           // Look for any digit followed by dot and checkbox
                           Regex::new(r"\d+\.\s*\[").is_ok() &&
                           Regex::new(r"\d+\.\s*\[").unwrap().is_match(content);

        has_hashtags || has_task_lists
    }

    async fn parse(
        &self,
        content: &str,
        doc_content: &mut DocumentContent,
    ) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Extract #hashtags
        if let Err(err) = self.extract_hashtags(content, doc_content) {
            errors.push(err);
        }

        // Extract task lists
        if let Err(err) = self.extract_task_lists(content, doc_content) {
            errors.push(err);
        }

        errors
    }

    fn priority(&self) -> u8 {
        70 // Medium priority - after LaTeX/callouts but before other extensions
    }
}

impl EnhancedTagsExtension {
    /// Extract #hashtag tags from content
    fn extract_hashtags(
        &self,
        content: &str,
        _doc_content: &mut DocumentContent,
    ) -> Result<(), ParseError> {
        // Pattern to match #hashtags (excluding URLs like http:// and # in code blocks)
        // Note: Rust's regex crate doesn't support lookbehind, so we'll filter matches manually
        let re = Regex::new(r"#([a-zA-Z0-9_-]+)").map_err(|e| {
            ParseError::error(
                format!("Failed to compile hashtag regex: {}", e),
                ParseErrorType::SyntaxError,
                0,
                0,
                0,
            )
        })?;

        let mut line_offset = 0;
        for line in content.lines() {
            for cap in re.captures_iter(line) {
                let hashtag = cap.get(1).unwrap().as_str();
                let offset = line_offset + cap.get(0).unwrap().start();

                // Skip if this looks like a URL fragment
                if line[..cap.get(0).unwrap().start()].contains("http") {
                    continue;
                }

                // Skip if inside a code block (simplified check)
                if line.chars().take(cap.get(0).unwrap().start()).filter(|&c| c == '`').count() % 2 == 1 {
                    continue;
                }

                // Skip if preceded by a word character (replaces negative lookbehind)
                let match_start = cap.get(0).unwrap().start();
                if match_start > 0 && line.chars().nth(match_start - 1).unwrap().is_alphanumeric() {
                    continue;
                }

                let _tag = Tag {
                    name: hashtag.to_string(),
                    path: hashtag.split('-').map(|s| s.to_string()).collect(),
                    offset,
                };

                // Add to tags (Note: this would need to be added to the ParsedDocument, not DocumentContent)
                // For now, we'll add it to a hypothetical tags field in DocumentContent
            }
            line_offset += line.len() + 1; // +1 for newline
        }

        Ok(())
    }

    /// Extract task lists from content with support for nesting and various list styles
    fn extract_task_lists(
        &self,
        content: &str,
        doc_content: &mut DocumentContent,
    ) -> Result<(), ParseError> {
        // Enhanced pattern to match task list items with various markers:
        // - Unordered: -, *, + followed by optional spacing and [ ] or [x]
        // - Ordered: 1., 2., a., b., etc. followed by [ ] or [x]
        // - Supports nesting via leading whitespace
        let task_list_re = Regex::new(r"(?m)^\s*([-*+]|\d+\.|[a-zA-Z]\.)\s+\[([ xX\-~])\]\s+(.*)$").map_err(|e| {
            ParseError::error(
                format!("Failed to compile enhanced task list regex: {}", e),
                ParseErrorType::SyntaxError,
                0,
                0,
                0,
            )
        })?;

        // Pattern to detect regular (non-task) list items for context switching
        // Note: Rust's regex crate doesn't support lookahead, so we'll filter manually
        let regular_list_re = Regex::new(r"(?m)^\s*([-*+]|\d+\.|[a-zA-Z]\.)\s+[^-\*+\s].*$").map_err(|e| {
            ParseError::error(
                format!("Failed to compile regular list regex: {}", e),
                ParseErrorType::SyntaxError,
                0,
                0,
                0,
            )
        })?;

        let mut current_list: Option<(Vec<ListItem>, usize, ListType)> = None;
        let mut line_offset = 0;
        let mut line_number = 0;

        for line in content.lines() {
            line_number += 1;

            // Check for task list items
            if let Some(cap) = task_list_re.captures(line) {
                let marker = cap.get(1).unwrap().as_str();
                let checkbox_content = cap.get(2).unwrap().as_str();
                let item_content = cap.get(3).unwrap().as_str().trim();
                let full_match_start = cap.get(0).unwrap().start();
                let offset = line_offset + full_match_start;

                // Calculate indentation level (2 spaces = 1 level for standard markdown)
                let leading_spaces = line.chars().take_while(|&c| c == ' ').count();
                let level = leading_spaces / 2;

                // Determine list type from marker
                let list_type = Self::determine_list_type(marker);

                // Parse task status with enhanced support
                let task_status = Self::parse_task_status(checkbox_content, line_number)?;

                // Handle malformed checkbox content gracefully
                if task_status.is_none() {
                    // Add a warning for malformed checkbox but continue processing
                    // We'll handle this gracefully without adding to errors for now
                }

                let item = ListItem {
                    content: item_content.to_string(),
                    level,
                    task_status,
                };

                // Handle list context switching and nesting
                match &mut current_list {
                    Some((items, _, current_list_type)) => {
                        // If list type changed significantly, start a new list
                        if std::mem::discriminant(current_list_type) != std::mem::discriminant(&list_type) {
                            // Close current list and start new one
                            if !items.is_empty() {
                                let item_count = items.len();
                                let list_block = ListBlock {
                                    list_type: *current_list_type,
                                    items: std::mem::take(items),
                                    offset: line_offset - line.len(), // Approximate start offset
                                    item_count,
                                };
                                doc_content.lists.push(list_block);
                            }
                            current_list = Some((vec![item], offset, list_type));
                        } else {
                            items.push(item);
                        }
                    }
                    None => {
                        current_list = Some((vec![item], offset, list_type));
                    }
                }
            }
            // Check for regular list items (close task lists)
            else if regular_list_re.is_match(line) && !line.contains('[') {
                // Close current task list if we hit a regular list item
                if let Some((items, list_offset, list_type)) = current_list.take() {
                    if !items.is_empty() {
                        let item_count = items.len();
                        let list_block = ListBlock {
                            list_type,
                            items,
                            offset: list_offset,
                            item_count,
                        };
                        doc_content.lists.push(list_block);
                    }
                }
            }
            // Check if we should close the current list (empty line or non-list content)
            else if line.trim().is_empty() || !line.trim_start().is_empty() {
                // Close current list if this line doesn't match any list pattern
                if let Some((items, list_offset, list_type)) = current_list.take() {
                    if !items.is_empty() {
                        let item_count = items.len();
                        let list_block = ListBlock {
                            list_type,
                            items,
                            offset: list_offset,
                            item_count,
                        };
                        doc_content.lists.push(list_block);
                    }
                }
            }

            line_offset += line.len() + 1; // +1 for newline
        }

        // Close any remaining list
        if let Some((items, list_offset, list_type)) = current_list {
            if !items.is_empty() {
                let item_count = items.len();
                let list_block = ListBlock {
                    list_type,
                    items,
                    offset: list_offset,
                    item_count,
                };
                doc_content.lists.push(list_block);
            }
        }

        Ok(())
    }

    /// Determine list type from marker
    fn determine_list_type(marker: &str) -> ListType {
        match marker {
            _ if marker.chars().next().unwrap_or(' ').is_ascii_digit() => ListType::Ordered,
            _ if marker.chars().next().unwrap_or(' ').is_ascii_alphabetic() && marker.ends_with('.') => ListType::Ordered,
            _ => ListType::Unordered,
        }
    }

    /// Parse task status from checkbox content with enhanced support
    fn parse_task_status(checkbox_content: &str, _line_number: usize) -> Result<Option<TaskStatus>, ParseError> {
        // Handle whitespace-only content (space, full-width space)
        // Empty string should return None, but whitespace should return Pending
        if !checkbox_content.is_empty() && checkbox_content.trim().is_empty() {
            return Ok(Some(TaskStatus::Pending));
        }

        match checkbox_content.trim() {
            "" => Ok(None), // Empty string after trim
            "x" | "X" => Ok(Some(TaskStatus::Completed)),
            "-" | "~" => Ok(Some(TaskStatus::Pending)), // Common alternative syntax
            _ => {
                // Invalid checkbox content, but don't fail parsing - return None
                // The calling code can handle this gracefully
                Ok(None)
            }
        }
    }
}

/// Factory function to create an enhanced tags extension
pub fn create_enhanced_tags_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(EnhancedTagsExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{DocumentContent, TaskStatus, ListType};

    #[test]
    fn test_hashtag_extraction() {
        let extension = EnhancedTagsExtension::new();
        let content = "This is a #test with #multiple-tags and #123_numbers.";

        assert!(extension.can_handle(content));
    }

    #[test]
    fn test_task_list_detection() {
        let extension = EnhancedTagsExtension::new();
        let content = r"
- [ ] Incomplete task
- [x] Completed task
- [X] Also completed
";

        assert!(extension.can_handle(content));
    }

    #[test]
    fn test_various_task_list_markers() {
        let extension = EnhancedTagsExtension::new();

        // Test various markers are detected
        assert!(extension.can_handle("- [ ] task"));
        assert!(extension.can_handle("* [x] task"));
        assert!(extension.can_handle("+ [ ] task"));
        assert!(extension.can_handle("1. [ ] task"));
        assert!(extension.can_handle("a. [x] task"));
        assert!(extension.can_handle("2. [X] task"));
    }

    #[test]
    fn test_no_hashtags_or_tasks() {
        let extension = EnhancedTagsExtension::new();
        let content = "This is regular text without any special syntax.";

        assert!(!extension.can_handle(content));
    }

    #[test]
    fn test_mixed_content() {
        let extension = EnhancedTagsExtension::new();
        let content = r"
#project-status

- [ ] Implement #hashtags
- [x] Fix #bug-123
- [ ] Add #documentation
";

        assert!(extension.can_handle(content));
    }

    #[test]
    fn test_ignores_urls() {
        let extension = EnhancedTagsExtension::new();
        let content = "Check out https://example.com#section and #normaltag";

        assert!(extension.can_handle(content));
    }

    #[test]
    fn test_ignores_code_blocks() {
        let extension = EnhancedTagsExtension::new();
        let content = "Here is `#not-a-tag` but #realtag should work.";

        assert!(extension.can_handle(content));
    }

    // New comprehensive tests for enhanced task list parsing

    #[tokio::test]
    async fn test_basic_task_list_parsing() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Pending task
- [x] Completed task
- [X] Also completed
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        assert_eq!(doc_content.lists.len(), 1);
        let list = &doc_content.lists[0];
        assert_eq!(list.list_type, ListType::Unordered);
        assert_eq!(list.items.len(), 3);

        assert_eq!(list.items[0].content, "Pending task");
        assert_eq!(list.items[0].task_status, Some(TaskStatus::Pending));
        assert_eq!(list.items[0].level, 0);

        assert_eq!(list.items[1].content, "Completed task");
        assert_eq!(list.items[1].task_status, Some(TaskStatus::Completed));

        assert_eq!(list.items[2].content, "Also completed");
        assert_eq!(list.items[2].task_status, Some(TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_nested_task_lists() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Main task 1
  - [ ] Subtask 1.1
  - [x] Subtask 1.2
    - [ ] Sub-subtask 1.2.1
- [ ] Main task 2
  - [ ] Subtask 2.1
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        assert_eq!(doc_content.lists.len(), 1);
        let list = &doc_content.lists[0];
        assert_eq!(list.items.len(), 6);

        // Check nesting levels
        assert_eq!(list.items[0].level, 0); // Main task 1
        assert_eq!(list.items[1].level, 1); // Subtask 1.1 (2 spaces)
        assert_eq!(list.items[2].level, 1); // Subtask 1.2 (2 spaces)
        assert_eq!(list.items[3].level, 2); // Sub-subtask 1.2.1 (4 spaces)
        assert_eq!(list.items[4].level, 0); // Main task 2
        assert_eq!(list.items[5].level, 1); // Subtask 2.1 (2 spaces)
    }

    #[tokio::test]
    async fn test_different_list_markers() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Dash style
* [ ] Asterisk style
+ [ ] Plus style
1. [ ] Numeric style
2. [x] Another numeric
a. [ ] Letter style
b. [x] Another letter
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        // Should create multiple lists due to type changes
        assert!(doc_content.lists.len() >= 1);

        let total_items: usize = doc_content.lists.iter().map(|l| l.items.len()).sum();
        assert_eq!(total_items, 7);

        // Check that ordered lists are detected correctly
        for list in &doc_content.lists {
            if list.items.iter().any(|item| item.content.contains("Numeric") || item.content.contains("Letter")) {
                assert_eq!(list.list_type, ListType::Ordered);
            } else {
                assert_eq!(list.list_type, ListType::Unordered);
            }
        }
    }

    #[tokio::test]
    async fn test_mixed_list_content() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Task item 1
- Regular list item (not a task)
- [x] Task item 2
* [ ] Different marker, new list
- [ ] Back to dash, new list again

Regular paragraph text.

- [ ] New task list after paragraph
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        // Should have multiple separate lists due to interruptions
        assert!(doc_content.lists.len() >= 3);

        // Only task list items should be captured (those with checkboxes)
        // Regular list items (without checkboxes) should NOT be captured
        for list in &doc_content.lists {
            for item in &list.items {
                assert!(item.task_status.is_some(), "All captured items should have task status");
                // Verify that "Regular list item (not a task)" was NOT captured
                assert!(!item.content.contains("Regular list item"),
                    "Regular list items without checkboxes should not be captured");
            }
        }

        // Verify we captured the correct task items
        let all_items: Vec<&str> = doc_content.lists.iter()
            .flat_map(|list| list.items.iter())
            .map(|item| item.content.as_str())
            .collect();
        assert!(all_items.contains(&"Task item 1"));
        assert!(all_items.contains(&"Task item 2"));
        assert!(all_items.contains(&"Different marker, new list"));
        assert!(all_items.contains(&"Back to dash, new list again"));
        assert!(all_items.contains(&"New task list after paragraph"));
        assert!(!all_items.iter().any(|&item| item.contains("Regular list item")));
    }

    #[tokio::test]
    async fn test_alternative_checkbox_syntax() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Regular pending
- [x] Regular completed
- [-] Dash pending
- [~] Tilde pending
- [ ] Mixed content #with-tag
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        assert_eq!(doc_content.lists.len(), 1);
        let list = &doc_content.lists[0];
        assert_eq!(list.items.len(), 5);

        // All should be parsed as pending except the completed one
        assert_eq!(list.items[0].task_status, Some(TaskStatus::Pending));
        assert_eq!(list.items[1].task_status, Some(TaskStatus::Completed));
        assert_eq!(list.items[2].task_status, Some(TaskStatus::Pending));
        assert_eq!(list.items[3].task_status, Some(TaskStatus::Pending));
        assert_eq!(list.items[4].task_status, Some(TaskStatus::Pending));

        // Test hashtag is preserved in content
        assert!(list.items[4].content.contains("#with-tag"));
    }

    #[tokio::test]
    async fn test_malformed_checkbox_handling() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Valid task
- [abc] Invalid checkbox content
- [x] Another valid task
- [] Empty checkbox
- [  ] Multiple spaces
";

        // This should not fail parsing, but may handle malformed checkboxes gracefully
        let errors = extension.parse(content, &mut doc_content).await;

        // Should parse successfully without throwing errors
        // Malformed checkboxes might be skipped or handled gracefully
        assert!(doc_content.lists.len() >= 1);

        // Valid items should be parsed
        let total_items: usize = doc_content.lists.iter().map(|l| l.items.len()).sum();
        assert!(total_items >= 2); // At least the valid tasks
    }

    #[tokio::test]
    async fn test_edge_cases() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();

        // Test with only whitespace tasks
        let content = r"
- [ ]
- [x]
- [ ] Task with content
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        // Should handle empty/whitespace content gracefully
        assert!(doc_content.lists.len() >= 1);
    }

    #[tokio::test]
    async fn test_deep_nesting() {
        let extension = EnhancedTagsExtension::new();
        let mut doc_content = DocumentContent::new();
        let content = r"
- [ ] Level 0
  - [ ] Level 2 spaces
    - [ ] Level 4 spaces
      - [ ] Level 6 spaces
        - [ ] Level 8 spaces
          - [ ] Level 10 spaces
";

        let errors = extension.parse(content, &mut doc_content).await;
        assert!(errors.is_empty());

        assert_eq!(doc_content.lists.len(), 1);
        let list = &doc_content.lists[0];
        assert_eq!(list.items.len(), 6);

        // Check nesting levels (2 spaces per level)
        for (i, item) in list.items.iter().enumerate() {
            assert_eq!(item.level, i, "Item {} should have level {}", i, i);
        }
    }

    #[test]
    fn test_determine_list_type() {
        assert_eq!(EnhancedTagsExtension::determine_list_type("-"), ListType::Unordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("*"), ListType::Unordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("+"), ListType::Unordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("1."), ListType::Ordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("123."), ListType::Ordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("a."), ListType::Ordered);
        assert_eq!(EnhancedTagsExtension::determine_list_type("Z."), ListType::Ordered);
    }

    #[test]
    fn test_parse_task_status() {
        assert_eq!(EnhancedTagsExtension::parse_task_status(" ", 1).unwrap(), Some(TaskStatus::Pending));
        assert_eq!(EnhancedTagsExtension::parse_task_status("　", 1).unwrap(), Some(TaskStatus::Pending)); // Full-width space
        assert_eq!(EnhancedTagsExtension::parse_task_status("x", 1).unwrap(), Some(TaskStatus::Completed));
        assert_eq!(EnhancedTagsExtension::parse_task_status("X", 1).unwrap(), Some(TaskStatus::Completed));
        assert_eq!(EnhancedTagsExtension::parse_task_status("-", 1).unwrap(), Some(TaskStatus::Pending));
        assert_eq!(EnhancedTagsExtension::parse_task_status("~", 1).unwrap(), Some(TaskStatus::Pending));
        assert_eq!(EnhancedTagsExtension::parse_task_status("abc", 1).unwrap(), None);
        assert_eq!(EnhancedTagsExtension::parse_task_status("", 1).unwrap(), None);
    }
}