//! Grammar-constrained generation for tool calling
//!
//! This crate provides infrastructure for testing and using GBNF grammar
//! constraints with local LLMs (via llama.cpp/llama-server).
//!
//! ## Features
//!
//! - GBNF grammar loading and validation
//! - OpenAI-compatible API client with grammar support
//! - Test harness for comparing constrained vs unconstrained generation
//! - Scoring metrics for tool call accuracy

pub mod api;
pub mod grammar;
pub mod harness;
pub mod scoring;

pub use api::{
    ChatMessage, CompletionRequest, CompletionResponse, LlamaClient, TextCompletionRequest,
    TextCompletionResponse,
};
pub use grammar::Grammar;
pub use harness::{ChatTemplate, Mode, TestCase, TestHarness, TestResult};
pub use scoring::{Score, Scorer};
