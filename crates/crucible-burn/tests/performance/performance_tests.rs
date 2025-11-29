//! Performance tests for Burn ML framework integration

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use crucible_burn::{models::ModelRegistry, config::BurnConfig};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use std::fs;

// Test data for performance benchmarks
fn create_large_model_directory(num_models: usize) -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    for i in 0..num_models {
        let model_path = temp_dir.path().join(format!("model-{}", i));
        fs::create_dir_all(&model_path).unwrap();

        // Create config.json
        let config_content = format!(r#"
{{
    "model_type": "{}",
    "hidden_size": 768,
    "num_parameters": {}
}}
"#, if i % 2 == 0 { "embedding" } else { "causal_lm" }, 100_000_000 + i * 10_000_000);

        fs::write(model_path.join("config.json"), config_content).unwrap();

        // Create tokenizer.json
        fs::write(model_path.join("tokenizer.json"), "{}").unwrap();

        // Create model file
        let model_file = if i % 3 == 0 {
            "model.safetensors"
        } else if i % 3 == 1 {
            "model.gguf"
        } else {
            "model.bin"
        };

        fs::write(model_path.join(model_file), b"fake_model_data").unwrap();

        // Add some additional files to simulate real models
        fs::write(model_path.join("special_tokens_map.json"), "{}").unwrap();
        fs::write(model_path.join("vocab.json"), "{}").unwrap();
    }

    temp_dir
}

fn bench_model_discovery_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_discovery");

    for &num_models in &[10, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("scan_models", num_models),
            &num_models,
            |b, &num_models| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let temp_dir = create_large_model_directory(num_models);
                        let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()])
                            .await
                            .unwrap();

                        black_box(registry.scan_models().await.unwrap());
                    });
            },
        );
    }

    group.finish();
}

fn bench_config_loading_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_loading");

    // Benchmark loading different config sizes
    group.bench_function("small_config", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let config = BurnConfig::default();
                black_box(config);
            });
    });

    group.bench_function("large_config", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let temp_dir = TempDir::new().unwrap();
                let config_path = temp_dir.path().join("large_config.toml");

                let config_content = r#"
[default_backend]
cpu = { num_threads = 8 }

model_dir = "/very/long/path/to/models/directory/with/many/subdirectories"
model_search_paths = [
    "/models/embeddings",
    "/models/llm",
    "/models/language",
    "/models/custom",
    "/home/user/.cache/huggingface/hub",
    "/opt/models",
    "/usr/local/share/models",
    "/var/lib/models",
    "/tmp/models",
]

[server]
host = "0.0.0.0"
port = 8080
max_request_size_mb = 100
enable_cors = true

[server.rate_limit]
requests_per_minute = 1000
burst_size = 100

[benchmarks]
output_dir = "/very/long/path/to/benchmark/output/directory"
generate_html_reports = true
default_iterations = 1000
warmup_iterations = 100

[hardware]
auto_detect = true
memory_limit_gb = 16
prefer_rocm_in_container = true
vulkan_validation = false
"#;

                fs::write(&config_path, config_content).unwrap();

                let config = BurnConfig::load(Some(&config_path)).await.unwrap();
                black_box(config);
            });
    });

    group.finish();
}

fn bench_model_search_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_search");

    // Create a large model registry
    let temp_dir = create_large_model_directory(1000);
    let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()])
        .tokio_test()
        .await;
    registry.scan_models().tokio_test().await;

    group.bench_function("exact_match_search", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let result = registry.find_model("model-42").tokio_test().await;
                black_box(result);
            });
    });

    group.bench_function("partial_match_search", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let result = registry.find_model("model-4").tokio_test().await;
                black_box(result);
            });
    });

    group.bench_function("list_all_models", |b| {
        b.iter(|| {
            let models = registry.list_models(None);
            black_box(models);
        });
    });

    group.bench_function("list_embedding_models", |b| {
        b.iter(|| {
            let models = registry.list_models(Some(crucible_burn::models::ModelType::Embedding));
            black_box(models);
        });
    });

    group.finish();
}

