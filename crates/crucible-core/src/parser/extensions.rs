//! The closed set of syntax extensions, and the registry that runs them.
//!
//! A new syntax is one `Extension` variant. The compiler then lists every
//! `match` that the variant must join.

use super::blockquotes::BlockquoteExtension;
use super::callouts::CalloutExtension;
use super::enhanced_tags::EnhancedTagsExtension;
use super::error::ParseError;
use super::footnotes::FootnoteExtension;
use super::inline_links::InlineLinkExtension;
use super::latex::LatexExtension;
use super::types::NoteContent;
use super::wikilinks::WikilinkExtension;

/// One syntax extension of the markdown parser.
#[derive(Debug, Clone)]
pub enum Extension {
    /// Headings, paragraphs, code blocks and tables from markdown-it.
    #[cfg(feature = "markdown-it-parser")]
    BasicMarkdownIt(super::basic_markdown_it::BasicMarkdownItExtension),
    /// `[[note]]`, `[[note|alias]]`, `![[embed]]`.
    Wikilink(WikilinkExtension),
    /// `[text](url "title")`.
    InlineLink(InlineLinkExtension),
    /// `$inline$` and `$$block$$` math.
    Latex(LatexExtension),
    /// `> [!type] title` callouts.
    Callout(CalloutExtension),
    /// Plain `> text` blockquotes.
    Blockquote(BlockquoteExtension),
    /// `#tags` and `- [ ]` task lists.
    EnhancedTags(EnhancedTagsExtension),
    /// `[^id]` references and definitions.
    Footnote(FootnoteExtension),
}

impl Extension {
    /// The unique name of the extension.
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "markdown-it-parser")]
            Self::BasicMarkdownIt(_) => "basic-markdown-it",
            Self::Wikilink(_) => "obsidian-wikilinks",
            Self::InlineLink(_) => "markdown-inline-links",
            Self::Latex(_) => "latex-math",
            Self::Callout(_) => "obsidian-callouts",
            Self::Blockquote(_) => "markdown-blockquotes",
            Self::EnhancedTags(_) => "enhanced-tags",
            Self::Footnote(_) => "markdown-footnotes",
        }
    }

    /// The run order. The registry runs a higher priority first.
    ///
    /// The basic markdown pass runs first, so that the later passes see the
    /// note structure it produced.
    pub fn priority(&self) -> u8 {
        match self {
            #[cfg(feature = "markdown-it-parser")]
            Self::BasicMarkdownIt(_) => 100,
            Self::Wikilink(_) => 80,
            Self::Latex(_) => 80,
            Self::Footnote(_) => 80,
            Self::InlineLink(_) => 75,
            Self::Callout(_) => 70,
            Self::EnhancedTags(_) => 70,
            Self::Blockquote(_) => 50,
        }
    }

    /// A fast check that tells whether `parse` can find anything in `content`.
    pub fn can_handle(&self, content: &str) -> bool {
        match self {
            #[cfg(feature = "markdown-it-parser")]
            Self::BasicMarkdownIt(ext) => ext.can_handle(content),
            Self::Wikilink(ext) => ext.can_handle(content),
            Self::InlineLink(ext) => ext.can_handle(content),
            Self::Latex(ext) => ext.can_handle(content),
            Self::Callout(ext) => ext.can_handle(content),
            Self::Blockquote(ext) => ext.can_handle(content),
            Self::EnhancedTags(ext) => ext.can_handle(content),
            Self::Footnote(ext) => ext.can_handle(content),
        }
    }

    /// Parse `content` and add what the extension recognizes to `doc_content`.
    ///
    /// Returns the parse errors. The errors are not fatal.
    pub fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        match self {
            #[cfg(feature = "markdown-it-parser")]
            Self::BasicMarkdownIt(ext) => ext.parse(content, doc_content),
            Self::Wikilink(ext) => ext.parse(content, doc_content),
            Self::InlineLink(ext) => ext.parse(content, doc_content),
            Self::Latex(ext) => ext.parse(content, doc_content),
            Self::Callout(ext) => ext.parse(content, doc_content),
            Self::Blockquote(ext) => ext.parse(content, doc_content),
            Self::EnhancedTags(ext) => ext.parse(content, doc_content),
            Self::Footnote(ext) => ext.parse(content, doc_content),
        }
    }
}

