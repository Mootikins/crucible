//! Convert markdown-it AST to Crucible's NoteContent format

use crate::error::ParserResult;
use crate::types::*;
use markdown_it::Node;

/// Converts markdown-it AST to NoteContent
pub struct AstConverter;

impl AstConverter {
    /// Convert a markdown-it AST to NoteContent
    pub fn convert(root: &Node) -> ParserResult<NoteContent> {
        let mut content = NoteContent::new();

        // Walk the AST and extract content
        Self::walk_node(root, &mut content)?;

        // Calculate word and character counts
        content.word_count = Self::calculate_word_count(&content);
        content.char_count = Self::calculate_char_count(&content);

        Ok(content)
    }

    fn walk_node(node: &Node, content: &mut NoteContent) -> ParserResult<()> {
        // Extract custom Obsidian-style syntax nodes

        // 1. Wikilinks
        if let Some(wikilink) = node.cast::<super::plugins::wikilink::WikilinkNode>() {
            content.wikilinks.push(Wikilink {
                target: wikilink.target.clone(),
                alias: wikilink.alias.clone(),
                offset: wikilink.offset,
                is_embed: wikilink.is_embed,
                block_ref: wikilink.block_ref.clone(),
                heading_ref: wikilink.heading_ref.clone(),
            });
        }

        // 2. Tags
        if let Some(tag) = node.cast::<super::plugins::tag::TagNode>() {
            content.tags.push(Tag::new(tag.name.clone(), tag.offset));
        }

        // 3. Callouts - TODO: Implement after fixing block rule API
        // if let Some(callout) = node.cast::<super::plugins::callout::CalloutNode>() { ... }

        // 4. LaTeX expressions
        if let Some(latex) = node.cast::<super::plugins::latex::LatexNode>() {
            content.latex_expressions.push(LatexExpression::new(
                latex.expression.clone(),
                latex.is_block,
                latex.offset,
                latex.expression.len() + if latex.is_block { 4 } else { 2 }, // Include delimiters
            ));
        }

        // Extract text for paragraphs (very simplified)
        let text = Self::extract_text(node);
        if !text.trim().is_empty() && text.len() > 10 {
            // Rough heuristic for paragraph detection
            content.paragraphs.push(Paragraph::new(text, 0));
        }

        // Recursively process children
        for child in node.children.iter() {
            Self::walk_node(child, content)?;
        }

        Ok(())
    }

    /// Extract plain text from a node and its children
    fn extract_text(node: &Node) -> String {
        use markdown_it::parser::inline::Text;

        let mut text = String::new();

        // If this is a text node, get its content
        if let Some(text_node) = node.cast::<Text>() {
            text.push_str(&text_node.content);
        }

        // Recursively collect text from children
        for child in node.children.iter() {
            let child_text = Self::extract_text(child);
            if !child_text.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&child_text);
            }
        }

        text
    }

    fn calculate_word_count(content: &NoteContent) -> usize {
        let mut count = 0;

        // Count words in paragraphs
        for para in &content.paragraphs {
            count += para.content.split_whitespace().count();
        }

        // Count words in headings
        for heading in &content.headings {
            count += heading.text.split_whitespace().count();
        }

        count
    }

    fn calculate_char_count(content: &NoteContent) -> usize {
        let mut count = 0;

        // Count chars in paragraphs
        for para in &content.paragraphs {
            count += para.content.chars().count();
        }

        // Count chars in headings
        for heading in &content.headings {
            count += heading.text.chars().count();
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_it::MarkdownIt;

    fn setup_parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        super::super::plugins::add_wikilink_plugin(&mut md);
        md
    }

    #[test]
    fn test_convert_simple_content() {
        let md = setup_parser();
        let ast = md.parse("# Heading\n\nParagraph text.");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.headings.len(), 1);
        assert_eq!(content.headings[0].text, "Heading");
        assert!(content.paragraphs.len() >= 1);
    }

    #[test]
    fn test_convert_wikilinks() {
        let md = setup_parser();
        let ast = md.parse("Link to [[Other Note]] here.");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.wikilinks.len(), 1);
        assert_eq!(content.wikilinks[0].target, "Other Note");
        assert_eq!(content.wikilinks[0].alias, None);
    }

    #[test]
    fn test_word_count() {
        let md = setup_parser();
        let ast = md.parse("# Title\n\nThis is a test paragraph.");

        let content = AstConverter::convert(&ast).unwrap();

        // "Title" + "This is a test paragraph" = 6 words
        assert!(content.word_count >= 6);
    }
}
