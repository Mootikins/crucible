//! Block Extractor for converting ParsedNote to ASTBlock format
//!
//! This module provides the BlockExtractor struct which converts the flat
//! content structure of ParsedNote into semantically meaningful AST blocks.
//! Each block represents a natural semantic boundary in the markdown note.

use crate::error::ParseError;
use crate::types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, Callout, CodeBlock, Heading, HorizontalRule,
    LatexExpression, ListBlock, ListType, ParsedNote, Table,
};
use std::collections::HashMap;

/// Tracks heading hierarchy using a tree structure
///
/// The HeadingTree maintains a tree of headings where each heading knows its parent
/// and children. This allows for proper depth calculation regardless of heading level.
///
/// Key insight: Depth is determined by position in the tree, NOT by heading level (H1-H6).
/// Multiple H1s can exist at different depths if they appear in different contexts.
///
/// Example:
/// - H1: Tree root, depth=0
/// - H2: Child of H1, depth=1
/// - H1: New root, depth=0 (not a sibling of first H1)
#[derive(Debug, Clone)]
struct HeadingTree {
    /// All heading nodes indexed by their block_index
    nodes: HashMap<usize, HeadingNode>,
    /// Current path from root to current heading: [(level, block_index), ...]
    /// This represents the "active" branch of the tree
    current_path: Vec<(u8, usize)>,
}

#[derive(Debug, Clone)]
struct HeadingNode {
    #[allow(dead_code)]
    level: u8,
    #[allow(dead_code)]
    block_index: usize,
    #[allow(dead_code)]
    parent_index: Option<usize>,
    children: Vec<usize>,
}

impl HeadingTree {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            current_path: Vec::new(),
        }
    }

    /// Add a new heading to the tree and return its parent index and depth
    ///
    /// This method:
    /// 1. Finds the appropriate parent based on heading level
    /// 2. Calculates the depth based on tree position
    /// 3. Updates the tree structure
    /// 4. Returns (parent_index, depth) for immediate use
    fn add_heading(&mut self, level: u8, block_index: usize) -> (Option<usize>, u32) {
        let parent_idx = self.find_parent_for_level(level);
        let depth = self.calculate_depth_for_level(level);

        // Create the node
        let node = HeadingNode {
            level,
            block_index,
            parent_index: parent_idx,
            children: Vec::new(),
        };

        // Add to parent's children if there is a parent
        if let Some(parent) = parent_idx {
            if let Some(parent_node) = self.nodes.get_mut(&parent) {
                parent_node.children.push(block_index);
            }
        }

        // Store the node
        self.nodes.insert(block_index, node);

        // Update current path
        self.update_path(level, block_index);

        (parent_idx, depth)
    }

    /// Find the appropriate parent for a heading at the given level
    ///
    /// Rules:
    /// - Walk backward through current_path
    /// - Find the first heading with level < new_level
    /// - That heading becomes the parent
    /// - If no such heading exists, this is a root (no parent)
    fn find_parent_for_level(&self, level: u8) -> Option<usize> {
        for (path_level, path_idx) in self.current_path.iter().rev() {
            if *path_level < level {
                return Some(*path_idx);
            }
        }
        None
    }

    /// Calculate the depth for a heading at the given level
    ///
    /// Depth is the number of headings in the path that will be ancestors
    /// of this new heading (i.e., headings with level < new_level)
    fn calculate_depth_for_level(&self, level: u8) -> u32 {
        self.current_path
            .iter()
            .filter(|(path_level, _)| *path_level < level)
            .count() as u32
    }

    /// Update the current path after adding a new heading
    ///
    /// This removes any headings at the same or deeper level,
    /// then adds the new heading to the path
    fn update_path(&mut self, level: u8, block_index: usize) {
        // Remove headings at same or deeper level (higher or equal level numbers)
        self.current_path
            .retain(|(path_level, _)| *path_level < level);
        // Add new heading to path
        self.current_path.push((level, block_index));
    }

    /// Get the current parent heading (if any)
    ///
    /// Returns the last heading in the current path, which is the
    /// parent context for any non-heading blocks
    fn current_parent(&self) -> Option<usize> {
        self.current_path.last().map(|(_, idx)| *idx)
    }

    /// Get the current depth for non-heading blocks
    ///
    /// This is the full length of the current path, since non-heading
    /// blocks are children of the most recent heading
    fn current_depth(&self) -> u32 {
        self.current_path.len() as u32
    }
}

/// Extracts AST blocks from a ParsedNote
///
/// The BlockExtractor is responsible for converting the flat content structure
/// of ParsedNote into semantically meaningful AST blocks. It analyzes the
/// various content elements (headings, paragraphs, code blocks, etc.) and
/// creates appropriate ASTBlock representations with proper positioning
/// information and metadata.
///
/// # Extraction Strategy
///
/// The extractor processes content in note order, creating blocks that
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

    /// Extract AST blocks from a ParsedNote
    ///
    /// This is the main entry point for block extraction. It processes the
    /// note content and creates a vector of AST blocks in note order.
    ///
    /// # Arguments
    ///
    /// * `note` - The ParsedNote to extract blocks from
    ///
    /// # Returns
    ///
    /// A vector of AST blocks in note order, or an error if extraction
    /// encounters invalid data.
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub fn extract_blocks(&self, note: &ParsedNote) -> Result<Vec<ASTBlock>, ParseError> {
        let mut blocks = Vec::new();
        let mut last_end = 0;
        let mut heading_tree = HeadingTree::new();

        // Create a map of all content positions for efficient lookup
        let content_map = self.build_content_map(note);

        // Process content in note order
        let positions = self.get_extraction_positions(&content_map);

        for position in positions {
            // Fill gaps with paragraphs if needed
            if position.start_offset > last_end && self.config.merge_consecutive_paragraphs {
                if let Some(paragraph_blocks) =
                    self.extract_gap_paragraphs(note, last_end, position.start_offset)?
                {
                    // Assign hierarchy to paragraph blocks based on current heading tree
                    for mut para_block in paragraph_blocks {
                        para_block = self.assign_hierarchy(para_block, &heading_tree, blocks.len());
                        if self.config.preserve_empty_blocks || !para_block.is_empty() {
                            blocks.push(para_block);
                        }
                    }
                }
            }

            // Extract the specific block type
            let block = match position.block_type {
                ExtractionType::Heading => self.extract_heading_block(note, &position)?,
                ExtractionType::CodeBlock => self.extract_code_block(note, &position)?,
                ExtractionType::List => self.extract_list_block(note, &position)?,
                ExtractionType::Callout => self.extract_callout_block(note, &position)?,
                ExtractionType::Latex => self.extract_latex_block(note, &position)?,
                ExtractionType::Blockquote => self.extract_blockquote_block(note, &position)?,
                ExtractionType::Table => self.extract_table_block(note, &position)?,
                ExtractionType::HorizontalRule => self.extract_horizontal_rule(note, &position)?,
                ExtractionType::ThematicBreak => self.extract_thematic_break(note, &position)?,
            };

            if let Some(mut block) = block {
                if self.config.preserve_empty_blocks || !block.is_empty() {
                    let block_index = blocks.len();

                    // For headings, calculate hierarchy BEFORE updating tree
                    if let Some(level) = block.heading_level() {
                        let (parent_idx, depth) = heading_tree.add_heading(level, block_index);
                        if let Some(parent) = parent_idx {
                            block.parent_block_id = Some(format!("block_{}", parent));
                        } else {
                            block.parent_block_id = None;
                        }
                        block.depth = Some(depth);
                    } else {
                        // For non-headings, use current tree state
                        block = self.assign_hierarchy(block, &heading_tree, block_index);
                    }

                    blocks.push(block);
                    last_end = position.end_offset;
                }
            }
        }

        // Handle any remaining content as paragraphs
        if last_end < note.content.plain_text.len() {
            if let Some(paragraph_blocks) =
                self.extract_gap_paragraphs(note, last_end, note.content.plain_text.len())?
            {
                for mut para_block in paragraph_blocks {
                    para_block = self.assign_hierarchy(para_block, &heading_tree, blocks.len());
                    if self.config.preserve_empty_blocks || !para_block.is_empty() {
                        blocks.push(para_block);
                    }
                }
            }
        }

        Ok(blocks)
    }

    /// Assign parent_block_id and depth to a block based on the current heading tree
    ///
    /// # Arguments
    ///
    /// * `block` - The block to assign hierarchy to
    /// * `heading_tree` - The current heading tree
    /// * `block_index` - The index this block will have in the blocks vector
    ///
    /// # Returns
    ///
    /// The block with parent_block_id and depth assigned
    fn assign_hierarchy(
        &self,
        mut block: ASTBlock,
        heading_tree: &HeadingTree,
        _block_index: usize,
    ) -> ASTBlock {
        // Get current depth from the tree
        let depth = heading_tree.current_depth();

        // Get parent block index from the tree (if any)
        if let Some(parent_idx) = heading_tree.current_parent() {
            // Generate a block ID for the parent
            // Format: block_{index}
            block.parent_block_id = Some(format!("block_{}", parent_idx));
            block.depth = Some(depth);
        } else {
            // Top-level block (no parent heading)
            block.parent_block_id = None;
            block.depth = Some(0);
        }

        block
    }

    /// Build a map of all content positions for efficient processing
    fn build_content_map(&self, note: &ParsedNote) -> ContentMap {
        let mut map = ContentMap::new();

        // Add headings
        for (index, heading) in note.content.headings.iter().enumerate() {
            map.add_heading(heading.clone(), index);
        }

        // Add code blocks
        for (index, code_block) in note.content.code_blocks.iter().enumerate() {
            map.add_code_block(code_block.clone(), index);
        }

        // Add lists
        for (index, list) in note.content.lists.iter().enumerate() {
            map.add_list(list.clone(), index);
        }

        // Add callouts from both note.content.callouts and note.callouts
        for (index, callout) in note.content.callouts.iter().enumerate() {
            map.add_callout(callout.clone(), index);
        }
        for (index, callout) in note.callouts.iter().enumerate() {
            if !note.content.callouts.contains(callout) {
                map.add_callout(callout.clone(), index + note.content.callouts.len());
            }
        }

        // Add LaTeX expressions
        for (index, latex) in note.content.latex_expressions.iter().enumerate() {
            map.add_latex(latex.clone(), index);
        }
        for (index, latex) in note.latex_expressions.iter().enumerate() {
            if !note.content.latex_expressions.contains(latex) {
                map.add_latex(latex.clone(), index + note.content.latex_expressions.len());
            }
        }

        // Add tables
        for (index, table) in note.content.tables.iter().enumerate() {
            map.add_table(table.clone(), index);
        }

        // Add horizontal rules
        for (index, hr) in note.content.horizontal_rules.iter().enumerate() {
            map.add_horizontal_rule(hr.clone(), index);
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
                end_offset: heading.heading.offset
                    + heading.heading.text.len()
                    + heading.heading.level as usize
                    + 1, // +1 for space
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

        // Add all tables
        for table in &content_map.tables {
            let table_len = table.table.raw_content.len();
            positions.push(ExtractionPosition {
                block_type: ExtractionType::Table,
                start_offset: table.table.offset,
                end_offset: table.table.offset + table_len,
                index: table.index,
            });
        }

        // Add all horizontal rules
        for hr in &content_map.horizontal_rules {
            positions.push(ExtractionPosition {
                block_type: ExtractionType::HorizontalRule,
                start_offset: hr.hr.offset,
                end_offset: hr.hr.offset + hr.hr.length(),
                index: hr.index,
            });
        }

        // Sort by start offset to maintain note order
        positions.sort_by_key(|p| p.start_offset);
        positions
    }

    /// Extract a heading block
    fn extract_heading_block(
        &self,
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let heading = &note.content.headings[position.index];
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
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let code_block = &note.content.code_blocks[position.index];
        let metadata = ASTBlockMetadata::code(code_block.language.clone(), code_block.line_count);

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
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        let list = &note.content.lists[position.index];
        let metadata = ASTBlockMetadata::list(list.list_type, list.item_count);

        // Combine all list items into a single content string
        let content = list
            .items
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
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // Try to get callout from content.callouts first, then from note.callouts
        let callout = if position.index < note.content.callouts.len() {
            &note.content.callouts[position.index]
        } else {
            &note.callouts[position.index - note.content.callouts.len()]
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
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // Try to get LaTeX from content.latex_expressions first, then from note.latex_expressions
        let latex = if position.index < note.content.latex_expressions.len() {
            &note.content.latex_expressions[position.index]
        } else {
            &note.latex_expressions[position.index - note.content.latex_expressions.len()]
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

    /// Extract blockquote blocks
    fn extract_blockquote_block(
        &self,
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        if position.index >= note.content.blockquotes.len() {
            return Ok(None);
        }

        let blockquote = &note.content.blockquotes[position.index];

        let metadata = ASTBlockMetadata::Generic;

        let block = ASTBlock::new(
            ASTBlockType::Blockquote,
            blockquote.content.clone(),
            position.start_offset,
            position.end_offset,
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract table blocks
    fn extract_table_block(
        &self,
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        if position.index >= note.content.tables.len() {
            return Ok(None);
        }

        let table = &note.content.tables[position.index];

        let metadata = ASTBlockMetadata::table(table.rows, table.columns, table.headers.clone());

        let block = ASTBlock::new(
            ASTBlockType::Table,
            table.raw_content.clone(),
            table.offset,
            table.offset + table.raw_content.len(),
            metadata,
        );

        Ok(Some(block))
    }

    /// Extract horizontal rule blocks
    fn extract_horizontal_rule(
        &self,
        note: &ParsedNote,
        position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        if position.index >= note.content.horizontal_rules.len() {
            return Ok(None);
        }

        let hr = &note.content.horizontal_rules[position.index];

        let block = ASTBlock::new(
            ASTBlockType::HorizontalRule,
            hr.raw_content.clone(),
            hr.offset,
            hr.offset + hr.raw_content.len(),
            ASTBlockMetadata::Generic,
        );

        Ok(Some(block))
    }

    /// Extract thematic break blocks (not yet implemented in ParsedNote)
    fn extract_thematic_break(
        &self,
        _document: &ParsedNote,
        _position: &ExtractionPosition,
    ) -> Result<Option<ASTBlock>, ParseError> {
        // TODO: Implement thematic break extraction when available in ParsedNote
        Ok(None)
    }

    /// Extract paragraph blocks from gaps between other content
    fn extract_gap_paragraphs(
        &self,
        note: &ParsedNote,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<Option<Vec<ASTBlock>>, ParseError> {
        if start_offset >= end_offset {
            return Ok(None);
        }

        // Extract the text content for this gap
        let content = if end_offset <= note.content.plain_text.len() {
            &note.content.plain_text[start_offset..end_offset]
        } else {
            &note.content.plain_text[start_offset..]
        };

        let content = content.trim();
        if content.is_empty() || content.len() < self.config.min_paragraph_length {
            return Ok(None);
        }

        // Split into paragraphs on double newlines
        let paragraphs: Vec<&str> = content
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();

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
    tables: Vec<IndexedTable>,
    horizontal_rules: Vec<IndexedHorizontalRule>,
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

#[derive(Debug, Clone)]
struct IndexedTable {
    table: Table,
    index: usize,
}

#[derive(Debug, Clone)]
struct IndexedHorizontalRule {
    hr: HorizontalRule,
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
            tables: Vec::new(),
            horizontal_rules: Vec::new(),
        }
    }

    fn add_heading(&mut self, heading: Heading, index: usize) {
        self.headings.push(IndexedHeading { heading, index });
    }

    fn add_code_block(&mut self, code_block: CodeBlock, index: usize) {
        self.code_blocks
            .push(IndexedCodeBlock { code_block, index });
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

    fn add_table(&mut self, table: Table, index: usize) {
        self.tables.push(IndexedTable { table, index });
    }

    fn add_horizontal_rule(&mut self, hr: HorizontalRule, index: usize) {
        self.horizontal_rules
            .push(IndexedHorizontalRule { hr, index });
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

    fn create_test_document() -> ParsedNote {
        let mut content = NoteContent::new();

        // Add some test content
        content.add_heading(Heading::new(1, "Test Note", 0));
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
        content = content.with_plain_text(
            "This is a test note.\n\nIt has multiple paragraphs.\n\nAnd various content types."
                .to_string(),
        );

        ParsedNote::builder(PathBuf::from("test.md"))
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
        let note = create_test_document();

        let blocks = extractor.extract_blocks(&note).unwrap();

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
        let note = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::Heading,
            start_offset: 0,
            end_offset: 16,
            index: 0,
        };

        let block = extractor
            .extract_heading_block(&note, &position)
            .unwrap()
            .unwrap();

        assert_eq!(block.block_type, ASTBlockType::Heading);
        assert_eq!(block.content, "Test Note");
        assert_eq!(block.start_offset, 0);

        if let ASTBlockMetadata::Heading { level, id } = block.metadata {
            assert_eq!(level, 1);
            assert_eq!(id, Some("test-note".to_string()));
        } else {
            panic!("Expected heading metadata");
        }
    }

    #[test]
    fn test_extract_code_block() {
        let extractor = BlockExtractor::new();
        let note = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::CodeBlock,
            start_offset: 50,
            end_offset: 90,
            index: 0,
        };

        let block = extractor
            .extract_code_block(&note, &position)
            .unwrap()
            .unwrap();

        assert_eq!(block.block_type, ASTBlockType::Code);
        assert!(block.content.contains("fn main"));
        assert_eq!(block.start_offset, 50);

        if let ASTBlockMetadata::Code {
            language,
            line_count,
        } = block.metadata
        {
            assert_eq!(language, Some("rust".to_string()));
            assert_eq!(line_count, 3);
        } else {
            panic!("Expected code metadata");
        }
    }

    #[test]
    fn test_extract_list_block() {
        let extractor = BlockExtractor::new();
        let note = create_test_document();

        let position = ExtractionPosition {
            block_type: ExtractionType::List,
            start_offset: 100,
            end_offset: 130,
            index: 0,
        };

        let block = extractor
            .extract_list_block(&note, &position)
            .unwrap()
            .unwrap();

        assert_eq!(block.block_type, ASTBlockType::List);
        assert!(block.content.contains("First item"));
        assert!(block.content.contains("Second item"));

        if let ASTBlockMetadata::List {
            list_type,
            item_count,
        } = block.metadata
        {
            assert_eq!(list_type, ListType::Unordered);
            assert_eq!(item_count, 2);
        } else {
            panic!("Expected list metadata");
        }
    }

    #[test]
    fn test_content_map_building() {
        let extractor = BlockExtractor::new();
        let note = create_test_document();

        let content_map = extractor.build_content_map(&note);

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
        let note = create_test_document();

        let paragraphs = extractor.extract_gap_paragraphs(&note, 0, 50).unwrap();

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
        let note = create_test_document();

        let paragraphs = extractor.extract_gap_paragraphs(&note, 1000, 1000).unwrap();
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
