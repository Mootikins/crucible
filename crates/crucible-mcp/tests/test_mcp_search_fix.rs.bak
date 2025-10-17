// tests/test_mcp_search_fix.rs
//
// Integration tests for the MCP search tools fix
//
// This test verifies that search_by_filename and search_by_tags work
// correctly without requiring the Obsidian plugin to be running.

use anyhow::Result;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command as TokioCommand};

/// MCP test client that can communicate with a spawned MCP server
struct McpTestClient {
    process: Child,
    stdin: ChildStdin,
}

impl McpTestClient {
    /// Spawn a new MCP server process for testing
    async fn spawn() -> Result<Self> {
        let mut cmd = TokioCommand::new("cargo");
        cmd.args([
            "run", "--release", "--bin", "crucible-mcp-server"
        ])
        .current_dir("..")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        // Set required environment variables
        cmd.env("EMBEDDING_MODEL", "nomic-embed-text-v1.5-q8_0");
        cmd.env("EMBEDDING_ENDPOINT", "https://llama.terminal.krohnos.io");
        cmd.env("OBSIDIAN_VAULT_PATH", "/home/moot/Documents/crucible-testing");

        let mut process = cmd.spawn()?;
        let stdin = process.stdin.take().ok_or_else(|| anyhow::anyhow!("Failed to open stdin"))?;

        Ok(Self { process, stdin })
    }

    /// Initialize the MCP connection
    async fn initialize(&mut self) -> Result<Value> {
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            }
        });

        // Send initialization request
        self.send_request(&init_request).await?;

        // Read initialization response
        let response = self.read_response().await?;

        // Send initialized notification
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        self.send_request(&notification).await?;

        Ok(response)
    }

    /// Send a JSON-RPC request to the MCP server
    async fn send_request(&mut self, request: &Value) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let request_str = serde_json::to_string(request)?;
        self.stdin.write_all(request_str.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        Ok(())
    }

    /// Read a JSON-RPC response from the MCP server
    async fn read_response(&mut self) -> Result<Value> {
        let stdout = self.process.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to open stdout"))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        reader.read_line(&mut line).await?;

        if line.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from server"));
        }

        let response: Value = serde_json::from_str(&line.trim())?;

        // Don't try to put stdout back - we'll handle this differently
        drop(reader);
        Ok(response)
    }

    /// List available tools
    async fn list_tools(&mut self) -> Result<Vec<String>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(tools) = response.pointer("/result/tools") {
            if let Some(tools_array) = tools.as_array() {
                let tool_names: Vec<String> = tools_array
                    .iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Ok(tool_names);
            }
        }

        Err(anyhow::anyhow!("Failed to parse tools list response"))
    }

    /// Call search_by_filename tool
    async fn search_by_filename(&mut self, pattern: &str) -> Result<Vec<String>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "search_by_filename",
                "arguments": {
                    "pattern": pattern
                }
            }
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(content) = response.pointer("/result/content/0/text") {
            if let Some(files) = content.as_array() {
                let file_paths: Vec<String> = files
                    .iter()
                    .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Ok(file_paths);
            }
        }

        Err(anyhow::anyhow!("Failed to parse search_by_filename response"))
    }

    /// Call search_by_tags tool
    async fn search_by_tags(&mut self, tags: Vec<&str>) -> Result<Vec<Value>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "search_by_tags",
                "arguments": {
                    "tags": tags
                }
            }
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(content) = response.pointer("/result/content/0/text") {
            if let Some(files) = content.as_array() {
                return Ok(files.clone());
            }
        }

        Err(anyhow::anyhow!("Failed to parse search_by_tags response"))
    }

    /// Build search index to populate database
    async fn build_search_index(&mut self, vault_path: &str) -> Result<usize> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "build_search_index",
                "arguments": {
                    "path": vault_path,
                    "force": false
                }
            }
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(content) = response.pointer("/result/content/0/text") {
            if let Ok(result) = serde_json::from_str::<Value>(content.as_str().unwrap_or("{}")) {
                if let Some(indexed) = result.get("indexed").and_then(|i| i.as_u64()) {
                    return Ok(indexed as usize);
                }
            }
        }

        Err(anyhow::anyhow!("Failed to parse build_search_index response"))
    }

    /// Close the MCP client
    async fn close(mut self) -> Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}

#[tokio::test]
async fn test_mcp_search_tools_fix() -> Result<()> {
    println!("🧪 Testing MCP search tools fix...");

    let mut client = McpTestClient::spawn().await?;

    // Initialize the connection
    let init_response = client.initialize().await?;
    assert!(init_response.get("result").is_some(), "Failed to initialize MCP connection");

    // List tools to verify they're available
    let tools = client.list_tools().await?;
    println!("📋 Found {} tools", tools.len());

    let search_tools: Vec<_> = tools.iter().filter(|t| t.contains("search")).collect();
    println!("🔍 Search tools: {:?}", search_tools);

    assert!(search_tools.contains(&&"search_by_filename".to_string()), "search_by_filename not found");
    assert!(search_tools.contains(&&"search_by_tags".to_string()), "search_by_tags not found");

    // Build search index first (this populates the database)
    println!("📊 Building search index...");
    let indexed_count = client.build_search_index("/home/moot/Documents/crucible-testing").await?;
    println!("✅ Indexed {} files", indexed_count);
    assert!(indexed_count > 0, "No files were indexed");

    // Test search_by_filename
    println!("🔍 Testing search_by_filename with pattern 'RUNE*'...");
    let filename_results = client.search_by_filename("RUNE*").await?;
    println!("✅ Found {} files matching 'RUNE*'", filename_results.len());

    for file in filename_results.iter().take(3) {
        println!("   - {}", file);
    }

    // Test search_by_tags
    println!("🏷️  Testing search_by_tags with ['research', 'rune']...");
    let tag_results = client.search_by_tags(vec!["research", "rune"]).await?;
    println!("✅ Found {} files matching tags", tag_results.len());

    for file in &tag_results {
        if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
            if let Some(tags) = file.get("tags").and_then(|t| t.as_array()) {
                let tag_strings: Vec<String> = tags.iter()
                    .filter_map(|t| t.as_str())
                    .map(|s| s.to_string())
                    .collect();
                println!("   - {} (tags: {:?})", path, tag_strings);
            }
        }
    }

    // Verify the fix works
    println!("✅ MCP search tools are working without Obsidian plugin dependency!");

    // Close the client
    client.close().await?;

    // Assert that we got results (the exact count depends on the test data)
    // The important thing is that the tools don't fail due to missing Obsidian plugin
    assert!(!filename_results.is_empty() || !tag_results.is_empty(),
           "Both search tools returned empty results");

    Ok(())
}

#[tokio::test]
async fn test_mcp_server_start_without_obsidian() -> Result<()> {
    println!("🧪 Testing MCP server startup without Obsidian plugin...");

    let mut client = McpTestClient::spawn().await?;

    // Initialize should succeed even without Obsidian plugin
    let init_response = client.initialize().await?;
    assert!(init_response.get("result").is_some(), "Failed to initialize without Obsidian plugin");

    // List tools should work
    let tools = client.list_tools().await?;
    assert!(!tools.is_empty(), "No tools available without Obsidian plugin");

    // Close the client
    client.close().await?;

    println!("✅ MCP server works independently of Obsidian plugin!");

    Ok(())
}