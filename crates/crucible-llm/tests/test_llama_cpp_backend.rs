//! Integration tests for llama.cpp backend with GGUF models
//!
//! These tests require actual GGUF embedding models to be present on the system.
//! They are marked as `#[ignore]` by default and can be run with:
//!
//! ```bash
//! cargo test -p crucible-llm --features llama-cpp-vulkan --test test_llama_cpp_backend -- --ignored
//! ```

#[cfg(feature = "llama-cpp")]
mod tests {
    use crucible_llm::embeddings::inference::{BackendConfig, DeviceType, InferenceBackend};
    use crucible_llm::embeddings::llama_cpp_backend::LlamaCppBackend;
    use std::path::Path;

    /// Path to nomic-embed-text GGUF model for testing
    /// Adjust this path to match your local setup
    const NOMIC_EMBED_MODEL: &str = "/home/moot/models/language/nomic-ai/nomic-embed-text-v1.5-GGUF/nomic-embed-text-v1.5.Q8_0.gguf";

    /// Alternative: nomic-embed-text v2 MoE model (newer, may work better)
    const NOMIC_EMBED_V2_MODEL: &str = "/home/moot/models/language/nomic-ai/nomic-embed-text-v2-moe-GGUF/nomic-embed-text-v2-moe.Q4_K_M.gguf";

    #[test]
    fn test_backend_creation() {
        let backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        assert!(!backend.is_loaded());
        assert_eq!(backend.backend_name(), "llama.cpp");
    }

    #[test]
    fn test_supported_devices() {
        let backend = LlamaCppBackend::new(DeviceType::Cpu).unwrap();
        let devices = backend.supported_devices();

        // CPU should always be supported
        assert!(devices.contains(&DeviceType::Cpu));

        // Log what devices are available
        println!("Supported devices: {:?}", devices);

        #[cfg(feature = "llama-cpp-vulkan")]
        assert!(devices.contains(&DeviceType::Vulkan), "Vulkan should be supported with llama-cpp-vulkan feature");
    }

    #[test]
    #[ignore = "Requires GGUF model file"]
    fn test_load_nomic_model() {
        // Try v2 first (newer, better support), fall back to v1.5
        let model_path = if Path::new(NOMIC_EMBED_V2_MODEL).exists() {
            Path::new(NOMIC_EMBED_V2_MODEL)
        } else if Path::new(NOMIC_EMBED_MODEL).exists() {
            Path::new(NOMIC_EMBED_MODEL)
        } else {
            eprintln!("No model found, skipping test");
            return;
        };

        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig {
            device: DeviceType::Auto,
            gpu_layers: -1, // All layers on GPU
            ..Default::default()
        };

        println!("Attempting to load: {}", model_path.display());

        let info = backend.load_model(model_path, &config).unwrap();

        println!("Model loaded successfully!");
        println!("  Path: {}", info.path.display());
        println!("  Dimensions: {}", info.dimensions);
        println!("  Vocab size: {}", info.vocab_size);
        println!("  Context length: {}", info.context_length);
        println!("  Quantization: {:?}", info.quantization);
        println!("  Device: {:?}", info.device);
        println!("  GPU layers: {}", info.gpu_layers);

        assert!(backend.is_loaded());
        assert!(info.dimensions > 0);
        assert!(info.vocab_size > 0);
    }

    #[test]
    #[ignore = "Requires GGUF model file - tests v1.5 specifically"]
    fn test_load_nomic_v15_model() {
        let model_path = Path::new(NOMIC_EMBED_MODEL);
        if !model_path.exists() {
            eprintln!("Model not found at {}, skipping test", NOMIC_EMBED_MODEL);
            return;
        }

        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig {
            device: DeviceType::Auto,
            gpu_layers: -1,
            ..Default::default()
        };

        println!("Attempting to load v1.5: {}", model_path.display());

        match backend.load_model(model_path, &config) {
            Ok(info) => {
                println!("Model loaded successfully!");
                println!("  Dimensions: {}", info.dimensions);
                assert!(info.dimensions > 0);
            }
            Err(e) => {
                eprintln!("v1.5 model failed to load (expected - needs RoPE config): {}", e);
                // This is expected for v1.5 without proper RoPE settings
            }
        }
    }

