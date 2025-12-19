//! Convert markdown-it AST to Crucible's NoteContent format

use crate::error::ParserResult;
use crate::types::*;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::plugins::cmark::block::heading::ATXHeading;
use markdown_it::plugins::cmark::block::hr::ThematicBreak;
use markdown_it::plugins::cmark::block::list::{BulletList, ListItem as MdListItem, OrderedList};
use markdown_it::plugins::extra::tables::{Table as MdTable, TableCell, TableHead, TableRow};
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

        // 3. Callouts
        if let Some(callout) = node.cast::<super::plugins::callout::CalloutNode>() {
            let crucible_callout = if let Some(title) = &callout.title {
                Callout::with_title(
                    callout.callout_type.clone(),
                    title.clone(),
                    callout.content.clone(),
                    callout.offset,
                )
            } else {
                Callout::new(
                    callout.callout_type.clone(),
                    callout.content.clone(),
                    callout.offset,
                )
            };
            content.callouts.push(crucible_callout);
        }

        // 4. LaTeX expressions
        if let Some(latex) = node.cast::<super::plugins::latex::LatexNode>() {
            content.latex_expressions.push(LatexExpression::new(
                latex.expression.clone(),
                latex.is_block,
                latex.offset,
                latex.expression.len() + if latex.is_block { 4 } else { 2 }, // Include delimiters
            ));
        }

        // 5. Headings (from CommonMark ATX heading syntax)
        if let Some(heading) = node.cast::<ATXHeading>() {
            let text = Self::extract_text(node);
            if !text.is_empty() {
                let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
                content
                    .headings
                    .push(Heading::new(heading.level, text, offset));
            }
        }

        // 6. Horizontal rules / thematic breaks (---, ***, ___)
        if let Some(hr) = node.cast::<ThematicBreak>() {
            let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
            let style = match hr.marker {
                '-' => "dash",
                '*' => "asterisk",
                '_' => "underscore",
                _ => "unknown",
            }
            .to_string();
            // Build raw_content from marker and marker_len
            let raw_content: String = std::iter::repeat_n(hr.marker, hr.marker_len).collect();
            content
                .horizontal_rules
                .push(HorizontalRule::new(raw_content, style, offset));
        }

        // 7. Code blocks (fenced code blocks ```language ... ```)
        if let Some(fence) = node.cast::<CodeFence>() {
            let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
            // Extract language from info string (first word)
            let language = fence
                .info
                .split_whitespace()
                .next()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            content
                .code_blocks
                .push(CodeBlock::new(language, fence.content.clone(), offset));
        }

        // 8. Ordered lists
        if node.cast::<OrderedList>().is_some() {
            let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
            let mut list_block = ListBlock::new(ListType::Ordered, offset);

            // Count items by iterating children
            for child in node.children.iter() {
                if child.cast::<MdListItem>().is_some() {
                    let item_text = Self::extract_text(child);
                    let (checkbox_status, cleaned_text) = Self::extract_checkbox(&item_text);
                    let mut item = ListItem::new(cleaned_text, 0);
                    item.checkbox_status = checkbox_status;
                    list_block.add_item(item);
                }
            }

            content.lists.push(list_block);
        }

        // 9. Unordered (bullet) lists
        if node.cast::<BulletList>().is_some() {
            let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
            let mut list_block = ListBlock::new(ListType::Unordered, offset);

            // Count items by iterating children
            for child in node.children.iter() {
                if child.cast::<MdListItem>().is_some() {
                    let item_text = Self::extract_text(child);
                    let (checkbox_status, cleaned_text) = Self::extract_checkbox(&item_text);
                    let mut item = ListItem::new(cleaned_text, 0);
                    item.checkbox_status = checkbox_status;
                    list_block.add_item(item);
                }
            }

            content.lists.push(list_block);
        }

        // 10. Tables (GFM tables)
        if node.cast::<MdTable>().is_some() {
            let offset = node.srcmap.map(|s| s.get_byte_offsets().0).unwrap_or(0);
            let (rows, columns, headers) = Self::extract_table_structure(node);

            // Build raw content by extracting text from all cells
            let raw_content = Self::extract_text(node);

            content
                .tables
                .push(Table::new(raw_content, headers, columns, rows, offset));
        }

        // Extract text for paragraphs (very simplified) - skip headings
        if node.cast::<ATXHeading>().is_none() {
            let text = Self::extract_text(node);
            if !text.trim().is_empty() && text.len() > 10 {
                // Rough heuristic for paragraph detection
                content.paragraphs.push(Paragraph::new(text, 0));
            }
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

    /// Extract checkbox status and cleaned text from list item content
    /// Returns (checkbox_status, cleaned_text)
    fn extract_checkbox(text: &str) -> (Option<CheckboxStatus>, String) {
        let trimmed = text.trim();

        // Check for checkbox pattern: [X] where X is a single character
        if trimmed.len() >= 3 && trimmed.starts_with('[') {
            if let Some(close_bracket) = trimmed[1..].find(']') {
                // close_bracket is relative to trimmed[1..], so add 1 for actual position
                let actual_pos = close_bracket + 1;
                if actual_pos == 2 {
                    // Single character checkbox: [X]
                    let checkbox_char = trimmed.chars().nth(1).unwrap();
                    if let Some(status) = CheckboxStatus::from_char(checkbox_char) {
                        // Extract the text after the checkbox (skip "[X] " or "[X]")
                        let remaining = if trimmed.len() > 3 && trimmed.chars().nth(3) == Some(' ')
                        {
                            trimmed[4..].to_string()
                        } else if trimmed.len() > 3 {
                            trimmed[3..].to_string()
                        } else {
                            String::new()
                        };
                        return (Some(status), remaining);
                    }
                }
            }
        }

        (None, text.to_string())
    }

    /// Extract table structure: (rows, columns, headers)
    fn extract_table_structure(node: &Node) -> (usize, usize, Vec<String>) {
        let mut rows = 0;
        let mut columns = 0;
        let mut headers = Vec::new();

        // Walk the table to find headers and count rows
        for child in node.children.iter() {
            // TableHead contains the header row
            if child.cast::<TableHead>().is_some() {
                for row in child.children.iter() {
                    if row.cast::<TableRow>().is_some() {
                        for cell in row.children.iter() {
                            if cell.cast::<TableCell>().is_some() {
                                let header_text = Self::extract_text(cell);
                                headers.push(header_text);
                                columns = columns.max(headers.len());
                            }
                        }
                        rows += 1;
                    }
                }
            }
            // TableBody contains data rows
            else {
                for row in child.children.iter() {
                    if row.cast::<TableRow>().is_some() {
                        let mut row_cols = 0;
                        for cell in row.children.iter() {
                            if cell.cast::<TableCell>().is_some() {
                                row_cols += 1;
                            }
                        }
                        columns = columns.max(row_cols);
                        rows += 1;
                    }
                }
            }
        }

        (rows, columns, headers)
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
    use crucible_core::parser::CheckboxStatus;
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

    #[test]
    fn parse_pending_checkbox() {
        let md = setup_parser();
        let ast = md.parse("- [ ] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::Pending)
        );
    }

    #[test]
    fn parse_done_checkbox() {
        let md = setup_parser();
        let ast = md.parse("- [x] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::Done)
        );
    }

    #[test]
    fn parse_in_progress_checkbox() {
        let md = setup_parser();
        let ast = md.parse("- [/] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::InProgress)
        );
    }

    #[test]
    fn parse_cancelled_checkbox() {
        let md = setup_parser();
        let ast = md.parse("- [-] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::Cancelled)
        );
    }

    #[test]
    fn parse_blocked_checkbox() {
        let md = setup_parser();
        let ast = md.parse("- [!] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::Blocked)
        );
    }

    #[test]
    fn parse_uppercase_x_as_done() {
        let md = setup_parser();
        let ast = md.parse("- [X] task");

        let content = AstConverter::convert(&ast).unwrap();

        assert_eq!(content.lists.len(), 1);
        assert_eq!(content.lists[0].items.len(), 1);
        assert_eq!(
            content.lists[0].items[0].checkbox_status,
            Some(CheckboxStatus::Done)
        );
    }
}
