// Backend integration tests for Crucible CLI
//
// These tests verify the LLM backend integration, specifically the Ollama backend.
// Note: These tests require a running Ollama instance to pass completely.

#[tokio::test]
async fn test_ollama_backend_creation() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());
    assert_eq!(backend.name(), "Ollama");
}

#[tokio::test]
async fn test_message_creation() {
    let user_msg = Message::user("Hello, world!");
    assert_eq!(user_msg.role, "user");
    assert_eq!(user_msg.content, "Hello, world!");

    let system_msg = Message::system("You are a helpful assistant.");
    assert_eq!(system_msg.role, "system");
    assert_eq!(system_msg.content, "You are a helpful assistant.");

    let assistant_msg = Message::assistant("I can help you with that.");
    assert_eq!(assistant_msg.role, "assistant");
    assert_eq!(assistant_msg.content, "I can help you with that.");
}

#[tokio::test]
async fn test_chat_params_creation() {
    let params = ChatParams {
        model: "llama3.2".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(500),
    };

    assert_eq!(params.model, "llama3.2");
    assert_eq!(params.temperature, Some(0.7));
    assert_eq!(params.max_tokens, Some(500));
}

#[tokio::test]
#[ignore] // Requires Ollama to be running
async fn test_list_models() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());

    match backend.list_models().await {
        Ok(models) => {
            println!("Found {} models:", models.len());
            for model in &models {
                println!("  - {} (modified: {:?})", model.name, model.modified_at);
            }
            // At least one model should be available if Ollama is running
            assert!(
                !models.is_empty(),
                "Expected at least one model to be available"
            );
        }
        Err(e) => {
            eprintln!("Failed to list models (is Ollama running?): {}", e);
            // Don't fail the test if Ollama is not running
        }
    }
}

#[tokio::test]
#[ignore] // Requires Ollama to be running
async fn test_chat_simple() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());

    let messages = vec![
        Message::system("You are a helpful assistant."),
        Message::user("Say 'Hello' and nothing else."),
    ];

    let params = ChatParams {
        model: "llama3.2".to_string(),
        temperature: Some(0.1), // Low temperature for consistent output
        max_tokens: Some(50),
    };

    match backend.chat(messages, &params).await {
        Ok(response) => {
            println!("Chat response: {}", response);
            assert!(!response.is_empty(), "Response should not be empty");
        }
        Err(e) => {
            eprintln!("Chat failed (is Ollama running?): {}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Requires Ollama to be running
async fn test_chat_with_context() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());

    let messages = vec![
        Message::system("You are a helpful assistant specialized in Rust programming."),
        Message::user("What is a Result type?"),
        Message::assistant("The Result type is an enum used for error handling in Rust."),
        Message::user("Can you give me an example?"),
    ];

    let params = ChatParams {
        model: "llama3.2".to_string(),
        temperature: Some(0.5),
        max_tokens: Some(200),
    };

    match backend.chat(messages, &params).await {
        Ok(response) => {
            println!("Chat response with context: {}", response);
            assert!(!response.is_empty(), "Response should not be empty");
            // Response should likely contain code or Result-related content
        }
        Err(e) => {
            eprintln!("Chat with context failed (is Ollama running?): {}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Requires Ollama to be running
async fn test_model_info_parsing() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());

    match backend.list_models().await {
        Ok(models) => {
            for model in models {
                println!("Model: {}", model.name);
                if let Some(details) = model.details {
                    println!("  Family: {:?}", details.family);
                    println!("  Parameter Size: {:?}", details.parameter_size);
                    println!("  Quantization: {:?}", details.quantization_level);
                }
                if let Some(size) = model.size {
                    println!("  Size: {} bytes", size);
                }
                println!("  Modified: {:?}", model.modified_at);
            }
        }
        Err(e) => {
            eprintln!("Failed to parse model info: {}", e);
        }
    }
}

#[tokio::test]
async fn test_backend_error_handling_invalid_endpoint() {
    let backend = OllamaBackend::new("http://invalid-endpoint:9999".to_string());

    let messages = vec![Message::user("Test")];
    let params = ChatParams {
        model: "test".to_string(),
        temperature: None,
        max_tokens: None,
    };

    let result = backend.chat(messages, &params).await;
    assert!(result.is_err(), "Expected error with invalid endpoint");
}

#[tokio::test]
#[ignore] // Requires Ollama to be running
async fn test_chat_with_different_temperatures() {
    let backend = OllamaBackend::new("http://localhost:11434".to_string());

    let base_messages = vec![
        Message::system("You are a creative writer."),
        Message::user("Write one sentence about the ocean."),
    ];

    for temp in [0.1, 0.5, 1.0, 1.5] {
        let params = ChatParams {
            model: "llama3.2".to_string(),
            temperature: Some(temp),
            max_tokens: Some(100),
        };

        match backend.chat(base_messages.clone(), &params).await {
            Ok(response) => {
                println!("Temperature {}: {}", temp, response);
            }
            Err(e) => {
                eprintln!("Failed at temperature {}: {}", temp, e);
            }
        }
    }
}
use crucible_cli::agents::backend::ollama::OllamaBackend;
use crucible_cli::agents::backend::{Backend, ChatParams, Message, Model};
