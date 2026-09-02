//! Convert markdown-it AST to Crucible's NoteContent format

use crate::parser::error::ParserResult;
use crate::parser::types::*;
use markdown_it::plugins::cmark::block::blockquote::Blockquote as MdBlockquote;
use markdown_it::plugins::cmark::block::code::CodeBlock as MdIndentedCode;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::hr::ThematicBreak;
use markdown_it::plugins::cmark::block::lheading::SetextHeader;
use markdown_it::plugins::cmark::block::list::{BulletList, OrderedList};
use markdown_it::plugins::cmark::block::paragraph::Paragraph as MdParagraph;
use markdown_it::plugins::extra::tables::Table as MdTable;
use markdown_it::Node;

/// Converts markdown-it AST to NoteContent
pub struct AstConverter;

impl AstConverter {
    /// Convert a markdown-it AST to NoteContent
    pub fn convert(root: &Node, source: &str) -> ParserResult<NoteContent> {
        let mut content = NoteContent::new();

        // One pass over the root's direct children: the document's top-level
        // blocks, in order, each carrying its own source-map span.
        content.blocks = Self::top_level_blocks(root, source);

        Ok(content)
    }

    /// Build the ordered block list from the root's direct children.
    ///
    /// A child of the root is a top-level block. Its source map gives both
    /// ends of its span, so nothing here measures a block by arithmetic over
    /// stripped text. A node whose kind is not one of the seven is skipped
    /// rather than guessed at.
    fn top_level_blocks(root: &Node, source: &str) -> Vec<Block> {
        root.children
            .iter()
            .filter_map(|node| {
                let (mut kind, text) = Self::classify(node)?;
                let (start_offset, end_offset) = node.srcmap?.get_byte_offsets();

                // Two of Crucible's block types are not markdown-it nodes, so
                // the source span decides, not the stripped text: a display
                // formula arrives as a paragraph, a callout as a blockquote.
                if kind == BlockKind::Paragraph
                    && Self::is_display_formula(source, start_offset, end_offset)
                {
                    kind = BlockKind::Latex;
                }
                if kind == BlockKind::Blockquote {
                    if let Some(callout_type) =
                        Self::callout_marker(source, start_offset, end_offset)
                    {
                        kind = BlockKind::Callout { callout_type };
                    }
                }

                Some(Block::new(kind, text, start_offset, end_offset, source))
            })
            .collect()
    }

    /// The callout type a `> [!type]` marker names, if the span opens with one.
    fn callout_marker(source: &str, start_offset: usize, end_offset: usize) -> Option<CalloutType> {
        let span = source.get(start_offset..end_offset)?;
        let first = span.lines().next()?.trim_start();
        let rest = first.strip_prefix('>')?.trim_start();
        let inner = rest.strip_prefix("[!")?;
        let name = inner.split(']').next()?;
        (!name.is_empty()).then(|| name.parse::<CalloutType>().unwrap_or(CalloutType::Note))
    }

    /// True when the span is a `$$ ... $$` display formula.
    fn is_display_formula(source: &str, start_offset: usize, end_offset: usize) -> bool {
        let Some(span) = source.get(start_offset..end_offset) else {
            return false;
        };
        let span = span.trim();
        span.len() > 4 && span.starts_with("$$") && span.ends_with("$$")
    }

    /// Classify one top-level node and take its text, or `None` when it is
    /// not a block Crucible names.
    ///
    /// Code holds its source on the node rather than in inline text children,
    /// so it is read from the node. Everything else joins its inline text.
    fn classify(node: &Node) -> Option<(BlockKind, String)> {
        if let Some(heading) = node.cast::<ATXHeading>() {
            let level = heading.level;
            return Some((BlockKind::Heading { level }, Self::extract_text(node)));
        }
        if let Some(heading) = node.cast::<SetextHeader>() {
            let level = heading.level;
            return Some((BlockKind::Heading { level }, Self::extract_text(node)));
        }
        if node.is::<MdParagraph>() {
            return Some((BlockKind::Paragraph, Self::extract_text(node)));
        }
        if let Some(fence) = node.cast::<CodeFence>() {
            let language = fence
                .info
                .split_whitespace()
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            return Some((BlockKind::Code { language }, fence.content.clone()));
        }
        if let Some(code) = node.cast::<MdIndentedCode>() {
            return Some((BlockKind::Code { language: None }, code.content.clone()));
        }
        if node.is::<BulletList>() {
            return Some((BlockKind::List { ordered: false }, Self::extract_text(node)));
        }
        if node.is::<OrderedList>() {
            return Some((BlockKind::List { ordered: true }, Self::extract_text(node)));
        }
        if node.is::<MdBlockquote>() {
            // A callout is a blockquote to markdown-it. The marker decides.
            return Some((BlockKind::Blockquote, Self::extract_text(node)));
        }
        if node.is::<MdTable>() {
            return Some((BlockKind::Table, Self::extract_text(node)));
        }
        if node.is::<ThematicBreak>() {
            return Some((BlockKind::HorizontalRule, String::new()));
        }
        None
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_it::MarkdownIt;

    fn setup_parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        md
    }

    fn blocks_of(source: &str) -> Vec<Block> {
        let md = setup_parser();
        let ast = md.parse(source);
        AstConverter::convert(&ast, source).unwrap().blocks
    }

    #[test]
    fn a_heading_and_a_paragraph_become_two_blocks() {
        let blocks = blocks_of("# Heading\n\nParagraph text.");

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(blocks[0].text, "Heading");
        assert_eq!(blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(blocks[1].text, "Paragraph text.");
    }

    #[test]
    fn a_task_list_is_one_list_block_carrying_its_items() {
        // Checkbox state is not a block property. `workflow.rs` parses task
        // syntax itself, over the raw body, for TASKS.md.
        let blocks = blocks_of("- [ ] first task\n- [x] second task");

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::List { ordered: false });
        assert!(blocks[0].text.contains("first task"));
        assert!(blocks[0].text.contains("second task"));
    }

    #[test]
    fn an_ordered_list_is_marked_ordered() {
        let blocks = blocks_of("1. one\n2. two");

        assert_eq!(blocks[0].kind, BlockKind::List { ordered: true });
    }

    #[test]
    fn a_thematic_break_is_a_block_with_no_text() {
        let blocks = blocks_of("before the rule\n\n---\n\nafter the rule");

        assert_eq!(blocks[1].kind, BlockKind::HorizontalRule);
        assert!(blocks[1].text.is_empty());
        assert!(blocks[1].end_offset > blocks[1].start_offset);
    }
}
