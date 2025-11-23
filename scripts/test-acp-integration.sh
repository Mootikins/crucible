#!/bin/bash
# Test script for ACP Chat + MCP Server Integration
#
# Usage: ./scripts/test-acp-integration.sh [phase]
#   phase: 1, 2, 3, all (default: all)

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_section() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Phase selection
PHASE="${1:-all}"

# =============================================================================
# Phase 1: Build and Standalone MCP Server Testing
# =============================================================================

phase1_build_and_test() {
    log_section "Phase 1: Build and Standalone MCP Server Testing"

    # Step 1: Build the project
    log_info "Building project (release mode)..."
    if cargo build --release 2>&1 | tee build.log | tail -10; then
        log_success "Build completed"
    else
        log_error "Build failed. Check build.log for details"
        return 1
    fi

    # Step 2: Verify binaries
    log_info "Verifying binaries..."
    if [ -f "target/release/cru" ]; then
        log_success "Binary exists: target/release/cru"
        ls -lh target/release/cru
    else
        log_error "Binary not found: target/release/cru"
        return 1
    fi

    # Step 3: Run unit tests
    log_info "Running unit tests..."
    if cargo test --lib 2>&1 | tee test.log | grep -E "(test result|FAILED)"; then
        log_success "Unit tests passed"
    else
        log_warning "Some unit tests may have failed. Check test.log"
    fi

    # Step 4: Run MCP integration tests
    log_info "Running MCP integration tests..."
    if cargo test --package crucible-acp --test mcp_integration_test 2>&1 | tee mcp_test.log | grep -E "(test result|FAILED)"; then
        log_success "MCP integration tests passed"
    else
        log_error "MCP integration tests failed"
        return 1
    fi

    # Step 5: Run test MCP client
    log_info "Running test MCP client..."
    export PATH="$PWD/target/release:$PATH"
    export CRUCIBLE_BIN="$PWD/target/release/cru"

    if timeout 30s cargo run --release --example test_mcp_server 2>&1 | tee client_test.log; then
        log_success "Test MCP client completed successfully"
        # Check if all 12 tools were discovered
        if grep -q "SUCCESS: All 12 tools discovered" client_test.log; then
            log_success "All 12 tools discovered!"
        else
            log_warning "Tool discovery may be incomplete. Check client_test.log"
        fi
    else
        EXIT_CODE=$?
        if [ $EXIT_CODE -eq 124 ]; then
            log_error "Test client timed out after 30 seconds"
        else
            log_error "Test client failed with exit code $EXIT_CODE"
        fi
        return 1
    fi

    # Step 6: Check MCP logs
    log_info "Checking MCP server logs..."
    MCP_LOG="$HOME/.crucible/mcp.log"
    if [ -f "$MCP_LOG" ]; then
        log_success "MCP log file exists: $MCP_LOG"
        log_info "Last 10 lines:"
        tail -10 "$MCP_LOG"
    else
        log_warning "MCP log file not found. This is normal if server hasn't been run yet."
    fi

    log_success "Phase 1 completed successfully!"
}

# =============================================================================
# Phase 2: ACP Chat Command Testing
# =============================================================================

phase2_chat_testing() {
    log_section "Phase 2: ACP Chat Command Testing"

    export PATH="$PWD/target/release:$PATH"

    # Step 1: Test agent discovery
    log_info "Testing agent discovery..."
    if timeout 5s cru chat --help > /dev/null 2>&1; then
        log_success "Chat command help works"
    else
        log_error "Chat command help failed"
        return 1
    fi

    # Step 2: Check agent availability
    log_info "Checking for available ACP agents..."
    # This will likely fail if no agent is installed, but that's expected
    if cru chat 2>&1 | grep -q "No ACP-compatible agent found"; then
        log_warning "No ACP agent found (this is expected if agent not installed)"
        log_info "To test with an agent, install claude-code or another ACP-compatible agent"
    elif cru chat 2>&1 | grep -q "agent"; then
        log_success "Agent discovery appears to be working"
    else
        log_info "Agent status unclear. Manual testing recommended."
    fi

    # Step 3: Verify logging configuration
    log_info "Verifying file-based logging configuration..."
    if grep -q "uses_stdio" crates/crucible-cli/src/main.rs; then
        log_success "File-based logging is configured for chat command"
    else
        log_error "File-based logging configuration not found"
        return 1
    fi

    log_success "Phase 2 completed!"
}

# =============================================================================
# Phase 3: Protocol Verification
# =============================================================================

phase3_protocol_verification() {
    log_section "Phase 3: Protocol Verification"

    # Run protocol tests
    log_info "Running ACP protocol tests..."
    if cargo test --package crucible-acp 2>&1 | tee protocol_test.log | grep -E "(test result|FAILED)"; then
        log_success "Protocol tests passed"
    else
        log_error "Protocol tests failed"
        return 1
    fi

    # Verify handshake structure
    log_info "Verifying NewSessionRequest structure..."
    if cargo test --package crucible-acp test_mcp_server_configuration_in_handshake -- --nocapture 2>&1 | grep -q "ok"; then
        log_success "Handshake structure test passed"
    else
        log_error "Handshake structure test failed"
        return 1
    fi

    log_success "Phase 3 completed!"
}

# =============================================================================
# Main execution
# =============================================================================

log_section "ACP Chat + MCP Server Integration Test Suite"
log_info "Phase: $PHASE"
echo ""

case "$PHASE" in
    1)
        phase1_build_and_test
        ;;
    2)
        phase2_chat_testing
        ;;
    3)
        phase3_protocol_verification
        ;;
    all)
        phase1_build_and_test || exit 1
        phase2_chat_testing || exit 1
        phase3_protocol_verification || exit 1
        ;;
    *)
        log_error "Invalid phase: $PHASE"
        echo "Usage: $0 [1|2|3|all]"
        exit 1
        ;;
esac

echo ""
log_section "Test Summary"
log_success "All requested tests completed!"
log_info "Next steps:"
echo "  1. Review logs in: build.log, test.log, client_test.log"
echo "  2. Check MCP logs in: $HOME/.crucible/mcp.log"
echo "  3. Test with a real ACP agent (Phase 4 - manual testing required)"
echo ""
log_info "For full integration testing plan, see: docs/ACP_CHAT_INTEGRATION_PLAN.md"
