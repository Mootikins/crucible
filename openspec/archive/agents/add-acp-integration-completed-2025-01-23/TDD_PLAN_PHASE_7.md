# Phase 7: MCP Server Integration - TDD Plan

## Overview

**Goal**: Expose Crucible tools to ACP agents via embedded MCP server using the ACP protocol's `mcp_servers` field in `NewSessionRequest`.

**Key Insight**: ACP 0.7 includes built-in MCP server exposure. The client publishes MCP servers to the agent via `NewSessionRequest.mcp_servers`, and the agent automatically connects to them.

**Architecture**:
```
Crucible CLI
├── Spawns MCP server process: `cru mcp-server --kiln /path`
├── Creates CrucibleClient (ACP Client implementation)
├── Spawns agent with spawn_agent()
└── Calls new_session() with mcp_servers: [McpServer::Stdio { command, args }]

Agent receives NewSessionRequest
├── Connects to MCP server via stdio
├── Discovers tools (create_note, read_note, semantic_search, etc.)
└── Can call both:
    - ACP methods: read_text_file, write_text_file (CrucibleClient)
    - MCP tools: create_note, semantic_search (CrucibleMcpServer)
```

---

## TDD Cycle 1: Unified MCP ServerHandler

**Test File**: `crates/crucible-tools/src/mcp_server.rs` (tests module)

### Red: Write Failing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_server_creation() {
        let temp = TempDir::new().unwrap();
        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string()
        );
        // Should create successfully
    }

    #[test]
    fn test_server_info() {
        let temp = TempDir::new().unwrap();
        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string()
        );
        let info = server.get_info();

        assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[tokio::test]
    async fn test_tool_routing() {
        let temp = TempDir::new().unwrap();
        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string()
        );

        // Verify tools are available
        let tools = server.list_tools(ListToolsRequest {}).await.unwrap();

        // Should have 10 tools
        assert_eq!(tools.tools.len(), 10);

        // Check specific tools exist
        let tool_names: Vec<&str> = tools.tools.iter()
            .map(|t| t.name.as_str())
            .collect();

        assert!(tool_names.contains(&"create_note"));
        assert!(tool_names.contains(&"read_note"));
        assert!(tool_names.contains(&"update_note"));
        assert!(tool_names.contains(&"delete_note"));
        assert!(tool_names.contains(&"list_notes"));
        assert!(tool_names.contains(&"read_metadata"));
        assert!(tool_names.contains(&"semantic_search"));
        assert!(tool_names.contains(&"text_search"));
        assert!(tool_names.contains(&"get_kiln_info"));
    }
}
```

### Green: Implement

```rust
// crates/crucible-tools/src/mcp_server.rs

use rmcp::{
    handler::server::tool::ToolRouter,
    model::*,
    tool_handler, ServerHandler, ServiceExt,
    transport::stdio,
};
use crate::{NoteTools, SearchTools, KilnTools};
use std::sync::Arc;
use crucible_core::traits::{KnowledgeRepository, EmbeddingProvider};

/// Unified MCP server exposing all Crucible tools
#[derive(Clone)]
pub struct CrucibleMcpServer {
    note_tools: NoteTools,
    search_tools: SearchTools,
    kiln_tools: KilnTools,
}

impl CrucibleMcpServer {
    /// Create a new MCP server for a kiln
    pub fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            note_tools: NoteTools::new(kiln_path.clone()),
            search_tools: SearchTools::new(
                kiln_path.clone(),
                knowledge_repo,
                embedding_provider,
            ),
            kiln_tools: KilnTools::new(kiln_path),
        }
    }

    /// Start serving via stdio transport
    pub async fn serve_stdio(self) -> rmcp::Result<()> {
        let service = self.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}

#[tool_handler(
    note_tools,
    search_tools,
    kiln_tools,
)]
impl ServerHandler for CrucibleMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Crucible knowledge management system. \
                 Use these tools to create, read, update, delete, and search notes \
                 in your personal knowledge base (kiln)."
                    .to_string()
            ),
        }
    }
}
```

### Refactor

- Export from `lib.rs`
- Add documentation
- Verify SOLID principles

---

## TDD Cycle 2: CLI MCP Server Subcommand

**Test File**: `crates/crucible-cli/tests/integration_tests.rs`

### Red: Write Failing Tests

```rust
#[test]
fn test_mcp_server_command_help() {
    let output = Command::cargo_bin("cru")
        .unwrap()
        .arg("mcp-server")
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("kiln"));
}

#[tokio::test]
async fn test_mcp_server_stdio() {
    let temp = TempDir::new().unwrap();

    // Spawn MCP server
    let mut child = Command::cargo_bin("cru")
        .unwrap()
        .arg("mcp-server")
        .arg("--kiln")
        .arg(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn MCP server");

    // Send MCP initialize request
    let stdin = child.stdin.as_mut().unwrap();
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {}
        }
    });

    writeln!(stdin, "{}", serde_json::to_string(&init_request).unwrap()).unwrap();

    // Read response
    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();

    // Verify it's a valid MCP response
    let parsed: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert!(parsed["result"].is_object());

    child.kill().unwrap();
}
```

### Green: Implement

```rust
// crates/crucible-cli/src/main.rs

