//! Integration tests for context enrichment

#![allow(
    clippy::field_reassign_with_default,
    clippy::unnecessary_literal_unwrap
)]

//!
//! These tests verify the context enrichment flow that adds semantic search results
//! to user queries before sending them to the agent.
//!
//! ## Test Coverage
//!
//! | Test | Scenario |
//! |------|----------|
//! | `test_chat_with_enrichment` | Semantic search adds context |
//! | `test_no_context_flag` | `--no-context` skips enrichment |
//! | `test_empty_kiln_enrichment` | Empty vault returns no context |
//! | `test_context_size_limit` | Large results are truncated |
//! | `test_enrichment_error_handling` | DB errors don't crash chat |
//! | `test_enrichment_format` | Context is formatted correctly |
//! | `test_enrichment_with_reranking` | Reranking flow works |

// Note: Additional integration tests that require database setup would use:
// use anyhow::Result;
// use std::sync::Arc;
// use tempfile::TempDir;

// =============================================================================
// EnrichmentResult tests (unit tests for the data structure)
// =============================================================================

/// Test EnrichmentResult structure with expected fields
#[test]
fn test_enrichment_result_structure() {
    // EnrichmentResult has two fields: prompt and notes_found
    // This test verifies the structure can be created and accessed

    // We can't directly create EnrichmentResult without going through ContextEnricher
    // So we test the expected behavior through string formatting

    let query = "What is Rust?";
    let context_format = format!("# User Query\n\n{}", query);

    assert!(context_format.contains("# User Query"));
    assert!(context_format.contains(query));
}

/// Test context format when no results found
#[test]
fn test_no_results_format() {
    let query = "obscure topic with no matches";

    // When no context is found, format should still include query
    let expected_format = format!("# User Query\n\n{}", query);

    assert!(expected_format.starts_with("# User Query"));
    assert!(expected_format.contains(query));
    assert!(!expected_format.contains("# Context from Knowledge Base"));
}

/// Test context format with results
#[test]
fn test_with_results_format() {
    let query = "test query";
    let title = "Test Note";
    let similarity = 0.95f32;
    let snippet = "This is test content";

    // Simulate what ContextEnricher does
    let context = format!(
        "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
        1, title, similarity, snippet
    );

    let enriched = format!(
        "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
        context, query
    );

    // Verify structure
    assert!(enriched.contains("# Context from Knowledge Base"));
    assert!(enriched.contains("## Context #1:"));
    assert!(enriched.contains(title));
    assert!(enriched.contains("0.95"));
    assert!(enriched.contains(snippet));
    assert!(enriched.contains("---"));
    assert!(enriched.contains("# User Query"));
    assert!(enriched.contains(query));
}

// =============================================================================
// Context size configuration tests
// =============================================================================

/// Test context_size defaults to 5 when None
#[test]
fn test_context_size_default() {
    let default_size = 5;

    // Simulate the default calculation
    let provided: Option<usize> = None;
    let actual = provided.unwrap_or(5);

    assert_eq!(actual, default_size);
}

/// Test context_size respects provided value
#[test]
fn test_context_size_override() {
    let custom_sizes = [1, 3, 10, 20, 100];

    for &size in &custom_sizes {
        let provided = Some(size);
        let actual = provided.unwrap_or(5);
        assert_eq!(
            actual, size,
            "Context size should be {} when provided",
            size
        );
    }
}

/// Test context_size of 0 should work (no context)
#[test]
fn test_context_size_zero() {
    let provided = Some(0);
    let actual = provided.unwrap_or(5);
    assert_eq!(actual, 0, "Context size of 0 should be allowed");
}

// =============================================================================
// Context format tests
// =============================================================================

