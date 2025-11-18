//! Pulldown-cmark based markdown parser implementation

use super::error::ParserResult;
use super::traits::{MarkdownParser, ParserCapabilities};
use super::types::*;
use async_trait::async_trait;
use chrono::Utc;
use pulldown_cmark::{Event, HeadingLevel, Parser as CmarkParser, Tag as CmarkTag, TagEnd};
use std::path::Path;

/// Markdown parser using pulldown-cmark
pub struct PulldownParser {
    capabilities: ParserCapabilities,
}

impl PulldownParser {
    /// Create a new pulldown parser
    pub fn new() -> Self {
        Self {
            capabilities: ParserCapabilities {
                name: "PulldownParser",
                version: "0.1.0",
                yaml_frontmatter: true,
                toml_frontmatter: false,
                wikilinks: true,
                tags: true,
                headings: true,
                code_blocks: true,
                full_content: true,
                max_file_size: Some(10 * 1024 * 1024), // 10 MB
                extensions: vec!["md", "markdown"],
            },
        }
    }
}

impl Default for PulldownParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarkdownParser for PulldownParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        // Read file as raw bytes first (matches scanner behavior for consistent hashing)
        let bytes = tokio::fs::read(path).await?;

        // Check file size limit
        if let Some(max_size) = self.capabilities.max_file_size {
            if bytes.len() > max_size {
                return Err(super::error::ParserError::FileTooLarge {
                    size: bytes.len(),
                    max: max_size,
                });
            }
        }

        // Convert to UTF-8 string for parsing
        let content = String::from_utf8(bytes.clone()).map_err(|e| {
            super::error::ParserError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("File is not valid UTF-8: {}", e),
            ))
        })?;

        // Parse with the raw bytes for consistent hashing with scanner
        self.parse_content_with_bytes(&content, &bytes, path)
    }

    fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        // For backwards compatibility, hash the content string
        // Note: New code should use parse_content_with_bytes for consistent hashing
        self.parse_content_with_bytes(content, content.as_bytes(), source_path)
    }

    fn parse_content_with_bytes(&self, content: &str, raw_bytes: &[u8], source_path: &Path) -> ParserResult<ParsedNote> {
        // Extract frontmatter (YAML between --- delimiters)
        let (frontmatter, body) = extract_frontmatter(content)?;

        // Parse wikilinks with regex
        let wikilinks = extract_wikilinks(body)?;

        // Parse tags with regex
        let tags = extract_tags(body)?;

        // Parse content structure with pulldown-cmark
        let doc_content = parse_content_structure(body)?;

        // Calculate content hash using BLAKE3 on raw bytes (matches FileScanner behavior)
        // This ensures parser and scanner produce identical hashes for the same file
        let mut hasher = blake3::Hasher::new();
        hasher.update(raw_bytes);
        let content_hash = hasher.finalize().to_hex().to_string();

        // Get file size from raw bytes (matches actual file size)
        let file_size = raw_bytes.len() as u64;

        Ok(ParsedNote {
            path: source_path.to_path_buf(),
            frontmatter,
            wikilinks,
            tags,
            content: doc_content,
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: Utc::now(),
            content_hash,
            file_size,
            parse_errors: Vec::new(),
        })
    }

    fn capabilities(&self) -> ParserCapabilities {
        self.capabilities.clone()
    }
}

/// Extract YAML frontmatter from content
fn extract_frontmatter(content: &str) -> ParserResult<(Option<Frontmatter>, &str)> {
    // Check for YAML frontmatter (--- ... ---)
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let yaml = &rest[..end_idx];
            let body = &rest[end_idx + 5..];
            let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);
            return Ok((Some(fm), body));
        }
    }

    // Also handle case where frontmatter ends with ---\r\n (Windows line endings)
    if let Some(rest) = content.strip_prefix("---\r\n") {
        if let Some(end_idx) = rest.find("\r\n---\r\n") {
            let yaml = &rest[..end_idx];
            let body = &rest[end_idx + 7..];
            let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);
            return Ok((Some(fm), body));
        }
    }

    Ok((None, content))
}

