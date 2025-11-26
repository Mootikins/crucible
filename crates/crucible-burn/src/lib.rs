//! Burn ML Framework Integration for Crucible
//!
//! This crate provides integration with the Burn ML framework for high-performance
//! inference on AMD hardware (Vulkan/ROCm) and other platforms. It includes:
//!
//! - Embedding providers implementing the crucible-core EmbeddingProvider trait
//! - LLM inference providers with streaming support
//! - Hardware detection and backend selection (Vulkan, ROCm, CPU)
//! - Model loading and management from ~/models directory
//! - HTTP inference server for API access
//! - Comprehensive benchmarking suite
//!
//! ## Architecture
//!
//! The crate is organized into several modules:
//! - `cli`: Command-line interface and commands
//! - `providers`: Burn-based embedding and LLM providers
//! - `hardware`: Hardware detection and backend management
//! - `models`: Model discovery and loading utilities
//! - `server`: HTTP inference server
//! - `benchmarks`: Performance benchmarking suite
//! - `config`: Configuration management

pub mod cli;
pub mod config;
pub mod hardware;
pub mod models;
pub mod providers;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "benchmarks")]
pub mod benchmarks;

// Re-export key types for convenience
pub use config::BurnConfig;
pub use hardware::{BackendType, HardwareInfo};
pub use providers::BurnEmbeddingProvider;