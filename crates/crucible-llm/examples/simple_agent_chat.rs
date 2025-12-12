//! Simple agent chat example with tool calling
//!
//! This example demonstrates how to use the AgentRuntime with a LlmProvider
//! and ToolExecutor to create an interactive agent.
//!
//! By default uses Ollama at http://localhost:11434.
//! Override via config file ~/.config/crucible/config.toml:
//!
//! ```toml
//! [chat]
//! provider = "ollama"
//! endpoint = "http://your-custom-endpoint:11434"
//! model = "llama3.2"
//! ```
//!
//! Usage:
//!   cargo run -p crucible-llm --example simple_agent_chat

use anyhow::Result;
use async_trait::async_trait;
use crucible_core::traits::{
    ExecutionContext, LlmMessage, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use crucible_llm::{create_chat_provider, AgentRuntime};
use std::io::{self, Write};

/// Simple tool executor with a few demo tools
struct DemoToolExecutor;

#[async_trait]
impl ToolExecutor for DemoToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        match name {
            "get_current_time" => {
                let now = chrono::Utc::now();
                Ok(serde_json::json!({
                    "time": now.to_rfc3339(),
                    "timestamp": now.timestamp(),
                }))
            }
            "calculate" => {
                let expression = params["expression"].as_str().ok_or_else(|| {
                    ToolError::InvalidParameters("Missing expression".to_string())
                })?;

                // Simple calculator (just for demo - would use a proper parser in production)
                match expression {
                    expr if expr.contains('+') => {
                        let parts: Vec<&str> = expr.split('+').collect();
                        if parts.len() == 2 {
                            let a: f64 = parts[0].trim().parse().map_err(|_| {
                                ToolError::InvalidParameters("Invalid number".to_string())
                            })?;
                            let b: f64 = parts[1].trim().parse().map_err(|_| {
                                ToolError::InvalidParameters("Invalid number".to_string())
                            })?;
                            Ok(serde_json::json!({ "result": a + b }))
                        } else {
                            Err(ToolError::InvalidParameters(
                                "Invalid expression".to_string(),
                            ))
                        }
                    }
                    _ => Err(ToolError::InvalidParameters(
                        "Only addition (+) is supported in this demo".to_string(),
                    )),
                }
            }
            "echo" => {
                let message = params["message"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Missing message".to_string()))?;
                Ok(serde_json::json!({
                    "echoed": message,
                }))
            }
            _ => Err(ToolError::NotFound(format!("Tool {} not found", name))),
        }
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        Ok(vec![
            ToolDefinition::new("get_current_time", "Get the current time in UTC").with_parameters(
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            ToolDefinition::new(
                "calculate",
                "Calculate a simple math expression (addition only)",
            )
            .with_parameters(serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Math expression like '5 + 3'"
                    }
                },
                "required": ["expression"]
            })),
            ToolDefinition::new("echo", "Echo back a message").with_parameters(serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to echo"
                    }
                },
                "required": ["message"]
            })),
        ])
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🤖 Simple Agent Chat with Tool Calling");
    println!("=====================================\n");

    // Use default configuration
    let chat_config = crucible_config::ChatConfig::default();

    println!("Provider: {:?}", chat_config.provider);
    println!("Model: {}", chat_config.chat_model());
    println!("Endpoint: {}\n", chat_config.llm_endpoint());

    // Create provider
    let provider = create_chat_provider(&chat_config).await?;

    // Create tool executor
    let executor = Box::new(DemoToolExecutor);

    // Create agent runtime
    let mut runtime = AgentRuntime::new(provider, executor).with_max_iterations(5);

    // Set system prompt
    runtime.set_system_prompt(
        "You are a helpful assistant with access to tools. \
         Use tools when appropriate to answer questions accurately. \
         Always explain what you're doing before calling a tool."
            .to_string(),
    );

    println!("Available tools:");
    println!("  - get_current_time: Get the current UTC time");
    println!("  - calculate: Do simple addition (e.g., '5 + 3')");
    println!("  - echo: Echo a message back\n");

    println!("Type your message (or 'quit' to exit):\n");

    // Interactive loop
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "quit" || input == "exit" {
            println!("Goodbye!");
            break;
        }

        // Run conversation
        match runtime.send_message(input.to_string()).await {
            Ok(response) => {
                println!("\n🤖 Assistant: {}", response.message.content);
                println!(
                    "   (tokens: {} prompt, {} completion)\n",
                    response.usage.prompt_tokens, response.usage.completion_tokens
                );
            }
            Err(e) => {
                eprintln!("❌ Error: {}\n", e);
            }
        }
    }

    Ok(())
}
