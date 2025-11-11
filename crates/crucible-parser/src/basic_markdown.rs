//! Basic Markdown Extension - Parses core markdown structures
//!
//! This extension handles fundamental markdown elements:
//! - Headings (h1-h6)
//! - Paragraphs
//! - Code blocks
//! - Lists
//!
//! It uses pulldown-cmark for robust markdown parsing.

use async_trait::async_trait;
use pulldown_cmark::{Event, HeadingLevel, Parser as CmarkParser, Tag as CmarkTag, TagEnd};
use std::sync::Arc;

use crate::error::ParseError;
use crate::extensions::SyntaxExtension;
use crate::types::{CodeBlock, NoteContent, Heading, ListBlock, ListItem, ListType, Paragraph};

/// Extension for parsing basic markdown structures
#[derive(Debug, Clone)]
pub struct BasicMarkdownExtension {
    enabled: bool,
}

impl BasicMarkdownExtension {
    /// Create a new basic markdown extension
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Create a disabled instance
    pub fn disabled() -> Self {
        Self { enabled: false }
    }

    /// Convert pulldown-cmark HeadingLevel to u8
    fn heading_level_to_u8(level: HeadingLevel) -> u8 {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }
    }

    /// Extract task list content and status
    /// Returns (content_without_checkbox, is_completed)
    fn extract_task_content(text: &str) -> Option<(String, bool)> {
        let trimmed = text.trim();

        if let Some(task_text) = trimmed.strip_prefix("[x] ") {
            Some((task_text.trim().to_string(), true))
        } else {
            trimmed
                .strip_prefix("[ ] ")
                .map(|task_text| (task_text.trim().to_string(), false))
        }
    }
}

