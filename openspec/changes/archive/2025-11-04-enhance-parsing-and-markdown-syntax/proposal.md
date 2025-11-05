## Why

The current Crucible system has basic markdown parsing capabilities but lacks the rich syntax extensions needed for advanced knowledge management. Without enhanced parsing, the system cannot extract structured metadata, handle transclusions, process query blocks, or support agent definitions embedded in documents. This limits the ability to build sophisticated knowledge graphs and AI-powered features.

## What Changes

- **Enhanced Parser Architecture**: Extend existing parser with pluggable syntax extensions and custom block types
- **Markdown Syntax Extensions**: Add transclusion syntax `[[Document]]`, query blocks, and custom inline elements
- **Frontmatter Schema Validation**: Implement comprehensive validation and schema enforcement for document metadata
- **Wikilink Processing**: Advanced relationship extraction and resolution with bidirectional linking
- **Metadata Extraction**: Rich extraction patterns for dates, tags, properties, and custom fields
- **Error Handling**: Comprehensive error reporting with precise location information for syntax errors

## Impact

- **Affected specs**:
  - `enhanced-parser` (extends existing parsing capability)
  - `markdown-syntax-extensions` (new capability)
  - `frontmatter-validation` (new capability)
- **Affected code**:
  - `crates/crucible-core/src/parser/` - enhanced parser architecture
  - `crates/crucible-core/src/types/` - extended document types
  - `crates/crucible-cli/src/commands/` - CLI integration for new syntax
  - Test suites across all affected crates
- **Performance impact**:
  - Maintain sub-100ms parsing performance for 1KB documents
  - Add caching for parsed documents to avoid reprocessing
  - Optimize block parsing for large documents
- **Feature impact**:
  - Enables document transclusions and dynamic content inclusion
  - Supports embedded queries that can be executed against the knowledge base
  - Provides foundation for agent definitions within documents
  - Enables rich metadata extraction for better search and organization