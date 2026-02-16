//! Backend comparison tests
//!
//! These tests compare embedding quality and consistency across different backends:
//! - FastEmbed (ONNX runtime, CPU)
//! - Ollama (remote GPU)
//!
//! Run with:
//! ```bash
//! cargo test -p crucible-llm --test test_backend_comparison -- --ignored --nocapture
//! ```

/// Test texts for semantic similarity comparison
const TEST_TEXTS: &[&str] = &[
    // Similar pairs (cat theme)
    "The cat sat on the mat.",
    "A feline was resting on the rug.",
    // Similar pairs (programming theme)
    "Rust is a systems programming language focused on safety.",
    "The Rust language prioritizes memory safety and performance.",
    // Dissimilar text
    "The stock market crashed yesterday.",
    "Apple pie is delicious with ice cream.",
];

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimensions");
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

/// L2 normalize a vector
fn normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter().map(|x| x / norm).collect()
    } else {
        v.to_vec()
    }
}

mod fastembed_tests {
    use super::*;
    use crucible_llm::embeddings::{create_provider, EmbeddingConfig};

    #[tokio::test]
    #[ignore = "Downloads ONNX models (~100MB)"]
    async fn test_fastembed_basic() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        let embedding = provider.embed("Hello, world!").await.unwrap();

        println!("FastEmbed dimensions: {}", embedding.len());
        println!(
            "First 5 values: {:?}",
            &embedding[..5.min(embedding.len())]
        );

        assert!(!embedding.is_empty());
        assert!(embedding.len() > 100); // Should be high-dimensional
    }

    #[tokio::test]
    #[ignore = "Downloads ONNX models (~100MB)"]
    async fn test_fastembed_semantic_similarity() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        // Get embeddings for all test texts
        let mut embeddings = Vec::new();
        for text in TEST_TEXTS {
            let embedding = provider.embed(text).await.unwrap();
            embeddings.push(normalize(&embedding));
        }

        // Calculate similarity matrix
        println!("\nFastEmbed Similarity Matrix:");
        println!(
            "    {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            "T0", "T1", "T2", "T3", "T4", "T5"
        );
        for i in 0..embeddings.len() {
            print!("T{}: ", i);
            for j in 0..embeddings.len() {
                let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
                print!("{:>6.3} ", sim);
            }
            println!();
        }

        // Verify semantic relationships
        let cat_sim = cosine_similarity(&embeddings[0], &embeddings[1]);
        let rust_sim = cosine_similarity(&embeddings[2], &embeddings[3]);
        let cat_stock = cosine_similarity(&embeddings[0], &embeddings[4]);
        let rust_pie = cosine_similarity(&embeddings[2], &embeddings[5]);

        println!("\nSemantic similarity checks:");
        println!("  Cat sentences: {:.3}", cat_sim);
        println!("  Rust sentences: {:.3}", rust_sim);
        println!("  Cat vs Stock: {:.3}", cat_stock);
        println!("  Rust vs Pie: {:.3}", rust_pie);

        // Similar texts should have higher similarity than dissimilar
        assert!(
            cat_sim > cat_stock,
            "Similar cat texts should be more similar than cat vs stock"
        );
        assert!(
            rust_sim > rust_pie,
            "Similar Rust texts should be more similar than Rust vs pie"
        );
    }

    #[tokio::test]
    #[ignore = "Downloads ONNX models (~100MB)"]
    async fn test_fastembed_batch() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        let start = std::time::Instant::now();
        let responses = provider.embed_batch(TEST_TEXTS).await.unwrap();
        let elapsed = start.elapsed();

        println!(
            "FastEmbed batch of {} texts took {:?}",
            responses.len(),
            elapsed
        );
        println!(
            "Throughput: {:.1} embeddings/sec",
            responses.len() as f64 / elapsed.as_secs_f64()
        );

        assert_eq!(responses.len(), TEST_TEXTS.len());
    }
}

/// Ollama backend tests
mod ollama_tests {
    use super::*;
    use crucible_llm::embeddings::{create_provider, EmbeddingConfig};

    const OLLAMA_ENDPOINT: &str = "https://llama.krohnos.io";
    const OLLAMA_MODEL: &str = "nomic-embed-text-v1.5-q8_0";

    #[tokio::test]
    #[ignore = "Requires Ollama server"]
    async fn test_ollama_basic() {
        let config = EmbeddingConfig::ollama(
            Some(OLLAMA_ENDPOINT.to_string()),
            Some(OLLAMA_MODEL.to_string()),
        );
        let provider = create_provider(config).await.unwrap();

        let embedding = provider.embed("Hello, world!").await.unwrap();

        println!("Ollama dimensions: {}", embedding.len());
        println!(
            "First 5 values: {:?}",
            &embedding[..5.min(embedding.len())]
        );

        assert!(!embedding.is_empty());
        assert_eq!(embedding.len(), 768); // nomic-embed-text has 768 dims
    }

    #[tokio::test]
    #[ignore = "Requires Ollama server"]
    async fn test_ollama_semantic_similarity() {
        let config = EmbeddingConfig::ollama(
            Some(OLLAMA_ENDPOINT.to_string()),
            Some(OLLAMA_MODEL.to_string()),
        );
        let provider = create_provider(config).await.unwrap();

        // Get embeddings for all test texts
        let mut embeddings = Vec::new();
        for text in TEST_TEXTS {
            let embedding = provider.embed(text).await.unwrap();
            embeddings.push(normalize(&embedding));
        }

        // Calculate similarity matrix
        println!("\nOllama Similarity Matrix:");
        println!(
            "    {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            "T0", "T1", "T2", "T3", "T4", "T5"
        );
        for i in 0..embeddings.len() {
            print!("T{}: ", i);
            for j in 0..embeddings.len() {
                let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
                print!("{:>6.3} ", sim);
            }
            println!();
        }

        // Verify semantic relationships
        let cat_sim = cosine_similarity(&embeddings[0], &embeddings[1]);
        let rust_sim = cosine_similarity(&embeddings[2], &embeddings[3]);
        let cat_stock = cosine_similarity(&embeddings[0], &embeddings[4]);
        let rust_pie = cosine_similarity(&embeddings[2], &embeddings[5]);

        println!("\nSemantic similarity checks:");
        println!("  Cat sentences: {:.3}", cat_sim);
        println!("  Rust sentences: {:.3}", rust_sim);
        println!("  Cat vs Stock: {:.3}", cat_stock);
        println!("  Rust vs Pie: {:.3}", rust_pie);

        // Similar texts should have higher similarity than dissimilar
        assert!(
            cat_sim > cat_stock,
            "Similar cat texts should be more similar than cat vs stock"
        );
        assert!(
            rust_sim > rust_pie,
            "Similar Rust texts should be more similar than Rust vs pie"
        );
    }

    #[tokio::test]
    #[ignore = "Requires Ollama server"]
    async fn test_ollama_batch_throughput() {
        let config = EmbeddingConfig::ollama(
            Some(OLLAMA_ENDPOINT.to_string()),
            Some(OLLAMA_MODEL.to_string()),
        );
        let provider = create_provider(config).await.unwrap();

        let texts: Vec<String> = (0..50)
            .map(|i| {
                format!(
                    "This is test sentence number {} for throughput benchmarking.",
                    i
                )
            })
            .collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        // Warm up
        let _ = provider.embed("warmup").await;

        let start = std::time::Instant::now();
        let responses = provider.embed_batch(&text_refs).await.unwrap();
        let elapsed = start.elapsed();

        println!(
            "Ollama batch of {} texts took {:?}",
            responses.len(),
            elapsed
        );
        println!(
            "Throughput: {:.1} embeddings/sec",
            responses.len() as f64 / elapsed.as_secs_f64()
        );

        assert_eq!(responses.len(), texts.len());
    }
}