/// Extract wikilinks from content using regex
fn extract_wikilinks(content: &str) -> ParserResult<Vec<Wikilink>> {
    use regex::Regex;
    let re = Regex::new(r"!?\[\[([^\]]+)\]\]").unwrap();

    let mut wikilinks = Vec::new();
    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let offset = full_match.start();
        let is_embed = full_match.as_str().starts_with('!');
        let inner = cap.get(1).unwrap().as_str();

        let link = Wikilink::parse(inner, offset, is_embed);
        wikilinks.push(link);
    }

    Ok(wikilinks)
}

/// Extract tags from content using regex
fn extract_tags(content: &str) -> ParserResult<Vec<Tag>> {
    use regex::Regex;
    // Simple regex that matches tags, we'll filter out false positives
    let re = Regex::new(r"#([\w/]+)").unwrap();

    let mut tags = Vec::new();
    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let offset = full_match.start();
        let tag_name = cap.get(1).unwrap().as_str();

        // Filter out false positives like in code blocks or URLs
        if should_include_tag(full_match.as_str(), content, offset) {
            tags.push(Tag::new(tag_name, offset));
        }
    }

    Ok(tags)
}

/// Check if a tag match should be included (filter false positives)
fn should_include_tag(match_text: &str, content: &str, offset: usize) -> bool {
    // Simple heuristic: don't include tags that are part of longer words
    // This filters out things like "header#section" where #section isn't a tag
    let after_end = offset + match_text.len();

    // Check if character after the match is a word character (continuation)
    if after_end < content.len() {
        if let Some(next_char) = content.chars().nth(after_end) {
            if next_char.is_alphanumeric() || next_char == '_' {
                return false; // Part of a word, not a tag
            }
        }
    }

    true
}

