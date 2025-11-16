# Parser Capability Specification

## MODIFIED Requirements

### Requirement: Block-Level Processing (Pure Transformation)
The parser SHALL remain a pure transformation layer (Markdown → AST → EPR), with NO embedding generation or business logic. The parser provides structured blocks that are consumed by the EnrichmentService.

**Architecture Clarification**:
- **Parser Responsibility**: Transform Markdown to structured AST blocks
- **Enrichment Responsibility**: Use AST blocks for embedding, metadata, relations
- **Clear Separation**: Parser is Phase 2, Enrichment is Phase 4

#### Scenario: AST block creation for enrichment pipeline
- **WHEN** parser processes markdown document
- **THEN** parser creates structured block entities (headings, paragraphs, lists, etc.)
- **AND** blocks include content, hierarchy, and position information
- **AND** NO embedding generation occurs during parsing
- **AND** blocks are suitable for Merkle tree construction in Phase 3

#### Scenario: Block metadata for downstream processing
- **WHEN** parser creates block entities
- **THEN** blocks include metadata useful for enrichment:
  - Content text (raw markdown)
  - Block type (heading, paragraph, list, code, etc.)
  - Position in document hierarchy
  - Block ID for referencing
- **AND** metadata is purely structural, NOT derived (no word counts, language detection)

#### Scenario: Parser output consumed by enrichment
- **WHEN** parser completes and returns ParsedNote
- **THEN** Phase 3 uses blocks to build Merkle tree
- **AND** Phase 4 EnrichmentService uses blocks for embedding generation
- **AND** EnrichmentService filters blocks (<5 words) during enrichment, not parsing

### Requirement: Document Metadata (Extracted in Enrichment, NOT Parser)
Document metadata derived from content analysis (word counts, language, complexity) SHALL be extracted in the EnrichmentService, NOT during parsing.

**Architecture Clarification**:
- **Parser**: Extracts only structural metadata (frontmatter, title, hierarchy)
- **EnrichmentService**: Computes derived metadata (word counts, language, reading time, complexity)
- **Storage**: Tracks processing metadata (last_embedded_at, embedding_model)

#### Scenario: Structural metadata from parser (Phase 2)
- **WHEN** parser processes document
- **THEN** parser extracts frontmatter properties (tags, date, author, etc.)
- **AND** parser identifies document title and structure
- **AND** NO content analysis (word counting, language detection) occurs

#### Scenario: Derived metadata from enrichment (Phase 4)
- **WHEN** EnrichmentService processes ParsedNote
- **THEN** MetadataExtractor computes derived metadata:
  - Word count per block and total
  - Language detection (if multi-language)
  - Reading time estimates
  - Content complexity scores
- **AND** derived metadata stored alongside structural metadata

#### Scenario: Processing metadata tracked by storage (Phase 5)
- **WHEN** enriched note is stored
- **THEN** storage layer tracks processing metadata:
  - `last_embedded_at` timestamp
  - `embedding_model` used
  - `blocks_embedded_count`
  - `last_enrichment_version`
- **AND** enables incremental processing queries