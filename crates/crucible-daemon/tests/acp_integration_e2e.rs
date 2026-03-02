//! ACP integration E2E tests
//!
//! Verifies that ACP plumbing remains intact after crate absorptions:
//! - Tool discovery via crucible-acp ToolRegistry
//! - Tool dispatch routing via DaemonToolDispatcher
//! - Delegation context construction
//! - MCP host initialization
//! - DaemonToolsBridge wiring to DaemonToolsApi

use crucible_acp::tools::{discover_tools, ToolRegistry};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::tool_dispatch::{DaemonToolDispatcher, ToolDispatcher};
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::tools::DelegationContext;
use crucible_daemon::tools_bridge::DaemonToolsBridge;
use crucible_daemon::InProcessMcpHost;
use crucible_lua::DaemonToolsApi;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Mock implementations (mirrors mcp_host.rs test mocks)
// ============================================================================

struct MockKnowledgeRepository;
struct MockEmbeddingProvider;

#[async_trait::async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }
}

// ============================================================================
// Test 1: Tool discovery via crucible-acp ToolRegistry
// ============================================================================

#[test]
fn test_discover_tools_returns_tool_list() {
    let mut registry = ToolRegistry::new();
    let count = discover_tools(&mut registry, "/tmp/test-kiln")
        .expect("discover_tools should succeed");

    // discover_tools registers 10 tools: 6 note + 3 search + 1 kiln
    assert_eq!(count, 10, "Expected 10 discovered tools, got {count}");
    assert_eq!(
        registry.count(),
        10,
        "Registry should contain all 10 tools"
    );

    // Verify tool categories are present
    let tools = registry.list();
    let note_tools: Vec<_> = tools.iter().filter(|t| t.category == "notes").collect();
    let search_tools: Vec<_> = tools.iter().filter(|t| t.category == "search").collect();
    let kiln_tools: Vec<_> = tools.iter().filter(|t| t.category == "kiln").collect();

    assert_eq!(note_tools.len(), 6, "Expected 6 note tools");
    assert_eq!(search_tools.len(), 3, "Expected 3 search tools");
    assert_eq!(kiln_tools.len(), 1, "Expected 1 kiln tool");

    // Verify specific well-known tools exist
    assert!(registry.contains("create_note"), "Should have create_note");
    assert!(registry.contains("read_note"), "Should have read_note");
    assert!(
        registry.contains("semantic_search"),
        "Should have semantic_search"
    );
    assert!(
        registry.contains("get_kiln_info"),
        "Should have get_kiln_info"
    );

    // Verify tool descriptors have valid schemas
    for tool in registry.list() {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(
            !tool.description.is_empty(),
            "Tool '{}' should have a description",
            tool.name
        );
        assert_eq!(
            tool.input_schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "Tool '{}' schema should be type=object",
            tool.name
        );
    }
}

// ============================================================================
// Test 2: Tool dispatch routes to daemon tools via DaemonToolDispatcher
// ============================================================================

#[test]
fn test_tool_dispatch_routes_to_daemon_tools() {
    let workspace_tools = Arc::new(WorkspaceTools::new(&PathBuf::from("/tmp")));
    let dispatcher = DaemonToolDispatcher::new(workspace_tools);

    // Verify known workspace tools are recognized
    assert!(
        dispatcher.has_tool("read_file"),
        "Dispatcher should route read_file"
    );
    assert!(
        dispatcher.has_tool("edit_file"),
        "Dispatcher should route edit_file"
    );
    assert!(
        dispatcher.has_tool("write_file"),
        "Dispatcher should route write_file"
    );
    assert!(
        dispatcher.has_tool("bash"),
        "Dispatcher should route bash"
    );
    assert!(
        dispatcher.has_tool("glob"),
        "Dispatcher should route glob"
    );
    assert!(
        dispatcher.has_tool("grep"),
        "Dispatcher should route grep"
    );

    // Verify unknown tools are rejected
    assert!(
        !dispatcher.has_tool("nonexistent_tool"),
        "Dispatcher should not route nonexistent_tool"
    );
    assert!(
        !dispatcher.has_tool(""),
        "Dispatcher should not route empty string"
    );
}

