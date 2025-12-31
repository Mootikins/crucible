#!/bin/bash
# VT (Validation Testing) - Basic Flows
# Tests real LLM interactions with the configured provider

set -euo pipefail

# Configuration
CRU="${CRU:-./target/release/cru}"
KILN="${CRUCIBLE_KILN_PATH:-./docs}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

passed=0
failed=0

log_pass() { echo -e "${GREEN}✓${NC} $1"; ((passed++)); }
log_fail() { echo -e "${RED}✗${NC} $1"; ((failed++)); }
log_info() { echo -e "${YELLOW}→${NC} $1"; }

# Check binary exists
check_prereqs() {
    if [[ ! -x "$CRU" ]]; then
        echo "Error: $CRU not found or not executable"
        echo "Run: cargo build --release -p crucible-cli"
        exit 1
    fi

    if [[ ! -d "$KILN" ]]; then
        echo "Error: Kiln not found at $KILN"
        exit 1
    fi

    log_info "Using binary: $CRU"
    log_info "Using kiln: $KILN"
}

# Test: Stats command works
test_stats() {
    log_info "Testing stats..."

    local stats
    stats=$(env CRUCIBLE_KILN_PATH="$KILN" "$CRU" stats --no-process 2>&1) || {
        log_fail "stats: command failed"
        return 1
    }

    if echo "$stats" | grep -qiE '(notes|files|total)'; then
        log_pass "stats: shows counts"
        return 0
    else
        log_fail "stats: unexpected output"
        echo "  Output: ${stats:0:100}"
        return 1
    fi
}

# Test: Basic chat produces non-empty response (no context)
test_basic_chat() {
    log_info "Testing basic chat (internal, no context)..."

    local response
    response=$(timeout 90 env CRUCIBLE_KILN_PATH="$KILN" \
        "$CRU" chat --internal --no-context --no-process \
        "Say hello in exactly 3 words" 2>&1 | grep -v '⟳\|✓\|Ready') || {
        log_fail "basic_chat: timeout or error"
        return 1
    }

    # Check response is non-empty
    if [[ -z "$response" ]]; then
        log_fail "basic_chat: empty response"
        return 1
    fi

    local word_count
    word_count=$(echo "$response" | wc -w)
    if [[ $word_count -lt 1 ]]; then
        log_fail "basic_chat: no words in response"
        return 1
    fi

    log_pass "basic_chat: got $word_count words"
    echo "  Response: ${response:0:80}"
    return 0
}

# Test: Chat follows instructions
test_instruction_following() {
    log_info "Testing instruction following..."

    local response
    response=$(timeout 90 env CRUCIBLE_KILN_PATH="$KILN" \
        "$CRU" chat --internal --no-context --no-process \
        "Reply with ONLY the word 'banana'. No other text." 2>&1 | grep -v '⟳\|✓\|Ready') || {
        log_fail "instruction: timeout or error"
        return 1
    }

    local response_lower
    response_lower=$(echo "$response" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')

    if [[ "$response_lower" == *"banana"* ]]; then
        log_pass "instruction: response contains 'banana'"
        echo "  Response: $response"
        return 0
    else
        log_fail "instruction: response missing 'banana'"
        echo "  Response: $response"
        return 1
    fi
}

# Test: Multi-turn context (simulated via longer prompt)
test_reasoning() {
    log_info "Testing basic reasoning..."

    local response
    response=$(timeout 120 env CRUCIBLE_KILN_PATH="$KILN" \
        "$CRU" chat --internal --no-context --no-process \
        "What is 2+2? Reply with just the number." 2>&1 | grep -v '⟳\|✓\|Ready') || {
        log_fail "reasoning: timeout or error"
        return 1
    }

    if echo "$response" | grep -q "4"; then
        log_pass "reasoning: correctly answered 2+2=4"
        return 0
    else
        log_fail "reasoning: incorrect answer"
        echo "  Response: $response"
        return 1
    fi
}

# Test: JSON output format (if model supports it)
test_json_format() {
    log_info "Testing JSON output..."

    local response
    response=$(timeout 120 env CRUCIBLE_KILN_PATH="$KILN" \
        "$CRU" chat --internal --no-context --no-process \
        "Output a JSON object with keys 'name' and 'age'. Example: {\"name\":\"test\",\"age\":1}" 2>&1 | grep -v '⟳\|✓\|Ready') || {
        log_fail "json: timeout or error"
        return 1
    }

    # Check for JSON-like structure
    if echo "$response" | grep -qE '\{.*"name".*:.*"age".*\}|\{.*"age".*:.*"name".*\}'; then
        log_pass "json: output contains JSON structure"
        return 0
    elif echo "$response" | grep -qE '\{.*\}'; then
        log_pass "json: output contains JSON-like structure (partial)"
        echo "  Response: ${response:0:100}"
        return 0
    else
        log_fail "json: no JSON structure found"
        echo "  Response: ${response:0:100}"
        return 1
    fi
}

# Test: Config show works
test_config() {
    log_info "Testing config show..."

    local config
    config=$(env CRUCIBLE_KILN_PATH="$KILN" "$CRU" config show 2>&1) || {
        log_fail "config: command failed"
        return 1
    }

    if echo "$config" | grep -qE '(providers|kiln|embedding)'; then
        log_pass "config: shows provider configuration"
        return 0
    else
        log_fail "config: unexpected output"
        return 1
    fi
}

# Run test suite
run_suite() {
    local suite="${1:-basic}"

    echo ""
    echo "========================================"
    echo "VT Basic Flows - $suite suite"
    echo "========================================"
    echo ""

    case "$suite" in
        quick)
            # Just verify CLI works
            test_stats || true
            test_config || true
            ;;
        basic)
            # Basic LLM tests
            test_stats || true
            test_config || true
            test_basic_chat || true
            test_instruction_following || true
            ;;
        full)
            # All tests
            test_stats || true
            test_config || true
            test_basic_chat || true
            test_instruction_following || true
            test_reasoning || true
            test_json_format || true
            ;;
        *)
            echo "Usage: $0 [quick|basic|full]"
            echo "  quick - CLI commands only (no LLM)"
            echo "  basic - Basic LLM tests"
            echo "  full  - All tests including reasoning"
            exit 1
            ;;
    esac

    echo ""
    echo "========================================"
    echo "Results: $passed passed, $failed failed"
    echo "========================================"

    [[ $failed -eq 0 ]]
}

# Main
main() {
    check_prereqs
    run_suite "${1:-basic}"
}

main "$@"
