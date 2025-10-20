// crates/crucible-daemon/tests/utils/semantic_assertions.rs

//! Test utilities for semantic similarity assertions

use crate::fixtures::semantic_corpus::{
    SemanticTestCorpus, SimilarityExpectation, SimilarityRange, TestDocument,
};

/// Calculate cosine similarity between two embedding vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "Vectors must have same dimension (got {} and {})",
        a.len(),
        b.len()
    );

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

/// Assert that two documents have similarity within expected range
pub fn assert_similarity_range(
    doc_a: &TestDocument,
    doc_b: &TestDocument,
    expected: SimilarityRange,
) {
    let embedding_a = doc_a
        .embedding
        .as_ref()
        .expect("Document A must have embedding");
    let embedding_b = doc_b
        .embedding
        .as_ref()
        .expect("Document B must have embedding");

    let similarity = cosine_similarity(embedding_a, embedding_b);

    assert!(
        expected.contains(similarity),
        "Similarity {:.4} not in {} range for '{}' vs '{}'\n  Expected: {:?}\n  Got: {:.4}",
        similarity,
        expected.name(),
        doc_a.id,
        doc_b.id,
        expected,
        similarity
    );
}

/// Assert that two documents are highly similar (>0.7)
pub fn assert_similar(doc_a: &TestDocument, doc_b: &TestDocument) {
    assert_similarity_range(doc_a, doc_b, SimilarityRange::HIGH);
}

/// Assert that two documents are dissimilar (<0.3)
pub fn assert_dissimilar(doc_a: &TestDocument, doc_b: &TestDocument) {
    assert_similarity_range(doc_a, doc_b, SimilarityRange::LOW);
}

/// Assert that two documents are moderately similar (0.4-0.7)
pub fn assert_moderately_similar(doc_a: &TestDocument, doc_b: &TestDocument) {
    assert_similarity_range(doc_a, doc_b, SimilarityRange::MEDIUM);
}

/// Helper to find document by ID in corpus
pub fn find_document<'a>(corpus: &'a [TestDocument], id: &str) -> Option<&'a TestDocument> {
    corpus.iter().find(|doc| doc.id == id)
}

/// Batch validation of all expectations in a corpus
pub fn validate_corpus_expectations(corpus: &SemanticTestCorpus) {
    let mut failed = Vec::new();

    for expectation in &corpus.expectations {
        let doc_a = find_document(&corpus.documents, &expectation.doc_a).expect(&format!(
            "Document {} not found in corpus",
            expectation.doc_a
        ));
        let doc_b = find_document(&corpus.documents, &expectation.doc_b).expect(&format!(
            "Document {} not found in corpus",
            expectation.doc_b
        ));

        let embedding_a = doc_a.embedding.as_ref().expect(&format!(
            "Document {} missing embedding",
            doc_a.id
        ));
        let embedding_b = doc_b.embedding.as_ref().expect(&format!(
            "Document {} missing embedding",
            doc_b.id
        ));

        let similarity = cosine_similarity(embedding_a, embedding_b);

        if !expectation.expected_range.contains(similarity) {
            failed.push((expectation.clone(), similarity));
        }
    }

    if !failed.is_empty() {
        eprintln!("\n❌ Failed similarity expectations:");
        for (exp, sim) in &failed {
            eprintln!(
                "  {} <-> {}: expected {}, got {:.4}",
                exp.doc_a,
                exp.doc_b,
                exp.expected_range.name(),
                sim
            );
            eprintln!("    Rationale: {}", exp.rationale);
        }
        panic!("\n{} similarity expectations failed", failed.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::semantic_corpus::DocumentCategory;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    #[should_panic(expected = "Vectors must have same dimension")]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        cosine_similarity(&a, &b);
    }

    #[test]
    fn test_find_document() {
        use crate::fixtures::semantic_corpus::DocumentMetadata;

        let docs = vec![
            TestDocument {
                id: "doc1".to_string(),
                content: "content1".to_string(),
                category: DocumentCategory::Code,
                metadata: DocumentMetadata {
                    language: None,
                    token_count: 5,
                    tags: vec![],
                    description: "test".to_string(),
                },
                embedding: None,
            },
            TestDocument {
                id: "doc2".to_string(),
                content: "content2".to_string(),
                category: DocumentCategory::Code,
                metadata: DocumentMetadata {
                    language: None,
                    token_count: 5,
                    tags: vec![],
                    description: "test".to_string(),
                },
                embedding: None,
            },
        ];

        assert!(find_document(&docs, "doc1").is_some());
        assert!(find_document(&docs, "doc2").is_some());
        assert!(find_document(&docs, "doc3").is_none());
    }
}
