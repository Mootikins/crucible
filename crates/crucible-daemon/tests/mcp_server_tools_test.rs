//! Integration tests for CrucibleMcpServer tool exposure
//!
//! The MCP surface is **kiln tools plus delegation** — what Crucible uniquely
//! offers a client that connects to it. Workspace tools (`read_file`,
//! `edit_file`, `write_file`, `bash`, `glob`, `grep`) are deliberately absent:
//! any harness speaking MCP already has its own, Crucible enforced no
//! permissions on the copies it served, and `agent_factory` added the same six
//! from `WorkspaceTools` so every kiln session advertised each one twice.

use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::tools::CrucibleMcpServer;
use rmcp::ServerHandler;
use std::sync::Arc;
use tempfile::TempDir;

/// Expected tool names that should be exposed by CrucibleMcpServer
const EXPECTED_TOOLS: &[&str] = &[
    // Note tools (6)
    "create_note",
    "read_note",
    "read_metadata",
    "update_note",
    "delete_note",
    "list_notes",
    // Search tools (3)
    "semantic_search",
    "text_search",
    "property_search",
    // Kiln tools (1)
    "get_kiln_info",
    // Job tools (3)
    "list_jobs",
    "get_job_result",
    "cancel_job",
    // Skills tools (1)
    "skill_view",
];

fn create_test_server() -> CrucibleMcpServer {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    CrucibleMcpServer::new(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
    )
}

/// The surface is exactly `EXPECTED_TOOLS`, and `delegate_session` is hidden
/// without a delegation context.
///
/// Named for the invariant rather than a number: this was
/// `test_mcp_server_exposes_13_tools` while asserting 21, because the count in
/// the name stopped being maintained the first time the surface changed.
#[tokio::test]
async fn test_mcp_server_exposes_exactly_the_expected_tools() {
    let server = create_test_server();

    // `EXPECTED_TOOLS` is the *listed* surface; `delegate_session` is
    // registered but filtered out without a delegation context.
    assert_eq!(
        server.tool_count(),
        EXPECTED_TOOLS.len() + 1,
        "every registered tool is EXPECTED_TOOLS plus delegate_session"
    );

    let listed_tools = server.list_tools();
    assert_eq!(
        listed_tools.len(),
        EXPECTED_TOOLS.len(),
        "delegate_session is filtered out when there is no delegation context"
    );

    for name in [
        "bash",
        "write_file",
        "edit_file",
        "read_file",
        "glob",
        "grep",
    ] {
        assert!(
            !EXPECTED_TOOLS.contains(&name),
            "{name} is a workspace tool; the MCP surface serves the kiln"
        );
    }
}

/// Test that all expected tools are present
#[tokio::test]
async fn test_mcp_server_has_all_expected_tools() {
    let server = create_test_server();

    let tools = server.list_tools();
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

    for expected_tool in EXPECTED_TOOLS {
        assert!(
            tool_names.iter().any(|n| n == *expected_tool),
            "Missing expected tool: '{}'. Found tools: {:?}",
            expected_tool,
            tool_names
        );
    }
}

/// Test that no unexpected tools are exposed
#[tokio::test]
async fn test_mcp_server_has_no_extra_tools() {
    let server = create_test_server();

    let tools = server.list_tools();

    for tool in &tools {
        let name = tool.name.as_ref();
        assert!(
            EXPECTED_TOOLS.contains(&name),
            "Unexpected tool found: '{}'. This may be intentional - update EXPECTED_TOOLS if so.",
            name
        );
    }
}

/// Test that each tool has a description
#[tokio::test]
async fn test_all_tools_have_descriptions() {
    let server = create_test_server();

    let tools = server.list_tools();

    for tool in &tools {
        assert!(
            tool.description.is_some(),
            "Tool '{}' is missing a description",
            tool.name
        );

        let desc = tool.description.as_ref().unwrap();
        assert!(
            !desc.is_empty(),
            "Tool '{}' has an empty description",
            tool.name
        );
    }
}

/// Test ServerHandler::get_info returns correct server metadata
#[tokio::test]
async fn test_server_info_metadata() {
    let server = create_test_server();

    let info = server.get_info();

    // Verify server name
    assert_eq!(info.server_info.name, "crucible-mcp-server");

    // Verify title
    assert!(info.server_info.title.is_some());
    assert_eq!(info.server_info.title.unwrap(), "Crucible MCP Server");

    // Verify instructions mention the tool count, derived rather than
    // hardcoded — a literal here is one more number to forget.
    assert!(info.instructions.is_some());
    let instructions = info.instructions.unwrap();
    let expected = format!("{} tools", EXPECTED_TOOLS.len() + 1);
    assert!(
        instructions.contains(&expected),
        "instructions should mention '{expected}', got: {instructions}"
    );

    // Verify tools capability is advertised
    assert!(info.capabilities.tools.is_some());
}

/// Test that tool categories are correct
#[tokio::test]
async fn test_tool_categories() {
    let server = create_test_server();

    let tools = server.list_tools();
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

    // Note tools (6)
    let note_tools = [
        "create_note",
        "read_note",
        "read_metadata",
        "update_note",
        "delete_note",
        "list_notes",
    ];
    let note_count = note_tools
        .iter()
        .filter(|t| tool_names.iter().any(|n| n == *t))
        .count();
    assert_eq!(note_count, 6, "Should have 6 note tools");

    // Search tools (3)
    let search_tools = ["semantic_search", "text_search", "property_search"];
    let search_count = search_tools
        .iter()
        .filter(|t| tool_names.iter().any(|n| n == *t))
        .count();
    assert_eq!(search_count, 3, "Should have 3 search tools");

    // Kiln tools (1)
    let kiln_tools = ["get_kiln_info"];
    let kiln_count = kiln_tools
        .iter()
        .filter(|t| tool_names.iter().any(|n| n == *t))
        .count();
    assert_eq!(kiln_count, 1, "Should have 1 kiln tool");

    let delegation_count = tool_names
        .iter()
        .filter(|t| *t == "delegate_session")
        .count();
    assert_eq!(
        delegation_count, 0,
        "Should not have delegate_session tool (no delegation context)"
    );
}

/// Test tool descriptions are meaningful (not just the tool name)
#[tokio::test]
async fn test_tool_descriptions_are_meaningful() {
    let server = create_test_server();

    let tools = server.list_tools();

    for tool in &tools {
        let desc = tool
            .description
            .as_ref()
            .expect("Tool should have description");
        let name = tool.name.as_ref();

        // Description should be longer than just the tool name
        assert!(
            desc.len() > name.len(),
            "Tool '{}' description '{}' should be more than just the name",
            name,
            desc
        );

        // Description should contain at least a few words
        let word_count = desc.split_whitespace().count();
        assert!(
            word_count >= 2,
            "Tool '{}' description should have at least 2 words, got: '{}'",
            name,
            desc
        );
    }
}

/// `tool_count` counts registrations; `list_tools` counts what a client sees.
#[tokio::test]
async fn test_tool_count_matches_list_length() {
    let server = create_test_server();

    assert_eq!(
        server.tool_count(),
        server.list_tools().len() + 1,
        "the only registered-but-unlisted tool is delegate_session"
    );
}
