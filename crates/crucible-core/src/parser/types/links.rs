//! Link types: wikilinks, tags, inline links, and footnotes

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Wikilink reference [[target|alias]]
///
/// Represents a link to another note in the kiln.
/// Supports both simple [[target]] and aliased [[target|alias]] forms,
/// as well as embeds ![[target]].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wikilink {
    /// Target note name (without .md extension)
    pub target: String,

    /// Optional display alias
    pub alias: Option<String>,

    /// Character offset in source note
    pub offset: usize,

    /// Whether this is an embed (![[note]])
    pub is_embed: bool,

    /// Block reference (#^block-id)
    pub block_ref: Option<String>,

    /// Heading reference (#heading)
    pub heading_ref: Option<String>,
}

impl Wikilink {
    /// Create a simple wikilink
    pub fn new(target: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: None,
            offset,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Create a wikilink with alias
    pub fn with_alias(target: impl Into<String>, alias: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: Some(alias.into()),
            offset,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Create an embed wikilink
    pub fn embed(target: impl Into<String>, offset: usize) -> Self {
        Self {
            target: target.into(),
            alias: None,
            offset,
            is_embed: true,
            block_ref: None,
            heading_ref: None,
        }
    }

    /// Get the display text (alias or target)
    pub fn display(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.target)
    }

    /// Parse a wikilink from raw text (e.g., "Note#heading|Alias")
    pub fn parse(text: &str, offset: usize, is_embed: bool) -> Self {
        let (target_part, alias) = if let Some((t, a)) = text.split_once('|') {
            (t, Some(a.to_string()))
        } else {
            (text, None)
        };

        let (target, heading_ref, block_ref) =
            if let Some((t, ref_part)) = target_part.split_once('#') {
                if let Some(stripped) = ref_part.strip_prefix('^') {
                    (t.to_string(), None, Some(stripped.to_string()))
                } else {
                    (t.to_string(), Some(ref_part.to_string()), None)
                }
            } else {
                (target_part.to_string(), None, None)
            };

        Self {
            target,
            alias,
            offset,
            is_embed,
            block_ref,
            heading_ref,
        }
    }
}

/// Tag reference #tag or #nested/tag
///
/// Represents a tag in the note. Supports nested tags with forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag {
    /// Full tag name (without #)
    pub name: String,

    /// Tag path components (for nested tags)
    pub path: Vec<String>,

    /// Character offset in source note
    pub offset: usize,
}

impl Tag {
    /// Create a new tag
    pub fn new(name: impl Into<String>, offset: usize) -> Self {
        let name = name.into();
        let path = name.split('/').map(|s| s.to_string()).collect();
        Self { name, path, offset }
    }

    /// Get the root tag (first component)
    pub fn root(&self) -> &str {
        self.path.first().map(|s| s.as_str()).unwrap_or(&self.name)
    }

    /// Get the leaf tag (last component)
    pub fn leaf(&self) -> &str {
        self.path.last().map(|s| s.as_str()).unwrap_or(&self.name)
    }

    /// Check if this tag is nested
    pub fn is_nested(&self) -> bool {
        self.path.len() > 1
    }

    /// Get parent tag path
    pub fn parent(&self) -> Option<String> {
        if self.path.len() > 1 {
            Some(self.path[..self.path.len() - 1].join("/"))
        } else {
            None
        }
    }
}

/// Inline markdown link [text](url)
///
/// Represents a standard markdown link (not a wikilink).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineLink {
    /// Link text (displayed to user)
    pub text: String,

    /// URL or relative path
    pub url: String,

    /// Optional link title attribute
    pub title: Option<String>,

    /// Character offset in source note
    pub offset: usize,
}

impl InlineLink {
    /// Create a new inline link
    pub fn new(text: String, url: String, offset: usize) -> Self {
        Self {
            text,
            url,
            title: None,
            offset,
        }
    }

    /// Create an inline link with title
    pub fn with_title(text: String, url: String, title: String, offset: usize) -> Self {
        Self {
            text,
            url,
            title: Some(title),
            offset,
        }
    }

    /// Check if this is an external link (starts with http:// or https://)
    pub fn is_external(&self) -> bool {
        self.url.starts_with("http://") || self.url.starts_with("https://")
    }

    /// Check if this is a relative link (internal to vault)
    pub fn is_relative(&self) -> bool {
        !self.is_external()
    }
}

/// Footnote definitions and references
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FootnoteMap {
    /// Footnote definitions [^1]: content
    pub definitions: HashMap<String, FootnoteDefinition>,

    /// Footnote references in text order
    pub references: Vec<FootnoteReference>,
}

impl FootnoteMap {
    /// Create a new empty footnote map
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a footnote definition
    pub fn add_definition(&mut self, identifier: String, definition: FootnoteDefinition) {
        self.definitions.insert(identifier, definition);
    }

    /// Add a footnote reference
    pub fn add_reference(&mut self, reference: FootnoteReference) {
        self.references.push(reference);
    }

    /// Get a footnote definition by identifier
    pub fn get_definition(&self, identifier: &str) -> Option<&FootnoteDefinition> {
        self.definitions.get(identifier)
    }

    /// Get all orphaned references (no definition found)
    pub fn orphaned_references(&self) -> Vec<&FootnoteReference> {
        self.references
            .iter()
            .filter(|ref_| !self.definitions.contains_key(&ref_.identifier))
            .collect()
    }

    /// Get all unused definitions (no references found)
    pub fn unused_definitions(&self) -> Vec<&String> {
        self.definitions
            .keys()
            .filter(|key| !self.references.iter().any(|ref_| &ref_.identifier == *key))
            .collect()
    }
}

/// Footnote definition [^1]: content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteDefinition {
    /// Footnote identifier (without [^] and :)
    pub identifier: String,

    /// Footnote content
    pub content: String,

    /// Character offset in source note
    pub offset: usize,

    /// Line number in source
    pub line_number: usize,
}

impl FootnoteDefinition {
    /// Create a new footnote definition
    pub fn new(identifier: String, content: String, offset: usize, line_number: usize) -> Self {
        Self {
            identifier,
            content,
            offset,
            line_number,
        }
    }
}

/// Footnote reference `[^1]`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootnoteReference {
    /// Footnote identifier (without [^])
    pub identifier: String,

    /// Character offset in source note
    pub offset: usize,

    /// Reference order number (for sequential numbering)
    pub order_number: Option<usize>,
}

impl FootnoteReference {
    /// Create a new footnote reference
    pub fn new(identifier: String, offset: usize) -> Self {
        Self {
            identifier,
            offset,
            order_number: None,
        }
    }

    /// Create a footnote reference with order number
    pub fn with_order(identifier: String, offset: usize, order_number: usize) -> Self {
        Self {
            identifier,
            offset,
            order_number: Some(order_number),
        }
    }
}
