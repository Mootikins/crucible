#!/bin/bash

echo "=== Comprehensive Test Compilation Error Analysis ==="
echo "Running cargo test --no-run to capture all compilation errors..."
echo ""

# Get the raw error output
RAW_OUTPUT="/tmp/raw_errors.txt"
cargo test --no-run 2>&1 | tee "$RAW_OUTPUT" > /dev/null

echo "Processing compilation errors..."
echo ""

# Create analysis file
ANALYSIS_FILE="/home/moot/crucible/test_error_breakdown.md"
cat > "$ANALYSIS_FILE" << 'EOF'
# Test Compilation Error Analysis - Comprehensive Breakdown

## Executive Summary

This analysis captures all compilation errors from the Rust test suite across all crates and provides a detailed breakdown for systematic resolution.

## Key Statistics

EOF

# Extract statistics
TOTAL_ERRORS=$(grep -c "error\[" "$RAW_OUTPUT" || echo "0")
FILES_WITH_ERRORS=$(grep "^-->" "$RAW_OUTPUT" | sort | uniq | wc -l)
DUPLICATE_FUNCTIONS=$(grep -c "E0428.*defined multiple times" "$RAW_OUTPUT" || echo "0")

# Add statistics to analysis
cat >> "$ANALYSIS_FILE" << EOF
- **Total Errors**: $TOTAL_ERRORS
- **Files with Errors**: $FILES_WITH_ERRORS
- **Duplicate Function Definitions**: $DUPLICATE_FUNCTIONS

## Critical Issues Summary

### 🚨 CRITICAL: Duplicate Functions (Must Fix First)

EOF

# Extract duplicate function errors (most critical)
echo "" >> "$ANALYSIS_FILE"
echo "Duplicate functions found:" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
grep -A 3 -B 1 "E0428.*defined multiple times" "$RAW_OUTPUT" | \
sed 's/^.*error\[E0428\]/  - /g' | \
sed 's/^.*--> /    File: /g' >> "$ANALYSIS_FILE"

# Extract all unresolved imports
echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "### 📦 Import Resolution Issues" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"

# Common missing types
echo "Most frequently missing types:" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
grep "failed to resolve.*undeclared type" "$RAW_OUTPUT" | \
sed 's/.*undeclared type `\([^`]*\)`.*/\1/' | \
sort | uniq -c | sort -nr | head -10 | \
sed 's/^/  - /g' >> "$ANALYSIS_FILE"

# Common missing imports
echo "" >> "$ANALYSIS_FILE"
echo "Most common import errors:" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
grep "unresolved import" "$RAW_OUTPUT" | \
sed 's/.*unresolved import `\([^`]*\)`.*/\1/' | \
sort | uniq -c | sort -nr | head -10 | \
sed 's/^/  - /g' >> "$ANALYSIS_FILE"

# Top 10 most problematic files
echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "### 📂 Top 10 Most Problematic Files" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
grep "^-->" "$RAW_OUTPUT" | sort | uniq -c | sort -nr | head -10 | \
sed 's/^ *//' | \
awk '{printf "  %d errors: %s\n", $1, $2}' >> "$ANALYSIS_FILE"

# Error distribution by type
echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "### 📊 Error Distribution by Type" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
grep "error\[" "$RAW_OUTPUT" | sort | uniq -c | sort -nr | head -15 | \
sed 's/^ *//' | \
awk '{printf "  %s: %d occurrences\n", $3, $1}' >> "$ANALYSIS_FILE"

# Files with specific error patterns
echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "### 🎯 Error Categories by Severity" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"

echo "**HIGH PRIORITY - Blockers:**" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"

