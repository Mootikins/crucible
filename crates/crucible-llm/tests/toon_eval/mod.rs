//! TOON LLM Evaluation Test Suite
//!
//! Tests how well local Ollama models can read/write TOON format.
//!
//! ## Configuration
//!
//! - `OLLAMA_BASE_URL`: Ollama endpoint (default: `http://localhost:11434`)
//! - `TOON_EVAL_MODEL`: Model to test (default: `qwen3:8b`)
//!
//! ## Running
//!
//! ```bash
//! # Run all TOON evaluation tests
//! cargo test --package crucible-llm --test toon_eval -- --ignored
//!
//! # With custom endpoint
//! OLLAMA_BASE_URL=https://your-ollama.example.com cargo test ...
//! ```

pub mod fixtures;
pub mod prompts;
pub mod report;
pub mod validation;

pub use fixtures::{
    all_fixtures, fixtures_by_complexity, query_fixtures, Complexity, QueryFixture,
};
pub use prompts::{build_prompt, build_query_prompt, ConversionDirection, PromptConfig};
pub use report::{EvalReport, TestResult};
pub use validation::{validate_json_output, validate_toon_output, ToonError, ValidationResult};
