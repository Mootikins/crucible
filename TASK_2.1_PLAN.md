# Task 2.1 Plan: Map All AST Block Types to Entities

**Status**: Planning
**OpenSpec Reference**: `openspec/changes/2025-11-08-enhance-markdown-parser-eav-mapping/tasks.md` Lines 461-507

## Current State

### What's Already Done ✅
The `build_blocks()` function in `crates/crucible-surrealdb/src/eav_graph/ingest.rs` (lines 486-573) currently handles:

1. **Headings** (lines 491-505)
   - Metadata: `level`, `text`
   - Block type: `"heading"`
   - ✅ COMPLETE

2. **Paragraphs** (lines 507-521)
   - Non-empty only
   - Block type: `"paragraph"`
   - ✅ COMPLETE

3. **Code Blocks** (lines 523-537)
   - Metadata: `language`, `line_count`
   - Block type: `"code"`
   - ✅ COMPLETE

4. **Lists** (lines 539-557)
   - Metadata: `type` (ordered/unordered), `item_count`
   - Task checkbox support
   - Block type: `"list"`
   - ✅ COMPLETE

5. **Callouts** (lines 559-573)
   - Metadata: `callout_type`, `title`
   - Block type: `"callout"`
   - ✅ COMPLETE

### What's Missing ❌

According to `ASTBlockType` enum (crucible-parser/src/types.rs lines 848-868):

6. **LaTeX** - Not implemented
7. **Blockquote** - Not implemented
8. **Table** - Not implemented
9. **HorizontalRule** - Not implemented
10. **ThematicBreak** - Not implemented

## Implementation Plan

### Phase 1: Add Missing Block Types (TDD Approach)

#### Step 1.1: LaTeX Blocks
**Files to Modify:**
- `crates/crucible-surrealdb/src/eav_graph/ingest.rs` (add to `build_blocks()`)

**TDD Steps:**
1. **RED**: Write test in `crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs`:
   ```rust
   #[tokio::test]
   async fn test_latex_block_mapping() {
       let content = r#"
   # Math Example

   Inline: $E = mc^2$

   Block:
   $$
   \int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
   $$
   "#;

       let parser = CrucibleParser::new();
       let doc = parser.parse_content(content).await.unwrap();

       let client = SurrealClient::new_isolated_memory().await.unwrap();
       apply_eav_graph_schema(&client).await.unwrap();
       let store = EAVGraphStore::new(client.clone());
       let ingestor = DocumentIngestor::new(&store);

       let entity_id = ingestor.ingest(&doc, "math.md").await.unwrap();
       let blocks = store.get_blocks(&entity_id.id).await.unwrap();

       let latex_blocks: Vec<_> = blocks.iter()
           .filter(|b| b.block_type == "latex")
           .collect();

       assert_eq!(latex_blocks.len(), 2); // 1 inline, 1 block

       // Check inline LaTeX
       let inline = latex_blocks.iter()
           .find(|b| b.metadata.get("inline") == Some(&serde_json::json!(true)))
           .unwrap();
       assert!(inline.content.contains("E = mc^2"));

       // Check block LaTeX
       let block = latex_blocks.iter()
           .find(|b| b.metadata.get("inline") == Some(&serde_json::json!(false)))
           .unwrap();
       assert!(block.content.contains("\\int"));
   }
   ```

2. **GREEN**: Add LaTeX handling to `build_blocks()`:
   ```rust
   // LaTeX expressions with inline flag
   for latex in &doc.content.latex_expressions {
       let metadata = serde_json::json!({
           "inline": latex.inline,
           "display_mode": !latex.inline
       });
       blocks.push(make_block_with_metadata(
           entity_id,
           &format!("latex{}", index),
           index,
           "latex",
           &latex.content,
           metadata,
       ));
       index += 1;
   }
   ```

3. **REFACTOR**: Ensure proper ordering with other blocks
4. **VERIFY**: Test passes

