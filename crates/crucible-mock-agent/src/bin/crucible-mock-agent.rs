//! Crucible Mock Agent binary
//!
//! Standalone mock ACP agent that can be spawned for integration testing.
//!
//! Usage:
//!   crucible-mock-agent [options]
//!
//! Options:
//!   --behavior <name>      Agent behavior (opencode, claude-acp, streaming, etc.)
//!   --protocol-version <v> Protocol version to advertise (default: 1)
//!   --delay <ms>           Response delay in milliseconds
//!   --inject-errors        Inject random errors for testing
//!   --require-auth         Require authentication
//!   --chunk-delay <ms>     Delay between streaming chunks in ms
//!   --help                 Show this help message

use crucible_mock_agent::{AgentBehavior, MockAgent, MockAgentConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut config = MockAgentConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--behavior" => {
                if i + 1 < args.len() {
                    config.behavior = AgentBehavior::from_str(&args[i + 1])
                        .unwrap_or_else(|| {
                            eprintln!("Unknown behavior: {}", args[i + 1]);
                            eprintln!("Available: opencode, claude-acp, gemini, codex, streaming, streaming-slow, streaming-incomplete");
                            std::process::exit(1);
                        });
                    i += 1;
                }
            }
            "--protocol-version" => {
                if i + 1 < args.len() {
                    config.protocol_version = args[i + 1].parse().unwrap_or_else(|_| {
                        eprintln!("Invalid protocol version: {}", args[i + 1]);
                        std::process::exit(1);
                    });
                    i += 1;
                }
            }
            "--delay" => {
                if i + 1 < args.len() {
                    let delay = args[i + 1].parse().unwrap_or_else(|_| {
                        eprintln!("Invalid delay: {}", args[i + 1]);
                        std::process::exit(1);
                    });
                    config.response_delay_ms = Some(delay);
                    i += 1;
                }
            }
            "--inject-errors" => {
                config.inject_errors = true;
            }
            "--require-auth" => {
                config.requires_auth = true;
            }
            "--chunk-delay" => {
                if i + 1 < args.len() {
                    let delay = args[i + 1].parse().unwrap_or_else(|_| {
                        eprintln!("Invalid chunk delay: {}", args[i + 1]);
                        std::process::exit(1);
                    });
                    config.streaming_chunk_delay_ms = Some(delay);
                    i += 1;
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Create and run the agent
    let mut agent = MockAgent::new(config);

    if let Err(e) = agent.run() {
        eprintln!("Agent error: {}", e);
        std::process::exit(1);
    }
}

fn print_help() {
    println!("Crucible Mock Agent - ACP protocol mock agent for testing");
    println!();
    println!("USAGE:");
    println!("    crucible-mock-agent [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --behavior <name>       Agent behavior profile:");
    println!("                              opencode         - OpenCode-compatible");
    println!("                              claude-acp       - Claude ACP-compatible");
    println!("                              gemini           - Gemini-compatible");
    println!("                              codex            - Codex-compatible");
    println!("                              streaming        - Streaming responses (4 chunks)");
    println!("                              streaming-slow   - Slow streaming (500ms delays)");
    println!("                              streaming-incomplete - Never sends final response");
    println!("    --protocol-version <v>  Protocol version (default: 1)");
    println!("    --delay <ms>            Response delay in milliseconds");
    println!("    --inject-errors         Inject errors for testing error handling");
    println!("    --require-auth          Require authentication");
    println!("    --chunk-delay <ms>      Delay between streaming chunks");
    println!("    --help, -h              Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    # Basic streaming agent");
    println!("    crucible-mock-agent --behavior streaming");
    println!();
    println!("    # Slow streaming for timeout testing");
    println!("    crucible-mock-agent --behavior streaming-slow");
    println!();
    println!("    # OpenCode-compatible with auth");
    println!("    crucible-mock-agent --behavior opencode --require-auth");
}
