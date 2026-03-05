#!/bin/bash

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

passed=0
failed=0

phase1_status="FAIL"
phase2_status="FAIL"
phase3a_status="FAIL"
phase3b_status="FAIL"
phase4a_status="FAIL"
phase4b_status="FAIL"
phase5_status="skipped"
phase3a_http="N/A"
phase3b_http="N/A"

chat_failed="false"
anthropic_chat_failed="false"
zai_chat_failed="false"

log_pass() { echo -e "${GREEN}✓${NC} $1"; ((passed++)) || true; }
log_fail() { echo -e "${RED}✗${NC} $1"; ((failed++)) || true; }
log_info() { echo -e "${YELLOW}→${NC} $1"; }

check_prereqs() {
    if [[ -z "${GLM_AUTH_TOKEN:-}" ]]; then
        echo "Error: GLM_AUTH_TOKEN not set"
        exit 1
    fi

    CRU="./target/release/cru"
    if [[ ! -x "$CRU" ]]; then
        CRU="./target/debug/cru"
    fi

    if [[ ! -x "$CRU" ]]; then
        echo "Error: cru binary not found at ./target/release/cru or ./target/debug/cru"
        echo "Run: cargo build -p crucible-cli"
        exit 1
    fi

    if [[ ! -d "./docs" ]]; then
        echo "Error: kiln not found at ./docs"
        exit 1
    fi

    WORKDIR=$(mktemp -d)
    trap 'rm -rf "$WORKDIR"' EXIT

    cat >"$WORKDIR/anthropic-config.toml" <<EOF
kiln_path = "./docs"
[llm]
default = "zai-coding"
[llm.providers.zai-coding]
type = "anthropic"
endpoint = "https://api.z.ai/api/anthropic"
api_key = "$GLM_AUTH_TOKEN"
default_model = "claude-sonnet-4-20250514"
EOF

    cat >"$WORKDIR/zai-native-config.toml" <<EOF
kiln_path = "./docs"
[llm]
default = "zai-native"
[llm.providers.zai-native]
type = "zai"
endpoint = "https://api.z.ai/api/coding/paas/v4"
api_key = "$GLM_AUTH_TOKEN"
default_model = "GLM-4.7"
EOF

    log_info "Using binary: $CRU"
    log_info "Using workdir: $WORKDIR"
    phase1_status="PASS"
    log_pass "Phase 1 preflight"
}

validate_config() {
    local config_path="$1"
    local provider_key="$2"
    local expected_type="$3"
    local expected_endpoint="$4"
    local stderr_file="$5"
    local ok=0
    local config_json

    if ! config_json=$("$CRU" --config "$config_path" config show --format json --standalone 2>"$stderr_file"); then
        log_fail "config show failed for $provider_key"
        return 1
    fi

    if [[ "$config_json" == *"$provider_key"* ]]; then
        log_pass "config has provider key: $provider_key"
    else
        log_fail "config missing provider key: $provider_key"
        ok=1
    fi

    if [[ "$config_json" == *"\"$expected_type\""* ]]; then
        log_pass "config type is $expected_type"
    else
        log_fail "config missing type $expected_type"
        ok=1
    fi

    if [[ "$config_json" == *"$expected_endpoint"* ]]; then
        log_pass "config endpoint matches $expected_endpoint"
    else
        log_fail "config endpoint missing $expected_endpoint"
        ok=1
    fi

    if [[ "$config_json" == *"{env:GLM_AUTH_TOKEN}"* ]]; then
        log_fail "config api_key still literal {env:GLM_AUTH_TOKEN}"
        ok=1
    else
        log_pass "config api_key injected as literal value"
    fi

    return $ok
}

run_curl_test() {
    local name="$1"
    local url="$2"
    local auth_header="$3"
    local extra_header="$4"
    local payload="$5"
    local stderr_file="$6"
    local status_var="$7"
    local http_var="$8"

    local curl_response
    local http_status
    local body

    curl_response=$(curl -s -w "\nHTTP_STATUS:%{http_code}" \
        -X POST "$url" \
        -H "$auth_header" \
        -H "$extra_header" \
        -H "content-type: application/json" \
        -d "$payload" \
        --max-time 30 \
        2>"$stderr_file")

    http_status=$(echo "$curl_response" | grep "HTTP_STATUS:" | cut -d: -f2)
    body=$(echo "$curl_response" | grep -v "HTTP_STATUS:")

    printf -v "$http_var" '%s' "$http_status"

    if [[ "$http_status" == "200" ]]; then
        if [[ "$body" == *"\"content\""* ]] || [[ "$body" == *"\"choices\""* ]]; then
            log_pass "curl $name: HTTP 200 with content"
            printf -v "$status_var" '%s' "PASS"
            return 0
        fi
        log_fail "curl $name: HTTP 200 but missing content fields"
    else
        log_fail "curl $name: HTTP $http_status"
    fi

    echo "  http_status: $http_status"
    echo "  body[0:200]: ${body:0:200}"
    printf -v "$status_var" '%s' "FAIL"
    return 1
}

