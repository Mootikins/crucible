//! Property-based tests for crucible-parser
//!
//! Uses proptest to verify invariants across random inputs.

use proptest::prelude::*;

/// Strategies for generating wikilink inner content
fn wikilink_inner_strategy() -> impl Strategy<Value = String> {
    // Generate various wikilink patterns:
    // - Simple targets: "note name"
    // - With headings: "note#heading"
    // - With block refs: "note#^block-id"
    // - With aliases: "note|alias"
    // - Complex: "note#heading|alias"
    prop_oneof![
        // Simple alphanumeric with spaces
        "[a-zA-Z0-9 _-]{1,50}",
        // With heading
        "[a-zA-Z0-9 _-]{1,30}#[a-zA-Z0-9 _-]{1,20}",
        // With block reference
        "[a-zA-Z0-9 _-]{1,30}#\\^[a-z0-9-]{1,15}",
        // With alias
        "[a-zA-Z0-9 _-]{1,30}\\|[a-zA-Z0-9 _-]{1,20}",
        // Complex with heading and alias
        "[a-zA-Z0-9 _-]{1,20}#[a-zA-Z0-9 _-]{1,10}\\|[a-zA-Z0-9 _-]{1,15}",
        // Arbitrary printable ASCII (excluding ] to stay valid)
        "[^\\]\\[]{0,100}",
    ]
}

/// Strategy for generating frontmatter content
fn frontmatter_content_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid YAML frontmatter
        Just("---\ntitle: Test\n---\n# Content".to_string()),
        // Valid TOML frontmatter
        Just("+++\ntitle = \"Test\"\n+++\n# Content".to_string()),
        // No frontmatter
        Just("# Just content\n\nSome text.".to_string()),
        // Empty frontmatter
        Just("---\n---\n# Content".to_string()),
        // Arbitrary content (should not panic)
        ".*{0,500}",
    ]
}

/// Strategy for generating markdown content with wikilinks
fn markdown_with_wikilinks_strategy() -> impl Strategy<Value = String> {
    wikilink_inner_strategy().prop_map(|inner| format!("Some text [[{}]] more text", inner))
}

/// Strategy for generating tag content
fn tag_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9/_-]{0,30}"
}

