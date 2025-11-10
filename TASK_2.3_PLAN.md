# Task 2.3 Plan: Implement Section Detection for Merkle Trees

**Status**: Planning
**OpenSpec Reference**: `openspec/changes/2025-11-08-enhance-markdown-parser-eav-mapping/tasks.md` Lines 580-642

## Background

**Purpose**: Enable Merkle tree mid-level nodes by detecting document sections.

**Why Needed**: Merkle trees require hierarchical structure:
- **Leaf nodes**: Individual blocks (paragraphs, code, etc.)
- **Mid-level nodes**: Sections (groups of blocks under a heading)
- **Root node**: Entire document

Currently, we have leaf nodes (blocks) and can compute a root hash, but we're missing the mid-level section nodes for efficient change detection.

## Current State

### What Exists ✅

1. **Block extraction** with hierarchy (`crucible-parser/src/block_extractor.rs`)
   - `HeadingTree` tracks parent-child relationships
   - Each block has `parent_block_id` and `depth` fields
   - Blocks know which heading they belong under

2. **Block storage** (`crucible-surrealdb/src/eav_graph/store.rs`)
   - `get_child_blocks()` retrieves blocks under a heading
   - Hierarchy preserved in storage

3. **Hybrid Merkle Tree** (`crucible-core/src/merkle/hybrid.rs`)
   - Exists but may need section-level support

### What's Missing ❌

1. **Section detection logic**: Group blocks into sections
2. **Section hash computation**: Merkle hash for each section
3. **Integration with HybridMerkleTree**: Feed sections to Merkle tree

## Architecture Decision

### Two Approaches:

#### Approach A: Section as Virtual Concept
- Sections are computed on-the-fly when building Merkle tree
- No new database entities
- Section = heading + its child blocks
- **Pros**: Simple, no schema changes
- **Cons**: Section boundaries recomputed each time

#### Approach B: Section as First-Class Entity
- Create `section` entity type in EAV schema
- Store section boundaries explicitly
- **Pros**: Pre-computed sections, queryable
- **Cons**: More complex, schema changes, data duplication

**Recommendation**: **Approach A** - Keep it simple, compute sections on-demand for Merkle tree

## Implementation Plan

### Phase 1: Section Detection Logic

**Files to Create:**
- `crates/crucible-core/src/merkle/section.rs`

**Data Structures:**

```rust
/// A section of a document, defined by a heading and its child blocks
#[derive(Debug, Clone)]
pub struct Section {
    /// The heading block that defines this section
    pub heading_block_id: String,

    /// The heading level (1-6)
    pub level: u8,

    /// IDs of all blocks directly under this heading
    /// (not including nested subsections)
    pub direct_child_block_ids: Vec<String>,

    /// IDs of subsections (headings with level > this heading's level)
    pub subsection_ids: Vec<String>,

    /// Merkle hash of this section (heading + direct children + subsection hashes)
    pub section_hash: BlockHash,
}

/// Detects sections from a flat list of blocks with hierarchy metadata
pub struct SectionDetector;

impl SectionDetector {
    pub fn detect_sections(blocks: &[Block]) -> Vec<Section> {
        // Algorithm:
        // 1. Iterate through blocks
        // 2. When we hit a heading:
        //    - Create new section
        //    - Add subsequent non-heading blocks as direct children
        //    - Add subsequent headings with higher level as subsections
        // 3. When we hit same/lower level heading:
        //    - Close current section
        //    - Start new section
    }

    pub fn compute_section_hash(section: &Section, block_hashes: &HashMap<String, BlockHash>) -> BlockHash {
        // Compute Merkle hash for section:
        // Hash(heading_hash || direct_children_hashes || subsection_hashes)
    }
}
```

**TDD Steps:**

#### Step 1.1: Simple Section Detection

1. **RED**: Write test for single H1 section:
   ```rust
   #[test]
   fn test_detect_single_h1_section() {
       let blocks = vec![
           Block::heading(1, "Introduction", "h1"),
           Block::paragraph("Intro text", "p1", Some("h1")),
           Block::paragraph("More text", "p2", Some("h1")),
       ];

       let sections = SectionDetector::detect_sections(&blocks);

       assert_eq!(sections.len(), 1);
       assert_eq!(sections[0].heading_block_id, "h1");
       assert_eq!(sections[0].level, 1);
       assert_eq!(sections[0].direct_child_block_ids, vec!["p1", "p2"]);
       assert_eq!(sections[0].subsection_ids.len(), 0);
   }
   ```

2. **GREEN**: Implement basic section detection

