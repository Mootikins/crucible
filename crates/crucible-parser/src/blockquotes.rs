//! Regular blockquote syntax extension
//!
//! This module implements support for regular markdown blockquotes
//! that are NOT Obsidian callouts (i.e., `> text` but not `> [!type]`).

use super::error::ParseError;
use super::extensions::SyntaxExtension;
use super::types::{Blockquote, NoteContent};
use async_trait::async_trait;
use regex::Regex;

/// Regular blockquote syntax extension
pub struct BlockquoteExtension;

impl BlockquoteExtension {
    /// Create a new blockquote extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for BlockquoteExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for BlockquoteExtension {
    fn name(&self) -> &'static str {
        "markdown-blockquotes"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports regular markdown blockquotes (> text) excluding Obsidian callouts"
    }

    fn can_handle(&self, content: &str) -> bool {
        content.contains('>')
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let errors = Vec::new();

        // Pattern to match blockquote lines
        let re = Regex::new(r"(?m)^(>+)\s*(.*)$")
            .expect("Blockquote regex is a compile-time constant and should never fail to compile");

        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        let mut offset = 0;
        let mut in_callout = false; // Track if we're inside a callout block

        while i < lines.len() {
            let line = lines[i];

            if let Some(cap) = re.captures(line) {
                let prefix = cap.get(1).unwrap().as_str();
                let text = cap.get(2).unwrap().as_str();

                // Check if this is the start of a callout
                if text.trim_start().starts_with("[!") {
                    in_callout = true;
                    offset += line.len() + 1;
                    i += 1;
                    continue;
                }

                // If we're in a callout, skip this line (it's a continuation)
                if in_callout {
                    offset += line.len() + 1;
                    i += 1;
                    continue;
                }

                // Count nesting level (number of > characters)
                let nested_level = prefix.chars().filter(|&c| c == '>').count() as u8;

                // Calculate total capacity needed for the blockquote content
                // by looking ahead at consecutive lines with the same nesting level.
                // We add +1 for each potential space separator between non-empty lines.
                let mut total_capacity = text.len();
                let mut lookahead = i + 1;
                while lookahead < lines.len() {
                    if let Some(next_cap) = re.captures(lines[lookahead]) {
                        let next_prefix = next_cap.get(1).unwrap().as_str();
                        let next_level = next_prefix.chars().filter(|&c| c == '>').count() as u8;

                        if next_level == nested_level {
                            let next_text = next_cap.get(2).unwrap().as_str();
                            // Add text length + 1 for potential space separator
                            total_capacity += next_text.len() + 1;
                            lookahead += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                // Collect all consecutive blockquote lines with same nesting level
                let mut full_content = String::with_capacity(total_capacity);
                full_content.push_str(text);
                let start_offset = offset;
                i += 1;
                offset += line.len() + 1;

                while i < lines.len() {
                    if let Some(next_cap) = re.captures(lines[i]) {
                        let next_prefix = next_cap.get(1).unwrap().as_str();
                        let next_level = next_prefix.chars().filter(|&c| c == '>').count() as u8;

                        // Same nesting level, continue
                        if next_level == nested_level {
                            let next_text = next_cap.get(2).unwrap().as_str();
                            if !full_content.is_empty() && !next_text.is_empty() {
                                full_content.push(' ');
                            }
                            full_content.push_str(next_text);
                            offset += lines[i].len() + 1;
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                // Only add non-empty blockquotes
                if !full_content.trim().is_empty() {
                    let blockquote = Blockquote::with_nesting(
                        full_content.trim().to_string(),
                        nested_level.saturating_sub(1), // 0-indexed: > = 0, >> = 1, etc.
                        start_offset,
                    );
                    doc_content.blockquotes.push(blockquote);
                }
            } else {
                // Not a blockquote line, reset callout tracking
                in_callout = false;
                offset += line.len() + 1;
                i += 1;
            }
        }

        errors
    }

    fn priority(&self) -> u8 {
        60 // Lower than callouts to allow callouts to be processed first
    }
}

impl BlockquoteExtension {
    /// Check if this extension supports blockquotes (convenience method for tests)
    pub fn supports_blockquotes(&self) -> bool {
        true
    }
}

/// Create a blockquote extension instance
pub fn create_blockquote_extension() -> std::sync::Arc<dyn SyntaxExtension> {
    std::sync::Arc::new(BlockquoteExtension::new())
}
