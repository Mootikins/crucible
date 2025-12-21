---
title: Parser Types & Data Flow Analysis
description: Architecture analysis of markdown parsing types and data flow
type: analysis
system: parser
status: review
updated: 2024-12-13
tags:
  - meta
  - analysis
  - parser
---

# Parser Types & Data Flow Analysis

## Executive Summary

The parser system exhibits **excellent type ownership architecture** following Dependency Inversion Principle. Types are canonically defined in `crucible-core` and re-exported by `crucible-parser` implementation crate.

**Key Findings:**
- ✅ Clean type ownership (canonical in core, re-exported in parser)
- ✅ Well-designed extension system for syntax features
- ✅ Modular parsing pipeline with async support
- ✅ Phase 2 block hashing integrated but disabled by default
- ⚠️ Some potential for extension ordering conflicts
- ⚠️ Block extraction complexity could be simplified

---

## Types Inventory

### Core Document Types
- **ParsedNote** - Main document structure (~3 KB per note)
  - Location: `crucible-core/src/parser/types/parsed_note.rs`
  - Builder pattern available via `ParsedNoteBuilder`

- **NoteContent** - Structured content container
  - ⚠️ **Duplication** - wikilinks, tags, latex, callouts, footnotes stored in both NoteContent AND ParsedNote

### Link Types
- **Wikilink** - `[[target]]` with aliases, heading refs, block refs
- **Tag** - `#tag` with nested path support (`#project/ai/llm`)
- **InlineLink** - Standard markdown `[text](url)`

### AST Types (Phase 2)
- **ASTBlock** - Block with type, content, offsets, hash, metadata
- **BlockHash** - 32-byte BLAKE3 hash wrapper
- **Frontmatter** - Lazy-parsed YAML/TOML metadata

---

## Parser Flow

```
Markdown File
    ↓
parse_file() - Read and validate
    ↓
parse_content()
    ↓
1. Parse frontmatter (YAML/TOML)
2. Initialize NoteContent
3. Apply extensions by priority:
   - BasicMarkdownIt (100)
   - Wikilinks (80)
   - Tags (70)
   - InlineLinks (60)
   - Callouts (55)
   - Latex (54)
   - Footnotes (53)
   - Blockquotes (52)
4. Extract structural metadata
5. Build ParsedNote
6. Optional: Block processing (Phase 2)
    ↓
ParsedNote (ready for storage)
```

---

## Issues Found

- [ ] **Data Duplication in ParsedNote** (Low - by design)
  - Fields duplicated between NoteContent and ParsedNote top-level
  - Recommendation: Document as intentional convenience pattern

- [ ] **Extension Priority Conflicts** (Low)
  - No warning when extensions have same priority
  - Recommendation: Add priority validation during registration

- [ ] **Block Extractor Complexity** (Medium)
  - Location: `crucible-parser/src/block_extractor.rs`
  - Multiple passes over content
  - Recommendation: Consider single-pass during extension parsing

---

## Type Ownership

**Canonical Definitions** (crucible-core/src/parser/types/):
- ✅ `parsed_note.rs` - ParsedNote, ParsedNoteBuilder
- ✅ `content.rs` - NoteContent, Heading, CodeBlock
- ✅ `links.rs` - Wikilink, Tag, InlineLink
- ✅ `ast.rs` - ASTBlock, ASTBlockType
- ✅ `block_hash.rs` - BlockHash
- ✅ `frontmatter.rs` - Frontmatter

**Re-exports** (crucible-parser/src/lib.rs):
- ✅ All core types properly re-exported

**No critical issues found.** Parser is well-designed.
