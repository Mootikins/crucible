//! Pulldown-cmark based markdown parser implementation

use super::traits::{MarkdownParser, ParserCapabilities};
use super::types::*;
use super::error::ParserResult;
use async_trait::async_trait;
use pulldown_cmark::{Parser as CmarkParser, Event, Tag as CmarkTag, TagEnd, HeadingLevel};
use std::path::Path;
use chrono::Utc;

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
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedDocument> {
        // Read file content
        let content = tokio::fs::read_to_string(path).await?;

        // Check file size limit
        if let Some(max_size) = self.capabilities.max_file_size {
            if content.len() > max_size {
                return Err(super::error::ParserError::FileTooLarge {
                    size: content.len(),
                    max: max_size,
                });
            }
        }

        self.parse_content(&content, path)
    }

    fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedDocument> {
        // Extract frontmatter (YAML between --- delimiters)
        let (frontmatter, body) = extract_frontmatter(content)?;

        // Parse wikilinks with regex
        let wikilinks = extract_wikilinks(body)?;

        // Parse tags with regex
        let tags = extract_tags(body)?;

        // Parse content structure with pulldown-cmark
        let doc_content = parse_content_structure(body)?;

        // Calculate content hash
        let content_hash = format!("{:x}", md5::compute(content));

        // Get file size
        let file_size = content.len() as u64;

        Ok(ParsedDocument {
            path: source_path.to_path_buf(),
            frontmatter,
            wikilinks,
            tags,
            content: doc_content,
            parsed_at: Utc::now(),
            content_hash,
            file_size,
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
    let re = Regex::new(r"#([\w/]+)").unwrap();

    let mut tags = Vec::new();
    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let offset = full_match.start();
        let tag_name = cap.get(1).unwrap().as_str();

        tags.push(Tag::new(tag_name, offset));
    }

    Ok(tags)
}

/// Parse document structure with pulldown-cmark
fn parse_content_structure(body: &str) -> ParserResult<DocumentContent> {
    let parser = CmarkParser::new(body);

    let mut headings = Vec::new();
    let mut code_blocks = Vec::new();
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

    for event in parser {
        match event {
            Event::Start(CmarkTag::Heading{level, id: _, classes: _, attrs: _}) => {
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
            Event::Start(CmarkTag::CodeBlock(kind)) => {
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
                } else {
                    plain_text.push_str(&text);
                    plain_text.push(' '); // Add space between text nodes
                }
                current_offset += text.len();
            }
            Event::Code(code) => {
                if !in_code_block {
                    plain_text.push_str(&code);
                    plain_text.push(' ');
                }
                current_offset += code.len();
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    current_code_content.push('\n');
                } else if !in_heading {
                    plain_text.push(' ');
                }
                current_offset += 1;
            }
            _ => {}
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

    Ok(DocumentContent {
        plain_text,
        headings,
        code_blocks,
        word_count,
        char_count,
    })
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
