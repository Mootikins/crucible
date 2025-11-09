# Enhance Markdown Parser with EAV+Graph Entity Mapping

**Change ID**: `2025-11-08-enhance-markdown-parser-eav-mapping`
**Status**: In Progress - Phase 1 Complete ✅
**Created**: 2025-11-08
**Author**: Matthew Krohn
**Last Updated**: 2025-11-09

## Problem Statement

The current parser (`crucible-parser`) exists but does not properly map Markdown AST nodes to EAV+Graph entities for storage in SurrealDB. This prevents:

1. **Proper knowledge graph construction**: Wikilinks, tags, and document structure not stored as entities/relations
2. **Block-level operations**: Cannot perform semantic search, change detection, or embedding at block granularity
3. **Merkle tree integration**: Missing section detection needed for hybrid Merkle tree structure
4. **Obsidian compatibility**: Incomplete support for Obsidian-flavored Markdown extensions
5. **Frontmatter portability**: No bidirectional sync between YAML frontmatter and database properties

## Proposed Solution

Enhance the parser to produce EAV+Graph entities directly from Markdown AST, with full support for Obsidian-flavored Markdown, frontmatter handling, and proper section/block hierarchy for Merkle tree integration.

### Key Changes

1. **Complete AST → EAV+Graph Mapping**
   - Document → `entities` (type: "note") with frontmatter in `properties` (namespace: "frontmatter")
   - Blocks (headings, paragraphs, lists, code, callouts) → `entities` (type: "block")
   - Wikilinks → `relations` (relation_type: "wikilink")
   - Tags → `entity_tags` junction table
   - Inline links → `relations` (relation_type: "link") with metadata

2. **Frontmatter Bidirectional Sync**
   - Extract YAML frontmatter → `properties` table (namespace: "frontmatter")
   - Always include `date_created` and `date_modified` in frontmatter for portability
   - Support common properties: tags, type, status, author, etc.
   - Enable property updates to trigger frontmatter rewrites (future)

3. **Section Detection**
   - Identify top-level headings as section boundaries
   - Build section hierarchy for Merkle tree mid-level nodes
   - Preserve parent-child relationships between blocks