run_chat_test() {
    local name="$1"
    local config_path="$2"
    local provider="$3"
    local stderr_file="$4"
    local status_var="$5"
    local fail_var="$6"
    local chat_exit=0
    local chat_response
    local trimmed

    set +e
    chat_response=$(timeout 60 "$CRU" --config "$config_path" \
        chat --standalone --no-context --provider "$provider" \
        "Reply with ONLY the word hello" \
        2>"$stderr_file")
    chat_exit=$?
    set -e

    if [[ $chat_exit -ne 0 ]]; then
        log_fail "chat ($name): command failed with exit $chat_exit"
        echo "  stderr:"
        head -20 "$stderr_file"
        printf -v "$status_var" '%s' "FAIL"
        printf -v "$fail_var" '%s' "true"
        chat_failed="true"
        return 1
    fi

    trimmed=$(echo "$chat_response" | tr -d '[:space:]')
    if [[ -z "$trimmed" ]]; then
        log_fail "chat ($name): SILENT FAILURE REPRODUCED - empty response"
        echo "  stderr:"
        head -20 "$stderr_file"
        printf -v "$status_var" '%s' "FAIL"
        printf -v "$fail_var" '%s' "true"
        chat_failed="true"
        return 1
    fi

    # Check for Phase A hard error messages in stderr
    local stderr_content
    stderr_content=$(cat "$stderr_file" 2>/dev/null || true)
    if echo "$stderr_content" | grep -q "LLM returned empty response\|LLM stream timed out\|LLM stream ended unexpectedly"; then
        local error_msg
        error_msg=$(echo "$stderr_content" | grep -o "LLM [^'\"]*" | head -1)
        log_fail "chat ($name): hard error from streaming pipeline: $error_msg"
        printf -v "$status_var" '%s' "FAIL"
        printf -v "$fail_var" '%s' "true"
        chat_failed="true"
        return 1
    fi

    log_pass "chat ($name): got response: ${chat_response:0:80}"
    printf -v "$status_var" '%s' "PASS"
    printf -v "$fail_var" '%s' "false"
    return 0
}

run_debug_diagnostics() {
    if [[ "$chat_failed" != "true" ]]; then
        phase5_status="skipped"
        return 0
    fi

    phase5_status="ran"
    log_info "Running debug diagnostics (RUST_LOG=genai=debug)..."

    if [[ "$anthropic_chat_failed" == "true" ]]; then
        RUST_LOG=genai=debug timeout 60 "$CRU" --config "$WORKDIR/anthropic-config.toml" \
            chat --standalone --no-context --provider zai-coding \
            "Say hello" \
            >"$WORKDIR/debug-anthropic-stdout.log" \
            2>"$WORKDIR/debug-anthropic-stderr.log" || true

        echo "--- genai debug log (anthropic) ---"
        cat "$WORKDIR/debug-anthropic-stderr.log"
        echo "--- end debug log ---"
    fi

    if [[ "$zai_chat_failed" == "true" ]]; then
        RUST_LOG=genai=debug timeout 60 "$CRU" --config "$WORKDIR/zai-native-config.toml" \
            chat --standalone --no-context --provider zai-native \
            "Say hello" \
            >"$WORKDIR/debug-zai-stdout.log" \
            2>"$WORKDIR/debug-zai-stderr.log" || true

        echo "--- genai debug log (zai-native) ---"
        cat "$WORKDIR/debug-zai-stderr.log"
        echo "--- end debug log ---"
    fi
}

print_summary() {
    echo ""
    echo "========================================"
    echo "Z.AI Provider Diagnosis Summary"
    echo "========================================"
    echo "Phase 1 (Preflight):          $phase1_status"
    echo "Phase 2 (Config validation):  $phase2_status"
    echo "Phase 3a (curl anthropic):    $phase3a_status [HTTP $phase3a_http]"
    echo "Phase 3b (curl zai-native):   $phase3b_status [HTTP $phase3b_http]"
    echo "Phase 4a (cru anthropic):     $phase4a_status"
    echo "Phase 4b (cru zai-native):    $phase4b_status"
    echo "Phase 5 (Debug diagnostics):  $phase5_status"
    echo ""
    echo "Results: $passed passed, $failed failed"
    echo "Note: Phase A error detection active — hard errors surface in stderr"
    echo "========================================"

    [[ $failed -eq 0 ]]
}

main() {
    check_prereqs

    log_info "Phase 2: Config validation"
    if validate_config "$WORKDIR/anthropic-config.toml" "zai-coding" "anthropic" "https://api.z.ai/api/anthropic" "$WORKDIR/config-anthropic-stderr.log"; then
        :
    fi
    if validate_config "$WORKDIR/zai-native-config.toml" "zai-native" "zai" "https://api.z.ai/api/coding/paas/v4" "$WORKDIR/config-zai-stderr.log"; then
        :
    fi
    if [[ $failed -eq 0 ]]; then
        phase2_status="PASS"
    else
        phase2_status="FAIL"
    fi

    log_info "Phase 3: Raw curl endpoint tests"
    run_curl_test \
        "anthropic" \
        "https://api.z.ai/api/anthropic/messages" \
        "x-api-key: $GLM_AUTH_TOKEN" \
        "anthropic-version: 2023-06-01" \
        '{"model":"claude-sonnet-4-20250514","max_tokens":50,"messages":[{"role":"user","content":"Say hello"}]}' \
        "$WORKDIR/curl-anthropic-stderr.log" \
        phase3a_status \
        phase3a_http || true

    run_curl_test \
        "zai-native" \
        "https://api.z.ai/api/coding/paas/v4/chat/completions" \
        "Authorization: Bearer $GLM_AUTH_TOKEN" \
        "Accept: application/json" \
        '{"model":"GLM-4.7","max_tokens":50,"messages":[{"role":"user","content":"Say hello"}]}' \
        "$WORKDIR/curl-zai-stderr.log" \
        phase3b_status \
        phase3b_http || true

    log_info "Phase 4: Crucible chat tests"
    run_chat_test "anthropic" "$WORKDIR/anthropic-config.toml" "zai-coding" "$WORKDIR/chat-anthropic-stderr.log" phase4a_status anthropic_chat_failed || true
    run_chat_test "zai-native" "$WORKDIR/zai-native-config.toml" "zai-native" "$WORKDIR/chat-zai-stderr.log" phase4b_status zai_chat_failed || true

    run_debug_diagnostics
    print_summary
}

main "$@"
