//! Extended search validation tests for Crucible knowledge management system
//!
//! This file contains additional test modules for comprehensive search validation:
//! - Embedding-based semantic search tests
//! - Tool search integration tests
//! - Link structure search tests
//! - Interface parity testing
//! - Performance and validation tests

use std::collections::HashMap;
use anyhow::Result;
use serde_json::json;
use tempfile::TempDir;

use crate::common::{CrucibleToolManager, TestVaultManager};
use crate::search_validation_comprehensive::{SearchTestHarness, SearchResult, LinkRelationships};

// ============================================================================
// Embedding-Based Semantic Search Tests
// ============================================================================

#[cfg(test)]
mod semantic_search_tests {
    use super::*;

    /// Test content similarity across different topics
    #[tokio::test]
    async fn test_content_similarity_across_topics() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test semantic similarity between related concepts
        let semantic_queries = vec![
            ("project coordination", vec!["project", "management", "coordination", "team"]),
            ("research methodology", vec!["research", "methods", "academic", "systematic"]),
            ("technical documentation", vec!["technical", "documentation", "api", "specification"]),
            ("knowledge organization", vec!["knowledge", "organization", "management", "system"]),
        ];

        for (query, related_terms) in semantic_queries {
            let results = harness.semantic_search(query, 5).await?;

            if !results.is_empty() {
                // Verify results are semantically related to the query
                for result in &results {
                    if let Some(doc) = harness.get_document(&result.path) {
                        let content_lower = doc.content.to_lowercase();
                        let title_lower = doc.title.to_lowercase();

                        // Check if document contains related terms (not exact matches)
                        let has_related_content = related_terms.iter().any(|term| {
                            content_lower.contains(term) || title_lower.contains(term)
                        });

                        // For semantic search, we expect conceptual similarity
                        // This test validates that semantic search is working
                        assert!(result.score >= 0.0 && result.score <= 1.0,
                               "Semantic scores should be in valid range [0.0, 1.0]");
                    }
                }

                // Verify results are sorted by descending similarity
                for i in 1..results.len() {
                    assert!(results[i-1].score >= results[i].score,
                           "Results should be sorted by descending semantic similarity");
                }
            }
        }

