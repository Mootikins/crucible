//! End-to-end tests for local model discovery and constrained generation
//!
//! These tests require:
//! 1. The `local-config` feature to be enabled
//! 2. A local GGUF model to be present in ~/models/language/
//!
//! Run with: `cargo test -p crucible-llm --features local-config -- --ignored`
//!
//! The tests will automatically discover available models and use them for testing.

#![cfg(feature = "local-config")]

use crucible_core::traits::provider::{CanConstrainGeneration, ConstrainedRequest, SchemaFormat};
use crucible_core::types::grammar::presets;
use crucible_llm::text_generation::{LlamaCppTextConfig, LlamaCppTextProvider};
use std::path::PathBuf;

/// Get the models directory from environment or default
fn get_models_dir() -> PathBuf {
    std::env::var("CRUCIBLE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("Could not find home directory")
                .join("models/language")
        })
}

/// Find a GGUF model in the models directory
fn find_gguf_model() -> Option<PathBuf> {
    let models_dir = get_models_dir();

    if !models_dir.exists() {
        eprintln!("Models directory does not exist: {}", models_dir.display());
        return None;
    }

    // Walk the directory looking for GGUF files
    // Prefer smaller quantized models for faster tests
    let mut gguf_files: Vec<PathBuf> = walkdir::WalkDir::new(&models_dir)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort by file size (prefer smaller models for faster tests)
    gguf_files.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX));

    // Prefer models with quantization in name (usually smaller)
    let quantized: Vec<_> = gguf_files
        .iter()
        .filter(|p| {
            let name = p.file_stem().unwrap_or_default().to_string_lossy();
            name.contains("Q4") || name.contains("Q5") || name.contains("Q8")
        })
        .cloned()
        .collect();

    if !quantized.is_empty() {
        Some(quantized[0].clone())
    } else {
        gguf_files.first().cloned()
    }
}

/// Find an embedding GGUF model (nomic, bge, etc.)
fn find_embedding_model() -> Option<PathBuf> {
    let models_dir = get_models_dir();
    let embeddings_dir = models_dir.parent()?.join("embeddings");

    // Check embeddings directory first
    for dir in [embeddings_dir, models_dir] {
        if !dir.exists() {
            continue;
        }

        for entry in walkdir::WalkDir::new(&dir)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("gguf") {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                // Look for common embedding model patterns
                if name.contains("nomic")
                    || name.contains("embed")
                    || name.contains("bge")
                    || name.contains("gte")
                {
                    return Some(path.to_path_buf());
                }
            }
        }
    }

    None
}

// ============================================================================
// Model Discovery Tests
// ============================================================================