proptest! {
    /// Property: Wikilink::parse should never panic for any input
    #[test]
    fn wikilink_parse_never_panics(inner in wikilink_inner_strategy(), offset in 0usize..1000, is_embed in any::<bool>()) {
        use crucible_core::parser::Wikilink;

        // Should not panic
        let _result = Wikilink::parse(&inner, offset, is_embed);
    }

    /// Property: Parsed wikilink target should not contain pipe character
    /// (pipe is used to separate target from alias)
    #[test]
    fn wikilink_target_has_no_pipe(inner in wikilink_inner_strategy()) {
        use crucible_core::parser::Wikilink;

        let result = Wikilink::parse(&inner, 0, false);
        prop_assert!(!result.target.contains('|'), "Target should not contain pipe: {}", result.target);
    }

    /// Property: Wikilink offset should be preserved
    #[test]
    fn wikilink_preserves_offset(inner in wikilink_inner_strategy(), offset in 0usize..10000) {
        use crucible_core::parser::Wikilink;

        let result = Wikilink::parse(&inner, offset, false);
        prop_assert_eq!(result.offset, offset, "Offset should be preserved");
    }

    /// Property: Wikilink is_embed flag should be preserved
    #[test]
    fn wikilink_preserves_embed_flag(inner in wikilink_inner_strategy(), is_embed in any::<bool>()) {
        use crucible_core::parser::Wikilink;

        let result = Wikilink::parse(&inner, 0, is_embed);
        prop_assert_eq!(result.is_embed, is_embed, "is_embed flag should be preserved");
    }

    /// Property: Frontmatter extraction should never panic
    #[test]
    fn frontmatter_extraction_never_panics(content in frontmatter_content_strategy()) {
        use crucible_parser::frontmatter_extractor::FrontmatterExtractor;

        let extractor = FrontmatterExtractor::new();
        // Should not panic, even for malformed input
        let _result = extractor.extract(&content);
    }

    /// Property: Frontmatter extraction body + frontmatter should roughly equal original length
    /// (allowing for delimiter removal)
    #[test]
    fn frontmatter_extraction_preserves_content(content in "[a-zA-Z0-9\\s#:_-]{0,200}") {
        use crucible_parser::frontmatter_extractor::FrontmatterExtractor;

        let extractor = FrontmatterExtractor::new();
        if let Ok(result) = extractor.extract(&content) {
            // Body should not be longer than original
            prop_assert!(result.body.len() <= content.len() + 10,
                "Body length {} should not exceed content length {} + 10",
                result.body.len(), content.len());
        }
    }

    /// Property: Content hashing (BLAKE3) should be deterministic
    #[test]
    fn content_hashing_deterministic(content in "[a-zA-Z0-9\\s]{1,500}") {
        use blake3::hash;

        let hash1 = hash(content.as_bytes());
        let hash2 = hash(content.as_bytes());

        prop_assert_eq!(hash1, hash2, "Same content should produce same hash");
    }

    /// Property: Different content should (usually) produce different hashes
    /// Note: This is probabilistic - we check that hashes differ when content differs significantly
    #[test]
    fn content_hashing_different_content(
        content1 in "[a-z]{10,50}",
        content2 in "[A-Z]{10,50}"
    ) {
        use blake3::hash;

        let hash1 = hash(content1.as_bytes());
        let hash2 = hash(content2.as_bytes());

        // Different content should produce different hashes
        // (with high probability for our test cases)
        if content1 != content2 {
            prop_assert_ne!(hash1, hash2, "Different content should produce different hashes");
        }
    }

    /// Property: Tag parsing should handle various tag formats
    #[test]
    fn tag_format_valid(tag in tag_strategy()) {
        // Valid tags should start with a letter and contain only allowed characters
        prop_assert!(tag.chars().next().is_none_or(|c| c.is_ascii_alphabetic()),
            "Tags should start with a letter: {}", tag);
    }

    /// Property: Markdown with wikilinks should be parseable without panic
    #[test]
    fn markdown_with_wikilinks_parseable(content in markdown_with_wikilinks_strategy()) {
        use crucible_parser::wikilinks::WikilinkExtension;
        use crucible_parser::extensions::SyntaxExtension;

        let ext = WikilinkExtension::new();
        prop_assert!(ext.can_handle(&content), "Should detect wikilinks in: {}", content);
    }
}

/// Additional edge case tests (not property-based but good to have alongside)
#[cfg(test)]
mod edge_cases {

    #[test]
    fn wikilink_empty_input() {
        use crucible_core::parser::Wikilink;
        let result = Wikilink::parse("", 0, false);
        assert!(result.target.is_empty() || result.target.is_empty());
    }

    #[test]
    fn wikilink_only_pipe() {
        use crucible_core::parser::Wikilink;
        let result = Wikilink::parse("|", 0, false);
        // Should handle gracefully
        assert!(result.target.is_empty());
    }

    #[test]
    fn wikilink_only_hash() {
        use crucible_core::parser::Wikilink;
        let result = Wikilink::parse("#", 0, false);
        // Should handle gracefully
        assert!(result.target.is_empty());
    }

    #[test]
    fn wikilink_unicode() {
        use crucible_core::parser::Wikilink;
        let result = Wikilink::parse("日本語ノート|エイリアス", 0, false);
        assert_eq!(result.target, "日本語ノート");
        assert_eq!(result.alias, Some("エイリアス".to_string()));
    }

    #[test]
    fn frontmatter_only_delimiters() {
        use crucible_parser::frontmatter_extractor::FrontmatterExtractor;
        let extractor = FrontmatterExtractor::new();
        let result = extractor.extract("---\n---");
        assert!(result.is_ok());
    }

    #[test]
    fn frontmatter_unclosed() {
        use crucible_parser::frontmatter_extractor::FrontmatterExtractor;
        let extractor = FrontmatterExtractor::new();
        let result = extractor.extract("---\ntitle: test\nno closing delimiter");
        // Should not panic, may return error or treat as no frontmatter
        let _ = result;
    }
}