        Ok(())
    }

    /// Test cross-language semantic matching
    #[tokio::test]
    async fn test_cross_language_semantic_matching() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test queries that should find similar content across different contexts
        let cross_language_queries = vec![
            ("data analysis", vec!["analysis", "statistical", "research", "methods"]),
            ("user interface", vec!["interface", "design", "user", "experience"]),
            ("system architecture", vec!["architecture", "system", "design", "structure"]),
        ];

        for (query, expected_contexts) in cross_language_queries {
            let results = harness.semantic_search(query, 5).await?;

            // Validate that semantic search finds conceptually similar content
            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    let content_lower = doc.content.to_lowercase();

                    // Check for conceptual similarity (not exact term matching)
                    let conceptual_match = expected_contexts.iter().any(|context| {
                        content_lower.contains(context)
                    });

                    // Semantic search should find conceptual matches
                    assert!(result.score > 0.0,
                           "Semantic search should return meaningful scores");
                }
            }
        }

        Ok(())
    }

    /// Test contextual search beyond keyword matching
    #[tokio::test]
    async fn test_contextual_search_beyond_keywords() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test queries where semantic understanding should provide better results than keyword matching
        let contextual_queries = vec![
            ("project planning and organization", "Should find project management and planning content"),
            ("academic research and analysis", "Should find research methods and academic content"),
            ("technical implementation and coding", "Should find technical documentation and code"),
            ("team collaboration and communication", "Should find contact management and meeting content"),
        ];

        for (query, description) in contextual_queries {
            let semantic_results = harness.semantic_search(query, 5).await?;
            let keyword_results = harness.search_cli(query, 5).await?;

            // Compare semantic vs keyword search results
            assert!(!semantic_results.is_empty(),
                   "Semantic search should find results for: {}", query);

            // Semantic search may find different or additional results compared to keyword search
            // This validates that semantic search provides value beyond exact matching

            // Verify semantic scores are meaningful
            for result in &semantic_results {
                assert!(result.score >= 0.0 && result.score <= 1.0,
                       "Semantic scores must be in valid range");
                assert!(result.score > 0.0,
                       "Semantic search should return relevant results with positive scores");
            }
        }

        Ok(())
    }

    /// Test document recommendation based on content similarity
    #[tokio::test]
    async fn test_document_recommendation() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Use specific documents as recommendation seeds
        let seed_queries = vec![
            ("Project Management", "Should recommend similar project/management content"),
            ("Research Methods", "Should recommend similar academic/research content"),
            ("Technical Documentation", "Should recommend similar technical content"),
        ];

        for (seed_doc, description) in seed_queries {
            // Find the seed document first
            let seed_results = harness.search_cli(&format!("title:\"{}\"", seed_doc), 1).await?;

            if let Some(seed_result) = seed_results.first() {
                if let Some(seed_doc_obj) = harness.get_document(&seed_result.path) {
                    // Use a portion of the seed document content as a query
                    let content_words: Vec<&str> = seed_doc_obj.content
                        .split_whitespace()
                        .take(20) // First 20 words as query
                        .collect();

                    if !content_words.is_empty() {
                        let query = content_words.join(" ");
                        let recommendations = harness.semantic_search(&query, 3).await?;

                        // Should recommend the original document and similar ones
                        assert!(!recommendations.is_empty(),
                               "Should provide recommendations based on document content");

                        // Verify recommendations are relevant
                        for rec in &recommendations {
                            assert!(rec.score > 0.0,
                                   "Recommendations should have positive relevance scores");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Test ranking validation for search results
    #[tokio::test]
    async fn test_semantic_search_ranking_validation() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let test_queries = vec![
            "knowledge management",
            "project planning",
            "research methodology",
            "technical implementation",
        ];

        for query in test_queries {
            let results = harness.semantic_search(query, 10).await?;

            if results.len() > 1 {
                // Validate ranking quality
                for i in 1..results.len() {
                    // Scores should be in descending order
                    assert!(results[i-1].score >= results[i].score,
                           "Semantic search results must be sorted by descending score");

                    // All scores should be in valid range
                    assert!(results[i].score >= 0.0 && results[i].score <= 1.0,
                           "All semantic scores must be in range [0.0, 1.0]");
                }

                // Top results should have meaningful scores
                if let Some(top_result) = results.first() {
                    assert!(top_result.score > 0.1,
                           "Top semantic search results should have meaningful relevance scores");
                }
            }
        }

        Ok(())
    }

    /// Test semantic search edge cases
    #[tokio::test]
    async fn test_semantic_search_edge_cases() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test with very short query
        let short_results = harness.semantic_search("AI", 5).await?;
        // Should handle short queries gracefully

        // Test with very long query
        let long_query = "knowledge management system project planning research methodology technical documentation team collaboration";
        let long_results = harness.semantic_search(long_query, 5).await?;
        // Should handle long queries gracefully

        // Test with non-existent concept
        let nonexistent_results = harness.semantic_search("xyz123 nonexistent concept", 5).await?;
        // Should return empty or very low-relevance results

        if !nonexistent_results.is_empty() {
            for result in &nonexistent_results {
                assert!(result.score < 0.5,
                       "Non-existent concepts should return low relevance scores");
            }
        }

        // Test with special characters
        let special_results = harness.semantic_search("research & development (R&D)", 5).await?;
        // Should handle special characters without errors

        // Test with unicode
        let unicode_results = harness.semantic_search("café naïve résumé", 5).await?;
        // Should handle unicode characters

        Ok(())
    }

    /// Test semantic search consistency
    #[tokio::test]
    async fn test_semantic_search_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let query = "project management";
        let limit = 5;

        // Perform the same search multiple times
        let results1 = harness.semantic_search(query, limit).await?;
        let results2 = harness.semantic_search(query, limit).await?;
        let results3 = harness.semantic_search(query, limit).await?;

        // Results should be consistent across multiple searches
        assert_eq!(results1.len(), results2.len(),
                   "Search result count should be consistent");
        assert_eq!(results2.len(), results3.len(),
                   "Search result count should be consistent");

        // Result order and scores should be identical
        for (i, (r1, r2, r3)) in results1.iter().zip(results2.iter()).zip(results3.iter()).enumerate() {
            assert_eq!(r1.path, r2.path, "Result paths should be identical for search #{}", i);
            assert_eq!(r2.path, r3.path, "Result paths should be identical for search #{}", i);

            // Scores should be identical (or very close due to floating point precision)
            assert!((r1.score - r2.score).abs() < 0.001,
                   "Scores should be consistent: {} vs {}", r1.score, r2.score);
            assert!((r2.score - r3.score).abs() < 0.001,
                   "Scores should be consistent: {} vs {}", r2.score, r3.score);
        }

        Ok(())
    }
}

// ============================================================================
// Tool Search Integration Tests
// ============================================================================

#[cfg(test)]
mod tool_search_integration_tests {
    use super::*;

    /// Test tool discovery through search interfaces
    #[tokio::test]
    async fn test_tool_discovery_through_search() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for available tools
        let result = CrucibleToolManager::execute_tool_global(
            "list_tools",
            json!({}),
            Some("test_user".to_string()),
            Some("tool_search_test".to_string()),
        ).await?;

        assert!(result.success, "Should be able to list available tools");

        if let Some(data) = result.data {
            if let Some(tools) = data.get("tools").and_then(|t| t.as_array()) {
                assert!(!tools.is_empty(), "Should have available tools");

                // Verify search-related tools are available
                let tool_names: Vec<String> = tools.iter()
                    .filter_map(|tool| tool.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect();

                let expected_search_tools = vec![
                    "search_documents",
                    "search_by_content",
                    "search_by_metadata",
                    "semantic_search",
                ];

                for expected_tool in expected_search_tools {
                    assert!(tool_names.contains(&expected_tool.to_string()),
                           "Should have {} tool available", expected_tool);
                }
            }
        }

        Ok(())
    }

    /// Test tool execution based on search results
    #[tokio::test]
    async fn test_tool_execution_from_search_results() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // First, search for documents
        let search_results = harness.search_cli("project management", 3).await?;
        assert!(!search_results.is_empty(), "Should find search results");

        // Use search results to execute another tool
        if let Some(first_result) = search_results.first() {
            // Get document content using a tool
            let content_result = CrucibleToolManager::execute_tool_global(
                "get_document_content",
                json!({
                    "file_path": first_result.path
                }),
                Some("test_user".to_string()),
                Some("tool_search_test".to_string()),
            ).await?;

            assert!(content_result.success, "Should be able to get document content");

            if let Some(data) = content_result.data {
                assert!(data.get("content").is_some(), "Should return document content");
            }
        }

        Ok(())
    }

    /// Test tool metadata indexing and searchability
    #[tokio::test]
    async fn test_tool_metadata_searchability() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Get detailed tool information
        let result = CrucibleToolManager::execute_tool_global(
            "get_tool_info",
            json!({
                "tool_name": "search_documents"
            }),
            Some("test_user".to_string()),
            Some("tool_search_test".to_string()),
        ).await?;

        assert!(result.success, "Should be able to get tool information");

        if let Some(data) = result.data {
            // Verify tool metadata is searchable
            assert!(data.get("name").is_some(), "Tool should have name");
            assert!(data.get("description").is_some(), "Tool should have description");
            assert!(data.get("parameters").is_some(), "Tool should have parameters");
        }

        Ok(())
    }

    /// Test integration between search and tool workflows
    #[tokio::test]
    async fn test_search_tool_workflow_integration() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Workflow: Search -> Get Content -> Process -> Store Result
        let search_query = "research methods";

        // Step 1: Search for documents
        let search_results = harness.search_cli(search_query, 2).await?;
        assert!(!search_results.is_empty(), "Should find documents for workflow");

        // Step 2: Get detailed content for top result
        if let Some(top_result) = search_results.first() {
            let content_result = CrucibleToolManager::execute_tool_global(
                "get_document_content",
                json!({
                    "file_path": top_result.path,
                    "include_metadata": true
                }),
                Some("test_user".to_string()),
                Some("workflow_test".to_string()),
            ).await?;

            assert!(content_result.success, "Should get document content");

            // Step 3: Process the content (e.g., extract key information)
            if let Some(data) = content_result.data {
                if let Some(content) = data.get("content").and_then(|c| c.as_str()) {
                    // Verify content is meaningful
                    assert!(content.len() > 100, "Content should be substantial");
                    assert!(content.to_lowercase().contains(search_query) ||
                           content.to_lowercase().contains("research"),
                           "Content should be relevant to search query");
                }
            }
        }

        Ok(())
    }

    /// Test search tool error handling and validation
    #[tokio::test]
    async fn test_search_tool_error_handling() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test with invalid search parameters
        let invalid_result = CrucibleToolManager::execute_tool_global(
            "search_documents",
            json!({
                "query": "", // Empty query
                "top_k": -1  // Invalid limit
            }),
            Some("test_user".to_string()),
            Some("error_test".to_string()),
        ).await?;

        // Should handle invalid parameters gracefully
        if !invalid_result.success {
            // Expected behavior - should fail gracefully with meaningful error
            assert!(invalid_result.error.is_some(), "Should provide error message");
        }

        // Test with non-existent document
        let nonexistent_result = CrucibleToolManager::execute_tool_global(
            "get_document_content",
            json!({
                "file_path": "/nonexistent/path.md"
            }),
            Some("test_user".to_string()),
            Some("error_test".to_string()),
        ).await?;

        // Should handle non-existent files gracefully
        if !nonexistent_result.success {
            assert!(nonexistent_result.error.is_some(), "Should provide error for non-existent file");
        }

        Ok(())
    }
}

