# Query Parser Fix: Record ID Lookup Support

## Problem

The query parser was incorrectly treating record ID queries as table name queries. When querying `SELECT * FROM notes:Projects_Rune_MCP_file_md`, the parser interpreted `notes:Projects_Rune_MCP_file_md` as a table name instead of recognizing it as a record ID lookup pattern.

**Symptoms:**
- Query: `SELECT * FROM notes:record_id` returned "Table not found" error
- Expected behavior: Direct O(1) record lookup by ID
- Actual behavior: Attempted to find a table named "notes:record_id"

## Root Cause

Location: `/home/moot/crucible/crates/crucible-surrealdb/src/multi_client.rs`

The `query()` method in `InMemoryClient` parsed `SELECT * FROM {identifier}` but didn't detect when `{identifier}` contained a colon (`:`) which indicates a record ID in SurrealDB's format: `table:id`.

## Solution

### Changes Made

1. **Added Record ID Detection** (line 2171-2177)
   - Added check for colon in SELECT queries before WHERE clause parsing
   - Delegates to `try_parse_record_id_query()` for record ID queries

2. **Implemented `try_parse_record_id_query()` Method** (lines 2603-2660)
   - Parses `SELECT * FROM table:id` syntax
   - Extracts table name and record ID from the query
   - Performs direct O(1) lookup in the storage HashMap
   - Returns `Some(QueryResult)` if this is a record ID query, `None` otherwise

### Implementation Details

```rust
// Detection code in query() method
if sql.contains(':') && !sql_trimmed.contains("where") {
    if let Some(record_result) = self.try_parse_record_id_query(sql).await? {
        return Ok(record_result);
    }
}

// Parsing logic
async fn try_parse_record_id_query(&self, sql: &str) -> DbResult<Option<QueryResult>> {
    // Extract FROM clause
    let from_part = extract_from_clause(sql)?;

    // Get table identifier (first token after FROM)
    let table_identifier = from_part.split_whitespace().next();

    // Check for colon (record ID format: table:id)
    if let Some((table_name, record_id_part)) = table_identifier.split_once(':') {
        let full_record_id = format!("{}:{}", table_name, record_id_part);

        // Direct O(1) lookup
        let storage = self.storage.read().await;
        if let Some(table_data) = storage.tables.get(table_name) {
            let record_id = RecordId(full_record_id);
            if let Some(record) = table_data.records.get(&record_id) {
                return Ok(Some(QueryResult::single(record)));
            }
        }

        // Return empty result if not found
        return Ok(Some(QueryResult::empty()));
    }

    Ok(None)
}
```

## Testing

Added comprehensive test suite: `/home/moot/crucible/crates/crucible-surrealdb/tests/query_parser_tests.rs`

### Test Coverage

1. **test_record_id_query_parsing**
   - Verifies `SELECT * FROM notes:record_id` syntax works
   - Tests non-existent record ID returns empty result
   - Validates case-insensitive keyword parsing

2. **test_record_id_vs_table_name**
   - Ensures `SELECT * FROM notes` returns all records (table query)
   - Confirms `SELECT * FROM notes:id` returns single record (record ID query)
   - Validates distinction between table and record ID queries

3. **test_record_id_with_complex_ids**
   - Tests various path formats:
     - Simple: `notes:simple_md`
     - Nested: `notes:Projects_Subfolder_File_md`
     - Deep: `notes:deeply_nested_path_to_file_md`

### Test Results

```
running 3 tests
✓ Record ID query parsing works correctly
✓ Non-existent record ID query returns empty result
✓ Case-insensitive keyword parsing works
✓ Record ID queries are distinct from table queries
✓ Record ID query works for: notes:simple_md
✓ Record ID query works for: notes:Projects_Subfolder_File_md
✓ Record ID query works for: notes:deeply_nested_path_to_file_md

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Impact on Phase 5

This fix enables proper record ID queries, which are essential for:

1. **Reranking Pipeline**: Can now retrieve individual documents by ID for reranking
2. **Direct Lookups**: O(1) performance for known record IDs
3. **Graph Traversal**: Supports record ID syntax in FROM clauses
4. **Vault Integration**: Matches SurrealDB's standard record ID format

## Performance

- **Before**: Table scan or error
- **After**: O(1) HashMap lookup by record ID
- **Memory**: No additional allocations beyond query parsing

## Compatibility

- Maintains backward compatibility with existing table queries
- Follows SurrealDB's standard `table:id` syntax
- Works with case-insensitive keywords (SELECT/select, FROM/from)
- Handles both existing and non-existent record IDs gracefully

## Files Modified

1. `/home/moot/crucible/crates/crucible-surrealdb/src/multi_client.rs`
   - Added record ID detection in `query()` method
   - Implemented `try_parse_record_id_query()` helper method

2. `/home/moot/crucible/crates/crucible-surrealdb/tests/query_parser_tests.rs`
   - New comprehensive test suite for record ID queries

## Build Status

✅ All tests passing
✅ Project builds successfully
✅ No breaking changes
✅ Zero performance regression
