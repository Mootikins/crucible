// crates/crucible-mcp-client/src/main.rs
//! MCP Client for testing and exploring the Crucible MCP server
//!
//! This client connects to the crucible-mcp-server via stdio transport
//! and provides commands to exercise different MCP protocol features.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};
use tracing::{debug, info, warn};

#[derive(Parser, Debug)]
#[command(name = "crucible-mcp-client")]
#[command(about = "MCP client for testing Crucible MCP server", long_about = None)]
struct Cli {
    /// Path to the crucible-mcp-server binary
    #[arg(long, default_value = "crucible-mcp-server")]
    server_path: String,

    /// Obsidian vault path
    #[arg(long, env = "OBSIDIAN_VAULT_PATH")]
    vault_path: Option<String>,

    /// Rune tool directory
    #[arg(long, env = "RUNE_TOOL_DIR")]
    tool_dir: Option<String>,

    /// Embedding model
    #[arg(long, env = "EMBEDDING_MODEL", default_value = "nomic-embed-text-v1.5-q8_0")]
    embedding_model: String,

    /// Embedding endpoint
    #[arg(long, env = "EMBEDDING_ENDPOINT", default_value = "http://localhost:11434")]
    embedding_endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all available tools
    ListTools,

    /// Call a tool by name
    CallTool {
        /// Tool name
        name: String,

        /// Arguments as JSON string
        #[arg(long)]
        args: Option<String>,
    },

    /// Test Rune tools specifically
    TestRune,

    /// Interactive shell for exploring MCP
    Shell,
}

//JSON-RPC structures
#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

struct McpClient {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _child: Child,
    next_id: u64,
}

impl McpClient {
    async fn new(mut child: Child) -> Result<Self> {
        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stdout = BufReader::new(stdout);

        let mut client = Self {
            stdin,
            stdout,
            _child: child,
            next_id: 1,
        };

        // Send initialize request
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "crucible-mcp-client",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        client.send_request("initialize", Some(init_params)).await?;

        // Send initialized notification
        client.send_notification("notifications/initialized", None).await?;

        Ok(client)
    }

    async fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let notification_json = serde_json::to_string(&notification)?;
        debug!("Sending notification: {}", notification_json);
        self.stdin.write_all(notification_json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        Ok(())
    }

    async fn send_request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        // Send request
        let request_json = serde_json::to_string(&request)?;
        debug!("Sending: {}", request_json);
        self.stdin.write_all(request_json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        // Read response
        let mut response_line = String::new();
        self.stdout.read_line(&mut response_line).await?;
        debug!("Received: {}", response_line);

        let response: JsonRpcResponse = serde_json::from_str(&response_line)?;

        if let Some(error) = response.error {
            anyhow::bail!("RPC Error: {}", error);
        }

        response
            .result
            .context("No result in response")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Start the MCP server as subprocess
    info!("Starting MCP server: {}", cli.server_path);
    let mut server_cmd = Command::new(&cli.server_path);

    // Set environment variables
    if let Some(vault) = &cli.vault_path {
        server_cmd.env("OBSIDIAN_VAULT_PATH", vault);
    }
    if let Some(tool_dir) = &cli.tool_dir {
        server_cmd.env("RUNE_TOOL_DIR", tool_dir);
    }
    server_cmd.env("EMBEDDING_MODEL", &cli.embedding_model);
    server_cmd.env("EMBEDDING_ENDPOINT", &cli.embedding_endpoint);

    server_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let server_process = server_cmd
        .spawn()
        .context("Failed to start MCP server")?;

    // Create MCP client
    info!("Connecting to MCP server...");
    let mut client = McpClient::new(server_process).await?;
    info!("Connected successfully!");

    // Execute command
    match cli.command {
        Commands::ListTools => list_tools(&mut client).await?,
        Commands::CallTool { name, args } => call_tool(&mut client, &name, args).await?,
        Commands::TestRune => test_rune_tools(&mut client).await?,
        Commands::Shell => interactive_shell(&mut client).await?,
    }

    Ok(())
}

async fn list_tools(client: &mut McpClient) -> Result<()> {
    info!("Listing tools...");

    let response = client.send_request("tools/list", None).await?;
    let tools = response["tools"]
        .as_array()
        .context("Expected tools array")?;

    println!("\n{} Tools Available:", tools.len());
    println!("{}", "=".repeat(80));

    // Separate native and Rune tools
    let mut native_tools = vec![];
    let mut rune_tools = vec![];

    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("unknown");
        if name.starts_with("hello_") || name.starts_with("read_note") || name.starts_with("search_notes") {
            rune_tools.push(tool);
        } else {
            native_tools.push(tool);
        }
    }