impl Default for BasicMarkdownExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for BasicMarkdownExtension {
    fn name(&self) -> &'static str {
        "basic-markdown"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn description(&self) -> &'static str {
        "Parses basic markdown structures: headings, paragraphs, code blocks, and lists"
    }

    fn can_handle(&self, _content: &str) -> bool {
        // This extension handles all markdown content
        true
    }

    fn priority(&self) -> u8 {
        // Highest priority - should run first to establish note structure
        100
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let errors = Vec::new();
        let parser = CmarkParser::new(content);

        let mut current_offset = 0;
        let mut in_heading = false;
        let mut current_heading_level: u8 = 0;
        let mut current_heading_text = String::new();
        let mut current_heading_offset = 0;
        let mut in_code_block = false;
        let mut current_code_lang: Option<String> = None;
        let mut current_code_content = String::new();
        let mut current_code_offset = 0;
        let mut in_paragraph = false;
        let mut current_paragraph_text = String::new();
        let mut current_paragraph_offset = 0;
        let mut current_list: Option<ListBlock> = None;
        let mut in_list_item = false;
        let mut current_list_item_text = String::new();

        for event in parser {
            match event {
                Event::Start(CmarkTag::Heading {
                    level,
                    id: _,
                    classes: _,
                    attrs: _,
                }) => {
                    // Close any open paragraph
                    if in_paragraph {
                        if !current_paragraph_text.trim().is_empty() {
                            doc_content.paragraphs.push(Paragraph::new(
                                current_paragraph_text.clone(),
                                current_paragraph_offset,
                            ));
                        }
                        in_paragraph = false;
                        current_paragraph_text.clear();
                    }

                    in_heading = true;
                    current_heading_level = Self::heading_level_to_u8(level);
                    current_heading_text.clear();
                    current_heading_offset = current_offset;
                }
                Event::End(TagEnd::Heading(_)) => {
                    if in_heading {
                        doc_content.headings.push(Heading::new(
                            current_heading_level,
                            current_heading_text.clone(),
                            current_heading_offset,
                        ));
                        in_heading = false;
                    }
                }
                Event::Start(CmarkTag::Paragraph) => {
                    // Close any open paragraph (shouldn't happen but be safe)
                    if in_paragraph {
                        if !current_paragraph_text.trim().is_empty() {
                            doc_content.paragraphs.push(Paragraph::new(
                                current_paragraph_text.clone(),
                                current_paragraph_offset,
                            ));
                        }
                        current_paragraph_text.clear();
                    }

                    in_paragraph = true;
                    current_paragraph_offset = current_offset;
                }
                Event::End(TagEnd::Paragraph) => {
                    if in_paragraph {
                        if !current_paragraph_text.trim().is_empty() {
                            doc_content.paragraphs.push(Paragraph::new(
                                current_paragraph_text.clone(),
                                current_paragraph_offset,
                            ));
                        }
                        in_paragraph = false;
                        current_paragraph_text.clear();
                    }
                }
                Event::Start(CmarkTag::List(_)) => {
                    // Close any open paragraph before starting a list
                    if in_paragraph {
                        if !current_paragraph_text.trim().is_empty() {
                            doc_content.paragraphs.push(Paragraph::new(
                                current_paragraph_text.clone(),
                                current_paragraph_offset,
                            ));
                        }
                        in_paragraph = false;
                        current_paragraph_text.clear();
                    }

                    current_list = Some(ListBlock::new(ListType::Unordered, current_offset));
                }
                Event::Start(CmarkTag::Item) => {
                    in_list_item = true;
                    current_list_item_text.clear();
                }
                Event::End(TagEnd::Item) => {
                    if in_list_item {
                        let item_text = current_list_item_text.trim().to_string();
                        if !item_text.is_empty() {
                            if let Some(ref mut list) = current_list {
                                if let Some(task_content) = Self::extract_task_content(&item_text) {
                                    list.add_item(ListItem::new_task(
                                        task_content.0,
                                        0,
                                        task_content.1,
                                    ));
                                } else {
                                    list.add_item(ListItem::new(item_text, 0));
                                }
                            }
                        }
                        in_list_item = false;
                        current_list_item_text.clear();
                    }
                }
                Event::End(TagEnd::List(_)) => {
                    if let Some(list) = current_list.take() {
                        if !list.items.is_empty() {
                            doc_content.lists.push(list);
                        }
                    }
                }
                Event::Start(CmarkTag::CodeBlock(kind)) => {
                    // Close any open paragraph/code block
                    if in_paragraph {
                        if !current_paragraph_text.trim().is_empty() {
                            doc_content.paragraphs.push(Paragraph::new(
                                current_paragraph_text.clone(),
                                current_paragraph_offset,
                            ));
                        }
                        in_paragraph = false;
                        current_paragraph_text.clear();
                    }

                    in_code_block = true;
                    current_code_lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                            if lang.is_empty() {
                                None
                            } else {
                                Some(lang.to_string())
                            }
                        }
                        pulldown_cmark::CodeBlockKind::Indented => None,
                    };
                    current_code_content.clear();
                    current_code_offset = current_offset;
                }
                Event::End(TagEnd::CodeBlock) => {
                    if in_code_block {
                        doc_content.code_blocks.push(CodeBlock::new(
                            current_code_lang.clone(),
                            current_code_content.clone(),
                            current_code_offset,
                        ));
                        in_code_block = false;
                    }
                }
                Event::Text(text) => {
                    if in_heading {
                        current_heading_text.push_str(&text);
                    } else if in_code_block {
                        current_code_content.push_str(&text);
                    } else if in_paragraph {
                        current_paragraph_text.push_str(&text);
                    } else if in_list_item {
                        current_list_item_text.push_str(&text);
                    }
                    current_offset += text.len();
                }
                Event::Code(code) => {
                    if in_code_block {
                        current_code_content.push_str(&code);
                    } else if in_paragraph {
                        current_paragraph_text.push_str(&code);
                    } else if in_list_item {
                        current_list_item_text.push_str(&code);
                    }
                    current_offset += code.len();
                }
                Event::SoftBreak | Event::HardBreak => {
                    if in_code_block {
                        current_code_content.push('\n');
                    } else if in_paragraph {
                        current_paragraph_text.push(' ');
                    } else if in_list_item {
                        current_list_item_text.push(' ');
                    }
                    current_offset += 1;
                }
                Event::Rule => {
                    // Horizontal rule detected
                    // Determine style based on the raw content (default to dash)
                    // Note: pulldown-cmark doesn't expose the original characters used,
                    // so we'll default to "dash" for now
                    let style = "dash".to_string();
                    let raw_content = "---".to_string();

                    doc_content.horizontal_rules.push(crate::types::HorizontalRule::new(
                        raw_content,
                        style,
                        current_offset,
                    ));

                    current_offset += 3; // Approximate length
                }
                _ => {}
            }
        }

        // Close any open structures at the end
        if in_paragraph && !current_paragraph_text.trim().is_empty() {
            doc_content.paragraphs.push(Paragraph::new(
                current_paragraph_text.clone(),
                current_paragraph_offset,
            ));
        }

        if in_list_item {
            let item_text = current_list_item_text.trim().to_string();
            if !item_text.is_empty() {
                if let Some(ref mut list) = current_list {
                    if let Some(task_content) = Self::extract_task_content(&item_text) {
                        list.add_item(ListItem::new_task(
                            task_content.0,
                            0,
                            task_content.1,
                        ));
                    } else {
                        list.add_item(ListItem::new(item_text, 0));
                    }
                }
            }
        }

        if let Some(list) = current_list.take() {
            if !list.items.is_empty() {
                doc_content.lists.push(list);
            }
        }

        errors
    }
}

