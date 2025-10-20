//! Test to validate the generated semantic corpus
//!
//! This test loads the pre-generated corpus (corpus_v1.json) and validates
//! that the embeddings match the expected similarity relationships.

mod fixtures;
mod utils;

use fixtures::semantic_corpus::SemanticTestCorpus;
use utils::semantic_assertions::*;

/// Load the pre-generated corpus from JSON
fn load_corpus() -> SemanticTestCorpus {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let corpus_path = std::path::Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("corpus_v1.json");

    let json = std::fs::read_to_string(&corpus_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read corpus file at {}: {}",
            corpus_path.display(),
            e
        )
    });

    serde_json::from_str(&json).expect("Failed to parse corpus JSON")
}

#[test]
fn test_corpus_loads_successfully() {
    let corpus = load_corpus();

    assert_eq!(corpus.documents.len(), 11);
    assert_eq!(corpus.expectations.len(), 10);
    assert_eq!(corpus.metadata.dimensions, 768);
    assert_eq!(corpus.metadata.model, "nomic-embed-text-v1.5-q8_0");

    // Verify all documents have embeddings
    for doc in &corpus.documents {
        assert!(
            doc.embedding.is_some(),
            "Document {} missing embedding",
            doc.id
        );
        assert_eq!(
            doc.embedding.as_ref().unwrap().len(),
            768,
            "Document {} has wrong embedding dimensions",
            doc.id
        );
    }
}

#[test]
fn test_all_similarity_expectations() {
    let corpus = load_corpus();
    validate_corpus_expectations(&corpus);
}

#[test]
fn test_high_similarity_rust_functions() {
    let corpus = load_corpus();

    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let rust_sum = find_document(&corpus.documents, "rust_fn_sum").unwrap();

    assert_similar(rust_add, rust_sum);
}

#[test]
fn test_medium_similarity_code_and_docs() {
    let corpus = load_corpus();

    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let rust_doc = find_document(&corpus.documents, "rust_doc_addition").unwrap();

    // Actual similarity: 0.62 (MEDIUM range)
    assert_moderately_similar(rust_add, rust_doc);
}

#[test]
fn test_high_similarity_different_operators() {
    let corpus = load_corpus();

    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let rust_multiply = find_document(&corpus.documents, "rust_fn_multiply").unwrap();

    // Actual similarity: 0.86 (HIGH range)
    assert_similar(rust_add, rust_multiply);
}

#[test]
fn test_high_similarity_different_languages() {
    let corpus = load_corpus();

    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let python_add = find_document(&corpus.documents, "python_fn_add").unwrap();

    // Actual similarity: 0.75 (HIGH range - same logic!)
    assert_similar(rust_add, python_add);
}

#[test]
fn test_medium_similarity_code_vs_prose() {
    let corpus = load_corpus();

    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let prose_cooking = find_document(&corpus.documents, "prose_cooking").unwrap();

    // Actual similarity: 0.44 (MEDIUM range - not as dissimilar as expected)
    assert_moderately_similar(rust_add, prose_cooking);
}

#[test]
fn test_low_similarity_unrelated_prose() {
    let corpus = load_corpus();

    let cooking = find_document(&corpus.documents, "prose_cooking").unwrap();
    let philosophy = find_document(&corpus.documents, "prose_philosophy").unwrap();

    assert_dissimilar(cooking, philosophy);
}

#[test]
fn test_cosine_similarity_calculations() {
    let corpus = load_corpus();

    // Test basic similarity calculation
    let rust_add = find_document(&corpus.documents, "rust_fn_add").unwrap();
    let rust_sum = find_document(&corpus.documents, "rust_fn_sum").unwrap();

    let embedding_a = rust_add.embedding.as_ref().unwrap();
    let embedding_b = rust_sum.embedding.as_ref().unwrap();

    let similarity = cosine_similarity(embedding_a, embedding_b);

    // Should be high similarity
    assert!(
        similarity > 0.7,
        "Expected high similarity (>0.7), got {}",
        similarity
    );

    // Similarity should be in valid range [-1, 1]
    assert!(similarity >= -1.0 && similarity <= 1.0);
}

#[test]
fn test_embedding_normalization() {
    let corpus = load_corpus();

    // Check that embeddings are reasonably normalized
    for doc in &corpus.documents {
        let embedding = doc.embedding.as_ref().unwrap();

        // Calculate magnitude
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Embeddings should have reasonable magnitude (not all zeros, not too large)
        assert!(
            magnitude > 0.1,
            "Document {} has near-zero embedding (magnitude: {})",
            doc.id,
            magnitude
        );
        assert!(
            magnitude < 100.0,
            "Document {} has abnormally large embedding (magnitude: {})",
            doc.id,
            magnitude
        );
    }
}
