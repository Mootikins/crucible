//! Benchmark comparing FastEmbed vs Burn embedding providers
//!
//! This benchmark measures the performance difference between:
//! - FastEmbed (CPU-based)
//! - Burn (GPU-capable, though mocked here)
//!
//! When actual Burn integration is complete, we can see real performance
//! improvements from GPU acceleration.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use crucible_config::{
    BurnBackendConfig, BurnEmbedConfig, EmbeddingProviderConfig, FastEmbedConfig,
};
use crucible_llm::embeddings::create_provider;
use std::time::Instant;

/// Generate test texts for embedding
fn generate_test_texts(count: usize) -> Vec<String> {
    let mut texts = Vec::with_capacity(count);
    for i in 0..count {
        texts.push(format!(
            "This is test document number {}. It contains some text that we want to generate embeddings for. The purpose is to measure performance of different embedding providers.",
            i
        ));
    }
    texts
}

fn bench_embedding_providers(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_providers");

    // Test different batch sizes
    for batch_size in [1, 10, 50, 100, 500] {
        // FastEmbed benchmark
        group.bench_with_input(
            BenchmarkId::new("fastembed", batch_size),
            &batch_size,
            |b, &batch_size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.iter(|| {
                    let batch = black_box(generate_test_texts(batch_size));
                    let config = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
                        model: "BAAI/bge-small-en-v1.5".to_string(),
                        batch_size: batch_size as u32,
                        ..Default::default()
                    });

                    let provider = rt.block_on(create_provider(config)).unwrap();

                    let start = Instant::now();
                    rt.block_on(provider.embed_batch(batch)).unwrap();
                    start.elapsed()
                });
            },
        );

        // Burn benchmark (mocked - will show similar performance until real integration)
        group.bench_with_input(
            BenchmarkId::new("burn", batch_size),
            &batch_size,
            |b, &batch_size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.iter(|| {
                    let batch = black_box(generate_test_texts(batch_size));
                    let config = EmbeddingProviderConfig::Burn(BurnEmbedConfig {
                        model: "test-model".to_string(),
                        backend: BurnBackendConfig::Cpu { num_threads: 4 },
                        dimensions: 384,
                        ..Default::default()
                    });

                    let provider = rt.block_on(create_provider(config)).unwrap();

                    let start = Instant::now();
                    rt.block_on(provider.embed_batch(batch)).unwrap();
                    start.elapsed()
                });
            },
        );
    }
}

fn bench_single_vs_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_vs_batch");

    // Single embeddings
    group.bench_function("fastembed_single", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let config = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
            model: "BAAI/bge-small-en-v1.5".to_string(),
            batch_size: 1,
            ..Default::default()
        });

        let provider = rt.block_on(create_provider(config)).unwrap();

        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(provider.embed_batch(vec!["single text".to_string()]))
                .unwrap();
        });
    });

    // Batch embeddings (same total amount)
    group.bench_function("fastembed_batch", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let texts = generate_test_texts(100);
        let config = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
            model: "BAAI/bge-small-en-v1.5".to_string(),
            batch_size: 100,
            ..Default::default()
        });

        let provider = rt.block_on(create_provider(config)).unwrap();

        b.iter(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(provider.embed_batch(texts.clone())).unwrap();
        });
    });
}

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    let num_texts = 1000;
    group.throughput(Throughput::Elements(num_texts as u64));

    group.bench_function("fastembed_throughput", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let config = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
            model: "BAAI/bge-small-en-v1.5".to_string(),
            batch_size: 32,
            ..Default::default()
        });

        let provider = rt.block_on(create_provider(config)).unwrap();

        b.iter(|| {
            let batch = black_box(generate_test_texts(num_texts));
            rt.block_on(provider.embed_batch(batch)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_embedding_providers,
    bench_single_vs_batch,
    bench_throughput
);
criterion_main!(benches);