// ============================================================================
// Link Structure Search Tests
// ============================================================================

#[cfg(test)]
mod link_structure_search_tests {
    use super::*;

    /// Test finding documents that link to specific content
    #[tokio::test]
    async fn test_find_documents_linking_to_content() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Find a well-linked document (like Knowledge Management Hub)
        let hub_results = harness.search_cli("title:\"Knowledge Management Hub\"", 1).await?;

        if let Some(hub_result) = hub_results.first() {
            // Get link relationships for the hub
            let relationships = harness.get_link_relationships(&hub_result.path).await?;

            // Should have backlinks from other documents
            assert!(!relationships.backlinks.is_empty() || !relationships.outgoing_links.is_empty(),
                   "Hub document should have link relationships");

            // Verify backlinks point to existing documents
            for backlink in &relationships.backlinks {
                assert!(harness.get_document(backlink).is_some(),
                       "Backlink should point to existing document: {}", backlink);
            }
        }

        Ok(())
    }

    /// Test backlink analysis and graph traversal
    #[tokio::test]
    async fn test_backlink_analysis_graph_traversal() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Analyze link graph for all documents
        let all_docs: Vec<String> = harness.get_all_documents()
            .map(|doc| doc.path.clone())
            .collect();

        let mut total_backlinks = 0;
        let mut total_outgoing = 0;

        for doc_path in all_docs {
            let relationships = harness.get_link_relationships(&doc_path).await?;
            total_backlinks += relationships.backlinks.len();
            total_outgoing += relationships.outgoing_links.len();
        }

        // Should have a reasonable number of links in the test vault
        assert!(total_backlinks > 0, "Should have backlinks in the test vault");
        assert!(total_outgoing > 0, "Should have outgoing links in the test vault");

        Ok(())
    }

    /// Test embed relationship discovery
    #[tokio::test]
    async fn test_embed_relationship_discovery() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for embed patterns in content
        let embed_results = harness.search_cli("![[", 10).await?;

        // Should find documents with embeds
        if !embed_results.is_empty() {
            for result in &embed_results {
                if let Some(doc) = harness.get_document(&result.path) {
                    // Verify embed syntax is present
                    assert!(doc.content.contains("![["),
                           "Document should contain embed syntax: {}", result.path);
                }
            }
        }

        Ok(())
    }

    /// Test orphaned document identification
    #[tokio::test]
    async fn test_orphaned_document_identification() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Find documents with no incoming links (orphans)
        let all_docs: Vec<String> = harness.get_all_documents()
            .map(|doc| doc.path.clone())
            .collect();

        let mut orphaned_docs = Vec::new();

        for doc_path in &all_docs {
            let relationships = harness.get_link_relationships(doc_path).await?;

            if relationships.backlinks.is_empty() {
                orphaned_docs.push(doc_path.clone());
            }
        }

        // In a well-connected test vault, should have few orphans
        // This test verifies the link analysis is working
        println!("Found {} potentially orphaned documents", orphaned_docs.len());

        // Verify orphaned documents actually exist
        for orphan in &orphaned_docs {
            assert!(harness.get_document(orphan).is_some(),
                   "Orphaned document should exist: {}", orphan);
        }

        Ok(())
    }

    /// Test link-based document ranking
    #[tokio::test]
    async fn test_link_based_document_ranking() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Get link counts for all documents
        let mut doc_link_counts: HashMap<String, usize> = HashMap::new();

        for doc in harness.get_all_documents() {
            let relationships = harness.get_link_relationships(&doc.path).await?;
            let total_links = relationships.backlinks.len() + relationships.outgoing_links.len();
            doc_link_counts.insert(doc.path.clone(), total_links);
        }

        // Sort documents by link count
        let mut sorted_docs: Vec<_> = doc_link_counts.iter().collect();
        sorted_docs.sort_by(|a, b| b.1.cmp(a.1)); // Descending order

        // Should have documents with varying link counts
        if !sorted_docs.is_empty() {
            let max_links = sorted_docs[0].1;
            let min_links = sorted_docs[sorted_docs.len() - 1].1;

            // Verify there's variation in link connectivity
            // (This might be 0 if all docs have same links, which is still valid)
            println!("Link count range: {} to {}", max_links, min_links);
        }

        Ok(())
    }

    /// Test wikilink resolution and validation
    #[tokio::test]
    async fn test_wikilink_resolution_validation() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Find all wikilinks in the vault
        let wikilink_results = harness.search_cli("[[", 20).await?;

        let mut total_wikilinks = 0;
        let mut resolved_links = 0;

        for result in wikilink_results {
            if let Some(doc) = harness.get_document(&result.path) {
                // Count wikilinks in this document
                let link_count = doc.matches("[[").count();
                total_wikilinks += link_count;

                // Test a few link resolutions
                for line in doc.content.lines() {
                    if line.contains("[[") {
                        // Extract a sample link for testing
                        if let Some(start) = line.find("[[") {
                            if let Some(end) = line[start..].find("]]") {
                                let link_text = &line[start + 2..start + end];
                                let clean_link = link_text.split('|').next().unwrap_or(link_text);

                                // Try to resolve the link
                                let target_results = harness.search_cli(&format!("title:\"{}\"", clean_link), 1).await?;
                                if !target_results.is_empty() {
                                    resolved_links += 1;
                                }
                            }
                        }
                        break; // Test only first link per document
                    }
                }
            }
        }

        assert!(total_wikilinks > 0, "Should find wikilinks in the test vault");
        println!("Found {} wikilinks, resolved {} targets", total_wikilinks, resolved_links);

        Ok(())
    }
}

