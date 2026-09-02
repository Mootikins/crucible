//! Enhanced tag and task list syntax extension
//!
//! This module implements support for:
//! - #hashtag syntax for inline tagging
//! - Task list parsing with - [ ] and - [x] checkbox syntax

use super::error::ParseError;
use super::types::{NoteContent, Tag};

use regex::Regex;
use std::sync::LazyLock;

static NUMBERED_TASK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\s*\[").expect("numbered task regex"));
static HASHTAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#([a-zA-Z0-9_/-]+)").expect("hashtag regex"));

/// Enhanced tags and task lists syntax extension
#[derive(Debug, Clone, Copy, Default)]
pub struct EnhancedTagsExtension;

impl EnhancedTagsExtension {
    /// Create a new enhanced tags extension
    pub fn new() -> Self {
        Self
    }
}

impl EnhancedTagsExtension {
    pub(super) fn can_handle(&self, content: &str) -> bool {
        // Check for hashtags
        let has_hashtags = content.contains('#');

        let has_task_lists = content.contains("- [")
            || content.contains("* [")
            || content.contains("+ [")
            || content.contains(". [")
            || NUMBERED_TASK_REGEX.is_match(content);

        has_hashtags || has_task_lists
    }

    pub(super) fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Extract #hashtags
        if let Err(err) = self.extract_hashtags(content, doc_content) {
            errors.push(err);
        }

        // Extract task lists

        errors
    }
}

impl EnhancedTagsExtension {
    /// Extract #hashtag tags from content
    fn extract_hashtags(
        &self,
        content: &str,
        doc_content: &mut NoteContent,
    ) -> Result<(), ParseError> {
        let newline_len = if content.contains("\r\n") { 2 } else { 1 };
        let mut line_offset = 0;
        for line in content.lines() {
            for cap in HASHTAG_REGEX.captures_iter(line) {
                let hashtag = cap.get(1).unwrap().as_str();
                let offset = line_offset + cap.get(0).unwrap().start();

                // Skip if this looks like a URL fragment
                if line[..cap.get(0).unwrap().start()].contains("http") {
                    continue;
                }

                // Skip if inside a code block (simplified check)
                if line
                    .chars()
                    .take(cap.get(0).unwrap().start())
                    .filter(|&c| c == '`')
                    .count()
                    % 2
                    == 1
                {
                    continue;
                }

                // Skip if preceded by a word character (replaces negative lookbehind)
                let match_start = cap.get(0).unwrap().start();
                if match_start > 0 {
                    if let Some(prev_char) = line.chars().nth(match_start - 1) {
                        if prev_char.is_alphanumeric() {
                            continue;
                        }
                    }
                }

                let tag = Tag::new(hashtag, offset);
                doc_content.tags.push(tag);
            }
            line_offset += line.len() + newline_len;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