/// Parse note structure with pulldown-cmark
fn parse_content_structure(body: &str) -> ParserResult<NoteContent> {
    let parser = CmarkParser::new(body);

    let mut headings = Vec::new();
    let mut code_blocks = Vec::new();
    let mut paragraphs = Vec::new();
    let mut lists = Vec::new();
    let mut horizontal_rules = Vec::new();
    let mut plain_text = String::new();
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
                        paragraphs.push(Paragraph::new(
                            current_paragraph_text.clone(),
                            current_paragraph_offset,
                        ));
                    }
                    in_paragraph = false;
                    current_paragraph_text.clear();
                }

                in_heading = true;
                current_heading_level = heading_level_to_u8(level);
                current_heading_text.clear();
                current_heading_offset = current_offset;
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_heading {
                    headings.push(Heading::new(
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
                        paragraphs.push(Paragraph::new(
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
                        paragraphs.push(Paragraph::new(
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
                        paragraphs.push(Paragraph::new(
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
                    // Create list item from accumulated text
                    let item_text = current_list_item_text.trim().to_string();
                    if !item_text.is_empty() {
                        if let Some(ref mut list) = current_list {
                            // Check if this is a task list item
                            if let Some(task_content) = extract_task_content(&item_text) {
                                list.add_item(ListItem::new_task(
                                    task_content.0,
                                    0, // level detection would need more complex logic
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
                        lists.push(list);
                    }
                }
            }
            Event::Start(CmarkTag::CodeBlock(kind)) => {
                // Close any open paragraph/code block
                if in_paragraph {
                    if !current_paragraph_text.trim().is_empty() {
                        paragraphs.push(Paragraph::new(
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
                    code_blocks.push(CodeBlock::new(
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
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&text);
                    plain_text.push(' ');
                } else if in_list_item {
                    // Accumulate text within list item
                    current_list_item_text.push_str(&text);
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&text);
                    plain_text.push(' ');
                } else {
                    plain_text.push_str(&text);
                    plain_text.push(' '); // Add space between text nodes
                }
                current_offset += text.len();
            }
            Event::Code(code) => {
                if in_code_block {
                    current_code_content.push_str(&code);
                } else if in_paragraph {
                    current_paragraph_text.push_str(&code);
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&code);
                    plain_text.push(' ');
                } else if in_list_item {
                    current_list_item_text.push_str(&code);
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&code);
                    plain_text.push(' ');
                } else {
                    plain_text.push_str(&code);
                    plain_text.push(' ');
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
                } else if !in_heading {
                    plain_text.push(' ');
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

                horizontal_rules.push(HorizontalRule::new(
                    raw_content,
                    style,
                    current_offset,
                ));

                current_offset += 3; // Approximate length
            }
            _ => {}
        }
    }

    // Close any open paragraph at the end
    if in_paragraph && !current_paragraph_text.trim().is_empty() {
        paragraphs.push(Paragraph::new(
            current_paragraph_text.clone(),
            current_paragraph_offset,
        ));
    }

    // Close any open list item at the end
    if in_list_item {
        let item_text = current_list_item_text.trim().to_string();
        if !item_text.is_empty() {
            if let Some(ref mut list) = current_list {
                // Check if this is a task list item
                if let Some(task_content) = extract_task_content(&item_text) {
                    list.add_item(ListItem::new_task(
                        task_content.0,
                        0, // level detection would need more complex logic
                        task_content.1,
                    ));
                } else {
                    list.add_item(ListItem::new(item_text, 0));
                }
            }
        }
    }

    // Close any open list at the end
    if let Some(list) = current_list.take() {
        if !list.items.is_empty() {
            lists.push(list);
        }
    }

    // Calculate word and character counts
    let word_count = plain_text.split_whitespace().count();
    let char_count = plain_text.chars().count();

    // Truncate to 1000 chars if needed
    let plain_text = if plain_text.len() > 1000 {
        let mut truncated: String = plain_text.chars().take(1000).collect();
        truncated.push_str("...");
        truncated
    } else {
        plain_text
    };

    Ok(NoteContent {
        plain_text,
        headings,
        code_blocks,
        paragraphs,
        lists,
        inline_links: Vec::new(),
        wikilinks: Vec::new(),
        tags: Vec::new(),
        latex_expressions: Vec::new(),
        callouts: Vec::new(),
        blockquotes: Vec::new(),
        footnotes: FootnoteMap::new(),
        tables: Vec::new(),
        horizontal_rules,
        word_count,
        char_count,
    })
}

/// Extract task list content and status
/// Returns (content_without_checkbox, is_completed)
fn extract_task_content(text: &str) -> Option<(String, bool)> {
    // Check for task list patterns: [x] or [ ]
    let trimmed = text.trim();

    if let Some(task_text) = trimmed.strip_prefix("[x] ") {
        Some((task_text.trim().to_string(), true))
    } else {
        trimmed
            .strip_prefix("[ ] ")
            .map(|task_text| (task_text.trim().to_string(), false))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_parse_simple_markdown() {
        let parser = PulldownParser::new();
        let content = "# Hello World\n\nThis is a test.";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).unwrap();
        assert_eq!(doc.content.headings.len(), 1);
        assert_eq!(doc.content.headings[0].text, "Hello World");

        assert!(doc.content.plain_text.contains("This is a test"));
    }

    #[tokio::test]
    async fn test_parse_frontmatter() {
        let parser = PulldownParser::new();
        let content = r#"---
title: Test Note
tags: [rust, testing]
---

# Content

Body text here."#;
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).unwrap();
        assert!(doc.frontmatter.is_some());
        let fm = doc.frontmatter.unwrap();
        assert_eq!(fm.get_string("title"), Some("Test Note".to_string()));
    }

    #[tokio::test]
    async fn test_parse_wikilinks() {
        let parser = PulldownParser::new();
        let content = "See [[Other Note]] and [[Reference|alias]].";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).unwrap();
        assert_eq!(doc.wikilinks.len(), 2);
        assert_eq!(doc.wikilinks[0].target, "Other Note");
        assert_eq!(doc.wikilinks[1].target, "Reference");
        assert_eq!(doc.wikilinks[1].alias, Some("alias".to_string()));
    }

    #[tokio::test]
    async fn test_parse_tags() {
        let parser = PulldownParser::new();
        let content = "This has #rust and #testing tags, plus #project/ai.";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).unwrap();
        assert_eq!(doc.tags.len(), 3);
        assert_eq!(doc.tags[0].name, "rust");
        assert_eq!(doc.tags[1].name, "testing");
        assert_eq!(doc.tags[2].name, "project/ai");
    }
}