#[tokio::test]
async fn test_tool_dispatch_executes_read_file() {
    let temp = TempDir::new().expect("temp dir");
    let test_file = temp.path().join("test.txt");
    std::fs::write(&test_file, "hello world").expect("write test file");

    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let dispatcher = DaemonToolDispatcher::new(workspace_tools);

    // Dispatch a read_file call
    let result: Result<serde_json::Value, String> = dispatcher
        .dispatch_tool(
            "read_file",
            serde_json::json!({ "path": test_file.to_string_lossy() }),
        )
        .await;

    assert!(
        result.is_ok(),
        "read_file dispatch should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    // The output should contain the file content
    let output_str = output.to_string();
    assert!(
        output_str.contains("hello world"),
        "read_file output should contain file content, got: {}",
        output_str
    );
}

// ============================================================================
// Test 3: Delegation context construction
// ============================================================================

#[test]
fn test_delegation_context_construction() {
    use crucible_config::DataClassification;
    use crucible_core::background::{
        BackgroundSpawner, JobError, JobId, JobInfo, JobResult,
    };
    use std::path::PathBuf as StdPathBuf;
    use std::time::Duration;

    // Minimal BackgroundSpawner mock using async_trait
    struct MockSpawner;

    #[async_trait::async_trait]
    impl BackgroundSpawner for MockSpawner {
        async fn spawn_bash(
            &self,
            _session_id: &str,
            _command: String,
            _workdir: Option<StdPathBuf>,
            _timeout: Option<Duration>,
        ) -> Result<JobId, JobError> {
            Ok("mock-bash-job".to_string())
        }

        async fn spawn_subagent(
            &self,
            _session_id: &str,
            _prompt: String,
            _context: Option<String>,
        ) -> Result<JobId, JobError> {
            Ok("mock-subagent-job".to_string())
        }

        fn list_jobs(&self, _session_id: &str) -> Vec<JobInfo> {
            vec![]
        }

        fn get_job_result(&self, _job_id: &JobId) -> Option<JobResult> {
            None
        }

        async fn cancel_job(&self, _job_id: &JobId) -> bool {
            false
        }
    }

    let spawner = Arc::new(MockSpawner) as Arc<dyn BackgroundSpawner>;
    let targets = vec!["claude".to_string(), "opencode".to_string()];

    let ctx = DelegationContext {
        background_spawner: spawner,
        session_id: "test-session-123".to_string(),
        targets: targets.clone(),
        enabled: true,
        depth: 0,
        data_classification: DataClassification::default(),
    };

    // Verify all fields are constructed correctly
    assert_eq!(ctx.session_id, "test-session-123");
    assert!(ctx.enabled);
    assert_eq!(ctx.depth, 0);
    assert_eq!(ctx.targets.len(), 2);
    assert!(ctx.targets.contains(&"claude".to_string()));
    assert!(ctx.targets.contains(&"opencode".to_string()));

    // Verify disabled delegation context
    let disabled_ctx = DelegationContext {
        background_spawner: Arc::new(MockSpawner),
        session_id: "disabled-session".to_string(),
        targets: vec![],
        enabled: false,
        depth: 0,
        data_classification: DataClassification::default(),
    };

    assert!(!disabled_ctx.enabled);
    assert!(disabled_ctx.targets.is_empty());
}

// ============================================================================
// Test 4: MCP host initializes without errors
// ============================================================================

#[tokio::test]
async fn test_mcp_host_initializes() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        None, // no delegation context
    )
    .await
    {
        Ok(host) => host,
        Err(err) => {
            // Permission denied can happen in CI containers
            let err_str = format!("{err:?}");
            if err_str.contains("Operation not permitted") {
                eprintln!("Skipping MCP host test (permission denied in environment): {err}");
                return;
            }
            panic!("InProcessMcpHost::start should succeed: {err:?}");
        }
    };

    // Verify server is bound to localhost
    let url = host.mcp_url();
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "MCP URL should bind to localhost, got: {url}"
    );
    assert!(
        url.ends_with("/mcp"),
        "MCP URL should end with /mcp, got: {url}"
    );

    // Verify port was assigned
    let port = host.address().port();
    assert!(port > 0, "MCP server should have a valid port, got: {port}");

    // Clean shutdown
    host.shutdown().await;
}

