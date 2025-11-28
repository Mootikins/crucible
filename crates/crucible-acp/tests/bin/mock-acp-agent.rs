//! Mock ACP agent binary for integration testing
//!
//! This binary can be spawned as a subprocess to simulate a real ACP agent.
//! It reads JSON-RPC messages from stdin and writes responses to stdout.
//!
//! Usage:
//!   mock-acp-agent [--behavior <opencode|claude-acp|gemini|codex>]
//!                  [--protocol-version <version>]
//!                  [--delay <ms>]
//!                  [--inject-errors]

use std::env;

// Include the mock agent support module
// Note: In tests, we can access this via path
#[path = "../support/mock_stdio_agent.rs"]
mod mock_stdio_agent;

use mock_stdio_agent::{AgentBehavior, MockStdioAgent, MockStdioAgentConfig};

fn main() {
    // Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    // Default to OpenCode behavior (most common for testing)
    let mut config = MockStdioAgentConfig::opencode();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--behavior" => {
                if i + 1 < args.len() {
                    config.behavior = match args[i + 1].as_str() {
                        "opencode" => AgentBehavior::OpenCode,
                        "claude-acp" => AgentBehavior::ClaudeAcp,
                        "gemini" => AgentBehavior::Gemini,
                        "codex" => AgentBehavior::Codex,
                        _ => {
                            eprintln!("Unknown behavior: {}", args[i + 1]);
                            std::process::exit(1);
                        }
                    };
                    i += 2;
                } else {
                    eprintln!("Missing value for --behavior");
                    std::process::exit(1);
                }
            }
            "--protocol-version" => {
                if i + 1 < args.len() {
                    config.protocol_version = args[i + 1].parse().unwrap_or(1);
                    i += 2;
                } else {
                    eprintln!("Missing value for --protocol-version");
                    std::process::exit(1);
                }
            }
            "--delay" => {
                if i + 1 < args.len() {
                    let delay: u64 = args[i + 1].parse().unwrap_or(0);
                    config.response_delay_ms = Some(delay);
                    i += 2;
                } else {
                    eprintln!("Missing value for --delay");
                    std::process::exit(1);
                }
            }
            "--inject-errors" => {
                config.inject_errors = true;
                i += 1;
            }
            "--help" | "-h" => {
                println!("Mock ACP Agent - Integration Testing Tool");
                println!();
                println!("Usage:");
                println!("  mock-acp-agent [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --behavior <type>        Agent behavior type (opencode, claude-acp, gemini, codex)");
                println!("  --protocol-version <n>   Protocol version to advertise (default: 1)");
                println!("  --delay <ms>             Response delay in milliseconds");
                println!("  --inject-errors          Inject errors in responses");
                println!("  --help, -h               Show this help message");
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }

    // Create and run the mock agent
    let mut agent = MockStdioAgent::new(config);
    if let Err(e) = agent.run() {
        eprintln!("Mock agent error: {}", e);
        std::process::exit(1);
    }
}
