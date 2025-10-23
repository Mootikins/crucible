//! Benchmark runner executable
//!
//! This is the main entry point for the comprehensive benchmarking framework.

use anyhow::Result;
use clap::{Arg, Command};
use std::path::PathBuf;

// Import the benchmark runner module
mod benchmark_runner;

use benchmark_runner::{BenchmarkRunner, BenchmarkRunnerConfig};

fn main() -> Result<()> {
    let matches = Command::new("benchmark_runner")
        .version("1.0.0")
        .about("Phase 6.1 Comprehensive Performance Benchmarking Framework")
        .arg(
            Arg::new("output-dir")
                .long("output-dir")
                .short('o')
                .value_name("DIR")
                .help("Output directory for benchmark results")
                .default_value("benchmark_results"),
        )
        .arg(
            Arg::new("quick-check")
                .long("quick-check")
                .short('q')
                .help("Run quick performance check only")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-comparisons")
                .long("no-comparisons")
                .help("Skip architecture comparison benchmarks")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-plots")
                .long("no-plots")
                .help("Disable plot generation")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export-format")
                .long("export-format")
                .short('f')
                .value_name("FORMAT")
                .help("Export format (markdown, json, csv)")
                .value_parser(["markdown", "json", "csv"])
                .num_args(0..=3)
                .default_values(["markdown", "json", "csv"]),
        )
        .arg(
            Arg::new("iterations")
                .long("iterations")
                .short('i')
                .value_name("COUNT")
                .help("Number of benchmark iterations")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("sample-size")
                .long("sample-size")
                .short('s')
                .value_name("COUNT")
                .help("Benchmark sample size for statistical accuracy")
                .value_parser(clap::value_parser!(u32)),
        )
        .get_matches();

    // Build configuration from command line arguments
    let config = BenchmarkRunnerConfig {
        output_dir: matches.get_one::<String>("output-dir").unwrap().clone(),
        run_comparisons: !matches.get_flag("no-comparisons"),
        generate_plots: !matches.get_flag("no-plots"),
        export_formats: matches
            .get_many::<String>("export-format")
            .unwrap_or_default()
            .map(|s| s.to_string())
            .collect(),
        iterations: matches.get_one::<u32>("iterations").copied(),
        sample_size: matches.get_one::<u32>("sample-size").copied(),
    };

    // Create and run benchmark runner
    let mut runner = BenchmarkRunner::new(config);

    if matches.get_flag("quick-check") {
        println!("⚡ Running quick performance check...");
        runner.run_quick_check()?;
    } else {
        println!("🚀 Starting comprehensive performance benchmarking...");
        runner.run_all_benchmarks()?;
    }

    Ok(())
}