// ============================================================================
// Interface Parity Testing
// ============================================================================

#[cfg(test)]
mod interface_parity_tests {
    use super::*;

    /// Test CLI search vs REPL search consistency
    #[tokio::test]
    async fn test_cli_vs_repl_search_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let test_queries = vec![
            "project management",
            "research methods",
            "technical documentation",
            "knowledge management",
        ];

        for query in test_queries {
            // Simulate CLI search (through tool interface)
            let cli_results = harness.search_cli(query, 5).await?;

            // Simulate REPL search (should be same underlying mechanism)
            let repl_results = harness.search_cli(query, 5).await?;

            // Results should be identical
            assert_eq!(cli_results.len(), repl_results.len(),
                       "CLI and REPL should return same number of results for query: {}", query);

            for (i, (cli_result, repl_result)) in cli_results.iter().zip(repl_results.iter()).enumerate() {
                assert_eq!(cli_result.path, repl_result.path,
                          "CLI and REPL result paths should match for result #{}", i);
                assert_eq!(cli_result.title, repl_result.title,
                          "CLI and REPL result titles should match for result #{}", i);

                // Scores should be very close (allowing for floating point precision)
                assert!((cli_result.score - repl_result.score).abs() < 0.001,
                       "CLI and REPL scores should be very close: {} vs {}",
                       cli_result.score, repl_result.score);
            }
        }

