//! Scripting Runtime FFI Comparison Benchmarks
//!
//! Compares FFI overhead across Rune, Steel, and Lua scripting runtimes.
//!
//! Run with:
//! ```bash
//! cargo bench -p crucible-benchmarks --features rune,steel,lua -- scripting
//! ```

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
#[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
use criterion::BenchmarkId;
#[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
use serde_json::json;

// =============================================================================
// Rune Benchmarks
// =============================================================================

#[cfg(feature = "rune")]
mod rune_bench {
    use crucible_rune::RuneExecutor;
    use serde_json::Value as JsonValue;
    use std::sync::Arc;

    // Re-export the Unit type from rune via crucible-rune
    pub type Unit = rune::Unit;

    pub struct RuneFixture {
        pub executor: RuneExecutor,
        pub simple_unit: Arc<Unit>,
        pub add_unit: Arc<Unit>,
        pub loop_unit: Arc<Unit>,
        pub complex_unit: Arc<Unit>,
    }

    pub fn setup() -> RuneFixture {
        let executor = RuneExecutor::new().unwrap();

        let simple_unit = executor
            .compile("simple", "pub fn main() { 1 + 2 + 3 }")
            .unwrap();

        let add_unit = executor
            .compile("add", "pub fn add(a, b) { a + b }")
            .unwrap();

        let loop_unit = executor
            .compile(
                "loop",
                r#"
                pub fn sum_to(n) {
                    let sum = 0;
                    for i in 0..n {
                        sum += i;
                    }
                    sum
                }
                "#,
            )
            .unwrap();

        let complex_unit = executor
            .compile(
                "complex",
                r#"
                pub fn count_active(data) {
                    let count = 0;
                    for item in data["items"] {
                        if item["active"] {
                            count += 1;
                        }
                    }
                    count
                }
                "#,
            )
            .unwrap();

        RuneFixture {
            executor,
            simple_unit,
            add_unit,
            loop_unit,
            complex_unit,
        }
    }

    /// Execute a simple arithmetic expression
    pub async fn simple_expr(fixture: &RuneFixture) -> JsonValue {
        fixture
            .executor
            .call_function(&fixture.simple_unit, "main", ())
            .await
            .unwrap()
    }

    /// Execute a function with arguments
    pub async fn with_args(fixture: &RuneFixture, a: i64, b: i64) -> JsonValue {
        fixture
            .executor
            .call_function(&fixture.add_unit, "add", (a, b))
            .await
            .unwrap()
    }

    /// Execute a loop to test iteration overhead
    pub async fn loop_sum(fixture: &RuneFixture, n: i64) -> JsonValue {
        fixture
            .executor
            .call_function(&fixture.loop_unit, "sum_to", (n,))
            .await
            .unwrap()
    }

    /// Execute with complex JSON data
    pub async fn complex_data(fixture: &RuneFixture, data: JsonValue) -> JsonValue {
        // Convert JSON to Rune value
        let rune_val = fixture.executor.json_to_rune_value(data).unwrap();
        let result: JsonValue = fixture
            .executor
            .call_function(&fixture.complex_unit, "count_active", (rune_val,))
            .await
            .unwrap();
        result
    }
}

// =============================================================================
// Steel Benchmarks
// =============================================================================

#[cfg(feature = "steel")]
mod steel_bench {
    use crucible_steel::SteelExecutor;
    use serde_json::Value as JsonValue;

    pub fn create_executor() -> SteelExecutor {
        SteelExecutor::new().unwrap()
    }

    /// Execute a simple arithmetic expression
    pub async fn simple_expr(executor: &SteelExecutor) -> JsonValue {
        executor.execute_source("(+ 1 2 3)").await.unwrap()
    }

    /// Execute a function with arguments
    pub async fn with_args(executor: &SteelExecutor, a: i64, b: i64) -> JsonValue {
        // Define the function first
        executor
            .execute_source("(define (add a b) (+ a b))")
            .await
            .unwrap();
        executor
            .call_function("add", vec![JsonValue::from(a), JsonValue::from(b)])
            .await
            .unwrap()
    }

    /// Execute a loop to test iteration overhead (uses recursion in Scheme)
    pub async fn loop_sum(executor: &SteelExecutor, n: i64) -> JsonValue {
        let source = format!(
            r#"
            (define (sum-to-impl n acc)
              (if (<= n 0)
                  acc
                  (sum-to-impl (- n 1) (+ acc n))))
            (sum-to-impl {} 0)
            "#,
            n - 1
        );
        executor.execute_source(&source).await.unwrap()
    }

