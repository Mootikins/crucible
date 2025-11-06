//! Block Extractor for converting ParsedDocument to ASTBlock format
//!
//! This module provides the BlockExtractor struct which converts the flat
//! content structure of ParsedDocument into semantically meaningful AST blocks.
//! Each block represents a natural semantic boundary in the markdown document.

use crate::error::ParseError;
use crate::types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, Callout, CodeBlock, Heading, LatexExpression,
    ListBlock, ListType, ParsedDocument,
};

/// Extracts AST blocks from a ParsedDocument
///
/// The BlockExtractor is responsible for converting the flat content structure
/// of ParsedDocument into semantically meaningful AST blocks. It analyzes the
/// various content elements (headings, paragraphs, code blocks, etc.) and
/// creates appropriate ASTBlock representations with proper positioning
/// information and metadata.
///
/// # Extraction Strategy
///
/// The extractor processes content in document order, creating blocks that
/// align with natural semantic boundaries:
/// - Headings become separate blocks
/// - Code blocks become separate blocks
/// - Lists become separate blocks
/// - Callouts become separate blocks
/// - LaTeX expressions become separate blocks
/// - Paragraphs fill the gaps between other block types
///
/// This approach ensures that blocks correspond to user mental models
/// and HTML rendering boundaries.
#[derive(Debug, Clone)]
pub struct BlockExtractor {
    /// Configuration options for extraction
    config: ExtractionConfig,
}

/// Configuration for block extraction
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Minimum paragraph length to create a separate block
    pub min_paragraph_length: usize,
    /// Whether to preserve empty blocks
    pub preserve_empty_blocks: bool,
    /// Whether to merge consecutive paragraphs
    pub merge_consecutive_paragraphs: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            min_paragraph_length: 10,
            preserve_empty_blocks: false,
            merge_consecutive_paragraphs: false,
        }
    }
}

impl Default for BlockExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockExtractor {
    /// Create a new BlockExtractor with default configuration
    pub fn new() -> Self {
        Self {
            config: ExtractionConfig::default(),
        }
    }

    /// Create a BlockExtractor with custom configuration
    pub fn with_config(config: ExtractionConfig) -> Self {
        Self { config }
    }

    /// Extract AST blocks from a ParsedDocument
    ///
    /// This is the main entry point for block extraction. It processes the
    /// document content and creates a vector of AST blocks in document order.
    ///
    /// # Arguments
    ///
    /// * `document` - The ParsedDocument to extract blocks from
    ///
    /// # Returns
    ///
    /// A vector of AST blocks in document order, or an error if extraction
    /// encounters invalid data.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_parser::BlockExtractor;
    /// use crucible_parser::types::ParsedDocument;
    /// use std::path::PathBuf;
    ///
    /// let extractor = BlockExtractor::new();
    /// let document = ParsedDocument::new(PathBuf::from("test.md"));
    /// let blocks = extractor.extract_blocks(&document)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn extract_blocks(&self, document: &ParsedDocument) -> Result<Vec<ASTBlock>, ParseError> {
        let mut blocks = Vec::new();
        let mut last_end = 0;

        // Create a map of all content positions for efficient lookup
        let content_map = self.build_content_map(document);

        // Process content in document order
        let positions = self.get_extraction_positions(&content_map);

        for position in positions {
            // Fill gaps with paragraphs if needed
            if position.start_offset > last_end && self.config.merge_consecutive_paragraphs {
                if let Some(paragraph_blocks) = self.extract_gap_paragraphs(
                    document,
                    last_end,
                    position.start_offset,
                )? {
                    blocks.extend(paragraph_blocks);
                }
            }

            // Extract the specific block type
            let block = match position.block_type {
                ExtractionType::Heading => self.extract_heading_block(document, &position)?,
                ExtractionType::CodeBlock => self.extract_code_block(document, &position)?,
                ExtractionType::List => self.extract_list_block(document, &position)?,
                ExtractionType::Callout => self.extract_callout_block(document, &position)?,
                ExtractionType::Latex => self.extract_latex_block(document, &position)?,
                ExtractionType::Blockquote => self.extract_blockquote_block(document, &position)?,
                ExtractionType::Table => self.extract_table_block(document, &position)?,
                ExtractionType::HorizontalRule => self.extract_horizontal_rule(document, &position)?,
                ExtractionType::ThematicBreak => self.extract_thematic_break(document, &position)?,
            };

            if let Some(block) = block {
                if self.config.preserve_empty_blocks || !block.is_empty() {
                    blocks.push(block);
                    last_end = position.end_offset;
                }
            }
        }