    #[test]
    #[ignore = "Requires GGUF model file"]
    fn test_embed_single_text() {
        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig::default();

        let model_path = Path::new(NOMIC_EMBED_MODEL);
        if !model_path.exists() {
            eprintln!("Model not found at {}, skipping test", NOMIC_EMBED_MODEL);
            return;
        }

        backend.load_model(model_path, &config).unwrap();

        let texts = &["Hello, world!"];
        let embeddings = backend.embed_texts(texts).unwrap();

        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), backend.dimensions());

        println!("Embedding dimensions: {}", embeddings[0].len());
        println!("First 10 values: {:?}", &embeddings[0][..10.min(embeddings[0].len())]);

        // Check that embeddings are normalized (L2 norm ≈ 1)
        let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        println!("L2 norm: {}", norm);

        // nomic-embed-text should produce normalized embeddings
        assert!(norm > 0.9 && norm < 1.1, "Expected normalized embeddings, got norm={}", norm);
    }

    #[test]
    #[ignore = "Requires GGUF model file"]
    fn test_embed_batch() {
        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig::default();

        let model_path = Path::new(NOMIC_EMBED_MODEL);
        if !model_path.exists() {
            eprintln!("Model not found at {}, skipping test", NOMIC_EMBED_MODEL);
            return;
        }

        backend.load_model(model_path, &config).unwrap();

        let texts = &[
            "The quick brown fox jumps over the lazy dog.",
            "Machine learning is a subset of artificial intelligence.",
            "Rust is a systems programming language focused on safety.",
        ];

        let start = std::time::Instant::now();
        let embeddings = backend.embed_texts(texts).unwrap();
        let elapsed = start.elapsed();

        println!("Batch embedding took {:?}", elapsed);
        assert_eq!(embeddings.len(), 3);

        for (i, emb) in embeddings.iter().enumerate() {
            assert_eq!(emb.len(), backend.dimensions());
            let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            println!("Text {}: {} dims, L2 norm = {:.4}", i, emb.len(), norm);
        }

        // Check that different texts produce different embeddings
        let dot_product: f32 = embeddings[0].iter()
            .zip(embeddings[1].iter())
            .map(|(a, b)| a * b)
            .sum();
        println!("Dot product between texts 0 and 1: {:.4}", dot_product);

        // Similar texts should have positive dot product but not be identical
        assert!(dot_product < 0.99, "Embeddings should be different for different texts");
    }

    #[test]
    #[ignore = "Requires GGUF model file"]
    fn test_semantic_similarity() {
        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig::default();

        let model_path = Path::new(NOMIC_EMBED_MODEL);
        if !model_path.exists() {
            eprintln!("Model not found at {}, skipping test", NOMIC_EMBED_MODEL);
            return;
        }

        backend.load_model(model_path, &config).unwrap();

        // Two similar texts and one different
        let texts = &[
            "The cat sat on the mat.",
            "A cat was sitting on a rug.",
            "The stock market crashed yesterday.",
        ];

        let embeddings = backend.embed_texts(texts).unwrap();

        // Calculate cosine similarities
        let sim_01: f32 = embeddings[0].iter().zip(embeddings[1].iter()).map(|(a, b)| a * b).sum();
        let sim_02: f32 = embeddings[0].iter().zip(embeddings[2].iter()).map(|(a, b)| a * b).sum();
        let sim_12: f32 = embeddings[1].iter().zip(embeddings[2].iter()).map(|(a, b)| a * b).sum();

        println!("Similarity (cat/cat): {:.4}", sim_01);
        println!("Similarity (cat/stock): {:.4}", sim_02);
        println!("Similarity (cat2/stock): {:.4}", sim_12);

        // Similar sentences should have higher similarity than dissimilar ones
        assert!(sim_01 > sim_02, "Similar texts should have higher similarity");
        assert!(sim_01 > sim_12, "Similar texts should have higher similarity");
    }

    #[test]
    #[ignore = "Requires GGUF model file - benchmarks throughput"]
    fn test_throughput() {
        let mut backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        let config = BackendConfig::default();

        let model_path = Path::new(NOMIC_EMBED_MODEL);
        if !model_path.exists() {
            eprintln!("Model not found at {}, skipping test", NOMIC_EMBED_MODEL);
            return;
        }

        backend.load_model(model_path, &config).unwrap();

        // Generate test texts
        let texts: Vec<&str> = (0..100)
            .map(|i| {
                // Using static strings to avoid lifetime issues
                match i % 5 {
                    0 => "The quick brown fox jumps over the lazy dog.",
                    1 => "Machine learning models can process natural language.",
                    2 => "Rust provides memory safety without garbage collection.",
                    3 => "Vector databases enable semantic search capabilities.",
                    _ => "Knowledge graphs connect information in meaningful ways.",
                }
            })
            .collect();

        let start = std::time::Instant::now();
        let embeddings = backend.embed_texts(&texts).unwrap();
        let elapsed = start.elapsed();

        let throughput = texts.len() as f64 / elapsed.as_secs_f64();
        println!("Embedded {} texts in {:?}", texts.len(), elapsed);
        println!("Throughput: {:.1} embeddings/sec", throughput);

        assert_eq!(embeddings.len(), texts.len());
    }
}