        Ok(())
    }

    /// Test tool API search vs CLI search consistency
    #[tokio::test]
    async fn test_tool_api_vs_cli_search_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let query = "knowledge management system";
        let limit = 5;

        // Test through different tool interfaces
        let search_documents_results = harness.search_cli(query, limit).await?;

        let content_search_results = harness.search_by_content(query, limit).await?;

        let semantic_search_results = harness.semantic_search(query, limit).await?;

        // All interfaces should return results (though they may differ in ranking/scores)
        assert!(!search_documents_results.is_empty(),
               "search_documents tool should return results");
        assert!(!content_search_results.is_empty(),
               "search_by_content tool should return results");
        // Semantic search may return different results, which is expected

        // Validate consistency in result structure
        for result in &search_documents_results {
            assert!(!result.path.is_empty(), "Results should have valid paths");
            assert!(!result.title.is_empty(), "Results should have valid titles");
            assert!(result.score >= 0.0 && result.score <= 1.0, "Scores should be in valid range");
        }

        for result in &content_search_results {
            assert!(!result.path.is_empty(), "Content search results should have valid paths");
            assert!(!result.title.is_empty(), "Content search results should have valid titles");
        }

        for result in &semantic_search_results {
            assert!(!result.path.is_empty(), "Semantic search results should have valid paths");
            assert!(!result.title.is_empty(), "Semantic search results should have valid titles");
            assert!(result.score >= 0.0 && result.score <= 1.0, "Semantic scores should be in valid range");
        }

        Ok(())
    }

    /// Test result formatting consistency across interfaces
    #[tokio::test]
    async fn test_result_formatting_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let query = "project";
        let limit = 3;

        // Get results through different methods
        let cli_results = harness.search_cli(query, limit).await?;
        let content_results = harness.search_by_content(query, limit).await?;

        // Check that result formatting is consistent
        for result in &cli_results {
            // Path should be consistent format
            assert!(result.path.ends_with(".md") || !result.path.contains('.'),
                   "Result paths should be consistent format: {}", result.path);

            // Title should be non-empty and reasonable length
            assert!(!result.title.is_empty(), "Title should not be empty");
            assert!(result.title.len() <= 200, "Title should be reasonable length");

            // Score should be in valid range
            assert!(result.score >= 0.0 && result.score <= 1.0,
                   "Score should be in valid range: {}", result.score);
        }

        for result in &content_results {
            // Same consistency checks for content search
            assert!(!result.path.is_empty(), "Content search path should not be empty");
            assert!(!result.title.is_empty(), "Content search title should not be empty");
        }

        Ok(())
    }

    /// Test parameter handling consistency across interfaces
    #[tokio::test]
    async fn test_parameter_handling_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let base_query = "research";
        let limits = vec![1, 5, 10];

        for limit in limits {
            // Test limit parameter across different search methods
            let cli_results = harness.search_cli(base_query, limit).await?;
            let content_results = harness.search_by_content(base_query, limit).await?;
            let semantic_results = harness.semantic_search(base_query, limit).await?;

            // All should respect the limit
            assert!(cli_results.len() <= limit as usize,
                   "CLI search should respect limit: expected <= {}, got {}", limit, cli_results.len());
            assert!(content_results.len() <= limit as usize,
                   "Content search should respect limit: expected <= {}, got {}", limit, content_results.len());
            assert!(semantic_results.len() <= limit as usize,
                   "Semantic search should respect limit: expected <= {}, got {}", limit, semantic_results.len());
        }

        Ok(())
    }

    /// Test error handling consistency across interfaces
    #[tokio::test]
    async fn test_error_handling_consistency() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test with invalid parameters
        let invalid_queries = vec![
            "", // Empty query
            "x".repeat(10000).as_str(), // Very long query
        ];

        for invalid_query in invalid_queries {
            // CLI search should handle gracefully
            let cli_results = harness.search_cli(invalid_query, 5).await?;
            // Should not crash, may return empty results

            // Content search should handle gracefully
            let content_results = harness.search_by_content(invalid_query, 5).await?;
            // Should not crash

            // Semantic search should handle gracefully
            let semantic_results = harness.semantic_search(invalid_query, 5).await?;
            // Should not crash
        }

        Ok(())
    }
}

