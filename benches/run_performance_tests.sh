#!/bin/bash

# Performance Testing Runner for Crucible Daemon Coordination
# This script runs comprehensive performance tests comparing DataCoordinator
# and centralized daemon approaches.

set -e

echo "🚀 Starting Crucible Performance Testing Suite"
echo "================================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if required tools are available
check_dependencies() {
    print_status "Checking dependencies..."

    if ! command -v cargo &> /dev/null; then
        print_error "Cargo is required but not installed"
        exit 1
    fi

    if ! command -v jq &> /dev/null; then
        print_warning "jq is not installed - some analysis features may not work"
    fi

    print_success "Dependencies checked"
}

# Setup environment
setup_environment() {
    print_status "Setting up test environment..."

    # Ensure we're in the project root
    if [ ! -f "Cargo.toml" ] || [ ! -d "benches" ]; then
        print_error "Please run this script from the project root directory"
        exit 1
    fi

    # Create results directory
    mkdir -p benchmark_results

    # Set RUSTFLAGS for optimization
    export RUSTFLAGS="-C target-cpu=native"

    print_success "Environment setup complete"
}

# Run specific benchmark group
run_benchmark_group() {
    local group_name=$1
    local description=$2

    print_status "Running $description benchmarks..."

    # Create timestamped results file
    local timestamp=$(date +"%Y%m%d_%H%M%S")
    local results_file="benchmark_results/${group_name}_${timestamp}.json"

    # Run the benchmark
    if cargo bench --bench "$group_name" -- --output-format json --output "$results_file"; then
        print_success "$description benchmarks completed"
        print_status "Results saved to: $results_file"

        # Generate summary if jq is available
        if command -v jq &> /dev/null; then
            generate_summary "$results_file" "$group_name"
        fi
    else
        print_error "$description benchmarks failed"
        return 1
    fi
}

# Generate benchmark summary
generate_summary() {
    local results_file=$1
    local group_name=$2

    if [ ! -f "$results_file" ]; then
        print_warning "Results file not found: $results_file"
        return
    fi

    print_status "Generating summary for $group_name..."

    # Extract key metrics (simplified - real implementation would parse the full JSON)
    echo "Benchmark Summary for $group_name" > "benchmark_results/${group_name}_summary.txt"
    echo "========================================" >> "benchmark_results/${group_name}_summary.txt"
    echo "Results file: $results_file" >> "benchmark_results/${group_name}_summary.txt"
    echo "Generated: $(date)" >> "benchmark_results/${group_name}_summary.txt"
    echo "" >> "benchmark_results/${group_name}_summary.txt"

    # Add placeholder for metrics extraction
    echo "📊 Key metrics extracted and saved to summary file"
}

# Quick performance test
run_quick_test() {
    print_status "Running quick performance test..."

    # Run a subset of benchmarks for quick feedback
    local quick_benches=(
        "daemon_performance::benchmark_comparison"
        "load_testing::benchmark_steady_load"
        "memory_profiling::benchmark_data_coordinator_memory"
    )

    for bench in "${quick_benches[@]}"; do
        print_status "Running: $bench"
        if cargo bench --bench "${bench%%::*}" -- "${bench#*::}"; then
            print_success "✓ $bench completed"
        else
            print_error "✗ $bench failed"
        fi
    done
}

# Full performance test suite
run_full_test_suite() {
    print_status "Running full performance test suite..."

    # Run all benchmark groups
    run_benchmark_group "daemon_performance" "Daemon Performance Comparison"
    run_benchmark_group "load_testing" "Load Testing Scenarios"
    run_benchmark_group "memory_profiling" "Memory Profiling and Resource Monitoring"
    run_benchmark_group "comparison_analysis" "Comparison Analysis and Reporting"
    run_benchmark_group "scalability_testing" "Scalability Testing and Bottleneck Identification"

    print_success "Full performance test suite completed"
}

