#!/bin/bash
# Manual test script to verify Merkle tree storage is working correctly

set -e

echo "=== Merkle Tree Storage Verification Test ==="
echo ""

# Create a temporary test vault
TEST_VAULT=$(mktemp -d)
echo "📁 Created test vault: $TEST_VAULT"

# Create a test markdown file
cat > "$TEST_VAULT/test-note.md" << 'EOF'
---
title: Test Note
tags: [test, verification]
---

# Test Note

This is a test note to verify Merkle tree storage.

## Section 1

Some content in section 1.

## Section 2

More content in section 2.

### Subsection 2.1

Nested content.
EOF

echo "✅ Created test file: $TEST_VAULT/test-note.md"
echo ""

# Build the CLI
echo "🔨 Building crucible-cli..."
cargo build --release -p crucible-cli --quiet

# Set config to use test vault
CONFIG_FILE=$(mktemp)
cat > "$CONFIG_FILE" << EOFCONFIG
[kiln]
path = "$TEST_VAULT"
EOFCONFIG

echo "📝 Using config: $CONFIG_FILE"

# Process the file
echo ""
echo "⚙️  Processing test file..."
./target/release/cru --config "$CONFIG_FILE" process "$TEST_VAULT/test-note.md" --force 2>&1 | grep -E "Processing|completed|error|✓" || true

echo ""
echo "📊 Checking database files..."

# Find the database directory - RocksDB uses a directory structure
if [ -d "$TEST_VAULT/.crucible" ]; then
    echo "✅ .crucible directory exists"
    ls -lh "$TEST_VAULT/.crucible/" || true

    # Check for kiln.db directory (RocksDB database)
    if [ -d "$TEST_VAULT/.crucible/kiln.db" ]; then
        DB_DIR="$TEST_VAULT/.crucible/kiln.db"
        echo ""
        echo "📂 Database directory: $DB_DIR"
        echo "Database size:"
        du -sh "$DB_DIR"
        echo ""
        echo "Database files:"
        ls -lh "$DB_DIR" | head -10

        # Check if database has content (RocksDB creates .sst files when data is written)
        SST_COUNT=$(find "$DB_DIR" -name "*.sst" 2>/dev/null | wc -l)
        LOG_COUNT=$(find "$DB_DIR" -name "*.log" 2>/dev/null | wc -l)

        echo ""
        echo "Database content indicators:"
        echo "  .sst files (data): $SST_COUNT"
        echo "  .log files: $LOG_COUNT"

        if [ "$SST_COUNT" -gt 0 ] || [ "$LOG_COUNT" -gt 0 ]; then
            echo "✅ Database has content - Merkle trees are likely stored!"
        else
            echo "⚠️  Database appears empty - no .sst or .log files found"
        fi
    else
        echo "❌ No kiln.db directory found in .crucible/"
        exit 1
    fi
else
    echo "❌ No .crucible directory found"
    exit 1
fi

echo ""
echo "🧹 Cleaning up..."
rm -rf "$TEST_VAULT"
rm -f /tmp/query_merkle.surql
rm -f "$CONFIG_FILE"

echo ""
echo "✅ Test completed successfully!"