#### Step 1.2: Blockquote Blocks
**TDD Steps:**
1. **RED**: Write test for blockquotes (distinguishing from callouts):
   ```rust
   #[tokio::test]
   async fn test_blockquote_vs_callout() {
       let content = r#"
   > This is a regular blockquote
   > It can span multiple lines

   > [!note] This is a callout
   > Different from blockquote
   "#;

       let parser = CrucibleParser::new();
       let doc = parser.parse_content(content).await.unwrap();

       let client = SurrealClient::new_isolated_memory().await.unwrap();
       apply_eav_graph_schema(&client).await.unwrap();
       let store = EAVGraphStore::new(client.clone());
       let ingestor = DocumentIngestor::new(&store);

       let entity_id = ingestor.ingest(&doc, "quotes.md").await.unwrap();
       let blocks = store.get_blocks(&entity_id.id).await.unwrap();

       let blockquotes: Vec<_> = blocks.iter()
           .filter(|b| b.block_type == "blockquote")
           .collect();
       let callouts: Vec<_> = blocks.iter()
           .filter(|b| b.block_type == "callout")
           .collect();

       assert_eq!(blockquotes.len(), 1);
       assert_eq!(callouts.len(), 1);

       assert!(blockquotes[0].content.contains("regular blockquote"));
       assert!(callouts[0].content.contains("callout"));
   }
   ```

2. **GREEN**: Add blockquote handling (need to check parser first - might be in callouts or separate):
   ```rust
   // Blockquotes (non-callout quotes)
   for blockquote in &doc.content.blockquotes {
       let metadata = serde_json::json!({
           "nested_level": blockquote.nested_level.unwrap_or(0)
       });
       blocks.push(make_block_with_metadata(
           entity_id,
           &format!("quote{}", index),
           index,
           "blockquote",
           &blockquote.content,
           metadata,
       ));
       index += 1;
   }
   ```

3. **VERIFY**: Test passes

#### Step 1.3: Table Blocks
**TDD Steps:**
1. **RED**: Write test for tables:
   ```rust
   #[tokio::test]
   async fn test_table_block_mapping() {
       let content = r#"
   # Data Table

   | Name | Age | Role |
   |------|-----|------|
   | Alice | 30 | Dev |
   | Bob | 25 | Designer |
   "#;

       let parser = CrucibleParser::new();
       let doc = parser.parse_content(content).await.unwrap();

       let client = SurrealClient::new_isolated_memory().await.unwrap();
       apply_eav_graph_schema(&client).await.unwrap();
       let store = EAVGraphStore::new(client.clone());
       let ingestor = DocumentIngestor::new(&store);

       let entity_id = ingestor.ingest(&doc, "table.md").await.unwrap();
       let blocks = store.get_blocks(&entity_id.id).await.unwrap();

       let table = blocks.iter()
           .find(|b| b.block_type == "table")
           .unwrap();

       assert_eq!(table.metadata.get("rows").unwrap(), &serde_json::json!(2));
       assert_eq!(table.metadata.get("columns").unwrap(), &serde_json::json!(3));
       assert!(table.metadata.get("headers").is_some());
   }
   ```

2. **GREEN**: Add table handling:
   ```rust
   // Tables with row/column metadata
   for table in &doc.content.tables {
       let metadata = serde_json::json!({
           "rows": table.rows.len(),
           "columns": table.columns.len(),
           "headers": table.headers.clone()
       });
       blocks.push(make_block_with_metadata(
           entity_id,
           &format!("table{}", index),
           index,
           "table",
           &table.raw_content,
           metadata,
       ));
       index += 1;
   }
   ```

3. **VERIFY**: Test passes

