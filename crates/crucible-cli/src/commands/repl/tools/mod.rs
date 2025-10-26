//! Tool registry and execution
//!
//! Manages tools that can be executed from the REPL.

mod registry;
mod rune_db;
mod system_tool_group;
mod tool_group;
mod types;
mod unified_registry;

#[cfg(test)]
mod benchmarks;

#[cfg(test)]
mod performance_test;

// Re-export main types
pub use registry::ToolRegistry;
pub use rune_db::{create_db_module, DbHandle};
pub use system_tool_group::SystemToolGroup;
pub use tool_group::{
    ParameterConverter, ResultConverter, ToolGroup, ToolGroupCacheConfig, ToolGroupError,
    ToolGroupMetrics, ToolGroupRegistry, ToolGroupResult, ToolSchema,
};
pub use types::{ToolResult, ToolStatus};
pub use unified_registry::{UnifiedRegistryMetrics, UnifiedRegistryStats, UnifiedToolRegistry};
// Export benchmark utilities only in test context
#[cfg(test)]
pub use benchmarks::{
    print_benchmark_results, print_comparison_results, BenchmarkConfig, BenchmarkResults,
    BenchmarkSummary, PerformanceBenchmarks,
};

#[cfg(test)]
pub use performance_test::{
    compare_caching_strategies, quick_performance_test, test_lazy_loading, test_memory_usage,
};