3. **REFACTOR**: Clean up

4. **VERIFY**: Test passes

#### Step 1.2: Nested Sections

1. **RED**: Test for H1 with H2 subsections:
   ```rust
   #[test]
   fn test_detect_nested_sections() {
       let blocks = vec![
           Block::heading(1, "Chapter 1", "h1"),
           Block::paragraph("Chapter intro", "p1", Some("h1")),
           Block::heading(2, "Section 1.1", "h2-1", Some("h1")),
           Block::paragraph("Section text", "p2", Some("h2-1")),
           Block::heading(2, "Section 1.2", "h2-2", Some("h1")),
           Block::paragraph("More section text", "p3", Some("h2-2")),
       ];

       let sections = SectionDetector::detect_sections(&blocks);

       // Should have 3 sections: 1 H1, 2 H2s
       assert_eq!(sections.len(), 3);

       // H1 section should reference H2 sections
       let h1_section = sections.iter()
           .find(|s| s.heading_block_id == "h1")
           .unwrap();
       assert_eq!(h1_section.direct_child_block_ids, vec!["p1"]);
       assert_eq!(h1_section.subsection_ids, vec!["h2-1", "h2-2"]);
   }
   ```

2. **GREEN**: Handle nested sections

3. **VERIFY**: Test passes

#### Step 1.3: Multiple Top-Level Sections

1. **RED**: Test for multiple H1 sections:
   ```rust
   #[test]
   fn test_detect_multiple_h1_sections() {
       let blocks = vec![
           Block::heading(1, "Introduction", "h1-1"),
           Block::paragraph("Intro", "p1", Some("h1-1")),
           Block::heading(1, "Methods", "h1-2"),
           Block::paragraph("Methods text", "p2", Some("h1-2")),
       ];

       let sections = SectionDetector::detect_sections(&blocks);

       assert_eq!(sections.len(), 2);
       assert_eq!(sections[0].heading_block_id, "h1-1");
       assert_eq!(sections[1].heading_block_id, "h1-2");
   }
   ```

2. **GREEN**: Handle multiple top-level sections

3. **VERIFY**: Test passes

### Phase 2: Section Hash Computation

**TDD Steps:**

1. **RED**: Test section hash computation:
   ```rust
   #[test]
   fn test_compute_section_hash() {
       let section = Section {
           heading_block_id: "h1".to_string(),
           level: 1,
           direct_child_block_ids: vec!["p1".to_string(), "p2".to_string()],
           subsection_ids: vec![],
           section_hash: BlockHash::zero(),
       };

       let mut block_hashes = HashMap::new();
       block_hashes.insert("h1".to_string(), BlockHash::from_hex("aaa...").unwrap());
       block_hashes.insert("p1".to_string(), BlockHash::from_hex("bbb...").unwrap());
       block_hashes.insert("p2".to_string(), BlockHash::from_hex("ccc...").unwrap());

       let hash = SectionDetector::compute_section_hash(&section, &block_hashes);

       // Hash should be deterministic
       assert!(!hash.is_zero());

       // Same inputs should produce same hash
       let hash2 = SectionDetector::compute_section_hash(&section, &block_hashes);
       assert_eq!(hash, hash2);
   }
   ```

2. **GREEN**: Implement hash computation using BLAKE3

3. **VERIFY**: Test passes

### Phase 3: Integration with HybridMerkleTree

**Files to Modify:**
- `crates/crucible-core/src/merkle/hybrid.rs`

**TDD Steps:**

1. **RED**: Test Merkle tree with sections:
   ```rust
   #[test]
   fn test_merkle_tree_with_sections() {
       let blocks = vec![
           Block::heading(1, "Introduction", "h1"),
           Block::paragraph("Text 1", "p1", Some("h1")),
           Block::heading(1, "Methods", "h2"),
           Block::paragraph("Text 2", "p2", Some("h2")),
       ];

       let sections = SectionDetector::detect_sections(&blocks);
       let tree = HybridMerkleTree::from_blocks_and_sections(&blocks, &sections);

       // Tree should have:
       // - Root node (entire document)
       // - 2 mid-level nodes (2 sections)
       // - 4 leaf nodes (2 headings + 2 paragraphs)
       assert_eq!(tree.leaf_count(), 4);
       assert_eq!(tree.section_count(), 2);

       let root_hash = tree.root_hash();
       assert!(!root_hash.is_zero());
   }
   ```

2. **GREEN**: Add section support to `HybridMerkleTree`

3. **REFACTOR**: Ensure efficient incremental updates