/// Create a basic markdown extension
pub fn create_basic_markdown_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(BasicMarkdownExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_headings() {
        let content = r#"# Level 1

## Level 2

### Level 3"#;

        let ext = BasicMarkdownExtension::new();
        let mut doc = NoteContent::new();
        let errors = ext.parse(content, &mut doc).await;

        assert!(errors.is_empty());
        assert_eq!(doc.headings.len(), 3);
        assert_eq!(doc.headings[0].level, 1);
        assert_eq!(doc.headings[0].text, "Level 1");
        assert_eq!(doc.headings[1].level, 2);
        assert_eq!(doc.headings[1].text, "Level 2");
        assert_eq!(doc.headings[2].level, 3);
        assert_eq!(doc.headings[2].text, "Level 3");
    }

    #[tokio::test]
    async fn test_parse_paragraphs() {
        let content = "This is a paragraph.\n\nThis is another paragraph.";

        let ext = BasicMarkdownExtension::new();
        let mut doc = NoteContent::new();
        let errors = ext.parse(content, &mut doc).await;

        assert!(errors.is_empty());
        assert_eq!(doc.paragraphs.len(), 2);
    }

    #[tokio::test]
    async fn test_parse_code_blocks() {
        let content = r#"```rust
let x = 42;
```

```
no language
```"#;

        let ext = BasicMarkdownExtension::new();
        let mut doc = NoteContent::new();
        let errors = ext.parse(content, &mut doc).await;

        assert!(errors.is_empty());
        assert_eq!(doc.code_blocks.len(), 2);
        assert_eq!(doc.code_blocks[0].language, Some("rust".to_string()));
        assert!(doc.code_blocks[0].content.contains("let x = 42"));
        assert_eq!(doc.code_blocks[1].language, None);
    }

    #[tokio::test]
    async fn test_parse_lists() {
        let content = r#"- Item 1
- Item 2
- [ ] Task 1
- [x] Task 2"#;

        let ext = BasicMarkdownExtension::new();
        let mut doc = NoteContent::new();
        let errors = ext.parse(content, &mut doc).await;

        assert!(errors.is_empty());
        assert_eq!(doc.lists.len(), 1);
        assert_eq!(doc.lists[0].items.len(), 4);
    }

    #[tokio::test]
    async fn test_mixed_content() {
        let content = r#"# Title

This is a paragraph.

## Section

```rust
fn main() {}
```

- Item 1
- Item 2"#;

        let ext = BasicMarkdownExtension::new();
        let mut doc = NoteContent::new();
        let errors = ext.parse(content, &mut doc).await;

        assert!(errors.is_empty());
        assert_eq!(doc.headings.len(), 2);
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(doc.code_blocks.len(), 1);
        assert_eq!(doc.lists.len(), 1);
    }
}
