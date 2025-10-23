//! Tool registry and execution
//!
//! Manages tools that can be executed from the REPL.

mod registry;
mod rune_db;
mod types;
mod tool_group;
mod system_tool_group;
mod unified_registry;
mod benchmarks;
mod performance_test;

// Re-export main types
pub use registry::ToolRegistry;
pub use rune_db::{create_db_module, DbHandle};
pub use types::{ToolResult, ToolStatus};
pub use tool_group::{
    ToolGroup, ToolGroupRegistry, ToolSchema, ToolGroupError, ToolGroupResult,
    ParameterConverter, ResultConverter, ToolGroupCacheConfig, ToolGroupMetrics
};
pub use system_tool_group::SystemToolGroup;
pub use unified_registry::{
    UnifiedToolRegistry, UnifiedRegistryMetrics, UnifiedRegistryStats
};
pub use benchmarks::{
    PerformanceBenchmarks, BenchmarkConfig, BenchmarkResults, BenchmarkSummary,
    print_benchmark_results, print_comparison_results
};
pub use performance_test::{
    quick_performance_test, compare_caching_strategies, test_lazy_loading, test_memory_usage
};