        // Handle any remaining content as paragraphs
        if last_end < document.content.plain_text.len() {
            if let Some(paragraph_blocks) = self.extract_gap_paragraphs(
                document,
                last_end,
                document.content.plain_text.len(),
            )? {
                blocks.extend(paragraph_blocks);
            }
        }

        Ok(blocks)
    }

    /// Build a map of all content positions for efficient processing
    fn build_content_map(&self, document: &ParsedDocument) -> ContentMap {
        let mut map = ContentMap::new();

        // Add headings
        for (index, heading) in document.content.headings.iter().enumerate() {
            map.add_heading(heading.clone(), index);
        }

        // Add code blocks
        for (index, code_block) in document.content.code_blocks.iter().enumerate() {
            map.add_code_block(code_block.clone(), index);
        }

        // Add lists
        for (index, list) in document.content.lists.iter().enumerate() {
            map.add_list(list.clone(), index);
        }

        // Add callouts from both document.content.callouts and document.callouts
        for (index, callout) in document.content.callouts.iter().enumerate() {
            map.add_callout(callout.clone(), index);
        }
        for (index, callout) in document.callouts.iter().enumerate() {
            if !document.content.callouts.contains(callout) {
                map.add_callout(callout.clone(), index + document.content.callouts.len());
            }
        }

        // Add LaTeX expressions
        for (index, latex) in document.content.latex_expressions.iter().enumerate() {
            map.add_latex(latex.clone(), index);
        }
        for (index, latex) in document.latex_expressions.iter().enumerate() {
            if !document.content.latex_expressions.contains(latex) {
                map.add_latex(latex.clone(), index + document.content.latex_expressions.len());
            }
        }

        map
    }

    /// Get sorted positions for extraction processing
    fn get_extraction_positions(&self, content_map: &ContentMap) -> Vec<ExtractionPosition> {
        let mut positions = Vec::new();

        // Add all headings
        for heading in &content_map.headings {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::Heading,
                start_offset: heading.heading.offset,
                end_offset: heading.heading.offset + heading.heading.text.len() + heading.heading.level as usize + 1, // +1 for space
                index: heading.index,
            });
        }

        // Add all code blocks
        for code_block in &content_map.code_blocks {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::CodeBlock,
                start_offset: code_block.code_block.offset,
                end_offset: code_block.code_block.offset + code_block.code_block.content.len() + 6, // Approximate ``` markers
                index: code_block.index,
            });
        }

        // Add all lists
        for list in &content_map.lists {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::List,
                start_offset: list.list.offset,
                end_offset: list.list.offset + self.estimate_list_length(&list.list),
                index: list.index,
            });
        }

        // Add all callouts
        for callout in &content_map.callouts {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::Callout,
                start_offset: callout.callout.offset,
                end_offset: callout.callout.offset + callout.callout.length(),
                index: callout.index,
            });
        }

        // Add all LaTeX expressions
        for latex in &content_map.latex_expressions {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::Latex,
                start_offset: latex.latex.offset,
                end_offset: latex.latex.offset + latex.latex.length,
                index: latex.index,
            });
        }

        // Sort by start offset to maintain document order
        positions.sort_by_key(|p| p.start_offset);
        positions
    }

    /// Extract a heading block
    fn extract_heading_block(
        &self,
        document: &ParsedDocument,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let heading = &document.content.headings[position.index];
        let metadata = ASTBlockMetadata::heading(heading.level, heading.id.clone());

        let block = ASTBlock::new(
            ASTBlockType::Heading,
            heading.text.clone(),
            heading.offset,
            heading.offset + heading.text.len() + heading.level as usize + 1,
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract a code block
    fn extract_code_block(
        &self,
        document: &ParsedDocument,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let code_block = &document.content.code_blocks[position.index];
        let metadata = ASTBlockMetadata::code(
            code_block.language.clone(),
            code_block.line_count,
        );

        let block = ASTBlock::new(
            ASTBlockType::Code,
            code_block.content.clone(),
            code_block.offset,
            code_block.offset + code_block.content.len() + 6, // Approximate ``` markers
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract a list block
    fn extract_list_block(
        &self,
        document: &ParsedDocument,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let list = &document.content.lists[position.index];
        let metadata = ASTBlockMetadata::list(list.list_type, list.item_count);

        // Combine all list items into a single content string
        let content = list.items
            .iter()
            .map(|item| {
                let prefix = match list.list_type {
                    ListType::Ordered => format!("{}. ", item.level + 1),
                    ListType::Unordered => "- ".repeat(item.level + 1),
                };
                format!("{}{}", prefix, item.content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let end_offset = list.offset + content.len();

        let block = ASTBlock::new(
            ASTBlockType::List,
            content,
            list.offset,
            end_offset,
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract a callout block
    fn extract_callout_block(
        &self,
        document: &ParsedDocument,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // Try to get callout from content.callouts first, then from document.callouts
        let callout = if position.index < document.content.callouts.len() {
            &document.content.callouts[position.index]
        } else {
            &document.callouts[position.index - document.content.callouts.len()]
        };

        let metadata = ASTBlockMetadata::callout(
            callout.callout_type.clone(),
            callout.title.clone(),
            callout.is_standard_type,
        );

        let block = ASTBlock::new(
            ASTBlockType::Callout,
            callout.content.clone(),
            callout.offset,
            callout.offset + callout.length(),
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract a LaTeX block
    fn extract_latex_block(
        &self,
        document: &ParsedDocument,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // Try to get LaTeX from content.latex_expressions first, then from document.latex_expressions
        let latex = if position.index < document.content.latex_expressions.len() {
            &document.content.latex_expressions[position.index]
        } else {
            &document.latex_expressions[position.index - document.content.latex_expressions.len()]
        };

        let metadata = ASTBlockMetadata::latex(latex.is_block);

        let block = ASTBlock::new(
            ASTBlockType::Latex,
            latex.expression.clone(),
            latex.offset,
            latex.offset + latex.length,
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract blockquote blocks (not yet implemented in ParsedDocument)
    fn extract_blockquote_block(
        &self,
        _document: &ParsedDocument,
        _position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // TODO: Implement blockquote extraction when available in ParsedDocument
        Ok(None)
    }

    /// Extract table blocks (not yet implemented in ParsedDocument)
    fn extract_table_block(
        &self,
        _document: &ParsedDocument,
        _position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // TODO: Implement table extraction when available in ParsedDocument
        Ok(None)
    }

    /// Extract horizontal rule blocks (not yet implemented in ParsedDocument)
    fn extract_horizontal_rule(
        &self,
        _document: &ParsedDocument,
        _position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // TODO: Implement horizontal rule extraction when available in ParsedDocument
        Ok(None)
    }

    /// Extract thematic break blocks (not yet implemented in ParsedDocument)
    fn extract_thematic_break(
        &self,
        _document: &ParsedDocument,
        _position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // TODO: Implement thematic break extraction when available in ParsedDocument
        Ok(None)
    }

    /// Extract paragraph blocks from gaps between other content
    fn extract_gap_paragraphs(
        &self,
        document: &ParsedDocument,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<Option<Vec<ASTBlock>>, ParseError> {
        if start_offset >= end_offset {
            return Ok(None);
        }

        // Extract the text content for this gap
        let content = if end_offset <= document.content.plain_text.len() {
            &document.content.plain_text[start_offset..end_offset]
        } else {
            &document.content.plain_text[start_offset..]
        };

        let content = content.trim();
        if content.is_empty() || content.len() < self.config.min_paragraph_length {
            return Ok(None);
        }

        // Split into paragraphs on double newlines
        let paragraphs: Vec<&str> = content.split("\n\n").filter(|p| !p.trim().is_empty()).collect();

        if paragraphs.is_empty() {
            return Ok(None);
        }

        let mut blocks = Vec::new();
        let mut current_offset = start_offset;

        for paragraph in paragraphs {
            let paragraph = paragraph.trim();
            if !paragraph.is_empty() {
                let metadata = ASTBlockMetadata::generic();
                let block = ASTBlock::new(
                    ASTBlockType::Paragraph,
                    paragraph.to_string(),
                    current_offset,
                    current_offset + paragraph.len(),
                    metadata,
                );
                blocks.push(block);
                current_offset += paragraph.len() + 2; // +2 for \n\n
            }
        }

        if blocks.is_empty() {
            Ok(None)
        } else {
            Ok(Some(blocks))
        }
    }

    /// Estimate the length of a list in characters
    fn estimate_list_length(&self, list: &ListBlock) -> usize {
        list.items
            .iter()
            .map(|item| {
                let prefix_len = match list.list_type {
                    ListType::Ordered => format!("{}. ", item.level + 1).len(),
                    ListType::Unordered => item.level + 1, // Number of '-' characters + spaces
                };
                prefix_len + item.content.len() + 1 // +1 for newline
            })
            .sum()
    }
}

/// Internal map of content positions for efficient processing
#[derive(Debug, Clone)]
struct ContentMap {
    headings: Vec<IndexedHeading>,
    code_blocks: Vec<IndexedCodeBlock>,
    lists: Vec<IndexedList>,
    callouts: Vec<IndexedCallout>,
    latex_expressions: Vec<IndexedLatex>,
}

#[derive(Debug, Clone)]
struct IndexedHeading {
    heading: Heading,
    index: usize,
}

#[derive(Debug, Clone)]
struct IndexedCodeBlock {
    code_block: CodeBlock,
    index: usize,
}

#[derive(Debug, Clone)]
struct IndexedList {
    list: ListBlock,
    index: usize,
}

#[derive(Debug, Clone)]
struct IndexedCallout {
    callout: Callout,
    index: usize,
}

#[derive(Debug, Clone)]
struct IndexedLatex {
    latex: LatexExpression,
    index: usize,
}

impl ContentMap {
    fn new() -> Self {
        Self {
            headings: Vec::new(),
            code_blocks: Vec::new(),
            lists: Vec::new(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
        }
    }

    fn add_heading(&mut self, heading: Heading, index: usize) {
        self.headings.push(IndexedHeading { heading, index });
    }

    fn add_code_block(&mut self, code_block: CodeBlock, index: usize) {
        self.code_blocks.push(IndexedCodeBlock { code_block, index });
    }

    fn add_list(&mut self, list: ListBlock, index: usize) {
        self.lists.push(IndexedList { list, index });
    }

    fn add_callout(&mut self, callout: Callout, index: usize) {
        self.callouts.push(IndexedCallout { callout, index });
    }

    fn add_latex(&mut self, latex: LatexExpression, index: usize) {
        self.latex_expressions.push(IndexedLatex { latex, index });
    }
}

/// Types of content that can be extracted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Future extensions planned
enum ExtractionType {
    Heading,
    CodeBlock,
    List,
    Callout,
    Latex,
    Blockquote,
    Table,
    HorizontalRule,
    ThematicBreak,
}

/// Position information for extraction
#[derive(Debug, Clone)]
struct ExtractionPosition {
    block_type: ExtractionType,
    start_offset: usize,
    end_offset: usize,
    index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn create_test_document() -> ParsedDocument {
        let mut content = DocumentContent::new();

        // Add some test content
        content.add_heading(Heading::new(1, "Test Document", 0));
        content.add_heading(Heading::new(2, "Section 1", 20));

        content.add_code_block(CodeBlock::new(
            Some("rust".to_string()),
            "fn main() {\n    println!(\"Hello, world!\");\n}".to_string(),
            50,
        ));

        let mut list = ListBlock::new(ListType::Unordered, 100);
        list.add_item(ListItem::new("First item".to_string(), 0));
        list.add_item(ListItem::new("Second item".to_string(), 0));
        content.lists.push(list);

        // Add plain text content
        content = content.with_plain_text("This is a test document.\n\nIt has multiple paragraphs.\n\nAnd various content types.".to_string());

        ParsedDocument::builder(PathBuf::from("test.md"))
            .with_content(content)
            .with_parsed_at(Utc::now())
            .with_content_hash("test_hash".to_string())
            .with_file_size(1024)
            .build()
    }

    #[test]
    fn test_block_extractor_creation() {
        let extractor = BlockExtractor::new();
        assert!(extractor.config.min_paragraph_length > 0);
        assert!(!extractor.config.preserve_empty_blocks);
        assert!(!extractor.config.merge_consecutive_paragraphs);
    }

    #[test]
    fn test_block_extractor_with_config() {
        let config = ExtractionConfig {
            min_paragraph_length: 20,
            preserve_empty_blocks: true,
            merge_consecutive_paragraphs: true,
        };

        let extractor = BlockExtractor::with_config(config);
        assert_eq!(extractor.config.min_paragraph_length, 20);
        assert!(extractor.config.preserve_empty_blocks);
        assert!(extractor.config.merge_consecutive_paragraphs);
    }

    #[test]
    fn test_extract_blocks_from_document() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let blocks = extractor.extract_blocks(&document).unwrap();

        // Should have extracted heading, code block, and list blocks
        assert!(!blocks.is_empty());

        // Check that we have the expected block types
        let block_types: Vec<_> = blocks.iter().map(|b| b.block_type).collect();
        assert!(block_types.contains(&ASTBlockType::Heading));
        assert!(block_types.contains(&ASTBlockType::Code));
        assert!(block_types.contains(&ASTBlockType::List));
    }

    #[test]
    fn test_extract_heading_block() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::Heading,
            start_offset: 0,
            end_offset: 16,
            index: 0,
        };

        let block = extractor.extract_heading_block(&document, &position).unwrap().unwrap();

        assert_eq!(block.block_type, ASTBlockType::Heading);
        assert_eq!(block.content, "Test Document");
        assert_eq!(block.start_offset, 0);

        if let ASTBlockMetadata::Heading { level, id } = block.metadata {
            assert_eq!(level, 1);
            assert_eq!(id, Some("test-document".to_string()));
        } else {
            panic!("Expected heading metadata");
        }
    }

    #[test]
    fn test_extract_code_block() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::CodeBlock,
            start_offset: 50,
            end_offset: 90,
            index: 0,
        };

        let block = extractor.extract_code_block(&document, &position).unwrap().unwrap();

        assert_eq!(block.block_type, ASTBlockType::Code);
        assert!(block.content.contains("fn main"));
        assert_eq!(block.start_offset, 50);

        if let ASTBlockMetadata::Code { language, line_count } = block.metadata {
            assert_eq!(language, Some("rust".to_string()));
            assert_eq!(line_count, 3);
        } else {
            panic!("Expected code metadata");
        }
    }

    #[test]
    fn test_extract_list_block() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::List,
            start_offset: 100,
            end_offset: 130,
            index: 0,
        };

        let block = extractor.extract_list_block(&document, &position).unwrap().unwrap();

        assert_eq!(block.block_type, ASTBlockType::List);
        assert!(block.content.contains("First item"));
        assert!(block.content.contains("Second item"));

        if let ASTBlockMetadata::List { list_type, item_count } = block.metadata {
            assert_eq!(list_type, ListType::Unordered);
            assert_eq!(item_count, 2);
        } else {
            panic!("Expected list metadata");
        }
    }

    #[test]
    fn test_content_map_building() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let content_map = extractor.build_content_map(&document);

        assert_eq!(content_map.headings.len(), 2);
        assert_eq!(content_map.code_blocks.len(), 1);
        assert_eq!(content_map.lists.len(), 1);
    }

    #[test]
    fn test_extraction_positions_sorting() {
        let extractor = BlockExtractor::new();
        let mut content_map = ContentMap::new();

        // Add content out of order
        content_map.add_heading(Heading::new(2, "Second", 100), 1);
        content_map.add_heading(Heading::new(1, "First", 0), 0);

        let positions = extractor.get_extraction_positions(&content_map);

        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].start_offset, 0);
        assert_eq!(positions[1].start_offset, 100);
    }

    #[test]
    fn test_extract_gap_paragraphs() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let paragraphs = extractor.extract_gap_paragraphs(&document, 0, 50).unwrap();

        // Should extract some paragraph content from the gap
        assert!(paragraphs.is_some());
        let paragraphs = paragraphs.unwrap();
        assert!(!paragraphs.is_empty());

        // Check that blocks are properly formed
        for block in paragraphs {
            assert_eq!(block.block_type, ASTBlockType::Paragraph);
            assert!(!block.content.is_empty());
        }
    }

    #[test]
    fn test_empty_gap_handling() {
        let extractor = BlockExtractor::new();
        let document = create_test_document();

        let paragraphs = extractor.extract_gap_paragraphs(&document, 1000, 1000).unwrap();
        assert!(paragraphs.is_none());
    }

    #[test]
    fn test_estimate_list_length() {
        let extractor = BlockExtractor::new();
        let mut list = ListBlock::new(ListType::Unordered, 0);
        list.add_item(ListItem::new("Item 1".to_string(), 0));
        list.add_item(ListItem::new("Item 2".to_string(), 0));

        let length = extractor.estimate_list_length(&list);
        assert!(length > 0);
        assert!(length > "Item 1".len() + "Item 2".len()); // Should include formatting
    }
}