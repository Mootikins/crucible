# Project Context

## Purpose
Crucible is a plaintext-first knowledge management system that combines wikilink-based graphs with block-level embeddings and semantic search. Built for agent-first knowledge discovery with the Agent Context Protocol (ACP).

## Tech Stack
- **Core**: Rust + Tokio async runtime
- **Database**: SurrealDB with vector extensions (optional, markdown files are source of truth)
- **Parser**: pulldown-cmark with custom extensions (LaTeX, callouts, query blocks)
- **Storage Model**: EAV+Graph (Entity-Attribute-Value + Property Graph)

## Project Conventions

### Code Style
[Describe your code style preferences, formatting rules, and naming conventions]

### Architecture Patterns
- **Dependency Inversion**: Core defines traits, infrastructure implements them
- **Interface Segregation**: Small, focused storage traits (EntityStorage, PropertyStorage, etc.)
- **Parser Extensions**: Pluggable syntax extensions (LaTeX, callouts, query blocks) via SyntaxExtension trait
- **EAV+Graph Storage**: Entities with properties in namespaces (frontmatter, core, plugin) + directed graph relations

### Testing Strategy
[Explain your testing approach and requirements]

### Git Workflow
[Describe your branching strategy and commit conventions]

## Domain Context

### Markdown Parser Extensions
Crucible extends standard CommonMark with:
- **LaTeX Math**: Inline (`$E=mc^2$`) and block (`$$\int_0^1 f(x)dx$$`) mathematical expressions
- **Callouts**: Obsidian-style callouts with variants (note, warning, tip, etc.)
- **Query Blocks**: Embedded queries for dynamic content
- **Wikilinks**: `[[Note Name]]` for bidirectional linking
- **Frontmatter**: YAML/TOML metadata with flat property structure

### Parser Extension Architecture
- All extensions implement `SyntaxExtension` trait
- Extensions are registered in `ExtensionRegistry`
- Located in `crates/crucible-core/src/parser/`
  - `latex.rs` - LaTeX mathematical expressions
  - `callouts.rs` - Obsidian-compatible callouts
  - `query_blocks.rs` - Dynamic query execution

## Important Constraints
[List any technical, business, or regulatory constraints]

## External Dependencies
[Document key external services, APIs, or systems]