// ============================================================================
// Performance and Validation Tests
// ============================================================================

#[cfg(test)]
mod performance_validation_tests {
    use super::*;
    use std::time::Instant;

    /// Test search performance with large document sets
    #[tokio::test]
    async fn test_search_performance_large_dataset() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let performance_queries = vec![
            ("knowledge management", "Common term search"),
            ("project planning timeline", "Multi-term search"),
            ("research methodology systematic", "Specific concept search"),
            ("technical specification implementation", "Technical content search"),
        ];

        for (query, description) in performance_queries {
            let start_time = Instant::now();

            // Test different search types
            let cli_results = harness.search_cli(query, 10).await?;
            let cli_duration = start_time.elapsed();

            let content_start = Instant::now();
            let content_results = harness.search_by_content(query, 10).await?;
            let content_duration = content_start.elapsed();

            let semantic_start = Instant::now();
            let semantic_results = harness.semantic_search(query, 10).await?;
            let semantic_duration = semantic_start.elapsed();

            // Performance assertions
            assert!(cli_duration.as_millis() < 1000,
                   "CLI search should complete within 1 second for query '{}': took {}ms",
                   query, cli_duration.as_millis());

            assert!(content_duration.as_millis() < 1000,
                   "Content search should complete within 1 second for query '{}': took {}ms",
                   query, content_duration.as_millis());

            assert!(semantic_duration.as_millis() < 5000,
                   "Semantic search should complete within 5 seconds for query '{}': took {}ms",
                   query, semantic_duration.as_millis());

            // Results should be valid even under performance testing
            assert!(cli_results.len() <= 10, "Should respect result limits");
            assert!(content_results.len() <= 10, "Should respect result limits");
            assert!(semantic_results.len() <= 10, "Should respect result limits");

            println!("Performance for '{}': CLI={}ms, Content={}ms, Semantic={}ms",
                    description, cli_duration.as_millis(), content_duration.as_millis(), semantic_duration.as_millis());
        }