4. **VERIFY**: Test passes

### Phase 4: Integration with Ingestor

**Files to Modify:**
- `crates/crucible-surrealdb/src/eav_graph/ingest.rs`

**Changes:**

```rust
impl DocumentIngestor<'_> {
    pub async fn ingest(&self, doc: &ParsedDocument, relative_path: &str) -> Result<RecordId<EntityRecord>> {
        // ... existing entity, property, block storage ...

        // NEW: Compute and store section hashes for Merkle tree
        let blocks = build_blocks(&entity_id, doc);
        let sections = SectionDetector::detect_sections(&blocks);

        // Store section hashes as entity properties
        for section in sections {
            let section_hash_prop = Property::new(
                property_id(&entity_id, "merkle", &format!("section_{}", section.heading_block_id)),
                entity_id.clone(),
                "merkle",
                &format!("section_{}", section.heading_block_id),
                PropertyValue::Text(section.section_hash.to_hex()),
            );
            self.store.upsert_property(&section_hash_prop).await?;
        }

        Ok(entity_id)
    }
}
```

**TDD Steps:**

1. **RED**: Integration test:
   ```rust
   #[tokio::test]
   async fn test_ingest_stores_section_hashes() {
       let content = r#"
   # Introduction
   Intro paragraph.

   # Methods
   Methods paragraph.
   "#;

       let parser = CrucibleParser::new();
       let doc = parser.parse_content(content).await.unwrap();

       let client = SurrealClient::new_isolated_memory().await.unwrap();
       apply_eav_graph_schema(&client).await.unwrap();
       let store = EAVGraphStore::new(client.clone());
       let ingestor = DocumentIngestor::new(&store);

       let entity_id = ingestor.ingest(&doc, "sections.md").await.unwrap();

       // Verify section hashes stored
       let merkle_props = store.get_properties(&entity_id.id, "merkle").await.unwrap();

       let section_props: Vec<_> = merkle_props.iter()
           .filter(|p| p.key.starts_with("section_"))
           .collect();

       assert_eq!(section_props.len(), 2); // 2 H1 sections
   }
   ```

2. **GREEN**: Implement section hash storage

3. **VERIFY**: Test passes

## Acceptance Criteria

- [ ] Top-level headings (H1) detected as sections
- [ ] Nested sections (H2 under H1) detected
- [ ] Section boundaries identified correctly
- [ ] Section hashes computed using BLAKE3
- [ ] Integration with HybridMerkleTree works
- [ ] Section hashes stored in EAV as properties
- [ ] 5+ section detection tests passing
- [ ] Performance: Section detection <10ms for 1000-block document

## Files to Create/Modify

### New Files:
1. `crates/crucible-core/src/merkle/section.rs` - Section detection logic

### Modified Files:
1. `crates/crucible-core/src/merkle/hybrid.rs` - Add section support to Merkle tree
2. `crates/crucible-core/src/merkle/mod.rs` - Export section module
3. `crates/crucible-surrealdb/src/eav_graph/ingest.rs` - Store section hashes
4. `crates/crucible-core/tests/merkle_tests.rs` - Add section detection tests

## Dependencies

- **Task 2.1** must be complete (all block types mapped)
- **Block hierarchy** must be working (already done via HeadingTree)

## Design Questions

### Q1: Should sections be recursive?
**Answer**: Yes, H1 contains H2s, H2s contain H3s, etc. But only compute hashes for H1 sections (mid-level nodes) to keep tree shallow.

### Q2: How to handle documents without H1s?
**Answer**: Treat entire document as single section OR treat each H2 as top-level section. Decision: Document without H1 = single section with all blocks.

### Q3: Store sections in database or compute on-demand?
**Answer**: Compute on-demand for Merkle tree, but store section hashes as properties for quick comparison.

## Estimated Effort

- **Phase 1** (Section detection): 3-4 hours
- **Phase 2** (Hash computation): 1-2 hours
- **Phase 3** (Merkle tree integration): 2-3 hours
- **Phase 4** (Ingestor integration): 1-2 hours

**Total**: 1-2 days

## Success Metrics

1. Section detection handles all heading levels correctly
2. Section hashes change when content changes
3. Merkle tree can efficiently detect which section changed
4. Performance: <10ms overhead for section detection on 1000-block doc

## Next Steps

1. Create `crucible-core/src/merkle/section.rs`
2. Implement `SectionDetector` with TDD
3. Add tests to `crucible-core/tests/merkle_tests.rs`
4. Integrate with `HybridMerkleTree`
5. Update ingestor to store section hashes
