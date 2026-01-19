//! Scripting Runtime FFI Comparison Benchmarks
//!
//! Compares FFI overhead for Lua scripting runtime.
//!
//! Run with:
//! ```bash
//! cargo bench -p crucible-benchmarks --features lua -- scripting
//! ```

#[cfg(feature = "lua")]
use criterion::BenchmarkId;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
#[cfg(feature = "lua")]
use serde_json::json;

// =============================================================================
// Lua Benchmarks
// =============================================================================

#[cfg(feature = "lua")]
mod lua_bench {
    use crucible_lua::LuaExecutor;
    use serde_json::{json, Value as JsonValue};

    pub fn create_executor() -> LuaExecutor {
        LuaExecutor::new().unwrap()
    }

    /// Check if Fennel compiler is available
    pub fn fennel_available(executor: &LuaExecutor) -> bool {
        executor.fennel_available()
    }

    /// Execute a simple arithmetic expression
    pub async fn simple_expr(executor: &LuaExecutor) -> JsonValue {
        executor
            .execute_source("return 1 + 2 + 3", false, JsonValue::Null)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }

    /// Execute a function with arguments
    pub async fn with_args(executor: &LuaExecutor, a: i64, b: i64) -> JsonValue {
        let source = "local a, b = ...; return a + b";
        let args = json!([a, b]);
        executor
            .execute_source(source, false, args)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }

    /// Execute a loop to test iteration overhead
    pub async fn loop_sum(executor: &LuaExecutor, n: i64) -> JsonValue {
        let source = format!(
            r#"
            local sum = 0
            for i = 0, {} - 1 do
                sum = sum + i
            end
            return sum
            "#,
            n
        );
        executor
            .execute_source(&source, false, JsonValue::Null)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }

    /// Execute with complex JSON data
    pub async fn complex_data(executor: &LuaExecutor, data: JsonValue) -> JsonValue {
        let source = r#"
            local data = ...
            local count = 0
            for _, item in ipairs(data.items) do
                if item.active then
                    count = count + 1
                end
            end
            return count
        "#;
        executor
            .execute_source(source, false, data)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }

    /// Execute Fennel (compiled to Lua)
    pub async fn fennel_expr(executor: &LuaExecutor) -> JsonValue {
        let source = "(+ 1 2 3)";
        executor
            .execute_source(source, true, JsonValue::Null)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }

    /// Execute Fennel loop
    pub async fn fennel_loop(executor: &LuaExecutor, n: i64) -> JsonValue {
        let source = format!(
            r#"
            (var sum 0)
            (for [i 0 {}]
              (set sum (+ sum i)))
            sum
            "#,
            n - 1
        );
        executor
            .execute_source(&source, true, JsonValue::Null)
            .await
            .unwrap()
            .content
            .unwrap_or(JsonValue::Null)
    }
}

// =============================================================================
// Simple Expression Benchmarks
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_simple_expr(c: &mut Criterion) {
    #[cfg(feature = "lua")]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/simple_expr");

    #[cfg(feature = "lua")]
    {
        let executor = lua_bench::create_executor();
        group.bench_function("lua", |b| {
            b.to_async(&rt).iter(|| lua_bench::simple_expr(&executor));
        });

        if lua_bench::fennel_available(&executor) {
            group.bench_function("fennel", |b| {
                b.to_async(&rt).iter(|| lua_bench::fennel_expr(&executor));
            });
        }
    }

    group.finish();
}

// =============================================================================
// Function Call with Arguments
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_function_args(c: &mut Criterion) {
    #[cfg(feature = "lua")]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/function_args");

    #[cfg(feature = "lua")]
    {
        let executor = lua_bench::create_executor();
        group.bench_function("lua", |b| {
            b.to_async(&rt)
                .iter(|| lua_bench::with_args(&executor, 10, 20));
        });
    }

    group.finish();
}

// =============================================================================
// Loop/Iteration Overhead
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_loop_iterations(c: &mut Criterion) {
    #[cfg(feature = "lua")]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/loop");

    #[cfg(feature = "lua")]
    for n in [100i64, 1000, 10000] {
        group.throughput(Throughput::Elements(n as u64));

        let executor = lua_bench::create_executor();
        group.bench_with_input(BenchmarkId::new("lua", n), &n, |b, &n| {
            b.to_async(&rt).iter(|| lua_bench::loop_sum(&executor, n));
        });

        if lua_bench::fennel_available(&executor) {
            group.bench_with_input(BenchmarkId::new("fennel", n), &n, |b, &n| {
                b.to_async(&rt)
                    .iter(|| lua_bench::fennel_loop(&executor, n));
            });
        }
    }

    group.finish();
}

// =============================================================================
// Complex Data Marshalling
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_complex_data(c: &mut Criterion) {
    #[cfg(feature = "lua")]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/complex_data");

    #[cfg(feature = "lua")]
    for item_count in [10usize, 100, 1000] {
        let data = json!({
            "items": (0..item_count).map(|i| {
                json!({
                    "id": i,
                    "name": format!("item_{}", i),
                    "active": i % 2 == 0,
                    "value": i * 10
                })
            }).collect::<Vec<_>>()
        });

        group.throughput(Throughput::Elements(item_count as u64));

        let executor = lua_bench::create_executor();
        group.bench_with_input(BenchmarkId::new("lua", item_count), &data, |b, data| {
            b.to_async(&rt)
                .iter(|| lua_bench::complex_data(&executor, data.clone()));
        });
    }

    group.finish();
}

// =============================================================================
// Executor Creation Overhead
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_executor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("scripting/executor_creation");

    #[cfg(feature = "lua")]
    {
        group.bench_function("lua", |b| {
            b.iter(|| lua_bench::create_executor());
        });
    }

    group.finish();
}

criterion_group!(
    scripting_benches,
    bench_simple_expr,
    bench_function_args,
    bench_loop_iterations,
    bench_complex_data,
    bench_executor_creation,
);

criterion_main!(scripting_benches);
