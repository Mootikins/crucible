#!/bin/bash
# Quick test to check if hash storage is working

cd /home/moot/crucible

# Create a minimal test vault
TEST_VAULT="/tmp/hash_test_vault"
rm -rf "$TEST_VAULT"
mkdir -p "$TEST_VAULT"

# Create a single test file
echo "# Getting Started" > "$TEST_VAULT/getting-started.md"

# Run CLI to process it
OBSIDIAN_KILN_PATH="$TEST_VAULT" RUST_LOG=debug ./target/release/cru stats 2>&1 | tee /tmp/hash_test_output.txt

# Check if hash was stored
echo ""
echo "=== Checking database for stored hash ==="
echo "SELECT * FROM notes" | surreal sql --conn memory --ns crucible --db kiln 2>&1 || echo "Cannot query - using different DB backend"

# Try scanning again to see if it detects changes
echo ""
echo "=== Second scan (should detect 0 changes) ==="
OBSIDIAN_KILN_PATH="$TEST_VAULT" ./target/release/cru stats 2>&1 | grep -i "change"