/// The ordered set of extensions that a parser runs.
#[derive(Debug, Clone, Default)]
pub struct ExtensionRegistry {
    /// Sorted by priority, highest first.
    extensions: Vec<Extension>,
}

impl ExtensionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with every extension that the crate compiles.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        let defaults = [
            #[cfg(feature = "markdown-it-parser")]
            Extension::BasicMarkdownIt(super::basic_markdown_it::BasicMarkdownItExtension::new()),
            Extension::Wikilink(WikilinkExtension::new()),
            Extension::InlineLink(InlineLinkExtension::new()),
            Extension::Latex(LatexExtension::new()),
            Extension::Callout(CalloutExtension::new()),
            Extension::Blockquote(BlockquoteExtension::new()),
            Extension::EnhancedTags(EnhancedTagsExtension::new()),
            Extension::Footnote(FootnoteExtension::new()),
        ];
        for extension in defaults {
            registry
                .register(extension)
                .expect("the default set has no duplicate name");
        }
        registry
    }

    /// Register an extension.
    ///
    /// Returns an error when an extension with the same name is registered.
    pub fn register(&mut self, extension: Extension) -> Result<(), String> {
        let name = extension.name();
        if self.extensions.iter().any(|e| e.name() == name) {
            return Err(format!("Extension '{}' already registered", name));
        }
        self.extensions.push(extension);
        self.extensions
            .sort_by_key(|e| std::cmp::Reverse(e.priority()));
        Ok(())
    }

    /// The registered extensions, highest priority first.
    pub fn extensions(&self) -> &[Extension] {
        &self.extensions
    }

    /// Run every extension that can handle `content`, in priority order.
    ///
    /// Returns the parse errors of all the extensions.
    pub fn apply(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        self.extensions
            .iter()
            .filter(|ext| ext.can_handle(content))
            .flat_map(|ext| ext.parse(content, doc_content))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_rejects_a_duplicate_name() {
        let mut registry = ExtensionRegistry::new();
        assert!(registry
            .register(Extension::Latex(LatexExtension::new()))
            .is_ok());
        assert!(registry
            .register(Extension::Latex(LatexExtension::new()))
            .is_err());
    }

    #[test]
    fn register_keeps_the_highest_priority_first() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Extension::Blockquote(BlockquoteExtension::new()))
            .unwrap();
        registry
            .register(Extension::Wikilink(WikilinkExtension::new()))
            .unwrap();
        registry
            .register(Extension::Callout(CalloutExtension::new()))
            .unwrap();

        let names: Vec<_> = registry.extensions().iter().map(|e| e.name()).collect();
        assert_eq!(
            names,
            [
                "obsidian-wikilinks",
                "obsidian-callouts",
                "markdown-blockquotes"
            ]
        );
    }

    #[test]
    fn apply_runs_only_the_extensions_that_can_handle_the_content() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Extension::Wikilink(WikilinkExtension::new()))
            .unwrap();
        registry
            .register(Extension::Callout(CalloutExtension::new()))
            .unwrap();

        let mut doc_content = NoteContent::new();
        let errors = registry.apply("See [[Other]].", &mut doc_content);
        assert!(errors.is_empty());
        assert_eq!(doc_content.wikilinks.len(), 1);
        assert!(doc_content.callouts.is_empty());
    }

    #[test]
    fn with_defaults_registers_every_variant() {
        let registry = ExtensionRegistry::with_defaults();
        let expected = if cfg!(feature = "markdown-it-parser") {
            8
        } else {
            7
        };
        assert_eq!(registry.extensions().len(), expected);
    }
}