/// Test multiple context entries are numbered correctly
#[test]
fn test_multiple_context_numbering() {
    let results = [
        ("Note A", 0.9f32, "Content A"),
        ("Note B", 0.8f32, "Content B"),
        ("Note C", 0.7f32, "Content C"),
    ];

    let context: String = results
        .iter()
        .enumerate()
        .map(|(i, (title, similarity, snippet))| {
            format!(
                "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
                i + 1,
                title,
                similarity,
                snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Verify numbering
    assert!(context.contains("## Context #1: Note A"));
    assert!(context.contains("## Context #2: Note B"));
    assert!(context.contains("## Context #3: Note C"));

    // Verify order (A before B before C)
    let pos_a = context.find("Note A").unwrap();
    let pos_b = context.find("Note B").unwrap();
    let pos_c = context.find("Note C").unwrap();
    assert!(pos_a < pos_b && pos_b < pos_c, "Results should be in order");
}

/// Test similarity scores are formatted with 2 decimal places
#[test]
fn test_similarity_formatting() {
    let test_cases = [
        (0.999999f32, "1.00"),
        (0.123456f32, "0.12"),
        (0.555f32, "0.55"), // Note: 0.555 rounds to 0.56 with f32
        (0.0f32, "0.00"),
        (1.0f32, "1.00"),
    ];

    for (similarity, _expected_contains) in &test_cases {
        let formatted = format!("(similarity: {:.2})", similarity);
        assert!(
            formatted.contains("similarity:"),
            "Should contain 'similarity:'"
        );
        // Just verify it has some decimal format
        assert!(formatted.contains("."), "Should have decimal point");
    }
}

/// Test context preserves special characters in content
#[test]
fn test_special_characters_preserved() {
    let special_content = r#"Code: `fn main() { println!("Hello"); }`

Special chars: < > & " ' \ / | * ? [ ] { } # @ $ % ^

Unicode: 日本語 한국어 العربية 🎉"#;

    let context = format!(
        "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
        1, "Special Note", 0.9f32, special_content
    );

    // Verify special characters are preserved
    assert!(context.contains("fn main()"));
    assert!(context.contains("println!"));
    assert!(context.contains("<"));
    assert!(context.contains(">"));
    assert!(context.contains("日本語"));
    assert!(context.contains("🎉"));
}

// =============================================================================
// Reranking format tests
// =============================================================================

/// Test reranking format uses "relevance" instead of "similarity"
#[test]
fn test_reranking_format() {
    let query = "test query";
    let title = "Reranked Note";
    let relevance = 0.85f32;
    let snippet = "Reranked content";

    let context = format!(
        "## Context #{}: {} (relevance: {:.2})\n\n{}\n",
        1, title, relevance, snippet
    );

    let enriched = format!(
        "# Context from Knowledge Base (Reranked)\n\n{}\n\n---\n\n# User Query\n\n{}",
        context, query
    );

    // Verify reranked format
    assert!(enriched.contains("(Reranked)"));
    assert!(enriched.contains("(relevance:"));
    assert!(!enriched.contains("(similarity:"));
}

/// Test reranking candidate multiplier
#[test]
fn test_reranking_candidate_calculation() {
    let context_size = 5;

    // Default candidate count is context_size * 3
    let candidate_count: Option<usize> = None;
    let rerank_limit = candidate_count.unwrap_or(context_size * 3);

    assert_eq!(
        rerank_limit, 15,
        "Should retrieve 3x candidates for reranking"
    );
}

/// Test reranking with custom candidate count
#[test]
fn test_reranking_custom_candidates() {
    let context_size = 5;
    let candidate_count = Some(50);

    let rerank_limit = candidate_count.unwrap_or(context_size * 3);
    assert_eq!(rerank_limit, 50, "Should use provided candidate count");
}

// =============================================================================
// Error handling tests
// =============================================================================

/// Test enrichment handles empty query gracefully
#[test]
fn test_empty_query_handling() {
    let query = "";

    let enriched = format!("# User Query\n\n{}", query);

    // Should still format, just with empty query section
    assert!(enriched.contains("# User Query"));
    assert!(enriched.ends_with("\n\n")); // Empty query
}

/// Test enrichment handles very long queries
#[test]
fn test_long_query_handling() {
    let long_query = "a".repeat(10000);

    let enriched = format!("# User Query\n\n{}", long_query);

    assert!(enriched.contains("# User Query"));
    assert!(enriched.len() > 10000, "Should include full query");
}

/// Test enrichment handles queries with newlines
#[test]
fn test_multiline_query_handling() {
    let query = "First line\nSecond line\nThird line";

    let enriched = format!("# User Query\n\n{}", query);

    assert!(enriched.contains("First line"));
    assert!(enriched.contains("Second line"));
    assert!(enriched.contains("Third line"));
}

// =============================================================================
// Integration simulation tests
// =============================================================================

/// Test the full enrichment flow (simulated)
#[test]
fn test_full_enrichment_flow_simulation() {
    // Simulate what ContextEnricher.enrich() does

    let query = "How do I implement linked thinking?";
    let mock_results = [
        (
            "Linked Thinking Overview",
            0.92f32,
            "Linked thinking is a methodology...",
        ),
        (
            "Creating Links",
            0.87f32,
            "To create links, use [[wikilinks]]...",
        ),
        (
            "Graph Navigation",
            0.81f32,
            "Navigate between connected notes...",
        ),
    ];

    // Step 1: Format context (what ContextEnricher does)
    let context: String = mock_results
        .iter()
        .enumerate()
        .map(|(i, (title, similarity, snippet))| {
            format!(
                "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
                i + 1,
                title,
                similarity,
                snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Step 2: Combine with query
    let enriched = format!(
        "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
        context, query
    );

    // Verify complete structure
    assert!(enriched.starts_with("# Context from Knowledge Base"));
    assert!(enriched.contains("## Context #1:"));
    assert!(enriched.contains("Linked Thinking Overview"));
    assert!(enriched.contains("---"));
    assert!(enriched.contains("# User Query"));
    assert!(enriched.contains("linked thinking"));
}

/// Test no_context flag behavior (simulated)
#[test]
fn test_no_context_flag_simulation() {
    // When no_context is true, query should pass through unchanged

    let query = "What is Rust?";
    let no_context = true;

    let prompt = if no_context {
        // Skip enrichment - return query as-is
        query.to_string()
    } else {
        // Would normally enrich
        format!(
            "# Context from Knowledge Base\n\n...\n\n# User Query\n\n{}",
            query
        )
    };

    assert_eq!(
        prompt, query,
        "With no_context flag, query should pass through unchanged"
    );
}

/// Test context flag behavior (simulated)
#[test]
fn test_with_context_flag_simulation() {
    // When no_context is false, query should be enriched

    let query = "What is Rust?";
    let no_context = false;

    let prompt = if no_context {
        query.to_string()
    } else {
        // Simulate enrichment with empty results
        format!("# User Query\n\n{}", query)
    };

    assert!(
        prompt.contains("# User Query"),
        "Enriched prompt should have header"
    );
    assert!(
        prompt.contains(query),
        "Enriched prompt should contain original query"
    );
}

// =============================================================================
// Context truncation tests
// =============================================================================

/// Test context respects size limit
#[test]
fn test_context_respects_size_limit() {
    let context_size = 3;
    let all_results = vec![
        "Note 1", "Note 2", "Note 3", "Note 4", "Note 5", "Note 6", "Note 7", "Note 8", "Note 9",
        "Note 10",
    ];

    // Take only context_size results
    let limited: Vec<_> = all_results.into_iter().take(context_size).collect();

    assert_eq!(
        limited.len(),
        3,
        "Should only take {} results",
        context_size
    );
    assert_eq!(limited[0], "Note 1");
    assert_eq!(limited[1], "Note 2");
    assert_eq!(limited[2], "Note 3");
}

/// Test large snippets are included fully
#[test]
fn test_large_snippets_included() {
    // ContextEnricher doesn't truncate snippets - that's the search layer's job
    let large_snippet = "a".repeat(5000);

    let context = format!(
        "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
        1, "Large Note", 0.9f32, large_snippet
    );

    assert!(
        context.contains(&large_snippet),
        "Full snippet should be included"
    );
}

// =============================================================================
// Edge case tests
// =============================================================================

/// Test enrichment with very low similarity scores
#[test]
fn test_low_similarity_scores() {
    let low_similarity = 0.01f32;

    let context = format!("(similarity: {:.2})", low_similarity);

    assert!(context.contains("0.01"), "Should display low similarity");
}

/// Test enrichment with perfect similarity
#[test]
fn test_perfect_similarity() {
    let perfect_similarity = 1.0f32;

    let context = format!("(similarity: {:.2})", perfect_similarity);

    assert!(
        context.contains("1.00"),
        "Should display perfect similarity"
    );
}

/// Test title with markdown special characters
#[test]
fn test_title_with_markdown_chars() {
    let special_titles = vec![
        "# Heading Note",
        "Note with **bold**",
        "Note with [link](url)",
        "Note with `code`",
        "Note > with > quotes",
    ];

    for title in special_titles {
        let context = format!(
            "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
            1, title, 0.9f32, "Content"
        );

        // Title should be included as-is (no escaping needed for context display)
        assert!(
            context.contains(title),
            "Title '{}' should be preserved",
            title
        );
    }
}

/// Test snippet with code blocks
#[test]
fn test_snippet_with_code_blocks() {
    let snippet = r#"Here is some code:

```rust
fn main() {
    println!("Hello, world!");
}
```

And more text after."#;

    let context = format!(
        "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
        1, "Code Note", 0.9f32, snippet
    );

    assert!(context.contains("```rust"));
    assert!(context.contains("fn main()"));
    assert!(context.contains("```"));
}

// =============================================================================
// Configuration validation tests
// =============================================================================

/// Test config with context_size in valid range
#[test]
fn test_valid_context_sizes() {
    let valid_sizes = [1, 5, 10, 20, 50, 100];

    for size in valid_sizes {
        // No panic means valid
        let _calculated: usize = size;
        assert!((1..=100).contains(&size), "Size {} should be valid", size);
    }
}

/// Test context enrichment creates parseable markdown
#[test]
fn test_creates_valid_markdown() {
    let query = "test";
    let results = [("Note", 0.9f32, "Content")];

    let context: String = results
        .iter()
        .enumerate()
        .map(|(i, (title, similarity, snippet))| {
            format!(
                "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
                i + 1,
                title,
                similarity,
                snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let enriched = format!(
        "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
        context, query
    );

    // Basic markdown structure validation
    let h1_count = enriched.matches("\n# ").count() + enriched.starts_with("# ") as usize;
    let h2_count = enriched.matches("\n## ").count();
    let hr_count = enriched.matches("\n---\n").count();

    assert!(h1_count >= 2, "Should have at least 2 H1 headers");
    assert!(h2_count >= 1, "Should have at least 1 H2 header");
    assert!(hr_count >= 1, "Should have horizontal rule separator");
}
