#!/bin/bash

# Test cases for commit-msg hook
# Tests conventional commit validation logic

set -euo pipefail

# Source the commit-msg hook functions for testing
# We'll extract the validation logic to make it testable

# Test framework
TESTS_PASSED=0
TESTS_FAILED=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test helper functions
assert_success() {
    local test_name="$1"
    local actual="$2"

    if [[ "$actual" == "0" ]]; then
        echo -e "${GREEN}✓ PASS${NC} $test_name"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗ FAIL${NC} $test_name (expected success, got exit code $actual)"
        ((TESTS_FAILED++))
    fi
}

assert_failure() {
    local test_name="$1"
    local actual="$2"

    if [[ "$actual" != "0" ]]; then
        echo -e "${GREEN}✓ PASS${NC} $test_name"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗ FAIL${NC} $test_name (expected failure, got exit code $actual)"
        ((TESTS_FAILED++))
    fi
}

# Extract validation function from commit-msg hook
validate_commit_message() {
    local message="$1"

    # Check for minimum length
    if [[ ${#message} -lt 10 ]]; then
        return 1
    fi

    # Check for conventional commit pattern
    # Format: type(scope)!: description
    # Where type is one of: feat, fix, docs, style, refactor, test, chore
    local pattern='^(feat|fix|docs|style|refactor|test|chore|perf|ci|build|revert)(\([^)]+\))?!?: .{10,}$'

    if [[ ! "$message" =~ $pattern ]]; then
        return 2
    fi

    # Check for proper line length (first line <= 100 characters)
    local first_line
    first_line=$(echo "$message" | head -n1)
    if [[ ${#first_line} -gt 100 ]]; then
        return 3
    fi

    # Check that description doesn't start with uppercase letter or period
    local type_and_scope="${message%%:*}"
    local description="${message#* }"
    local first_char="${description:0:1}"

    if [[ "$first_char" =~ [A-Z] ]]; then
        return 4
    fi

    if [[ "$first_char" == "." ]]; then
        return 5
    fi

    return 0
}

# Test cases
echo "Running commit-msg hook validation tests..."
echo

# Valid commit messages
assert_success "Valid simple feature" "$(validate_commit_message "feat: add new user authentication system")"
assert_success "Valid feature with scope" "$(validate_commit_message "feat(cli): add new search command")"
assert_success "Valid breaking change" "$(validate_commit_message "feat(api)!: remove deprecated endpoint")"
assert_success "Valid fix" "$(validate_commit_message "fix: resolve memory leak in parser")"
assert_success "Valid docs" "$(validate_commit_message "docs: update API documentation")"
assert_success "Valid refactor" "$(validate_commit_message "refactor: simplify error handling logic")"
assert_success "Valid test" "$(validate_commit_message "test: add integration tests for search")"
assert_success "Valid chore" "$(validate_commit_message "chore: update dependencies")"
assert_success "Valid with multiline" "$(validate_commit_message $'feat: add comprehensive search functionality\n\nThis implements the new search engine with full-text\nsupport and semantic search capabilities.')"

# Invalid commit messages
assert_failure "Too short" "$(validate_commit_message "feat: add")"
assert_failure "Missing type" "$(validate_commit_message "add new feature")"
assert_failure "Missing colon" "$(validate_commit_message "feat add new feature")"
assert_failure "Missing description" "$(validate_commit_message "feat: ")"
assert_failure "Description starts with uppercase" "$(validate_commit_message "feat: Add new feature")"
assert_failure "Description starts with period" "$(validate_commit_message "feat: .add new feature")"
assert_failure "Line too long" "$(validate_commit_message "feat: this is a very long commit message that exceeds one hundred characters and should be rejected by the validation logic")"
assert_failure "Invalid type" "$(validate_commit_message "random: add new feature")"
assert_failure "Empty message" "$(validate_commit_message "")"

# Edge cases
echo
echo "Testing edge cases..."

# Minimum valid length
assert_success "Minimum valid length" "$(validate_commit_message "feat: add a")"

# Exactly 100 characters
local hundred_char_msg="feat: add a new feature that is exactly one hundred characters long to test the boundary condition properly ok"
assert_success "Exactly 100 characters" "$(validate_commit_message "$hundred_char_msg")"

# 101 characters (should fail)
local over_hundred_msg="feat: add a new feature that is exactly one hundred one characters long to test boundary condition properly fail"
assert_failure "101 characters" "$(validate_commit_message "$over_hundred_msg")"

# Summary
echo
echo "Test Summary:"
echo -e "  ${GREEN}Passed: $TESTS_PASSED${NC}"
echo -e "  ${RED}Failed: $TESTS_FAILED${NC}"

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi