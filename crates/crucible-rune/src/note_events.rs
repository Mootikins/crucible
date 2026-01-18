//! Note Event Integration
//!
//! This module provides utilities for emitting note events and processing
//! them through the event bus. It's designed to be used by parsers and
//! file watchers to integrate with the unified hook system.
//!
//! ## Event Types
//!
//! - `note:parsed` - Emitted when a note is fully parsed, includes AST structure
//! - `note:created` - Emitted when a new note file is created
//! - `note:modified` - Emitted when note content changes
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::note_events::{NoteEventEmitter, NotePayload};
//!
//! let emitter = NoteEventEmitter::new();
//!
//! // After parsing a note
//! let payload = NotePayload::from_parsed_note(&parsed_note);
//! let (event, ctx, errors) = emitter.emit_parsed("notes/example.md", payload);
//!
//! // When a note is created
//! let (event, ctx, errors) = emitter.emit_created("notes/new.md", frontmatter);
//!
//! // When a note is modified
//! let (event, ctx, errors) = emitter.emit_modified("notes/example.md", changes);
//! ```

#![allow(deprecated)]

use crate::event_bus::{Event, EventBus, EventContext, EventType, HandlerError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

/// Note event emitter for parsers and file watchers
///
/// Wraps an EventBus and provides convenient methods for emitting
/// note:parsed, note:created, and note:modified events.
pub struct NoteEventEmitter {
    bus: EventBus,
}

impl Default for NoteEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl NoteEventEmitter {
    /// Create a new emitter
    pub fn new() -> Self {
        Self {
            bus: EventBus::new(),
        }
    }

    /// Create from an existing EventBus
    pub fn with_bus(bus: EventBus) -> Self {
        Self { bus }
    }

    /// Get mutable reference to the underlying EventBus
    ///
    /// Use this to register custom hooks.
    pub fn bus_mut(&mut self) -> &mut EventBus {
        &mut self.bus
    }

    /// Get reference to the underlying EventBus
    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Emit a note:parsed event
    ///
    /// This event includes the full parsed note structure including:
    /// - Path to the note
    /// - Frontmatter (if present)
    /// - Extracted wikilinks, tags, inline links
    /// - AST blocks (headings, paragraphs, code blocks, etc.)
    /// - Metadata (word count, etc.)
    pub fn emit_parsed(
        &self,
        note_path: impl AsRef<Path>,
        payload: NotePayload,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let path_str = note_path.as_ref().display().to_string();
        let payload_json = serde_json::to_value(&payload).unwrap_or(JsonValue::Null);

        let event = Event::note_parsed(&path_str, payload_json).with_source("kiln");

        debug!("Emitting note:parsed for {}", path_str);
        self.bus.emit(event)
    }

    /// Emit a note:created event
    ///
    /// This event is emitted when a new note file is detected.
    /// Includes basic metadata about the new note.
    pub fn emit_created(
        &self,
        note_path: impl AsRef<Path>,
        metadata: NoteCreatedPayload,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let path_str = note_path.as_ref().display().to_string();
        let payload = serde_json::to_value(&metadata).unwrap_or(JsonValue::Null);

        let event = Event::note_created(&path_str, payload).with_source("kiln");

        debug!("Emitting note:created for {}", path_str);
        self.bus.emit(event)
    }

    /// Emit a note:modified event
    ///
    /// This event is emitted when note content changes.
    /// Includes information about what changed.
    pub fn emit_modified(
        &self,
        note_path: impl AsRef<Path>,
        changes: NoteModifiedPayload,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let path_str = note_path.as_ref().display().to_string();
        let payload = serde_json::to_value(&changes).unwrap_or(JsonValue::Null);

        let event = Event::note_modified(&path_str, payload).with_source("kiln");

        debug!("Emitting note:modified for {}", path_str);
        self.bus.emit(event)
    }

    /// Get count of registered handlers for note events
    pub fn handler_count(&self) -> usize {
        self.bus.count_handlers(EventType::NoteParsed)
            + self.bus.count_handlers(EventType::NoteCreated)
            + self.bus.count_handlers(EventType::NoteModified)
    }
}

/// Payload for note:parsed events
///
/// Contains the full parsed note structure including AST blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotePayload {
    /// Note path (relative to kiln root)
    pub path: String,

    /// Title (from frontmatter or filename)
    pub title: String,

    /// Frontmatter as key-value pairs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<HashMap<String, JsonValue>>,

    /// Tags extracted from content and frontmatter
    #[serde(default)]
    pub tags: Vec<String>,

    /// Wikilinks found in the note
    #[serde(default)]
    pub wikilinks: Vec<WikilinkInfo>,

    /// Inline markdown links
    #[serde(default)]
    pub inline_links: Vec<InlineLinkInfo>,

    /// AST blocks (headings, paragraphs, code blocks, etc.)
    #[serde(default)]
    pub blocks: Vec<BlockInfo>,

    /// Structural metadata
    #[serde(default)]
    pub metadata: NoteMetadata,

    /// Content hash for change detection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// File size in bytes
    #[serde(default)]
    pub file_size: u64,
}

impl NotePayload {
    /// Create a new empty payload
    pub fn new(path: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            title: title.into(),
            frontmatter: None,
            tags: Vec::new(),
            wikilinks: Vec::new(),
            inline_links: Vec::new(),
            blocks: Vec::new(),
            metadata: NoteMetadata::default(),
            content_hash: None,
            file_size: 0,
        }
    }

    /// Set frontmatter
    pub fn with_frontmatter(mut self, frontmatter: HashMap<String, JsonValue>) -> Self {
        self.frontmatter = Some(frontmatter);
        self
    }

    /// Set tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set wikilinks
    pub fn with_wikilinks(mut self, wikilinks: Vec<WikilinkInfo>) -> Self {
        self.wikilinks = wikilinks;
        self
    }

    /// Set inline links
    pub fn with_inline_links(mut self, links: Vec<InlineLinkInfo>) -> Self {
        self.inline_links = links;
        self
    }

    /// Set blocks
    pub fn with_blocks(mut self, blocks: Vec<BlockInfo>) -> Self {
        self.blocks = blocks;
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: NoteMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set content hash
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    /// Set file size
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = size;
        self
    }
}

/// Wikilink information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikilinkInfo {
    /// Target note (without [[ ]])
    pub target: String,
    /// Display text (if different from target)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Section reference (after #)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Line number where the link appears
    #[serde(default)]
    pub line: usize,
}

/// Inline link information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineLinkInfo {
    /// Link text
    pub text: String,
    /// Link URL
    pub url: String,
    /// Title attribute (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Line number where the link appears
    #[serde(default)]
    pub line: usize,
}

/// AST block information
///
/// Represents a discrete content block in the note's AST structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    /// Block type (heading, paragraph, code_block, list, blockquote, etc.)
    pub block_type: BlockType,
    /// Text content of the block (may be truncated for large blocks)
    pub content: String,
    /// Block-specific metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, JsonValue>,
    /// Line number where the block starts
    #[serde(default)]
    pub start_line: usize,
    /// Line number where the block ends
    #[serde(default)]
    pub end_line: usize,
    /// Content hash for the block (for fine-grained change detection)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl BlockInfo {
    /// Create a new block info
    pub fn new(block_type: BlockType, content: impl Into<String>) -> Self {
        Self {
            block_type,
            content: content.into(),
            attributes: HashMap::new(),
            start_line: 0,
            end_line: 0,
            hash: None,
        }
    }

    /// Create a heading block
    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::Heading, text);
        block.attributes.insert("level".to_string(), json!(level));
        block
    }

    /// Create a paragraph block
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(BlockType::Paragraph, text)
    }

    /// Create a code block
    pub fn code_block(language: Option<&str>, code: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::CodeBlock, code);
        if let Some(lang) = language {
            block.attributes.insert("language".to_string(), json!(lang));
        }
        block
    }

    /// Create a list block
    pub fn list(ordered: bool, items: Vec<String>) -> Self {
        let content = items.join("\n");
        let mut block = Self::new(BlockType::List, content);
        block
            .attributes
            .insert("ordered".to_string(), json!(ordered));
        block
            .attributes
            .insert("item_count".to_string(), json!(items.len()));
        block
    }

    /// Create a blockquote
    pub fn blockquote(text: impl Into<String>) -> Self {
        Self::new(BlockType::Blockquote, text)
    }

    /// Create a callout block (Obsidian-style)
    pub fn callout(callout_type: &str, title: Option<&str>, content: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::Callout, content);
        block
            .attributes
            .insert("callout_type".to_string(), json!(callout_type));
        if let Some(t) = title {
            block.attributes.insert("title".to_string(), json!(t));
        }
        block
    }

    /// Set line range
    pub fn with_lines(mut self, start: usize, end: usize) -> Self {
        self.start_line = start;
        self.end_line = end;
        self
    }

    /// Set hash
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    /// Add an attribute
    pub fn with_attr(mut self, key: impl Into<String>, value: JsonValue) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }
}

/// Block types in the AST
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    /// Heading (h1-h6)
    Heading,
    /// Paragraph
    Paragraph,
    /// Fenced code block
    CodeBlock,
    /// Unordered or ordered list
    List,
    /// Block quote
    Blockquote,
    /// Horizontal rule
    HorizontalRule,
    /// Table
    Table,
    /// Obsidian-style callout
    Callout,
    /// LaTeX math block ($$...$$)
    MathBlock,
    /// Footnote definition
    FootnoteDefinition,
    /// HTML block
    HtmlBlock,
    /// Generic/unknown block type
    Other,
}

/// Structural metadata about the note
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// Total word count
    #[serde(default)]
    pub word_count: usize,
    /// Character count (excluding whitespace)
    #[serde(default)]
    pub char_count: usize,
    /// Number of headings
    #[serde(default)]
    pub heading_count: usize,
    /// Number of code blocks
    #[serde(default)]
    pub code_block_count: usize,
    /// Number of lists
    #[serde(default)]
    pub list_count: usize,
    /// Number of paragraphs
    #[serde(default)]
    pub paragraph_count: usize,
    /// Number of callouts
    #[serde(default)]
    pub callout_count: usize,
    /// Number of LaTeX expressions
    #[serde(default)]
    pub latex_count: usize,
    /// Number of footnotes
    #[serde(default)]
    pub footnote_count: usize,
    /// Number of wikilinks
    #[serde(default)]
    pub wikilink_count: usize,
    /// Number of inline links
    #[serde(default)]
    pub inline_link_count: usize,
}

/// Payload for note:created events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteCreatedPayload {
    /// Note path
    pub path: String,
    /// File size in bytes
    #[serde(default)]
    pub file_size: u64,
    /// Initial frontmatter (if parseable quickly)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<HashMap<String, JsonValue>>,
    /// Timestamp when created (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl NoteCreatedPayload {
    /// Create a new payload
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            file_size: 0,
            frontmatter: None,
            created_at: None,
        }
    }

    /// Set file size
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = size;
        self
    }

    /// Set frontmatter
    pub fn with_frontmatter(mut self, frontmatter: HashMap<String, JsonValue>) -> Self {
        self.frontmatter = Some(frontmatter);
        self
    }

    /// Set creation timestamp
    pub fn with_created_at(mut self, timestamp: impl Into<String>) -> Self {
        self.created_at = Some(timestamp.into());
        self
    }
}

/// Payload for note:modified events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteModifiedPayload {
    /// Note path
    pub path: String,
    /// Type of modification
    pub change_type: NoteChangeType,
    /// Previous content hash (if known)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    /// New content hash
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_hash: Option<String>,
    /// Changed blocks (for fine-grained updates)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_blocks: Vec<BlockChange>,
    /// Timestamp of modification (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

impl NoteModifiedPayload {
    /// Create a new payload
    pub fn new(path: impl Into<String>, change_type: NoteChangeType) -> Self {
        Self {
            path: path.into(),
            change_type,
            old_hash: None,
            new_hash: None,
            changed_blocks: Vec::new(),
            modified_at: None,
        }
    }

    /// Set old hash
    pub fn with_old_hash(mut self, hash: impl Into<String>) -> Self {
        self.old_hash = Some(hash.into());
        self
    }

    /// Set new hash
    pub fn with_new_hash(mut self, hash: impl Into<String>) -> Self {
        self.new_hash = Some(hash.into());
        self
    }

    /// Set changed blocks
    pub fn with_changed_blocks(mut self, blocks: Vec<BlockChange>) -> Self {
        self.changed_blocks = blocks;
        self
    }

    /// Set modification timestamp
    pub fn with_modified_at(mut self, timestamp: impl Into<String>) -> Self {
        self.modified_at = Some(timestamp.into());
        self
    }
}

/// Type of note change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteChangeType {
    /// Content was modified
    Content,
    /// Frontmatter was modified
    Frontmatter,
    /// Both content and frontmatter changed
    Both,
    /// Note was renamed/moved
    Renamed,
}

/// Information about a changed block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockChange {
    /// Change operation
    pub operation: BlockChangeOperation,
    /// Block hash (old hash for deleted, new hash for added/modified)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Block type
    pub block_type: BlockType,
    /// Line number
    #[serde(default)]
    pub line: usize,
}

impl BlockChange {
    /// Create a new block change
    pub fn new(operation: BlockChangeOperation, block_type: BlockType) -> Self {
        Self {
            operation,
            hash: None,
            block_type,
            line: 0,
        }
    }

    /// Set hash
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    /// Set line
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = line;
        self
    }
}

/// Block change operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockChangeOperation {
    /// Block was added
    Added,
    /// Block was removed
    Removed,
    /// Block content was modified
    Modified,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::Handler;

    #[test]
    fn test_emitter_creation() {
        let emitter = NoteEventEmitter::new();
        assert_eq!(emitter.handler_count(), 0);
    }

    #[test]
    fn test_emit_parsed() {
        let emitter = NoteEventEmitter::new();

        let payload = NotePayload::new("notes/test.md", "Test Note")
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_content_hash("abc123");

        let (event, _ctx, errors) = emitter.emit_parsed("notes/test.md", payload);

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::NoteParsed);
        assert_eq!(event.identifier, "notes/test.md");
        assert_eq!(event.source, Some("kiln".to_string()));
    }

    #[test]
    fn test_emit_created() {
        let emitter = NoteEventEmitter::new();

        let metadata = NoteCreatedPayload::new("notes/new.md").with_file_size(1024);

        let (event, _ctx, errors) = emitter.emit_created("notes/new.md", metadata);

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::NoteCreated);
        assert_eq!(event.identifier, "notes/new.md");
    }

    #[test]
    fn test_emit_modified() {
        let emitter = NoteEventEmitter::new();

        let changes = NoteModifiedPayload::new("notes/test.md", NoteChangeType::Content)
            .with_old_hash("old123")
            .with_new_hash("new456");

        let (event, _ctx, errors) = emitter.emit_modified("notes/test.md", changes);

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::NoteModified);
        assert_eq!(event.payload["change_type"], json!("content"));
    }

    #[test]
    fn test_hook_can_modify_parsed_payload() {
        let mut emitter = NoteEventEmitter::new();

        // Register a hook that adds processed flag
        emitter.bus_mut().register(Handler::new(
            "enrich_note",
            EventType::NoteParsed,
            "*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("enriched".to_string(), json!(true));
                }
                Ok(event)
            },
        ));

        let payload = NotePayload::new("test.md", "Test");
        let (event, _ctx, _) = emitter.emit_parsed("test.md", payload);

        assert_eq!(event.payload["enriched"], json!(true));
    }

    #[test]
    fn test_hook_pattern_matching() {
        let mut emitter = NoteEventEmitter::new();

        // Register a hook that only matches notes in "daily/" folder
        emitter.bus_mut().register(Handler::new(
            "daily_processor",
            EventType::NoteParsed,
            "daily/*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("is_daily".to_string(), json!(true));
                }
                Ok(event)
            },
        ));

        // Daily note should be processed
        let payload = NotePayload::new("daily/2024-01-15.md", "Daily");
        let (event, _ctx, _) = emitter.emit_parsed("daily/2024-01-15.md", payload);
        assert_eq!(event.payload["is_daily"], json!(true));

        // Non-daily note should not be processed
        let payload = NotePayload::new("projects/rust.md", "Rust");
        let (event, _ctx, _) = emitter.emit_parsed("projects/rust.md", payload);
        assert!(event.payload.get("is_daily").is_none());
    }

    #[test]
    fn test_note_payload_builder() {
        let payload = NotePayload::new("test.md", "Test Note")
            .with_tags(vec!["tag1".to_string()])
            .with_wikilinks(vec![WikilinkInfo {
                target: "other".to_string(),
                display: None,
                section: None,
                line: 5,
            }])
            .with_blocks(vec![
                BlockInfo::heading(1, "Introduction"),
                BlockInfo::paragraph("Some text here."),
            ])
            .with_metadata(NoteMetadata {
                word_count: 100,
                heading_count: 1,
                paragraph_count: 1,
                ..Default::default()
            });

        assert_eq!(payload.tags.len(), 1);
        assert_eq!(payload.wikilinks.len(), 1);
        assert_eq!(payload.blocks.len(), 2);
        assert_eq!(payload.metadata.word_count, 100);
    }

    #[test]
    fn test_block_info_builders() {
        let heading = BlockInfo::heading(2, "Section Title").with_lines(10, 10);
        assert_eq!(heading.block_type, BlockType::Heading);
        assert_eq!(heading.attributes["level"], json!(2));
        assert_eq!(heading.start_line, 10);

        let code = BlockInfo::code_block(Some("rust"), "fn main() {}").with_hash("hash123");
        assert_eq!(code.block_type, BlockType::CodeBlock);
        assert_eq!(code.attributes["language"], json!("rust"));
        assert_eq!(code.hash, Some("hash123".to_string()));

        let list = BlockInfo::list(true, vec!["Item 1".to_string(), "Item 2".to_string()]);
        assert_eq!(list.block_type, BlockType::List);
        assert_eq!(list.attributes["ordered"], json!(true));
        assert_eq!(list.attributes["item_count"], json!(2));

        let callout = BlockInfo::callout("note", Some("Title"), "Content");
        assert_eq!(callout.block_type, BlockType::Callout);
        assert_eq!(callout.attributes["callout_type"], json!("note"));
        assert_eq!(callout.attributes["title"], json!("Title"));
    }

    #[test]
    fn test_block_change() {
        let change = BlockChange::new(BlockChangeOperation::Modified, BlockType::Paragraph)
            .with_hash("abc123")
            .with_line(42);

        assert_eq!(change.operation, BlockChangeOperation::Modified);
        assert_eq!(change.hash, Some("abc123".to_string()));
        assert_eq!(change.line, 42);
    }

    #[test]
    fn test_note_change_types() {
        let content = NoteModifiedPayload::new("test.md", NoteChangeType::Content);
        assert_eq!(content.change_type, NoteChangeType::Content);

        let fm = NoteModifiedPayload::new("test.md", NoteChangeType::Frontmatter);
        assert_eq!(fm.change_type, NoteChangeType::Frontmatter);

        let both = NoteModifiedPayload::new("test.md", NoteChangeType::Both);
        assert_eq!(both.change_type, NoteChangeType::Both);

        let renamed = NoteModifiedPayload::new("test.md", NoteChangeType::Renamed);
        assert_eq!(renamed.change_type, NoteChangeType::Renamed);
    }

    #[test]
    fn test_serialization() {
        let payload = NotePayload::new("test.md", "Test")
            .with_tags(vec!["rust".to_string()])
            .with_blocks(vec![BlockInfo::paragraph("Hello")]);

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: NotePayload = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.path, "test.md");
        assert_eq!(parsed.tags.len(), 1);
        assert_eq!(parsed.blocks.len(), 1);
    }

    #[test]
    fn test_handler_receives_full_payload() {
        let mut emitter = NoteEventEmitter::new();

        // Register a hook that extracts and stores data in context
        emitter.bus_mut().register(Handler::new(
            "data_extractor",
            EventType::NoteParsed,
            "*",
            |ctx, event| {
                // Store extracted data in context
                if let Some(wikilinks) = event.payload.get("wikilinks") {
                    ctx.set(
                        "wikilink_count",
                        json!(wikilinks.as_array().map(|a| a.len()).unwrap_or(0)),
                    );
                }
                if let Some(blocks) = event.payload.get("blocks") {
                    ctx.set(
                        "block_count",
                        json!(blocks.as_array().map(|a| a.len()).unwrap_or(0)),
                    );
                }
                Ok(event)
            },
        ));

        let payload = NotePayload::new("test.md", "Test")
            .with_wikilinks(vec![
                WikilinkInfo {
                    target: "a".to_string(),
                    display: None,
                    section: None,
                    line: 1,
                },
                WikilinkInfo {
                    target: "b".to_string(),
                    display: None,
                    section: None,
                    line: 2,
                },
            ])
            .with_blocks(vec![
                BlockInfo::heading(1, "Title"),
                BlockInfo::paragraph("Content"),
                BlockInfo::paragraph("More content"),
            ]);

        let (_event, ctx, _) = emitter.emit_parsed("test.md", payload);

        assert_eq!(ctx.get("wikilink_count"), Some(&json!(2)));
        assert_eq!(ctx.get("block_count"), Some(&json!(3)));
    }
}