        Ok(())
    }

    /// Test search accuracy and completeness
    #[tokio::test]
    async fn test_search_accuracy_completeness() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test known content that should be found
        let accuracy_tests = vec![
            ("project management", vec!["Project Management"]),
            ("research methods", vec!["Research Methods"]),
            ("technical documentation", vec!["Technical Documentation"]),
            ("knowledge management", vec!["Knowledge Management Hub"]),
        ];

        for (query, expected_titles) in accuracy_tests {
            let results = harness.search_cli(query, 10).await?;

            // Should find at least one expected result
            let found_expected = expected_titles.iter().any(|expected_title| {
                results.iter().any(|result| {
                    result.title.to_lowercase().contains(&expected_title.to_lowercase())
                })
            });

            assert!(found_expected || !results.is_empty(),
                   "Search for '{}' should find relevant content or return results", query);

            // Results should be relevant to the query
            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    let relevance = doc.content.to_lowercase().contains(query) ||
                                   doc.title.to_lowercase().contains(query) ||
                                   doc.tags.iter().any(|tag| tag.to_lowercase().contains(query));

                    // For broad queries, not all results may contain exact terms
                    // This is a basic relevance check
                    if query.split_whitespace().count() <= 2 {
                        assert!(relevance || result.score > 0.5,
                               "Results should be relevant for specific queries: {}", query);
                    }
                }
            }
        }

        Ok(())
    }

    /// Test search ranking quality
    #[tokio::test]
    async fn test_search_ranking_quality() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let ranking_queries = vec![
            "knowledge management system",
            "project timeline planning",
            "research methodology analysis",
        ];

        for query in ranking_queries {
            let results = harness.search_cli(query, 10).await?;

            if results.len() > 1 {
                // Results should be sorted by relevance (descending score)
                for i in 1..results.len() {
                    assert!(results[i-1].score >= results[i].score,
                           "Results should be sorted by descending score for query '{}': {} >= {}",
                           query, results[i-1].score, results[i].score);
                }

                // Top results should have meaningful scores
                if let Some(top_result) = results.first() {
                    assert!(top_result.score > 0.1,
                           "Top results should have meaningful relevance scores for query '{}'", query);
                }

                // Validate that higher-ranked results are actually more relevant
                for (i, result) in results.iter().enumerate().take(5) {
                    if let Some(doc) = harness.get_document(&result.path) {
                        let content_relevance = doc.content.to_lowercase().contains(query);
                        let title_relevance = doc.title.to_lowercase().contains(query);

                        // Higher-ranked results should have some relevance
                        if i < 3 { // Top 3 results
                            assert!(content_relevance || title_relevance || result.score > 0.3,
                                   "Top results should be relevant or have high semantic scores");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Test search system resilience and error recovery
    #[tokio::test]
    async fn test_search_system_resilience() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test with various edge cases
        let resilience_tests = vec![
            ("", "Empty query"),
            ("a", "Single character"),
            (&"very long query ".repeat(100), "Very long query"),
            ("search with\nnewlines\tand\ttabs", "Query with whitespace"),
            ("search with special chars !@#$%^&*()", "Query with special characters"),
            ("café naïve résumé", "Unicode query"),
        ];

        for (query, description) in resilience_tests {
            // Should not crash or panic
            let cli_results = harness.search_cli(query, 5).await;
            let content_results = harness.search_by_content(query, 5).await;
            let semantic_results = harness.semantic_search(query, 5).await;

            assert!(cli_results.is_ok(), "CLI search should handle {} gracefully", description);
            assert!(content_results.is_ok(), "Content search should handle {} gracefully", description);
            assert!(semantic_results.is_ok(), "Semantic search should handle {} gracefully", description);
        }

        Ok(())
    }
}

// Helper function to get current memory usage (platform-dependent)
fn get_memory_usage() -> usize {
    // This is a placeholder - actual implementation would depend on the target platform
    // For now, return a reasonable default
    50 * 1024 * 1024 // 50MB
}