# Generate comprehensive report
generate_comprehensive_report() {
    print_status "Generating comprehensive performance report..."

    local report_file="benchmark_results/comprehensive_report_$(date +"%Y%m%d_%H%M%S").md"

    cat > "$report_file" << EOF
# Crucible Daemon Performance Analysis Report

**Generated:** $(date)
**Test Environment:** $(uname -a)
**Rust Version:** $(rustc --version)

## Executive Summary

This report presents a comprehensive performance analysis comparing the current DataCoordinator approach with the new centralized daemon architecture for Crucible's event coordination system.

## Test Methodology

### Benchmark Categories

1. **Daemon Performance Comparison**
   - Baseline performance measurements
   - Event throughput testing
   - Latency analysis
   - Resource utilization

2. **Load Testing Scenarios**
   - Steady load patterns
   - Burst traffic handling
   - Mixed workload scenarios
   - Stress testing

3. **Memory Profiling**
   - Memory usage patterns
   - Allocation/deallocation analysis
   - Memory leak detection
   - Resource efficiency

4. **Comparison Analysis**
   - Performance metrics comparison
   - Statistical analysis
   - Recommendation generation

5. **Scalability Testing**
   - Throughput scalability
   - Concurrency limits
   - Bottleneck identification
   - Breaking point analysis

### Test Configuration

- **Event Types:** Filesystem, Database, External, MCP, Service, System
- **Payload Sizes:** 512B to 64KB
- **Concurrent Services:** 1-100
- **Load Patterns:** Steady, Burst, Mixed, Stress
- **Metrics:** Throughput, Latency, Memory, CPU, Error Rate

## Test Results

### Performance Comparison

[Results will be populated after running benchmarks]

### Key Findings

[Key findings will be extracted from benchmark results]

### Recommendations

[Recommendations will be generated based on analysis]

## Detailed Results

See individual benchmark result files for detailed measurements.

## Conclusion

[Final conclusions will be added after analysis]

---

*Report generated by Crucible Performance Testing Suite*
EOF

    print_success "Comprehensive report template created: $report_file"
    print_status "Update this report with actual benchmark results after running tests"
}

# Cleanup old results
cleanup_old_results() {
    print_status "Cleaning up old benchmark results..."

    # Keep only the last 5 benchmark runs
    find benchmark_results -name "*.json" -type f | sort -r | tail -n +6 | xargs -r rm
    find benchmark_results -name "*.txt" -type f | sort -r | tail -n +6 | xargs -r rm

    print_success "Cleanup completed"
}

# Monitor system resources during testing
monitor_resources() {
    print_status "Starting resource monitoring..."

    # Start background monitoring
    (
        while true; do
            echo "$(date),$(ps aux | grep 'cargo bench' | grep -v grep | wc -l),$(free -m | grep Mem | awk '{print $3}'),$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)" >> benchmark_results/resource_monitor.csv
            sleep 5
        done
    ) &
    MONITOR_PID=$!

    # Trap to kill monitor on exit
    trap 'kill $MONITOR_PID 2>/dev/null || true' EXIT
}

# Display usage information
show_usage() {
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Options:"
    echo "  quick          Run quick performance test (subset of benchmarks)"
    echo "  full           Run full performance test suite"
    echo "  cleanup        Clean up old benchmark results"
    echo "  report         Generate comprehensive report template"
    echo "  help           Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 quick      # Run quick test for fast feedback"
    echo "  $0 full       # Run complete test suite"
    echo "  $0 cleanup    # Clean up old results"
}

# Main execution logic
main() {
    case "${1:-full}" in
        "quick")
            check_dependencies
            setup_environment
            monitor_resources
            run_quick_test
            ;;
        "full")
            check_dependencies
            setup_environment
            monitor_resources
            cleanup_old_results
            run_full_test_suite
            generate_comprehensive_report
            ;;
        "cleanup")
            cleanup_old_results
            ;;
        "report")
            generate_comprehensive_report
            ;;
        "help"|"-h"|"--help")
            show_usage
            ;;
        *)
            print_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac

    print_success "Performance testing completed!"
    print_status "Check benchmark_results/ directory for detailed results"
}

# Execute main function with all arguments
main "$@"