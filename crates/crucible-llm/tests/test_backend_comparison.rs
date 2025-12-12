//! Backend comparison tests
//!
//! These tests compare embedding quality and consistency across different backends:
//! - FastEmbed (ONNX runtime, CPU)
//! - LlamaCpp (llama.cpp, Vulkan GPU)
//!
//! Run with:
//! ```bash
//! cargo test -p crucible-llm --features llama-cpp-vulkan --test test_backend_comparison -- --ignored --nocapture
//! ```
//!
//! Note: GPU tests are marked with `#[serial]` to prevent Vulkan context contention
//! when running tests in parallel.

use serial_test::serial;

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
    async fn test_fastembed_basic() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        let response = provider.embed("Hello, world!").await.unwrap();

        println!("FastEmbed dimensions: {}", response.embedding.len());
        println!(
            "First 5 values: {:?}",
            &response.embedding[..5.min(response.embedding.len())]
        );

        assert!(!response.embedding.is_empty());
        assert!(response.embedding.len() > 100); // Should be high-dimensional
    }

    #[tokio::test]
    async fn test_fastembed_semantic_similarity() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        // Get embeddings for all test texts
        let mut embeddings = Vec::new();
        for text in TEST_TEXTS {
            let response = provider.embed(text).await.unwrap();
            embeddings.push(normalize(&response.embedding));
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
    async fn test_fastembed_batch() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let provider = create_provider(config).await.unwrap();

        let texts: Vec<String> = TEST_TEXTS.iter().map(|s| s.to_string()).collect();
        let start = std::time::Instant::now();
        let responses = provider.embed_batch(texts).await.unwrap();
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

#[cfg(feature = "llama-cpp")]
mod llama_cpp_tests {
    use super::*;
    use crucible_llm::embeddings::inference::DeviceType;
    use crucible_llm::embeddings::llama_cpp_backend::LlamaCppBackend;
    use crucible_llm::embeddings::EmbeddingProvider;
    use std::path::{Path, PathBuf};

    const NOMIC_V15_MODEL: &str = "/home/moot/models/language/nomic-ai/nomic-embed-text-v1.5-GGUF/nomic-embed-text-v1.5.Q8_0.gguf";
    const NOMIC_V2_MODEL: &str = "/home/moot/models/language/nomic-ai/nomic-embed-text-v2-moe-GGUF/nomic-embed-text-v2-moe.Q4_K_M.gguf";

    fn get_model_path() -> Option<&'static str> {
        if Path::new(NOMIC_V15_MODEL).exists() {
            Some(NOMIC_V15_MODEL)
        } else if Path::new(NOMIC_V2_MODEL).exists() {
            Some(NOMIC_V2_MODEL)
        } else {
            None
        }
    }

    /// Test runtime device detection - no model required
    #[test]
    fn test_list_available_devices() {
        let devices = LlamaCppBackend::list_available_devices();

        println!("\n=== Available Compute Devices ===");
        for (i, dev) in devices.iter().enumerate() {
            println!("Device {}:", i);
            println!("  Name: {}", dev.name);
            println!("  Description: {}", dev.description);
            println!("  Backend: {}", dev.backend);
            println!(
                "  Memory: {:.2} GB total, {:.2} GB free",
                dev.memory_total as f64 / 1e9,
                dev.memory_free as f64 / 1e9
            );
            println!("  Type: {:?}", dev.device_type);
        }

        // Should at least have CPU
        assert!(
            !devices.is_empty(),
            "Should detect at least one device (CPU)"
        );
    }

    #[tokio::test]
    #[ignore = "Requires GGUF model"]
    #[serial]
    async fn test_llama_cpp_basic() {
        let model_path = match get_model_path() {
            Some(p) => p,
            None => {
                eprintln!("No GGUF model found, skipping test");
                return;
            }
        };

        // Use EmbeddingProvider interface with background loading
        let provider =
            LlamaCppBackend::new_with_model(PathBuf::from(model_path), DeviceType::Auto).unwrap();

        let response = provider.embed("Hello, world!").await.unwrap();

        println!("LlamaCpp dimensions: {}", response.embedding.len());
        println!(
            "First 5 values: {:?}",
            &response.embedding[..5.min(response.embedding.len())]
        );
        println!("Model: {}", provider.model_name());
        println!("Provider: {}", provider.provider_name());

        assert!(!response.embedding.is_empty());
        assert!(response.embedding.len() > 100);
    }

    #[tokio::test]
    #[ignore = "Requires GGUF model"]
    #[serial]
    async fn test_llama_cpp_semantic_similarity() {
        let model_path = match get_model_path() {
            Some(p) => p,
            None => {
                eprintln!("No GGUF model found, skipping test");
                return;
            }
        };

        let provider =
            LlamaCppBackend::new_with_model(PathBuf::from(model_path), DeviceType::Auto).unwrap();

        // Get embeddings for all test texts using EmbeddingProvider trait
        let mut embeddings = Vec::new();
        for text in TEST_TEXTS {
            let response = provider.embed(text).await.unwrap();
            embeddings.push(normalize(&response.embedding));
        }

        // Calculate similarity matrix
        println!("\nLlamaCpp Similarity Matrix:");
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
    #[ignore = "Requires GGUF model"]
    #[serial]
    async fn test_llama_cpp_throughput() {
        let model_path = match get_model_path() {
            Some(p) => p,
            None => {
                eprintln!("No GGUF model found, skipping test");
                return;
            }
        };

        let provider =
            LlamaCppBackend::new_with_model(PathBuf::from(model_path), DeviceType::Auto).unwrap();

        // Warm up
        let _ = provider.embed("warmup").await.unwrap();

        // Benchmark using batch API
        let texts: Vec<String> = TEST_TEXTS.iter().map(|s| s.to_string()).collect();
        let iterations = 10;

        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let _ = provider.embed_batch(texts.clone()).await.unwrap();
        }
        let elapsed = start.elapsed();

        let total_embeddings = texts.len() * iterations;
        let throughput = total_embeddings as f64 / elapsed.as_secs_f64();

        println!("LlamaCpp: {} embeddings in {:?}", total_embeddings, elapsed);
        println!("Throughput: {:.1} embeddings/sec", throughput);
    }

    #[tokio::test]
    #[ignore = "Requires GGUF model"]
    #[serial]
    async fn test_llama_cpp_batch() {
        let model_path = match get_model_path() {
            Some(p) => p,
            None => {
                eprintln!("No GGUF model found, skipping test");
                return;
            }
        };

        let provider =
            LlamaCppBackend::new_with_model(PathBuf::from(model_path), DeviceType::Auto).unwrap();

        let texts: Vec<String> = TEST_TEXTS.iter().map(|s| s.to_string()).collect();
        let start = std::time::Instant::now();
        let responses = provider.embed_batch(texts.clone()).await.unwrap();
        let elapsed = start.elapsed();

        println!(
            "LlamaCpp batch of {} texts took {:?}",
            responses.len(),
            elapsed
        );
        println!(
            "Throughput: {:.1} embeddings/sec",
            responses.len() as f64 / elapsed.as_secs_f64()
        );

        assert_eq!(responses.len(), TEST_TEXTS.len());
        for response in &responses {
            assert!(!response.embedding.is_empty());
        }
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

        let response = provider.embed("Hello, world!").await.unwrap();

        println!("Ollama dimensions: {}", response.embedding.len());
        println!(
            "First 5 values: {:?}",
            &response.embedding[..5.min(response.embedding.len())]
        );

        assert!(!response.embedding.is_empty());
        assert_eq!(response.embedding.len(), 768); // nomic-embed-text has 768 dims
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
            let response = provider.embed(text).await.unwrap();
            embeddings.push(normalize(&response.embedding));
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

        // Warm up
        let _ = provider.embed("warmup").await;

        let start = std::time::Instant::now();
        let responses = provider.embed_batch(texts.clone()).await.unwrap();
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

/// Compare all three backends side by side
#[cfg(feature = "llama-cpp")]
mod comparison_tests {
    use super::*;
    use crucible_llm::embeddings::inference::DeviceType;
    use crucible_llm::embeddings::llama_cpp_backend::LlamaCppBackend;
    use crucible_llm::embeddings::{create_provider, EmbeddingConfig, EmbeddingProvider};
    use std::path::{Path, PathBuf};

    const NOMIC_V15_MODEL: &str = "/home/moot/models/language/nomic-ai/nomic-embed-text-v1.5-GGUF/nomic-embed-text-v1.5.Q8_0.gguf";
    const OLLAMA_ENDPOINT: &str = "https://llama.krohnos.io";
    const OLLAMA_MODEL: &str = "nomic-embed-text-v1.5-q8_0";

    #[tokio::test]
    #[ignore = "Requires GGUF model and runs both backends"]
    #[serial]
    async fn test_backend_comparison() {
        // Skip if model not available
        if !Path::new(NOMIC_V15_MODEL).exists() {
            eprintln!("Model not found, skipping comparison test");
            return;
        }

        println!("\n=== Backend Comparison Test ===\n");

        // FastEmbed setup
        let fastembed_config = EmbeddingConfig::fastembed(None, None, None);
        let fastembed = create_provider(fastembed_config).await.unwrap();

        // LlamaCpp setup using EmbeddingProvider interface
        let llama_cpp =
            LlamaCppBackend::new_with_model(PathBuf::from(NOMIC_V15_MODEL), DeviceType::Auto)
                .unwrap();

        // Test texts
        let texts = vec![
            "The quick brown fox jumps over the lazy dog.",
            "A fast auburn canine leaps above the sleepy hound.",
            "Machine learning enables computers to learn from data.",
            "The weather forecast predicts rain tomorrow.",
        ];

        println!("Test sentences:");
        for (i, text) in texts.iter().enumerate() {
            println!("  T{}: {}", i, text);
        }

        // Get embeddings from both backends using EmbeddingProvider trait
        println!("\n--- FastEmbed ---");
        let mut fastembed_embeddings = Vec::new();
        let fe_start = std::time::Instant::now();
        for text in &texts {
            let response = fastembed.embed(text).await.unwrap();
            fastembed_embeddings.push(normalize(&response.embedding));
        }
        let fe_elapsed = fe_start.elapsed();
        println!("Dimensions: {}", fastembed_embeddings[0].len());
        println!(
            "Time: {:?} ({:.1} emb/sec)",
            fe_elapsed,
            texts.len() as f64 / fe_elapsed.as_secs_f64()
        );

        println!("\n--- LlamaCpp (GPU) ---");
        let mut llama_embeddings = Vec::new();
        let llama_start = std::time::Instant::now();
        for text in &texts {
            let response = llama_cpp.embed(text).await.unwrap();
            llama_embeddings.push(normalize(&response.embedding));
        }
        let llama_elapsed = llama_start.elapsed();
        println!("Dimensions: {}", llama_embeddings[0].len());
        println!(
            "Time: {:?} ({:.1} emb/sec)",
            llama_elapsed,
            texts.len() as f64 / llama_elapsed.as_secs_f64()
        );

        // Compare similarity matrices
        println!("\n--- Similarity Comparison ---");
        println!("\nFastEmbed similarities:");
        print_similarity_matrix(&fastembed_embeddings);

        println!("\nLlamaCpp similarities:");
        print_similarity_matrix(&llama_embeddings);

        // Check semantic quality for both
        println!("\n--- Semantic Quality Check ---");

        // Similar pair (fox/canine sentences)
        let fe_similar = cosine_similarity(&fastembed_embeddings[0], &fastembed_embeddings[1]);
        let llama_similar = cosine_similarity(&llama_embeddings[0], &llama_embeddings[1]);

        // Dissimilar pair (fox vs weather)
        let fe_dissimilar = cosine_similarity(&fastembed_embeddings[0], &fastembed_embeddings[3]);
        let llama_dissimilar = cosine_similarity(&llama_embeddings[0], &llama_embeddings[3]);

        println!(
            "FastEmbed:  similar={:.3}, dissimilar={:.3}, delta={:.3}",
            fe_similar,
            fe_dissimilar,
            fe_similar - fe_dissimilar
        );
        println!(
            "LlamaCpp:   similar={:.3}, dissimilar={:.3}, delta={:.3}",
            llama_similar,
            llama_dissimilar,
            llama_similar - llama_dissimilar
        );

        // Both should show semantic understanding
        assert!(
            fe_similar > fe_dissimilar,
            "FastEmbed should rank similar texts higher"
        );
        assert!(
            llama_similar > llama_dissimilar,
            "LlamaCpp should rank similar texts higher"
        );

        println!("\n=== Both backends show proper semantic understanding ===");
    }

    fn print_similarity_matrix(embeddings: &[Vec<f32>]) {
        print!("    ");
        for i in 0..embeddings.len() {
            print!("{:>6} ", format!("T{}", i));
        }
        println!();

        for i in 0..embeddings.len() {
            print!("T{}: ", i);
            for j in 0..embeddings.len() {
                let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
                print!("{:>6.3} ", sim);
            }
            println!();
        }
    }

    #[tokio::test]
    #[ignore = "Throughput benchmark requiring GGUF model"]
    #[serial]
    async fn test_throughput_comparison() {
        if !Path::new(NOMIC_V15_MODEL).exists() {
            eprintln!("Model not found, skipping throughput test");
            return;
        }

        println!("\n=== Throughput Comparison ===\n");

        // Setup
        let fastembed_config = EmbeddingConfig::fastembed(None, None, None);
        let fastembed = create_provider(fastembed_config).await.unwrap();

        let llama_cpp =
            LlamaCppBackend::new_with_model(PathBuf::from(NOMIC_V15_MODEL), DeviceType::Auto)
                .unwrap();

        let test_count = 50;
        let texts: Vec<String> = (0..test_count)
            .map(|i| {
                format!(
                    "This is test sentence number {} for throughput benchmarking.",
                    i
                )
            })
            .collect();

        // Warm up
        let _ = fastembed.embed("warmup").await;
        let _ = llama_cpp.embed("warmup").await;

        // FastEmbed benchmark
        let fe_start = std::time::Instant::now();
        let _ = fastembed.embed_batch(texts.clone()).await.unwrap();
        let fe_elapsed = fe_start.elapsed();
        let fe_throughput = test_count as f64 / fe_elapsed.as_secs_f64();

        // LlamaCpp benchmark (batched via EmbeddingProvider)
        let llama_start = std::time::Instant::now();
        let _ = llama_cpp.embed_batch(texts.clone()).await.unwrap();
        let llama_elapsed = llama_start.elapsed();
        let llama_throughput = test_count as f64 / llama_elapsed.as_secs_f64();

        println!(
            "FastEmbed:  {} embeddings in {:?} ({:.1} emb/sec)",
            test_count, fe_elapsed, fe_throughput
        );
        println!(
            "LlamaCpp:   {} embeddings in {:?} ({:.1} emb/sec)",
            test_count, llama_elapsed, llama_throughput
        );

        let speedup = if llama_throughput > fe_throughput {
            format!("LlamaCpp {:.1}x faster", llama_throughput / fe_throughput)
        } else {
            format!("FastEmbed {:.1}x faster", fe_throughput / llama_throughput)
        };
        println!("\nResult: {}", speedup);
    }

    /// Compare all three backends: FastEmbed, LlamaCpp (local GPU), Ollama (remote GPU)
    #[tokio::test]
    #[ignore = "Full 3-way comparison requiring GGUF model and Ollama server"]
    #[serial]
    async fn test_three_way_comparison() {
        if !Path::new(NOMIC_V15_MODEL).exists() {
            eprintln!("Local model not found, skipping test");
            return;
        }

        println!("\n=== Three-Way Backend Comparison ===\n");
        println!("Models:");
        println!("  FastEmbed: BAAI/bge-small-en-v1.5 (33M params, 384 dims)");
        println!("  LlamaCpp:  nomic-embed-text-v1.5 Q8_0 (137M params, 768 dims)");
        println!("  Ollama:    nomic-embed-text-v1.5 Q8_0 (137M params, 768 dims)");
        println!();

        let test_count = 50;
        let texts: Vec<String> = (0..test_count)
            .map(|i| {
                format!(
                    "This is test sentence number {} for throughput benchmarking.",
                    i
                )
            })
            .collect();

        // FastEmbed setup and benchmark
        println!("--- FastEmbed (ONNX, CPU) ---");
        let fastembed_config = EmbeddingConfig::fastembed(None, None, None);
        let fastembed = create_provider(fastembed_config).await.unwrap();
        let _ = fastembed.embed("warmup").await;

        let fe_start = std::time::Instant::now();
        let _ = fastembed.embed_batch(texts.clone()).await.unwrap();
        let fe_elapsed = fe_start.elapsed();
        let fe_throughput = test_count as f64 / fe_elapsed.as_secs_f64();
        println!(
            "  {} embeddings in {:?} ({:.1} emb/sec)",
            test_count, fe_elapsed, fe_throughput
        );

        // LlamaCpp setup and benchmark using EmbeddingProvider
        println!("\n--- LlamaCpp (Vulkan, Local GPU) ---");
        let llama_cpp =
            LlamaCppBackend::new_with_model(PathBuf::from(NOMIC_V15_MODEL), DeviceType::Auto)
                .unwrap();
        let _ = llama_cpp.embed("warmup").await;

        let llama_start = std::time::Instant::now();
        let _ = llama_cpp.embed_batch(texts.clone()).await.unwrap();
        let llama_elapsed = llama_start.elapsed();
        let llama_throughput = test_count as f64 / llama_elapsed.as_secs_f64();
        println!(
            "  {} embeddings in {:?} ({:.1} emb/sec)",
            test_count, llama_elapsed, llama_throughput
        );

        // Ollama setup and benchmark
        println!("\n--- Ollama (Remote GPU via HTTPS) ---");
        let ollama_config = EmbeddingConfig::ollama(
            Some(OLLAMA_ENDPOINT.to_string()),
            Some(OLLAMA_MODEL.to_string()),
        );
        match create_provider(ollama_config).await {
            Ok(ollama) => {
                let _ = ollama.embed("warmup").await;

                let ollama_start = std::time::Instant::now();
                let _ = ollama.embed_batch(texts.clone()).await.unwrap();
                let ollama_elapsed = ollama_start.elapsed();
                let ollama_throughput = test_count as f64 / ollama_elapsed.as_secs_f64();
                println!(
                    "  {} embeddings in {:?} ({:.1} emb/sec)",
                    test_count, ollama_elapsed, ollama_throughput
                );

                // Summary
                println!("\n=== Summary ===");
                println!("  FastEmbed: {:.1} emb/sec (baseline)", fe_throughput);
                println!(
                    "  LlamaCpp:  {:.1} emb/sec ({:.1}x vs FastEmbed)",
                    llama_throughput,
                    llama_throughput / fe_throughput
                );
                println!(
                    "  Ollama:    {:.1} emb/sec ({:.1}x vs FastEmbed)",
                    ollama_throughput,
                    ollama_throughput / fe_throughput
                );
            }
            Err(e) => {
                println!("  Ollama not available: {}", e);
                println!("\n=== Summary (without Ollama) ===");
                println!("  FastEmbed: {:.1} emb/sec", fe_throughput);
                println!(
                    "  LlamaCpp:  {:.1} emb/sec ({:.1}x)",
                    llama_throughput,
                    llama_throughput / fe_throughput
                );
            }
        }
    }
}
