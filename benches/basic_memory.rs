//! Basic memory profiling for Phase 6.3
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

fn bench_basic_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_memory");

    // Test 1: String allocation patterns
    group.bench_function("string_allocation", |b| {
        b.iter(|| {
            let mut strings = Vec::new();
            for i in 0..1000 {
                strings.push(black_box(format!("test_string_{}", i)));
            }
            strings.len()
        });
    });

    // Test 2: Vec operations
    group.bench_function("vec_operations", |b| {
        b.iter(|| {
            let mut vec = Vec::with_capacity(1000);
            for i in 0..1000 {
                vec.push(black_box(i * 2));
            }
            vec.iter().sum::<i32>()
        });
    });

    group.finish();
}

fn bench_concurrent_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_patterns");

    group.bench_function("tokio_spawn", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..10)
                    .map(|i| {
                        tokio::spawn(async move {
                            black_box(i * 2)
                        })
                    })
                    .collect();

                futures::future::join_all(handles)
                    .await
                    .into_iter()
                    .map(|r| r.unwrap())
                    .sum::<i32>()
            })
        });
    });

    group.finish();
}

criterion_group!(
    memory_tests,
    bench_basic_memory_patterns,
    bench_concurrent_patterns
);

criterion_main!(memory_tests);