//! Integration tests for CrucibleMcpServer tool exposure
//!
//! These tests verify that the MCP server correctly exposes all 13 Crucible tools
//! and that they can be listed and called via the MCP protocol.

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
    // Delegation tool (1)
    "delegate_session",
    // Job tools (3)
    "list_jobs",
    "get_job_result",
    "cancel_job",
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

/// Test that CrucibleMcpServer exposes exactly 16 tools
#[tokio::test]
async fn test_mcp_server_exposes_13_tools() {
    let server = create_test_server();

    let tool_count = server.tool_count();
    assert_eq!(
        tool_count, 14,
        "Should expose exactly 14 tools, got {}",
        tool_count
    );
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

    // Verify instructions mention 14 tools
    assert!(info.instructions.is_some());
    let instructions = info.instructions.unwrap();
    assert!(
        instructions.contains("14 tools"),
        "Instructions should mention 14 tools"
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
    assert_eq!(delegation_count, 1, "Should have delegate_session tool");
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

/// Test that tool_count matches list_tools length
#[tokio::test]
async fn test_tool_count_matches_list_length() {
    let server = create_test_server();

    let count = server.tool_count();
    let tools = server.list_tools();

    assert_eq!(
        count,
        tools.len(),
        "tool_count() should match list_tools().len()"
    );
}