#[derive(Parser)]
#[command(name = "cru")]
#[command(about = "Crucible knowledge management CLI")]
enum Cli {
    // ... existing commands

    /// Start MCP server (internal use - for agent integration)
    #[command(hide = true)]
    McpServer {
        /// Path to the kiln directory
        #[arg(long)]
        kiln: PathBuf,
    },
}

// In main():
match cli {
    Cli::McpServer { kiln } => {
        commands::mcp_server::execute(kiln).await?;
    }
    // ... other commands
}
```

```rust
// crates/crucible-cli/src/commands/mcp_server.rs

use anyhow::Result;
use std::path::PathBuf;
use crucible_tools::CrucibleMcpServer;
use std::sync::Arc;

pub async fn execute(kiln_path: PathBuf) -> Result<()> {
    // Initialize knowledge repo and embedding provider
    let config = crucible_config::load_config()?;
    let knowledge_repo = Arc::new(/* create repo */);
    let embedding_provider = Arc::new(/* create provider */);

    // Create and serve MCP server
    let server = CrucibleMcpServer::new(
        kiln_path.to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    server.serve_stdio().await?;
    Ok(())
}
```

### Refactor

- Error handling
- Logging setup
- Resource cleanup

---

## TDD Cycle 3: Populate mcp_servers in NewSessionRequest

**Test File**: `crates/crucible-acp/src/acp_client.rs` (tests module)

### Red: Write Failing Tests

```rust
#[tokio::test]
async fn test_spawn_agent_with_mcp_server() {
    use agent_client_protocol::McpServer;

    let temp = TempDir::new().unwrap();
    let client = CrucibleClient::new(temp.path().to_path_buf(), false);

    let mcp_config = McpServer::Stdio {
        name: "crucible".to_string(),
        command: "cru".to_string(),
        args: vec![
            "mcp-server".to_string(),
            "--kiln".to_string(),
            temp.path().to_str().unwrap().to_string(),
        ],
        env: vec![],
    };

    // This should be part of spawn_agent or a wrapper
    // We need a way to pass mcp_servers config
}

#[tokio::test]
async fn test_new_session_includes_mcp_servers() {
    // Mock agent that verifies NewSessionRequest includes mcp_servers
    let mock_agent = MockAgent::new()
        .with_handler(|request| {
            if let ClientRequest::NewSessionRequest(req) = request {
                assert!(!req.mcp_servers.is_empty());
                assert_eq!(req.mcp_servers[0].name(), "crucible");
            }
        });

    // ... spawn and verify
}
```

### Green: Implement

**Option A**: Add `mcp_servers` parameter to `spawn_agent()`

```rust
// crates/crucible-acp/src/acp_client.rs

pub async fn spawn_agent(
    agent_path: PathBuf,
    client: CrucibleClient,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
) -> Result<(
    ClientSideConnection,
    Child,
    std::pin::Pin<Box<dyn std::future::Future<Output = AcpResult<()>>>>,
)> {
    // ... existing spawn logic

    // Store mcp_servers in connection for use in new_session
    // OR pass via connection initialization
}
```

**Option B**: Store in `CrucibleClient` and use in callbacks

```rust
pub struct CrucibleClient {
    kiln_path: PathBuf,
    read_only: bool,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
    notifications: Arc<Mutex<Vec<SessionNotification>>>,
}

impl CrucibleClient {
    pub fn with_mcp_servers(
        mut self,
        mcp_servers: Vec<agent_client_protocol::McpServer>
    ) -> Self {
        self.mcp_servers = mcp_servers;
        self
    }
}
```

**Recommendation**: Option A (explicit parameter) follows DI better.

### Refactor

- Document MCP server configuration
- Add examples
- Validate mcp_servers config

---

## TDD Cycle 4: CLI Integration - Spawn MCP + Agent

**Test File**: `crates/crucible-cli/tests/e2e_mcp_test.rs` (NEW)

### Red: Write Failing Tests

```rust
#[tokio::test]
async fn test_chat_with_mcp_tools() {
    let temp_kiln = TempDir::new().unwrap();

    // Create a test note
    std::fs::write(
        temp_kiln.path().join("test.md"),
        "# Test Note\nThis is a test."
    ).unwrap();

    // Start chat with mock agent
    let config = Config::default();
    let result = commands::chat::execute(
        &config,
        Some("List all notes in the kiln"),
        /* other params */
    ).await;

    assert!(result.is_ok());
    // Verify agent received MCP tools
    // Verify agent can call list_notes
}

#[tokio::test]
async fn test_mcp_server_spawning() {
    // Test that CLI correctly spawns MCP server
    // and passes it to agent in NewSessionRequest
}
```

### Green: Implement

```rust
// crates/crucible-cli/src/commands/chat.rs

pub async fn execute(...) -> Result<()> {
    // ... existing setup

    // 1. Spawn MCP server process
    let mcp_child = spawn_mcp_server(&kiln_path).await?;

    // 2. Create MCP server config for agent
    let mcp_servers = vec![
        agent_client_protocol::McpServer::Stdio {
            name: "crucible".to_string(),
            command: env::current_exe()?
                .to_str().unwrap().to_string(),
            args: vec![
                "mcp-server".to_string(),
                "--kiln".to_string(),
                kiln_path.to_str().unwrap().to_string(),
            ],
            env: vec![],
        }
    ];

    // 3. Create ACP client
    let client = CrucibleClient::new(kiln_path.clone(), read_only);

    // 4. Spawn agent with MCP config
    let (connection, child, io_task) = spawn_agent(
        agent_path,
        client,
        mcp_servers,
    ).await?;

    // 5. Run in LocalSet
    let local = LocalSet::new();
    local.run_until(async {
        tokio::spawn_local(io_task);

        // ... existing chat loop

    }).await?;

    // 6. Cleanup
    drop(connection);
    child.kill().await?;
    mcp_child.kill().await?;

    Ok(())
}

async fn spawn_mcp_server(kiln_path: &PathBuf) -> Result<Child> {
    Ok(tokio::process::Command::new(env::current_exe()?)
        .arg("mcp-server")
        .arg("--kiln")
        .arg(kiln_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?)
}
```

### Refactor

- Extract MCP spawning logic
- Add error recovery
- Graceful shutdown

---

## TDD Cycle 5: MockAgent MCP Verification

**Test File**: `crates/crucible-acp/src/mock_agent.rs` (enhancement)

### Red: Write Failing Tests

```rust
#[tokio::test]
async fn test_mock_agent_receives_mcp_servers() {
    let mock = MockAgent::new();

    let (rx, handler) = mock.spawn_with_handler(|request| {
        if let ClientRequest::NewSessionRequest(req) = request {
            assert!(!req.mcp_servers.is_empty());
            assert_eq!(req.mcp_servers.len(), 1);
            assert_eq!(req.mcp_servers[0].name(), "crucible");
        }
    });

    // ... send NewSessionRequest with mcp_servers
}
```

### Green: Implement

```rust
// Enhance MockAgent to verify MCP server configs
impl MockAgent {
    pub fn with_mcp_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&Vec<McpServer>) + Send + 'static
    {
        // Store validator for NewSessionRequest
        self
    }
}
```

### Refactor

- Document MCP testing utilities
- Add assertion helpers

---

## Implementation Order

1. **Cycle 1**: MCP ServerHandler (2-3 hours)
   - ✅ Create `mcp_server.rs`
   - ✅ Combine tool routers
   - ✅ Implement ServerHandler
   - ✅ Write tests

2. **Cycle 2**: CLI Subcommand (1-2 hours)
   - ✅ Add `mcp-server` command
   - ✅ Wire up stdio serving
   - ✅ Test with stdio I/O

3. **Cycle 3**: ACP Integration (2-3 hours)
   - ✅ Add mcp_servers to spawn_agent
   - ✅ Populate NewSessionRequest
   - ✅ Test with MockAgent

4. **Cycle 4**: CLI Full Integration (3-4 hours)
   - ✅ Spawn MCP server in chat
   - ✅ Pass to agent
   - ✅ E2E testing

5. **Cycle 5**: Verification (1-2 hours)
   - ✅ Enhance MockAgent
   - ✅ Add MCP-specific tests
   - ✅ Document

---

## Success Criteria

- [ ] All tests pass (target: 160+ tests)
- [ ] MCP server exposes 10 Crucible tools
- [ ] Agent receives mcp_servers in NewSessionRequest
- [ ] Tools discoverable via MCP protocol
- [ ] E2E test with MockAgent works
- [ ] Can test locally with `claude` if available
- [ ] Zero test failures
- [ ] SOLID principles maintained
- [ ] Documentation updated

---

## Dependencies to Resolve

1. **SearchTools dependencies**: Need `KnowledgeRepository` and `EmbeddingProvider`
   - **Solution**: Create mock/stub implementations for MCP server
   - **Or**: Make search tools optional in MCP server

2. **LocalSet requirement**: Verify if needed for MCP server
   - **Solution**: Test with regular tokio::spawn first

3. **Agent MCP discovery**: How does `claude` actually discover MCP servers?
   - **Solution**: Check ACP spec implementation in agent
   - **Fallback**: Test with MockAgent first

---

## Timeline

- **Day 1**: Cycles 1-2 (MCP server + CLI command)
- **Day 2**: Cycles 3-4 (ACP integration + CLI wiring)
- **Day 3**: Cycle 5 + documentation + testing

**Total**: 2-3 days for complete implementation
