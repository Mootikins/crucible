use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{config::BurnConfig, hardware::HardwareInfo};

pub mod commands;

#[derive(Parser)]
#[command(name = "burn-test")]
#[command(about = "Burn ML Framework Testing and Benchmarking for Crucible")]
#[command(version)]
#[command(arg_required_else_help = false)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Config file path (defaults to ~/.config/crucible/burn.toml)
    #[arg(short = 'C', long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Hardware detection and information
    Detect {
        #[command(subcommand)]
        command: DetectCommand,
    },

    /// Embedding model testing and inference
    Embed {
        #[command(subcommand)]
        command: EmbedCommand,
    },

    /// LLM model testing and inference
    Llm {
        #[command(subcommand)]
        command: LlmCommand,
    },

    /// Start HTTP inference server
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },

    /// Run performance benchmarks
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum DetectCommand {
    /// Show hardware detection results
    Hardware,

    /// List available backends (Vulkan, ROCm, CPU)
    Backends,

    /// Test backend functionality
    TestBackend {
        /// Backend to test (vulkan, rocm, cpu)
        backend: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum EmbedCommand {
    /// Test embedding inference with a model
    Test {
        /// Model name or path
        model: String,

        /// Text to embed
        text: String,

        /// Backend to use (auto, vulkan, rocm, cpu)
        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// Batch embedding test
    Batch {
        /// Model name or path
        model: String,

        /// Input file with texts (one per line)
        file: PathBuf,

        /// Backend to use (auto, vulkan, rocm, cpu)
        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// Compare embedding performance across backends
    Compare {
        /// Model name or path
        model: String,

        /// Text to embed
        text: String,

        /// Number of iterations per backend
        #[arg(long, default_value = "10")]
        iterations: usize,
    },

    /// List available embedding models
    List,
}

#[derive(Subcommand, Debug)]
pub enum LlmCommand {
    /// Test LLM inference
    Infer {
        /// Model name or path
        model: String,

        /// Prompt text
        prompt: String,

        /// Maximum tokens to generate
        #[arg(long, default_value = "100")]
        max_tokens: usize,

        /// Backend to use (auto, vulkan, rocm, cpu)
        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// Streaming LLM inference
    Stream {
        /// Model name or path
        model: String,

        /// Prompt text
        prompt: String,

        /// Maximum tokens to generate
        #[arg(long, default_value = "100")]
        max_tokens: usize,

        /// Backend to use (auto, vulkan, rocm, cpu)
        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// List available LLM models
    List,
}

#[derive(Subcommand, Debug)]
pub enum ServerCommand {
    /// Start inference server
    Start {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Backend to use (auto, vulkan, rocm, cpu)
        #[arg(long, default_value = "auto")]
        backend: String,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Show server status
    Status,
}

#[derive(Subcommand, Debug)]
pub enum BenchCommand {
    /// Benchmark embedding models
    Embed {
        /// Model to benchmark (or 'all' for all models)
        model: String,

        /// Number of iterations
        #[arg(long, default_value = "100")]
        iterations: usize,

        /// Generate HTML report
        #[arg(long)]
        html_report: bool,
    },

    /// Benchmark LLM models
    Llm {
        /// Model to benchmark (or 'all' for all models)
        model: String,

        /// Number of iterations
        #[arg(long, default_value = "10")]
        iterations: usize,

        /// Generate HTML report
        #[arg(long)]
        html_report: bool,
    },

    /// Compare Burn vs FastEmbed performance
    Compare {
        /// Models to compare
        models: Vec<String>,

        /// Generate HTML report
        #[arg(long)]
        html_report: bool,
    },
}

/// Handle CLI commands
pub async fn handle_command(
    command: Commands,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    match command {
        Commands::Detect { command } => {
            crate::cli::commands::detect::handle(command, hardware_info).await?
        }
        Commands::Embed { command } => {
            crate::cli::commands::embed::handle(command, config, hardware_info).await?
        }
        Commands::Llm { command } => {
            crate::cli::commands::llm::handle(command, config, hardware_info).await?
        }
        Commands::Server { command } => {
            #[cfg(feature = "server")]
            {
                crate::cli::commands::server::handle(command, config, hardware_info).await?
            }
            #[cfg(not(feature = "server"))]
            {
                eprintln!("Server functionality requires the 'server' feature");
                return Ok(());
            }
        }
        Commands::Bench { command } => {
            #[cfg(feature = "benchmarks")]
            {
                crate::cli::commands::bench::handle(command, config, hardware_info).await?
            }
            #[cfg(not(feature = "benchmarks"))]
            {
                eprintln!("Benchmarking requires the 'benchmarks' feature");
                return Ok(());
            }
        }
    }

    Ok(())
}