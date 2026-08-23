//! Parser capabilities.
//!
//! `CrucibleParser` is the one parser. This type describes what it supports.

/// Parser capabilities and configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserCapabilities {
    pub name: &'static str,
    pub version: &'static str,
    pub yaml_frontmatter: bool,
    pub toml_frontmatter: bool,
    pub wikilinks: bool,
    pub tags: bool,
    pub headings: bool,
    pub code_blocks: bool,
    pub tables: bool,
    pub callouts: bool,
    pub latex_expressions: bool,
    pub footnotes: bool,
    pub blockquotes: bool,
    pub horizontal_rules: bool,
    pub full_content: bool,
    pub max_file_size: Option<usize>,
    pub extensions: Vec<&'static str>,
}

impl ParserCapabilities {
    pub fn full() -> Self {
        Self {
            name: "crucible-parser",
            version: env!("CARGO_PKG_VERSION"),
            yaml_frontmatter: true,
            toml_frontmatter: true,
            wikilinks: true,
            tags: true,
            headings: true,
            code_blocks: true,
            tables: true,
            callouts: true,
            latex_expressions: true,
            footnotes: true,
            blockquotes: true,
            horizontal_rules: true,
            full_content: true,
            max_file_size: Some(10 * 1024 * 1024),
            extensions: crate::kiln::KilnFileKind::NOTE_EXTENSIONS.to_vec(),
        }
    }
}

impl Default for ParserCapabilities {
    fn default() -> Self {
        Self::full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The advertised list is not a second predicate: it is the `Note` arm of
    /// `KilnFileKind`, so it cannot drift from `is_note_file`.
    #[test]
    fn advertised_extensions_come_from_the_canonical_note_list() {
        assert_eq!(
            ParserCapabilities::full().extensions,
            crate::kiln::KilnFileKind::NOTE_EXTENSIONS.to_vec()
        );
    }
}