    if !native_tools.is_empty() {
        println!("\n📦 Native Tools ({}):", native_tools.len());
        for tool in native_tools {
            let name = tool["name"].as_str().unwrap_or("unknown");
            let desc = tool["description"].as_str().unwrap_or("");
            println!("  • {}", name);
            if !desc.is_empty() {
                println!("    {}", desc);
            }
        }
    }

    if !rune_tools.is_empty() {
        println!("\n⚡ Rune Tools ({}):", rune_tools.len());
        for tool in rune_tools {
            let name = tool["name"].as_str().unwrap_or("unknown");
            let desc = tool["description"].as_str().unwrap_or("");
            println!("  • {}", name);
            if !desc.is_empty() {
                println!("    {}", desc);
            }
        }
    }

    println!();
    Ok(())
}

async fn call_tool(client: &mut McpClient, name: &str, args: Option<String>) -> Result<()> {
    info!("Calling tool: {}", name);

    let arguments: Value = if let Some(args_str) = args {
        serde_json::from_str(&args_str)
            .context("Failed to parse arguments JSON")?
    } else {
        json!({})
    };

    let params = json!({
        "name": name,
        "arguments": arguments
    });

    let response = client.send_request("tools/call", Some(params)).await?;

    println!("\n🔧 Tool: {}", name);
    println!("{}", "=".repeat(80));

    let is_error = response["isError"].as_bool().unwrap_or(false);
    if is_error {
        println!("❌ Error:");
    } else {
        println!("✅ Success:");
    }

    if let Some(content_array) = response["content"].as_array() {
        for content in content_array {
            if let Some(text) = content["text"].as_str() {
                println!("{}", text);
            } else if content["type"].as_str() == Some("image") {
                println!("[Image: {} bytes]", content["data"].as_str().map(|s| s.len()).unwrap_or(0));
            } else if content["type"].as_str() == Some("resource") {
                println!("[Resource: {}]", content["uri"].as_str().unwrap_or(""));
            }
        }
    }

    println!();
    Ok(())
}

async fn test_rune_tools(client: &mut McpClient) -> Result<()> {
    info!("Testing Rune tools...");

    // First list tools to find Rune tools
    let response = client.send_request("tools/list", None).await?;
    let tools = response["tools"]
        .as_array()
        .context("Expected tools array")?;

    let rune_tools: Vec<_> = tools
        .iter()
        .filter(|t| {
            let name = t["name"].as_str().unwrap_or("");
            name.starts_with("hello_") || name.starts_with("read_note") || name.starts_with("search_notes")
        })
        .collect();

    if rune_tools.is_empty() {
        warn!("No Rune tools found!");
        println!("\n⚠️  No Rune tools detected.");
        println!("Make sure RUNE_TOOL_DIR is set correctly.");
        return Ok(());
    }

    println!("\n⚡ Testing {} Rune Tools", rune_tools.len());
    println!("{}", "=".repeat(80));

    // Test hello_world if available
    if let Some(hello_tool) = rune_tools.iter().find(|t| t["name"].as_str() == Some("hello_world")) {
        let tool_name = hello_tool["name"].as_str().unwrap();
        println!("\n1. Testing: {}", tool_name);

        let params = json!({
            "name": tool_name,
            "arguments": {
                "name": "Crucible"
            }
        });

        match client.send_request("tools/call", Some(params)).await {
            Ok(response) => {
                println!("   ✅ Success");
                if let Some(content_array) = response["content"].as_array() {
                    for content in content_array {
                        if let Some(text) = content["text"].as_str() {
                            println!("   {}", text);
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ Error: {}", e);
            }
        }
    }

    // Test search_notes if available
    if let Some(search_tool) = rune_tools.iter().find(|t| t["name"].as_str() == Some("search_notes")) {
        let tool_name = search_tool["name"].as_str().unwrap();
        println!("\n2. Testing: {}", tool_name);

        let params = json!({
            "name": tool_name,
            "arguments": {
                "query": "test"
            }
        });

        match client.send_request("tools/call", Some(params)).await {
            Ok(response) => {
                println!("   ✅ Success");
                if let Some(content_array) = response["content"].as_array() {
                    for content in content_array {
                        if let Some(text) = content["text"].as_str() {
                            // Truncate long output
                            let truncated = if text.len() > 200 {
                                format!("{}...", &text[..200])
                            } else {
                                text.to_string()
                            };
                            println!("   {}", truncated);
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ Error: {}", e);
            }
        }
    }

    println!();
    Ok(())
}

async fn interactive_shell(_client: &mut McpClient) -> Result<()> {
    println!("\n🐚 Interactive Shell (Not Yet Implemented)");
    println!("This will allow interactive exploration of MCP protocol features:");
    println!("  - Tools (list, call)");
    println!("  - Resources (list, read)");
    println!("  - Prompts (list, get)");
    println!("  - Logging");
    println!("\nComing soon!");
    Ok(())
}