    /// Execute with complex JSON data (embedded in source)
    pub async fn complex_data(executor: &SteelExecutor, data: &JsonValue) -> JsonValue {
        let items = data["items"].as_array().unwrap();
        let scheme_items: Vec<String> = items
            .iter()
            .map(|item| {
                let active = item["active"].as_bool().unwrap_or(false);
                format!("(hash 'active {})", if active { "#t" } else { "#f" })
            })
            .collect();
        let source = format!(
            r#"
            (define items (list {}))
            (length (filter (lambda (item) (hash-get item 'active)) items))
            "#,
            scheme_items.join(" ")
        );
        executor.execute_source(&source).await.unwrap()
    }
}

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
    #[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/simple_expr");

    #[cfg(feature = "rune")]
    {
        let fixture = rune_bench::setup();
        group.bench_function("rune", |b| {
            b.to_async(&rt).iter(|| rune_bench::simple_expr(&fixture));
        });
    }

    #[cfg(feature = "steel")]
    {
        let executor = steel_bench::create_executor();
        group.bench_function("steel", |b| {
            b.to_async(&rt).iter(|| steel_bench::simple_expr(&executor));
        });
    }

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
    #[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/function_args");

    #[cfg(feature = "rune")]
    {
        let fixture = rune_bench::setup();
        group.bench_function("rune", |b| {
            b.to_async(&rt)
                .iter(|| rune_bench::with_args(&fixture, 10, 20));
        });
    }

    #[cfg(feature = "steel")]
    {
        let executor = steel_bench::create_executor();
        group.bench_function("steel", |b| {
            b.to_async(&rt)
                .iter(|| steel_bench::with_args(&executor, 10, 20));
        });
    }

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
    #[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/loop");

    for n in [100i64, 1000, 10000] {
        group.throughput(Throughput::Elements(n as u64));

        #[cfg(feature = "rune")]
        {
            let fixture = rune_bench::setup();
            group.bench_with_input(BenchmarkId::new("rune", n), &n, |b, &n| {
                b.to_async(&rt).iter(|| rune_bench::loop_sum(&fixture, n));
            });
        }

        #[cfg(feature = "steel")]
        {
            let executor = steel_bench::create_executor();
            // Steel uses recursion which is slower for large N, skip 10000
            if n <= 1000 {
                group.bench_with_input(BenchmarkId::new("steel", n), &n, |b, &n| {
                    b.to_async(&rt).iter(|| steel_bench::loop_sum(&executor, n));
                });
            }
        }

        #[cfg(feature = "lua")]
        {
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
    }

    group.finish();
}

// =============================================================================
// Complex Data Marshalling
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_complex_data(c: &mut Criterion) {
    #[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/complex_data");

    // Generate test data with N items
    #[cfg(any(feature = "rune", feature = "steel", feature = "lua"))]
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

        #[cfg(feature = "rune")]
        {
            let fixture = rune_bench::setup();
            group.bench_with_input(BenchmarkId::new("rune", item_count), &data, |b, data| {
                b.to_async(&rt)
                    .iter(|| rune_bench::complex_data(&fixture, data.clone()));
            });
        }

        #[cfg(feature = "steel")]
        {
            let executor = steel_bench::create_executor();
            // Steel data embedding is expensive for large datasets
            if item_count <= 100 {
                group.bench_with_input(BenchmarkId::new("steel", item_count), &data, |b, data| {
                    b.to_async(&rt)
                        .iter(|| steel_bench::complex_data(&executor, data));
                });
            }
        }

        #[cfg(feature = "lua")]
        {
            let executor = lua_bench::create_executor();
            group.bench_with_input(BenchmarkId::new("lua", item_count), &data, |b, data| {
                b.to_async(&rt)
                    .iter(|| lua_bench::complex_data(&executor, data.clone()));
            });
        }
    }

    group.finish();
}

// =============================================================================
// Executor Creation Overhead
// =============================================================================

#[allow(unused_variables, unused_mut)]
fn bench_executor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("scripting/executor_creation");

    #[cfg(feature = "rune")]
    {
        group.bench_function("rune", |b| {
            b.iter(|| crucible_rune::RuneExecutor::new().unwrap());
        });
    }

    #[cfg(feature = "steel")]
    {
        group.bench_function("steel", |b| {
            b.iter(|| steel_bench::create_executor());
        });
    }

    #[cfg(feature = "lua")]
    {
        group.bench_function("lua", |b| {
            b.iter(|| lua_bench::create_executor());
        });
    }

    group.finish();
}

// =============================================================================
// Compilation Overhead (Rune-specific: compile vs reuse)
// =============================================================================

#[cfg(feature = "rune")]
fn bench_compilation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scripting/rune_compilation");

    let executor = crucible_rune::RuneExecutor::new().unwrap();
    let source = r#"
        pub fn process(x) {
            let result = 0;
            for i in 0..x {
                result += i * 2;
            }
            result
        }
    "#;

    // Compile each time
    group.bench_function("compile_each_call", |b| {
        b.to_async(&rt).iter(|| async {
            let unit = executor.compile("bench", source).unwrap();
            executor.call_function(&unit, "process", (100i64,)).await
        });
    });

    // Pre-compiled unit
    let unit = executor.compile("bench", source).unwrap();
    group.bench_function("reuse_compiled", |b| {
        b.to_async(&rt)
            .iter(|| executor.call_function(&unit, "process", (100i64,)));
    });

    group.finish();
}

// =============================================================================
// Benchmark Registration
// =============================================================================

#[cfg(feature = "rune")]
criterion_group!(rune_specific, bench_compilation,);

criterion_group!(
    scripting_benches,
    bench_simple_expr,
    bench_function_args,
    bench_loop_iterations,
    bench_complex_data,
    bench_executor_creation,
);

#[cfg(feature = "rune")]
criterion_main!(scripting_benches, rune_specific);

#[cfg(not(feature = "rune"))]
criterion_main!(scripting_benches);