#[tokio::test]
async fn test_mcp_host_initializes_with_delegation_context() {
    use crucible_config::DataClassification;
    use crucible_core::background::{
        BackgroundSpawner, JobError, JobId, JobInfo, JobResult,
    };
    use std::path::PathBuf as StdPathBuf;
    use std::time::Duration;

    struct MockSpawner;

    #[async_trait::async_trait]
    impl BackgroundSpawner for MockSpawner {
        async fn spawn_bash(
            &self,
            _session_id: &str,
            _command: String,
            _workdir: Option<StdPathBuf>,
            _timeout: Option<Duration>,
        ) -> Result<JobId, JobError> {
            Ok("mock-bash-job".to_string())
        }

        async fn spawn_subagent(
            &self,
            _session_id: &str,
            _prompt: String,
            _context: Option<String>,
        ) -> Result<JobId, JobError> {
            Ok("mock-subagent-job".to_string())
        }

        fn list_jobs(&self, _session_id: &str) -> Vec<JobInfo> {
            vec![]
        }

        fn get_job_result(&self, _job_id: &JobId) -> Option<JobResult> {
            None
        }

        async fn cancel_job(&self, _job_id: &JobId) -> bool {
            false
        }
    }

    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let delegation_ctx = DelegationContext {
        background_spawner: Arc::new(MockSpawner),
        session_id: "mcp-delegation-test".to_string(),
        targets: vec!["claude".to_string()],
        enabled: true,
        depth: 0,
        data_classification: DataClassification::default(),
    };

    let host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        Some(delegation_ctx),
    )
    .await
    {
        Ok(host) => host,
        Err(err) => {
            let err_str = format!("{err:?}");
            if err_str.contains("Operation not permitted") {
                eprintln!("Skipping MCP host with delegation test: {err}");
                return;
            }
            panic!("InProcessMcpHost::start with delegation should succeed: {err:?}");
        }
    };

    // Verify server binds even with delegation context
    assert!(host.address().port() > 0);
    host.shutdown().await;
}

// ============================================================================
// Test 5: DaemonToolsBridge wires to DaemonToolsApi
// ============================================================================

#[tokio::test]
async fn test_tools_bridge_list_tools() {
    let temp = TempDir::new().expect("temp dir");
    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let bridge = DaemonToolsBridge::new(workspace_tools);

    // DaemonToolsApi::list_tools should return tool definitions as JSON
    let tools = bridge.list_tools().await.expect("list_tools should succeed");

    assert!(
        !tools.is_empty(),
        "Bridge should expose workspace tools, got empty list"
    );

    // Verify each tool has a name field
    for tool_json in &tools {
        assert!(
            tool_json.get("name").is_some(),
            "Each tool should have a name field: {:?}",
            tool_json
        );
    }
}

#[tokio::test]
async fn test_tools_bridge_call_tool_routes_correctly() {
    let temp = TempDir::new().expect("temp dir");
    let test_file = temp.path().join("bridge_test.txt");
    std::fs::write(&test_file, "bridge content").expect("write test file");

    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let bridge = DaemonToolsBridge::new(workspace_tools);

    // Call read_file through the bridge
    let result = bridge
        .call_tool(
            "read_file".to_string(),
            serde_json::json!({ "path": test_file.to_string_lossy() }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Bridge call_tool(read_file) should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let output_str = output.to_string();
    assert!(
        output_str.contains("bridge content"),
        "Bridge output should contain file content, got: {}",
        output_str
    );
}
