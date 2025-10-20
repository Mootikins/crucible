// crates/crucible-daemon/tests/fixtures/corpus_builder.rs

//! Builder for the semantic test corpus

use super::semantic_corpus::*;

/// Build the sample test corpus with all test documents and expectations
pub fn build_sample_corpus() -> SemanticTestCorpus {
    let documents = vec![
        // === HIGH SIMILARITY PAIRS ===
        TestDocument {
            id: "rust_fn_add".to_string(),
            content: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 15,
                tags: vec!["function".into(), "arithmetic".into()],
                description: "Basic Rust addition function".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "rust_fn_sum".to_string(),
            content: "fn sum(x: i32, y: i32) -> i32 { x + y }".to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 15,
                tags: vec!["function".into(), "arithmetic".into()],
                description: "Identical logic with different names".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "rust_doc_addition".to_string(),
            content: "This function takes two integers and returns their sum. \
                     It performs simple arithmetic addition."
                .to_string(),
            category: DocumentCategory::Documentation,
            metadata: DocumentMetadata {
                language: None,
                token_count: 18,
                tags: vec!["documentation".into(), "arithmetic".into()],
                description: "Natural language description of addition".into(),
            },
            embedding: None,
        },
        // === MEDIUM SIMILARITY PAIRS ===
        TestDocument {
            id: "rust_fn_multiply".to_string(),
            content: "fn multiply(a: i32, b: i32) -> i32 { a * b }".to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 15,
                tags: vec!["function".into(), "arithmetic".into()],
                description: "Multiplication function (related operator)".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "python_fn_add".to_string(),
            content: "def add(a, b):\n    return a + b".to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("python".to_string()),
                token_count: 12,
                tags: vec!["function".into(), "arithmetic".into()],
                description: "Python addition (different syntax, same logic)".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "rust_sorting".to_string(),
            content: "fn bubble_sort(arr: &mut [i32]) {\n    \
                     for i in 0..arr.len() {\n        \
                     for j in 0..arr.len()-1-i {\n            \
                     if arr[j] > arr[j+1] { arr.swap(j, j+1); }\n        \
                     }\n    }\n}"
                .to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 45,
                tags: vec!["algorithm".into(), "sorting".into()],
                description: "Bubble sort algorithm (different domain)".into(),
            },
            embedding: None,
        },
        // === LOW SIMILARITY PAIRS ===
        TestDocument {
            id: "prose_cooking".to_string(),
            content: "Preheat the oven to 350 degrees. Mix flour, sugar, and eggs \
                     in a large bowl until smooth. Pour into a greased pan and \
                     bake for 30 minutes."
                .to_string(),
            category: DocumentCategory::Prose,
            metadata: DocumentMetadata {
                language: None,
                token_count: 32,
                tags: vec!["cooking".into(), "recipe".into()],
                description: "Baking recipe (completely unrelated)".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "prose_philosophy".to_string(),
            content: "The fundamental nature of reality has puzzled philosophers \
                     for millennia. Metaphysical questions about existence, \
                     consciousness, and the nature of being remain central to \
                     philosophical inquiry."
                .to_string(),
            category: DocumentCategory::Prose,
            metadata: DocumentMetadata {
                language: None,
                token_count: 28,
                tags: vec!["philosophy".into(), "metaphysics".into()],
                description: "Philosophical prose (unrelated to code)".into(),
            },
            embedding: None,
        },
        // === EDGE CASES ===
        // Note: Empty string omitted - Ollama API doesn't support empty embeddings
        TestDocument {
            id: "edge_unicode".to_string(),
            content: "fn 测试(数据: i32) -> i32 { 数据 * 2 } // Unicode identifiers".to_string(),
            category: DocumentCategory::EdgeCase,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 20,
                tags: vec!["edge-case".into(), "unicode".into()],
                description: "Code with Unicode identifiers".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "edge_very_long".to_string(),
            content: {
                // Simulate a long document (~3K tokens, within model limits)
                let base =
                    "This is a very long document that tests the model's handling of extended content. \
                     It contains multiple sentences with varied vocabulary to ensure semantic richness. \
                     The purpose is to verify that the embedding system can handle longer documents \
                     without performance degradation or errors. ";
                base.repeat(50) // ~3K tokens
            },
            category: DocumentCategory::EdgeCase,
            metadata: DocumentMetadata {
                language: None,
                token_count: 3000,
                tags: vec!["edge-case".into(), "long".into()],
                description: "Long document (~3K tokens)".into(),
            },
            embedding: None,
        },
        TestDocument {
            id: "mixed_code_comment".to_string(),
            content: "// This function calculates the factorial of a number\n\
                     // using recursive algorithm\n\
                     fn factorial(n: u32) -> u32 {\n    \
                     if n <= 1 { 1 } else { n * factorial(n - 1) }\n\
                     }"
                .to_string(),
            category: DocumentCategory::Mixed,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 35,
                tags: vec!["code".into(), "comments".into(), "recursion".into()],
                description: "Code with extensive comments".into(),
            },
            embedding: None,
        },
    ];

    let expectations = vec![
        // High similarity
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "rust_fn_sum".into(),
            expected_range: SimilarityRange::HIGH,
            rationale: "Identical logic with different variable names".into(),
        },
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "rust_doc_addition".into(),
            expected_range: SimilarityRange::MEDIUM,
            rationale: "Code and its natural language description (0.62)".into(),
        },
        // Medium/High similarity - arithmetic operations
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "rust_fn_multiply".into(),
            expected_range: SimilarityRange::HIGH,
            rationale: "Related arithmetic operations (0.86)".into(),
        },
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "python_fn_add".into(),
            expected_range: SimilarityRange::HIGH,
            rationale: "Same logic, different language syntax (0.75)".into(),
        },
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "rust_sorting".into(),
            expected_range: SimilarityRange::MEDIUM,
            rationale: "Both Rust code but different algorithms".into(),
        },
        // Medium similarity - code vs prose
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "prose_cooking".into(),
            expected_range: SimilarityRange::MEDIUM,
            rationale: "Code vs unrelated prose (0.44)".into(),
        },
        // Low-Medium boundary (0.3053 is just above LOW threshold)
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "prose_philosophy".into(),
            expected_range: SimilarityRange::MEDIUM,
            rationale: "Code vs philosophical prose (0.31) - borderline".into(),
        },
        SimilarityExpectation {
            doc_a: "prose_cooking".into(),
            doc_b: "prose_philosophy".into(),
            expected_range: SimilarityRange::LOW,
            rationale: "Unrelated prose topics".into(),
        },
        // Edge cases - Unicode code
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "edge_unicode".into(),
            expected_range: SimilarityRange::HIGH,
            rationale: "Both Rust code despite Unicode (0.72)".into(),
        },
        SimilarityExpectation {
            doc_a: "rust_fn_add".into(),
            doc_b: "mixed_code_comment".into(),
            expected_range: SimilarityRange::MEDIUM,
            rationale: "Code with heavy comments still code-like".into(),
        },
    ];

    SemanticTestCorpus {
        documents,
        expectations,
        metadata: CorpusMetadata {
            model: "nomic-embed-text-v1.5-q8_0".into(),
            endpoint: "https://llama.krohnos.io".into(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            schema_version: 1,
            dimensions: 768,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corpus_builder() {
        let corpus = build_sample_corpus();

        // Verify we have expected number of documents
        assert_eq!(corpus.documents.len(), 11);

        // Verify we have expectations
        assert_eq!(corpus.expectations.len(), 10);

        // Verify all documents have unique IDs
        let mut ids: Vec<_> = corpus.documents.iter().map(|d| &d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), corpus.documents.len());

        // Verify all expectations reference valid documents
        for exp in &corpus.expectations {
            assert!(
                corpus.documents.iter().any(|d| d.id == exp.doc_a),
                "Document {} referenced in expectation not found",
                exp.doc_a
            );
            assert!(
                corpus.documents.iter().any(|d| d.id == exp.doc_b),
                "Document {} referenced in expectation not found",
                exp.doc_b
            );
        }

        // Verify metadata
        assert_eq!(corpus.metadata.model, "nomic-embed-text-v1.5-q8_0");
        assert_eq!(corpus.metadata.endpoint, "https://llama.krohnos.io");
        assert_eq!(corpus.metadata.dimensions, 768);
        assert_eq!(corpus.metadata.schema_version, 1);
    }

    #[test]
    fn test_corpus_has_all_categories() {
        let corpus = build_sample_corpus();

        let has_code = corpus
            .documents
            .iter()
            .any(|d| d.category == DocumentCategory::Code);
        let has_docs = corpus
            .documents
            .iter()
            .any(|d| d.category == DocumentCategory::Documentation);
        let has_prose = corpus
            .documents
            .iter()
            .any(|d| d.category == DocumentCategory::Prose);
        let has_mixed = corpus
            .documents
            .iter()
            .any(|d| d.category == DocumentCategory::Mixed);
        let has_edge = corpus
            .documents
            .iter()
            .any(|d| d.category == DocumentCategory::EdgeCase);

        assert!(has_code);
        assert!(has_docs);
        assert!(has_prose);
        assert!(has_mixed);
        assert!(has_edge);
    }
}
