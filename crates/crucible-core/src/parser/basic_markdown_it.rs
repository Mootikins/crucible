//! Basic Markdown Extension using markdown-it parser
//!
//! This extension handles fundamental markdown elements:
//! - Headings (h1-h6)
//! - Paragraphs
//!
//! It uses markdown-it for robust markdown parsing.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use super::error::ParseError;
use super::markdown_it::converter::AstConverter;
use super::types::NoteContent;

/// Extension for parsing basic markdown structures using markdown-it
#[derive(Debug, Clone)]
pub struct BasicMarkdownItExtension {
    md: Arc<markdown_it::MarkdownIt>,
}

impl BasicMarkdownItExtension {
    /// Create a new basic markdown extension using markdown-it
    pub fn new() -> Self {
        let mut md = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        // Add GFM tables support
        markdown_it::plugins::extra::tables::add(&mut md);

        Self { md: Arc::new(md) }
    }
}

impl Default for BasicMarkdownItExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicMarkdownItExtension {
    pub(super) fn can_handle(&self, _content: &str) -> bool {
        // This extension handles all markdown content
        true
    }

    pub(super) fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Parse with markdown-it, catching any panics (e.g., upstream bug in emph_pair.rs)
        // See: https://github.com/rlidwka/markdown-it.rs/issues/48
        let md = Arc::clone(&self.md);
        let content_owned = content.to_string();
        let parse_result = panic::catch_unwind(AssertUnwindSafe(|| md.parse(&content_owned)));

        let ast = match parse_result {
            Ok(ast) => ast,
            Err(panic_info) => {
                // Extract panic message for debugging
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };

                // Find potentially problematic emphasis patterns for diagnostics
                let problematic_patterns = find_emphasis_patterns(content);

                let error_detail = if problematic_patterns.is_empty() {
                    format!(
                        "markdown-it parser panicked: {}. Content length: {} chars",
                        panic_msg,
                        content.len()
                    )
                } else {
                    format!(
                        "markdown-it parser panicked: {}. Likely trigger patterns:\n{}",
                        panic_msg,
                        problematic_patterns.join("\n")
                    )
                };

                tracing::error!("{}", error_detail);

                errors.push(ParseError {
                    message: error_detail,
                    error_type: super::error::ParseErrorType::SyntaxError,
                    line: 0,
                    column: 0,
                    offset: 0,
                    severity: super::error::ErrorSeverity::Error,
                });

                return errors;
            }
        };

        // Convert AST to extract markdown structures
        match AstConverter::convert(&ast, content) {
            Ok(converted) => {
                // Merge extracted content from the AST conversion
                doc_content.blocks.extend(converted.blocks);
                doc_content.headings.extend(converted.headings);
                doc_content.paragraphs.extend(converted.paragraphs);
                doc_content
                    .horizontal_rules
                    .extend(converted.horizontal_rules);
                doc_content.code_blocks.extend(converted.code_blocks);
                doc_content.lists.extend(converted.lists);
                doc_content.tables.extend(converted.tables);
            }
            Err(e) => {
                // The other extensions can still run, so the error is not fatal.
                tracing::error!(error = ?e, "markdown-it conversion failed");
            }
        }

        errors
    }
}

/// Find potentially problematic emphasis patterns that may trigger markdown-it bugs.
/// Returns a list of suspicious patterns with their line numbers.
fn find_emphasis_patterns(content: &str) -> Vec<String> {
    let mut patterns = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num + 1; // 1-indexed

        // Pattern 1: Emphasis marker at start of list item that spans lines
        // e.g., "- _foo" without closing on same line
        if (line.trim_start().starts_with("- _")
            || line.trim_start().starts_with("- *")
            || line.trim_start().starts_with("* _")
            || line.trim_start().starts_with("* *"))
            && !has_balanced_emphasis(line)
        {
            patterns.push(format!(
                "  Line {}: Unbalanced emphasis in list item: {}",
                line_num,
                truncate_line(line, 60)
            ));
        }

        // Pattern 2: Emphasis spanning indented continuation lines
        if line.starts_with("  ") && (line.contains("_") || line.contains("*")) {
            let trimmed = line.trim();
            if trimmed.ends_with('_') || trimmed.ends_with('*') {
                patterns.push(format!(
                    "  Line {}: Emphasis closing in indented block: {}",
                    line_num,
                    truncate_line(line, 60)
                ));
            }
        }

        // Pattern 3: Mixed emphasis markers that might confuse the parser
        if line.contains("_*") || line.contains("*_") {
            patterns.push(format!(
                "  Line {}: Mixed emphasis markers: {}",
                line_num,
                truncate_line(line, 60)
            ));
        }
    }

    // Limit to first 5 patterns to avoid spam
    patterns.truncate(5);
    patterns
}