#[test]
#[ignore = "Requires local GGUF model"]
fn test_model_discovery() {
    let model = find_gguf_model();

    match model {
        Some(path) => {
            println!("Found GGUF model: {}", path.display());
            let size = std::fs::metadata(&path)
                .map(|m| m.len() / 1024 / 1024)
                .unwrap_or(0);
            println!("  Size: {} MB", size);

            assert!(path.exists());
            assert!(path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gguf")));
        }
        None => {
            eprintln!("No GGUF model found in {}", get_models_dir().display());
            eprintln!("Skipping test - add a GGUF model to enable");
        }
    }
}

#[test]
#[ignore = "Requires local embedding model"]
fn test_embedding_model_discovery() {
    let model = find_embedding_model();

    match model {
        Some(path) => {
            println!("Found embedding model: {}", path.display());
            let size = std::fs::metadata(&path)
                .map(|m| m.len() / 1024 / 1024)
                .unwrap_or(0);
            println!("  Size: {} MB", size);

            assert!(path.exists());
        }
        None => {
            eprintln!("No embedding model found");
            eprintln!("Skipping test - add a nomic/bge embedding model to enable");
        }
    }
}

// ============================================================================
// LlamaCpp Provider Tests
// ============================================================================

#[test]
#[ignore = "Requires local GGUF model and is slow"]
fn test_llama_cpp_provider_creation() {
    let model_path = match find_gguf_model() {
        Some(p) => p,
        None => {
            eprintln!("Skipping - no model found");
            return;
        }
    };

    println!("Creating provider with model: {}", model_path.display());

    let provider = LlamaCppTextProvider::new_with_model(model_path);
    assert!(
        provider.is_ok(),
        "Failed to create provider: {:?}",
        provider.err()
    );
}

#[tokio::test]
#[ignore = "Requires local GGUF model and is slow"]
async fn test_llama_cpp_simple_generation() {
    let model_path = match find_gguf_model() {
        Some(p) => p,
        None => {
            eprintln!("Skipping - no model found");
            return;
        }
    };

    println!("Testing generation with: {}", model_path.display());

    let provider = LlamaCppTextProvider::new_with_config(LlamaCppTextConfig {
        model_path,
        gpu_layers: Some(-1),    // Use GPU if available
        context_size: Some(512), // Small context for testing
        temperature: Some(0.7),
        ..Default::default()
    })
    .expect("Failed to create provider");

    // Simple generation without grammar
    let result = provider.generate_text(
        "The capital of France is",
        None, // No grammar
        32,   // Max tokens
        0.0,  // Greedy
        None,
    );

    match result {
        Ok((text, tokens)) => {
            println!("Generated text: {}", text);
            println!("Total tokens: {}", tokens);
            assert!(!text.is_empty());
        }
        Err(e) => {
            panic!("Generation failed: {:?}", e);
        }
    }
}

// ============================================================================
// Grammar-Constrained Generation Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires local GGUF model and is slow"]
async fn test_grammar_constrained_yes_no() {
    let model_path = match find_gguf_model() {
        Some(p) => p,
        None => {
            eprintln!("Skipping - no model found");
            return;
        }
    };

    println!("Testing yes/no grammar with: {}", model_path.display());

    let provider = LlamaCppTextProvider::new_with_config(LlamaCppTextConfig {
        model_path,
        gpu_layers: Some(-1),
        context_size: Some(512),
        ..Default::default()
    })
    .expect("Failed to create provider");

    let grammar = presets::yes_no();

    let result = provider.generate_text(
        "Is the sky blue? Answer with yes or no:",
        Some(grammar.as_str()),
        8,
        0.0, // Greedy for deterministic output
        None,
    );

    match result {
        Ok((text, _tokens)) => {
            let text_clean = text.trim().to_lowercase();
            println!("Generated: '{}'", text_clean);
            assert!(
                text_clean == "yes" || text_clean == "no",
                "Expected 'yes' or 'no', got: '{}'",
                text_clean
            );
        }
        Err(e) => {
            panic!("Generation failed: {:?}", e);
        }
    }
}

#[tokio::test]
#[ignore = "Requires local GGUF model and is slow"]
async fn test_grammar_constrained_tool_call() {
    let model_path = match find_gguf_model() {
        Some(p) => p,
        None => {
            eprintln!("Skipping - no model found");
            return;
        }
    };

    println!("Testing tool call grammar with: {}", model_path.display());

    let provider = LlamaCppTextProvider::new_with_config(LlamaCppTextConfig {
        model_path,
        gpu_layers: Some(-1),
        context_size: Some(1024),
        ..Default::default()
    })
    .expect("Failed to create provider");

    let grammar = presets::l0_l1_tools();

    let prompt = r#"You are a coding assistant. When asked to perform a task, output a tool call.

Available tools:
- read(path="<file>") - Read a file
- ls(path="<dir>") - List directory contents
- git(args="<args>") - Run git command

User: Show me the contents of the README file
Assistant:"#;

    let result = provider.generate_text(prompt, Some(grammar.as_str()), 64, 0.7, None);

    match result {
        Ok((text, _tokens)) => {
            let text_clean = text.trim();
            println!("Generated tool call: {}", text_clean);

            // Should be a valid tool call
            assert!(
                text_clean.starts_with("read(")
                    || text_clean.starts_with("ls(")
                    || text_clean.starts_with("git("),
                "Expected a tool call, got: '{}'",
                text_clean
            );
        }
        Err(e) => {
            panic!("Generation failed: {:?}", e);
        }
    }
}

#[tokio::test]
#[ignore = "Requires local GGUF model and is slow"]
async fn test_can_constrain_generation_trait() {
    let model_path = match find_gguf_model() {
        Some(p) => p,
        None => {
            eprintln!("Skipping - no model found");
            return;
        }
    };

    let provider = LlamaCppTextProvider::new_with_config(LlamaCppTextConfig {
        model_path,
        gpu_layers: Some(-1),
        context_size: Some(512),
        ..Default::default()
    })
    .expect("Failed to create provider");

    // Verify trait implementation
    assert!(provider.supports_format(SchemaFormat::Gbnf));
    assert!(!provider.supports_format(SchemaFormat::JsonSchema));

    let formats = provider.supported_formats();
    assert_eq!(formats.len(), 1);
    assert_eq!(formats[0], SchemaFormat::Gbnf);

    // Test via trait method
    let grammar = presets::yes_no();
    let request = ConstrainedRequest::gbnf("Is water wet? Answer yes or no:", grammar.as_str())
        .with_max_tokens(8)
        .with_temperature(0.0);

    let response = provider.generate_constrained(request).await;

    match response {
        Ok(resp) => {
            let text = resp.text.trim().to_lowercase();
            println!("CanConstrainGeneration response: '{}'", text);
            assert!(text == "yes" || text == "no");
            assert!(!resp.truncated);
        }
        Err(e) => {
            panic!("CanConstrainGeneration failed: {:?}", e);
        }
    }
}

// ============================================================================
// Mock Provider Tests (don't require local model)
// ============================================================================

#[cfg(feature = "test-utils")]
mod mock_tests {
    use super::*;
    use crucible_llm::constrained_mock::MockConstrainedProvider;

    #[tokio::test]
    async fn test_mock_constrained_with_presets() {
        let provider = MockConstrainedProvider::new();

        // Configure responses using content patterns from the grammars
        // l0_l1_tools contains "read | write | edit | ls | git | rg"
        provider.set_response(
            "read | write | edit | ls | git | rg",
            r#"read(path="src/main.rs")"#,
        );
        // yes_no contains '"yes" | "no"'
        provider.set_response(r#""yes" | "no""#, "yes");

        // Test L0+L1 tools grammar
        let tool_request =
            ConstrainedRequest::gbnf("Read the main file", presets::l0_l1_tools().as_str());
        let tool_response = provider.generate_constrained(tool_request).await.unwrap();
        assert_eq!(tool_response.text, r#"read(path="src/main.rs")"#);

        // Test yes/no grammar
        let yn_request = ConstrainedRequest::gbnf("Is this a test?", presets::yes_no().as_str());
        let yn_response = provider.generate_constrained(yn_request).await.unwrap();
        assert_eq!(yn_response.text, "yes");
    }

    #[tokio::test]
    async fn test_mock_constrained_call_tracking() {
        let provider = MockConstrainedProvider::new();

        let request = ConstrainedRequest::gbnf("Test prompt", presets::simple_tool_call().as_str())
            .with_max_tokens(100)
            .with_temperature(0.5);

        let _ = provider.generate_constrained(request).await;

        let history = provider.call_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].prompt, "Test prompt");
        assert_eq!(history[0].max_tokens, Some(100));
        assert_eq!(history[0].temperature, Some(0.5));
        assert_eq!(history[0].format, SchemaFormat::Gbnf);
    }

    #[tokio::test]
    async fn test_mock_json_schema_format() {
        let provider = MockConstrainedProvider::json_schema_only();

        // GBNF should fail
        let gbnf_request = ConstrainedRequest::gbnf("Test", "grammar");
        assert!(provider.generate_constrained(gbnf_request).await.is_err());

        // JSON Schema should work
        let json_request = ConstrainedRequest::json_schema(
            "Generate a response",
            r#"{"type": "object", "properties": {"answer": {"type": "string"}}}"#,
        );
        assert!(provider.generate_constrained(json_request).await.is_ok());
    }
}

// ============================================================================
// Grammar Preset Tests (no model required)
// ============================================================================

#[test]
fn test_grammar_presets_valid() {
    // Ensure all presets create valid grammar strings
    let presets_list = [
        ("simple_tool_call", presets::simple_tool_call()),
        ("l0_l1_tools", presets::l0_l1_tools()),
        (
            "l0_l1_tools_with_thinking",
            presets::l0_l1_tools_with_thinking(),
        ),
        ("tool_or_prose", presets::tool_or_prose()),
        ("yes_no", presets::yes_no()),
        ("json_object", presets::json_object()),
    ];

    for (name, grammar) in presets_list {
        let content = grammar.as_str();

        // Basic validation: must have content
        assert!(!content.is_empty(), "Preset '{}' has empty content", name);

        // Must have a root rule
        assert!(
            content.contains("root"),
            "Preset '{}' missing root rule",
            name
        );

        // Must have proper assignment operator
        assert!(
            content.contains("::="),
            "Preset '{}' missing assignment operator",
            name
        );

        println!("✓ Preset '{}' validated ({} chars)", name, content.len());
    }
}

#[test]
fn test_grammar_from_string() {
    use crucible_core::types::grammar::Grammar;

    let custom_grammar = Grammar::new(r#"root ::= "hello" | "world""#);
    assert!(custom_grammar.as_str().contains("hello"));
    assert!(custom_grammar.name.is_none());

    let named_grammar = Grammar::named("greeting", r#"root ::= "hi" | "bye""#);
    assert_eq!(named_grammar.name, Some("greeting".to_string()));
}