4. **Obsidian Extension Support**
   - Wikilinks: `[[Note Name]]`, `[[Note|Alias]]`
   - Tags: `#tag`, `#nested/tag`
   - Callouts: `> [!note]`, `> [!warning]`, etc. ([spec](https://help.obsidian.md/callouts))
   - Frontmatter: YAML metadata
   - Footnotes: `[^1]`
   - Embedded images: `![[image.png]]`
   - Inline links: Preserve relation metadata

5. **Block-Level Granularity**
   - Each block becomes an entity with unique ID
   - Lists treated as single block (embedding for whole list)
   - Inline elements (bold, italic) preserved in markdown text
   - Nested structures via parent-child relations

## Success Criteria

1. Parser outputs EAV+Graph entities compatible with `crucible-surrealdb` storage layer
2. All Obsidian-flavored Markdown extensions supported
3. Frontmatter properly extracted and stored with namespace "frontmatter"
4. Section hierarchy properly detected for Merkle tree integration
5. Block-level entities enable semantic search and change detection
6. Comprehensive test suite with Obsidian syntax fixtures
7. No breaking changes to existing parser API (extend, don't replace)

## Scope

### In Scope
- AST → EAV+Graph entity mapping
- Frontmatter extraction and storage (namespace: "frontmatter")
- Obsidian-flavored Markdown support
- Section and block detection
- Relation extraction (wikilinks, tags, inline links)
- Integration with `crucible-core` storage traits
- Test fixtures for all Obsidian syntax

### Out of Scope
- File watching (separate proposal)
- Merkle tree building (depends on parser, separate proposal)
- Embedding generation (separate concern)
- Frontmatter write-back (bidirectional sync - future enhancement)
- CRDT integration (long-term)
- Custom markdown extensions beyond Obsidian (future)
- DataView query support (deferred to scripting layer)

## Architecture Impact

### Data Flow
```
Markdown File
├── Frontmatter (YAML)
│   ├── tags: ["project", "ai"]     → properties (namespace: "frontmatter")
│   ├── type: "template"            → properties (namespace: "frontmatter")
│   ├── status: "draft"             → properties (namespace: "frontmatter")
│   ├── date_created: 2025-11-08    → properties (namespace: "frontmatter")
│   └── date_modified: 2025-11-08   → properties (namespace: "frontmatter")
└── Content (Markdown AST)
    ├── # Heading 1                 → entities (type: "block", block_type: "heading", level: 1)
    │   ├── Paragraph               → entities (type: "block", parent_block_id → heading)
    │   └── List                    → entities (type: "block", parent_block_id → heading)
    └── # Heading 2                 → entities (type: "block", block_type: "heading", level: 1)
        └── Code Block              → entities (type: "block", parent_block_id → heading)

Document metadata → entities (type: "note")
Wikilinks [[Note]] → relations (relation_type: "wikilink")
Tags #project      → entity_tags junction

Block Hierarchy via parent_block_id enables:
- Efficient Merkle tree (rehash only changed subtree)
- Document structure queries ("all blocks under 'Getting Started'")
- Precise change detection with context breadcrumbs
- Auto-generated ToC from heading hierarchy
```

### AST → Entity Mapping

| Markdown Element | Entity Type | Storage Details |
|------------------|-------------|-----------------|
| Document | `entities` (type: "note") | Frontmatter → `properties` (namespace: "frontmatter") |
| Heading (h1-h6) | `entities` (type: "block") | block_type: "heading", depth: 1-6 |
| Paragraph | `entities` (type: "block") | block_type: "paragraph" |
| List (ul/ol) | `entities` (type: "block") | block_type: "list", internal list_item tree |
| Code Block | `entities` (type: "block") | block_type: "code", language in metadata |
| Callout | `entities` (type: "block") | block_type: "callout", variant in metadata |
| Blockquote | `entities` (type: "block") | block_type: "blockquote" |
| Table | `entities` (type: "block") | block_type: "table" |
| Wikilink `[[Note]]` | `relations` | relation_type: "wikilink", from_block → to_note |
| Tag `#tag` | `entity_tags` | entity_id → tag_id, source: "parser" |
| Inline link | `relations` | relation_type: "link", metadata contains URL |
| Footnote `[^1]` | `relations` | relation_type: "footnote", from_block → footnote_block |
| Embedded image `![[img]]` | `relations` | relation_type: "embedded", to asset entity |

## Implementation Plan

See `tasks.md` for detailed task breakdown.

### High-Level Phases

1. **Phase 1: Frontmatter Extraction** (Week 1)
   - Parse YAML frontmatter
   - Store in `properties` table with namespace "frontmatter"
   - Test with common Obsidian properties

2. **Phase 2: Block Parsing** (Week 2)
   - Map AST nodes to block entities
   - Implement section detection
   - Handle nested structures (lists, blockquotes)

3. **Phase 3: Relation Extraction** (Week 3)
   - Parse wikilinks and create relation entities
   - Parse tags and populate entity_tags
   - Handle inline links and footnotes

4. **Phase 4: Obsidian Extensions** (Week 4)
   - Implement callout parsing
   - Handle embedded images
   - Support wikilink aliases

5. **Phase 5: Integration & Testing** (Week 5)
   - Integrate with storage traits
   - Comprehensive test suite
   - Performance optimization

## Risks & Mitigation

**Risk**: Breaking existing parser consumers
**Mitigation**: Add new EAV output path alongside existing output, migrate incrementally

**Risk**: Performance regression on large files
**Mitigation**: Benchmark against existing parser, optimize if needed

**Risk**: Incomplete Obsidian syntax support
**Mitigation**: Comprehensive test fixtures, reference [Obsidian documentation](https://help.obsidian.md/obsidian-flavored-markdown)

**Risk**: Frontmatter schema conflicts (heterogeneous properties)
**Mitigation**: EAV model handles this naturally via typed value fields

## Dependencies

- `crucible-surrealdb` EAV+Graph schema v0.2.0 (✅ implemented)
- `crucible-core` storage traits (need trait definitions)
- Markdown parsing library (pulldown-cmark or similar)

## Related Work

- [ARCHITECTURE.md](../../../docs/ARCHITECTURE.md): Parser architecture section, frontmatter handling
- [Obsidian Flavored Markdown](https://help.obsidian.md/obsidian-flavored-markdown)
- [Obsidian Callouts](https://help.obsidian.md/callouts)
- [STATUS.md](../../../STATUS.md): Current refactor status

## Open Questions

1. Should we support DataView queries now or defer to scripting layer?
   - **Recommendation**: Defer to scripting layer (mid-term)

2. How to handle conflicting wikilink targets (multiple notes with same name)?
   - **Recommendation**: Store all candidates in relation metadata, resolve at query time

3. Should embedded images create asset entities or just relations?
   - **Recommendation**: Create asset entities for future media management

4. What's the strategy for custom markdown extensions beyond Obsidian?
   - **Recommendation**: Plugin system (long-term), not in this proposal

5. Should we persist section entities separately from blocks?
   - **Decision**: No, headings are blocks with `block_type: "heading"` and `level: 1-6`. Hierarchy tracked via `parent_block_id` for cleaner schema and more homogeneous code structures.

## Success Metrics

- [ ] All Obsidian syntax test fixtures pass
- [x] Frontmatter properties correctly stored with namespace "frontmatter" (Phase 1 ✅)
- [ ] Section hierarchy enables Merkle tree integration
- [x] Performance: Batch operations optimized (N+1 queries eliminated - Phase 1 ✅)
- [x] Zero breaking changes to existing parser API (maintained - Phase 1 ✅)
- [x] Test coverage >90% for new code (Phase 1: 8/8 integration tests + 11/11 trait tests ✅)

## Implementation Progress

### Phase 1: Frontmatter Extraction ✅ COMPLETE (2025-11-09)

**Status**: Fully implemented with optimizations

**What was completed:**
1. ✅ Frontmatter parsing (pre-existing, 98 tests)
2. ✅ FrontmatterPropertyMapper created (`crucible-core/src/parser/frontmatter_mapper.rs`)
3. ✅ PropertyStorage trait implemented (`crucible-surrealdb/src/eav_graph/store.rs`)
4. ✅ Integration tests (8/8 passing)
5. ✅ **BONUS**: Security fixes (SQL injection vulnerability patched)
6. ✅ **BONUS**: Performance optimizations (N+1 query prevention, zero-allocation namespaces)
7. ✅ **BONUS**: Extensibility improvements (tagged PropertyValue enum for schema evolution)

**Commits:**
- `e5631fd` - Schema simplification to JSON PropertyValue
- `986d5e9` - Security fixes and code quality improvements
- `a5b871d` - Advanced optimizations (performance + extensibility)

**Test Coverage**: 8 integration tests + 11 trait tests = 19 tests

**QA Checkpoint Passed**: Code review completed, all antipatterns addressed, all tests passing

### Phase 2: Block Parsing with Heading Hierarchy (Next)

**Status**: Not started

**See**: `tasks.md` for detailed implementation plan