# Critical missing types
echo "- Missing core types (causing cascading failures):" >> "$ANALYSIS_FILE"
grep "failed to resolve.*undeclared type.*TestContext\|MemoryUsage\|MockService\|ServiceHealth\|Arc" "$RAW_OUTPUT" | \
sed 's/^.*--> /  - /g' | \
sort | uniq | head -10 >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "- Missing modules/crates:" >> "$ANALYSIS_FILE"
grep "crucible_services\|crucible_daemon::tools" "$RAW_OUTPUT" | \
sed 's/^.*--> /  - /g' | \
sort | uniq | head -5 >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "**MEDIUM PRIORITY - Type Issues:**" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "- Method calls on futures/Results (async/await issues):" >> "$ANALYSIS_FILE"
grep "no method.*found for.*Future\|Result" "$RAW_OUTPUT" | \
sed 's/^.*--> /  - /g' | \
sort | uniq | head -5 >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "- Type mismatches:" >> "$ANALYSIS_FILE"
grep "E0308.*mismatched types" "$RAW_OUTPUT" | \
sed 's/^.*--> /  - /g' | \
sort | uniq | head -5 >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "**LOW PRIORITY - Cleanup:**" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "- Character escape issues:" >> "$ANALYSIS_FILE"
grep "unknown character escape" "$RAW_OUTPUT" | \
sed 's/^.*--> /  - /g' | \
sort | uniq >> "$ANALYSIS_FILE"

# Missing imports, methods, and traits summary
echo "" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "### 🔧 Missing Components Summary" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"

echo "**Missing Types:**" >> "$ANALYSIS_FILE"
grep "failed to resolve.*undeclared type" "$RAW_OUTPUT" | \
sed 's/.*undeclared type `\([^`]*\)`.*/\1/' | \
sort | uniq | \
sed 's/^/  - /g' >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "**Missing Methods/Functions:**" >> "$ANALYSIS_FILE"
grep "no method.*found\|cannot find function" "$RAW_OUTPUT" | \
sed 's/.*no method named `\([^`]*\)`.*/\1/' | \
sed 's/.*cannot find function `\([^`]*\)`.*/\1/' | \
sort | uniq | \
sed 's/^/  - /g' >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "**Missing Imports:**" >> "$ANALYSIS_FILE"
grep "unresolved import" "$RAW_OUTPUT" | \
sed 's/.*unresolved import `\([^`]*\)`.*/\1/' | \
sort | uniq | \
sed 's/^/  - /g' >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "## 🚀 Recommended Fix Order" >> "$ANALYSIS_FILE"
echo "" >> "$ANALYSIS_FILE"
echo "1. **Fix duplicate functions first** (E0428 errors)" >> "$ANALYSIS_FILE"
echo "2. **Add missing core types** (TestContext, MemoryUsage, etc.)" >> "$ANALYSIS_FILE"
echo "3. **Fix module imports** (crucible_services, tools, etc.)" >> "$ANALYSIS_FILE"
echo "4. **Resolve trait implementation issues**" >> "$ANALYSIS_FILE"
echo "5. **Fix async/await and method call issues**" >> "$ANALYSIS_FILE"
echo "6. **Address type mismatches**" >> "$ANALYSIS_FILE"
echo "7. **Fix character escape issues**" >> "$ANALYSIS_FILE"

echo "" >> "$ANALYSIS_FILE"
echo "---" >> "$ANALYSIS_FILE"
echo "*Analysis generated on $(date)*" >> "$ANALYSIS_FILE"

echo "✅ Analysis complete!"
echo "📁 Detailed breakdown saved to: $ANALYSIS_FILE"
echo ""
echo "📊 Quick Summary:"
echo "   - Total Errors: $TOTAL_ERRORS"
echo "   - Files with Errors: $FILES_WITH_ERRORS"
echo "   - Duplicate Functions: $DUPLICATE_FUNCTIONS"
echo ""

# Display top 5 most problematic files
echo "🔥 Top 5 Most Problematic Files:"
grep "^-->" "$RAW_OUTPUT" | sort | uniq -c | sort -nr | head -5 | \
awk '{printf "   %d errors: %s\n", $1, $2}'

echo ""
echo "🎯 Priority Actions:"
echo "   1. Fix duplicate functions (E0428)"
echo "   2. Add missing TestContext, MemoryUsage, MockService types"
echo "   3. Fix crucible_services imports"