fn bench_hardware_detection_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("hardware_detection");

    group.bench_function("detect_hardware", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let hardware_info = crucible_burn::hardware::HardwareInfo::detect().tokio_test().await;
                black_box(hardware_info);
            });
    });

    group.finish();
}

fn bench_memory_usage_models(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage_models");

    // Test memory usage when loading many model metadata entries
    group.bench_function("large_registry_memory", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let temp_dir = create_large_model_directory(5000);
                let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()])
                    .await
                    .unwrap();

                // Load all models
                registry.scan_models().await.unwrap();

                // Access all models to ensure they're loaded in memory
                let all_models = registry.get_all_models();
                black_box(all_models.len());
            });
    });

    group.finish();
}

fn bench_backend_selection_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("backend_selection");

    let test_gpus = vec![
        crucible_burn::hardware::GpuInfo {
            name: "NVIDIA RTX 4090".to_string(),
            vendor: crucible_burn::hardware::GpuVendor::Nvidia,
            memory_mb: 24576,
            vulkan_support: true,
            rocm_support: false,
            device_id: Some(0),
        },
        crucible_burn::hardware::GpuInfo {
            name: "AMD Radeon RX 7900 XTX".to_string(),
            vendor: crucible_burn::hardware::GpuVendor::Amd,
            memory_mb: 24576,
            vulkan_support: true,
            rocm_support: true,
            device_id: Some(1),
        },
    ];

    group.bench_function("backend_recommendation", |b| {
        b.iter(|| {
            let backend = crucible_burn::hardware::HardwareInfo::recommend_backend(
                &test_gpus,
                16
            );
            black_box(backend);
        });
    });

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_operations");

    let temp_dir = create_large_model_directory(100);

    group.bench_function("concurrent_model_scanning", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let mut handles = vec![];

                for i in 0..10 {
                    let temp_dir_clone = temp_dir.path().to_path_buf();
                    let handle = tokio::spawn(async move {
                        let mut registry = ModelRegistry::new(vec![temp_dir_clone])
                            .await
                            .unwrap();
                        registry.scan_models().await.unwrap()
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    black_box(handle.await.unwrap());
                }
            });
    });

    group.finish();
}

// Custom benchmark for embedding generation (placeholder since Burn integration isn't complete)
fn bench_embedding_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_generation");

    let test_texts = vec![
        "Hello, world!",
        "This is a longer test text for embedding generation performance testing.",
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
    ];

    for (i, text) in test_texts.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("placeholder_embedding", i),
            text,
            |b, text| {
                b.iter(|| {
                    // Simulate embedding generation (placeholder logic)
                    let dimensions = 384;
                    let hash = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    let mut hasher = hash;
                    text.hash(&mut hasher);

                    let embedding: Vec<f32> = (0..dimensions)
                        .map(|dim| {
                            dim.hash(&mut hasher);
                            ((hasher.finish() % 1000) as f32 - 500.0) / 1000.0
                        })
                        .collect();

                    black_box(embedding);
                });
            },
        );
    }

    group.finish();
}

// Memory stress test
fn bench_memory_stress_test(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_stress");

    group.measurement_time(Duration::from_secs(10));

    group.bench_function("large_model_registry_stress", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Create a very large number of models
                let temp_dir = create_large_model_directory(10000);
                let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()])
                    .await
                    .unwrap();

                // Scan and keep in memory
                registry.scan_models().await.unwrap();

                // Perform multiple operations
                for i in 0..100 {
                    let _ = registry.list_models(None);
                    let _ = registry.find_model(&format!("model-{}", i % 1000)).tokio_test().await;
                }
            });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_model_discovery_performance,
    bench_config_loading_performance,
    bench_model_search_performance,
    bench_hardware_detection_performance,
    bench_memory_usage_models,
    bench_backend_selection_performance,
    bench_concurrent_operations,
    bench_embedding_generation,
    bench_memory_stress_test
);

criterion_main!(benches);