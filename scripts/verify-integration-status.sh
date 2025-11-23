#!/bin/bash
# Quick verification script for ACP + MCP integration status
# Checks that all required components are in place

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

check() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $2"
        return 0
    else
        echo -e "${RED}✗${NC} $2"
        return 1
    fi
}

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  ACP + MCP Integration Status Verification${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

FAILURES=0

# Check 1: MCP Server Implementation
echo "1. MCP Server Implementation"
[ -f "crates/crucible-tools/src/mcp_server.rs" ] && \
    grep -q "impl ServerHandler" crates/crucible-tools/src/mcp_server.rs
check $? "  ServerHandler implementation exists" || ((FAILURES++))

grep -q "CrucibleMcpServer" crates/crucible-tools/src/mcp_server.rs
check $? "  CrucibleMcpServer struct exists" || ((FAILURES++))

grep -q "tool_router" crates/crucible-tools/src/mcp_server.rs
check $? "  tool_router field exists" || ((FAILURES++))

# Check 2: CLI MCP Command
echo ""
echo "2. CLI MCP Command"
[ -f "crates/crucible-cli/src/commands/mcp.rs" ]
check $? "  MCP command file exists" || ((FAILURES++))

grep -q "pub async fn execute" crates/crucible-cli/src/commands/mcp.rs
check $? "  Execute function exists" || ((FAILURES++))

grep -q "Mcp" crates/crucible-cli/src/cli.rs
check $? "  Mcp command registered in CLI" || ((FAILURES++))

grep -q "Commands::Mcp" crates/crucible-cli/src/main.rs
check $? "  Mcp command dispatched in main" || ((FAILURES++))

# Check 3: ACP Client MCP Configuration
echo ""
echo "3. ACP Client MCP Configuration"
[ -f "crates/crucible-acp/src/client.rs" ]
check $? "  ACP client file exists" || ((FAILURES++))

grep -q "mcp_servers" crates/crucible-acp/src/client.rs
check $? "  mcp_servers field populated" || ((FAILURES++))

grep -q "McpServer::Stdio" crates/crucible-acp/src/client.rs
check $? "  McpServer::Stdio configuration exists" || ((FAILURES++))

# Check 4: File-Based Logging
echo ""
echo "4. File-Based Logging Configuration"
grep -q "uses_stdio" crates/crucible-cli/src/main.rs
check $? "  stdio detection logic exists" || ((FAILURES++))

grep -q "CRUCIBLE_MCP_LOG_FILE" crates/crucible-cli/src/main.rs
check $? "  MCP log file configuration exists" || ((FAILURES++))

grep -q "file_layer" crates/crucible-cli/src/main.rs
check $? "  File logger configured" || ((FAILURES++))

# Check 5: Test Client
echo ""
echo "5. Test MCP Client"
[ -f "crates/crucible-cli/examples/test_mcp_server.rs" ]
check $? "  Test client exists" || ((FAILURES++))

grep -q "TokioChildProcess" crates/crucible-cli/examples/test_mcp_server.rs
check $? "  Uses TokioChildProcess for spawning" || ((FAILURES++))

grep -q "list_tools" crates/crucible-cli/examples/test_mcp_server.rs
check $? "  Calls list_tools" || ((FAILURES++))

# Check 6: Integration Tests
echo ""
echo "6. Integration Tests"
[ -f "crates/crucible-acp/tests/mcp_integration_test.rs" ]
check $? "  MCP integration tests exist" || ((FAILURES++))

TEST_COUNT=$(grep -c "#\[tokio::test\]" crates/crucible-acp/tests/mcp_integration_test.rs || echo "0")
[ "$TEST_COUNT" -ge 5 ]
check $? "  At least 5 tests defined ($TEST_COUNT found)" || ((FAILURES++))

# Check 7: Documentation
echo ""
echo "7. Documentation"
[ -f "docs/MCP_INTEGRATION.md" ]
check $? "  MCP integration guide exists" || ((FAILURES++))

[ -f "docs/ACP_TESTING_PLAN.md" ]
check $? "  ACP testing plan exists" || ((FAILURES++))

[ -f "docs/ACP_CHAT_INTEGRATION_PLAN.md" ]
check $? "  ACP chat integration plan exists" || ((FAILURES++))

# Check 8: Tool Implementation
echo ""
echo "8. Tool Modules"
[ -f "crates/crucible-tools/src/notes.rs" ]
check $? "  NoteTools module exists" || ((FAILURES++))

[ -f "crates/crucible-tools/src/search.rs" ]
check $? "  SearchTools module exists" || ((FAILURES++))

[ -f "crates/crucible-tools/src/kiln.rs" ]
check $? "  KilnTools module exists" || ((FAILURES++))

# Summary
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}✓ All components verified! Integration appears complete.${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Run: ./scripts/test-acp-integration.sh"
    echo "  2. Check: docs/ACP_CHAT_INTEGRATION_PLAN.md"
else
    echo -e "${RED}✗ $FAILURES checks failed. Review errors above.${NC}"
    exit 1
fi
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