/// Check if a line has balanced emphasis markers (rough heuristic)
fn has_balanced_emphasis(line: &str) -> bool {
    let underscores = line.chars().filter(|&c| c == '_').count();
    let asterisks = line.chars().filter(|&c| c == '*').count();
    underscores % 2 == 0 && asterisks % 2 == 0
}

/// Truncate a line for display, counting chars rather than bytes.
///
/// A byte-offset cut panics mid-codepoint, and this runs inside the recovery path
/// for an upstream markdown-it panic — an abort there loses the diagnostic.
fn truncate_line(line: &str, max_len: usize) -> String {
    if line.chars().count() <= max_len {
        line.to_string()
    } else {
        format!("{}...", line.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown_it_headings() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "# Heading 1\n\n## Heading 2\n\nParagraph text.",
            &mut content,
        );

        assert!(errors.is_empty());
        assert_eq!(content.headings.len(), 2);
        assert_eq!(content.headings[0].level, 1);
        assert_eq!(content.headings[0].text, "Heading 1");
        assert_eq!(content.headings[1].level, 2);
        assert_eq!(content.headings[1].text, "Heading 2");
    }

    #[test]
    fn test_basic_markdown_it_paragraphs() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "This is a paragraph.\n\nThis is another paragraph.",
            &mut content,
        );

        assert!(errors.is_empty());
        assert_eq!(content.paragraphs.len(), 2);
    }

    #[test]
    fn truncate_line_measures_the_limit_in_chars() {
        // 120 bytes but only 40 chars: under the limit, so it passes through whole.
        // This runs inside the panic-recovery diagnostic, so a byte-offset cut here
        // turned a recovered markdown-it panic into a process abort.
        let line = "\u{65E5}".repeat(40);

        assert_eq!(truncate_line(&line, 60), line);
    }

    #[test]
    fn truncate_line_cuts_multibyte_lines_on_a_char_boundary() {
        // Byte 60 falls inside a 日 here, which aborted the diagnostic outright.
        let line = format!("x{}", "\u{65E5}".repeat(70));

        let truncated = truncate_line(&line, 60);

        assert_eq!(truncated, format!("x{}...", "\u{65E5}".repeat(59)));
    }

    #[test]
    fn each_paragraph_is_emitted_once() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "# Title\n\nAlpha has enough words here.\n\nBeta has enough words here.\n\nGamma has enough words here.",
            &mut content,
        );

        assert!(errors.is_empty());
        let texts: Vec<&str> = content
            .paragraphs
            .iter()
            .map(|p| p.content.as_str())
            .collect();
        assert_eq!(
            texts,
            vec![
                "Alpha has enough words here.",
                "Beta has enough words here.",
                "Gamma has enough words here.",
            ]
        );
    }

    #[test]
    fn paragraph_offsets_point_at_the_source_bytes() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        let source = "Alpha has enough words here.\n\nBeta has enough words here.";

        let errors = ext.parse(source, &mut content);

        assert!(errors.is_empty());
        assert_eq!(content.paragraphs.len(), 2);
        assert_eq!(content.paragraphs[0].offset, 0);
        assert_eq!(content.paragraphs[1].offset, 30);
        assert!(source[content.paragraphs[1].offset..].starts_with("Beta"));
    }

    #[test]
    fn a_container_is_not_re_emitted_as_a_paragraph() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "- item one has several words\n- item two has several words\n\n> quoted text with several words\n\n```rust\nlet x = 42;\n```",
            &mut content,
        );

        assert!(errors.is_empty());
        let texts: Vec<&str> = content
            .paragraphs
            .iter()
            .map(|p| p.content.as_str())
            .collect();
        // A tight list item holds no paragraph node, and `content.lists`
        // already carries the items. A code fence holds no paragraph either.
        // Only the blockquote wraps one.
        assert_eq!(texts, vec!["quoted text with several words"]);
        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 2);
        assert_eq!(content.code_blocks.len(), 1);
    }

    #[test]
    fn the_document_root_is_not_a_paragraph() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();

        let errors = ext.parse(
            "Alpha has enough words here.\n\nBeta has enough words here.",
            &mut content,
        );

        assert!(errors.is_empty());
        assert!(
            !content
                .paragraphs
                .iter()
                .any(|p| p.content.contains("Alpha") && p.content.contains("Beta")),
            "no paragraph spans the whole document"
        );
    }

    #[test]
    fn blocks_come_out_in_document_order_with_real_spans() {
        use crate::parser::types::BlockKind;

        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        let source = concat!(
            "# Title\n\n",
            "Alpha has enough words here.\n\n",
            "- item one\n- item two\n\n",
            "```rust\nlet x = 42;\n```\n\n",
            "> quoted text here\n\n",
            "---\n"
        );

        let errors = ext.parse(source, &mut content);

        assert!(errors.is_empty());
        let kinds: Vec<&BlockKind> = content.blocks.iter().map(|b| &b.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &BlockKind::Heading { level: 1 },
                &BlockKind::Paragraph,
                &BlockKind::List { ordered: false },
                &BlockKind::Code {
                    language: Some("rust".to_string())
                },
                &BlockKind::Blockquote,
                &BlockKind::HorizontalRule,
            ]
        );

        // Spans are ascending, non-overlapping, and slice the real source.
        let mut last_end = 0;
        for block in &content.blocks {
            assert!(
                block.start_offset >= last_end,
                "block {:?} starts before the previous one ended",
                block.kind
            );
            assert!(block.end_offset > block.start_offset);
            assert!(block.end_offset <= source.len());
            last_end = block.end_offset;
        }
        assert!(source[content.blocks[1].start_offset..].starts_with("Alpha"));
        assert!(source[content.blocks[3].start_offset..].starts_with("```rust"));
    }

    #[test]
    fn every_block_carries_its_own_text() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        let source = concat!(
            "# Title\n\n",
            "Alpha has enough words here.\n\n",
            "- item one\n- item two\n\n",
            "```rust\nlet x = 42;\n```\n\n",
            "> quoted text here\n"
        );

        let errors = ext.parse(source, &mut content);

        assert!(errors.is_empty());
        let texts: Vec<&str> = content.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts[0], "Title");
        assert_eq!(texts[1], "Alpha has enough words here.");
        assert!(texts[2].contains("item one"));
        assert!(
            texts[3].contains("let x = 42;"),
            "a code block must carry its source, got {:?}",
            texts[3]
        );
        assert!(texts[4].contains("quoted text here"));
    }

    #[test]
    fn a_block_hashes_its_own_source_bytes() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        let source = "# Title\n\nAlpha has enough words here.\n";

        ext.parse(source, &mut content);

        for block in &content.blocks {
            let bytes = &source.as_bytes()[block.start_offset..block.end_offset];
            let expected = crate::parser::BlockHash::new(*blake3::hash(bytes).as_bytes());
            assert_eq!(
                block.content_hash, expected,
                "block {:?} must hash the source it spans, not its stripped text",
                block.kind
            );
        }
    }

    #[test]
    fn two_identical_blocks_share_a_hash_but_not_a_position() {
        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        // The same sentence twice. The hash is a reuse key, so it collides on
        // purpose; identity has to come from the span.
        let source = "Alpha has enough words here.\n\nAlpha has enough words here.\n";

        ext.parse(source, &mut content);

        assert_eq!(content.blocks.len(), 2);
        assert_eq!(
            content.blocks[0].content_hash, content.blocks[1].content_hash,
            "identical source must reuse one embedding"
        );
        assert_ne!(
            content.blocks[0].start_offset, content.blocks[1].start_offset,
            "the span is what tells the two apart"
        );
    }

    #[test]
    fn a_display_formula_is_its_own_kind() {
        use crate::parser::types::BlockKind;

        let ext = BasicMarkdownItExtension::new();
        let mut content = NoteContent::default();
        let source = "$$\nE = mc^2\n$$\n\n```mermaid\ngraph TD;\nA-->B;\n```\n";

        ext.parse(source, &mut content);

        assert_eq!(content.blocks[0].kind, BlockKind::Latex);
        assert_eq!(
            content.blocks[1].kind,
            BlockKind::Code {
                language: Some("mermaid".to_string())
            },
            "a mermaid fence stays a code block, named by its info string"
        );
    }
}