#### Step 1.4: HorizontalRule / ThematicBreak
**TDD Steps:**
1. **RED**: Write test:
   ```rust
   #[tokio::test]
   async fn test_horizontal_rule_mapping() {
       let content = r#"
   Section 1

   ---

   Section 2

   ***

   Section 3
   "#;

       let parser = CrucibleParser::new();
       let doc = parser.parse_content(content).await.unwrap();

       let client = SurrealClient::new_isolated_memory().await.unwrap();
       apply_eav_graph_schema(&client).await.unwrap();
       let store = EAVGraphStore::new(client.clone());
       let ingestor = DocumentIngestor::new(&store);

       let entity_id = ingestor.ingest(&doc, "sections.md").await.unwrap();
       let blocks = store.get_blocks(&entity_id.id).await.unwrap();

       let rules: Vec<_> = blocks.iter()
           .filter(|b| b.block_type == "horizontal_rule")
           .collect();

       assert_eq!(rules.len(), 2);
   }
   ```

2. **GREEN**: Add horizontal rule handling:
   ```rust
   // Horizontal rules / thematic breaks
   for rule in &doc.content.horizontal_rules {
       let metadata = serde_json::json!({
           "style": rule.style.clone() // "---" or "***"
       });
       blocks.push(make_block_with_metadata(
           entity_id,
           &format!("hr{}", index),
           index,
           "horizontal_rule",
           &rule.raw_content,
           metadata,
       ));
       index += 1;
   }
   ```

3. **VERIFY**: Test passes

### Phase 2: Verify Parser Support

**Before implementing**, check if the parser actually extracts these block types:

1. Check `ParsedDocument` structure in `crucible-parser/src/types.rs`
2. Check `DocumentContent` fields
3. If missing, may need to:
   - Add parser extension for that block type
   - Or use the existing AST blocks from `parsed_blocks` field

**Action**: Inspect parser capabilities first, then adjust plan accordingly.

### Phase 3: Integration Testing

Create comprehensive integration test:

```rust
#[tokio::test]
async fn test_all_block_types_mapped() {
    let content = include_str!("../fixtures/all_block_types.md");

    let parser = CrucibleParser::new();
    let doc = parser.parse_content(content).await.unwrap();

    let client = SurrealClient::new_isolated_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client.clone());
    let ingestor = DocumentIngestor::new(&store);

    let entity_id = ingestor.ingest(&doc, "all_types.md").await.unwrap();
    let blocks = store.get_blocks(&entity_id.id).await.unwrap();

    // Verify all block types present
    let block_types: HashSet<_> = blocks.iter()
        .map(|b| b.block_type.as_str())
        .collect();

    assert!(block_types.contains("heading"));
    assert!(block_types.contains("paragraph"));
    assert!(block_types.contains("code"));
    assert!(block_types.contains("list"));
    assert!(block_types.contains("callout"));
    assert!(block_types.contains("latex"));
    assert!(block_types.contains("blockquote"));
    assert!(block_types.contains("table"));
    assert!(block_types.contains("horizontal_rule"));
}
```

## Acceptance Criteria

- [x] Headings mapped (already done)
- [x] Paragraphs mapped (already done)
- [x] Code blocks mapped (already done)
- [x] Lists mapped (already done)
- [x] Callouts mapped (already done)
- [ ] LaTeX mapped with inline flag
- [ ] Blockquotes mapped (distinguished from callouts)
- [ ] Tables mapped with row/column metadata
- [ ] Horizontal rules mapped
- [ ] All 9 block types have tests
- [ ] BLAKE3 hashes computed for all block types
- [ ] Integration test passes

## Files to Modify

1. `crates/crucible-surrealdb/src/eav_graph/ingest.rs` - Add missing block types to `build_blocks()`
2. `crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs` - Add tests for each block type
3. `crates/crucible-surrealdb/tests/fixtures/all_block_types.md` - Create comprehensive test fixture

## Dependencies

- **Parser capability check**: Verify parser extracts LaTeX, blockquotes, tables, horizontal rules
- **If parser doesn't extract**: May need to add parser extensions first (separate task)

## Estimated Effort

- **With parser support**: 2-3 hours (straightforward addition to `build_blocks()`)
- **Without parser support**: 1-2 days (need to add parser extensions first)

## Next Steps

1. Inspect parser capabilities (`ParsedDocument` and `DocumentContent` structures)
2. Determine which block types are already extracted by parser
3. Implement missing block types in `build_blocks()` following TDD
4. Create comprehensive integration test
