---
description: Technical architecture of the dev-kiln documentation system
tags:
  - meta
  - architecture
  - testing
status: implemented
---

# Dev Kiln Architecture

This kiln serves as both documentation and test fixture for Crucible.

## Purpose

The dev-kiln is:

1. **Living Documentation** - User guides and reference material
2. **Test Fixture** - Integration tests validate parsing and indexing
3. **Example Kiln** - Demonstrates best practices for kiln organization

## Directory Structure

```
docs/
├── Index.md                 # Main entry point
├── Guides/                  # Tutorial content
├── Help/                    # Reference documentation
│   ├── CLI/                 # Command references
│   ├── Config/              # Configuration guides
│   ├── Concepts/            # Core concepts
│   ├── Extending/           # Extension guides
│   ├── Rune/                # Scripting reference
│   ├── TUI/                 # Terminal UI reference
│   └── ...
├── Organization Styles/     # PKM methodology guides
├── Agents/                  # Example agent cards
├── Scripts/                 # Example Rune scripts
├── plugins/                 # Example plugins
└── Meta/                    # Internal documentation
```

## Conventions

### File Naming

- Titlecase with spaces: `Component Architecture.md`
- Index files for directories: `Index.md`

### Frontmatter

Every file includes YAML frontmatter:

```yaml
---
description: Brief summary
tags:
  - relevant
  - tags
status: implemented|draft|planned
---
```

### Wikilinks

Internal references use wikilinks:

- `[[Help/Concepts/Kilns]]` - Full path
- `[[Kilns]]` - Title match (ambiguous)
- `[[Help/CLI/chat|chat command]]` - With display text

## Integration Testing

Tests in `crates/crucible-core/tests/` validate:

- All wikilinks resolve to existing files
- Frontmatter parses correctly
- Tags are properly indexed
- Block references are unique

## Maps of Content

The kiln uses MoC (Map of Content) pattern:

- Top-level `Index.md` links to major sections
- Each section has its own `Index.md`
- Cross-references connect related topics

## See Also

- [[Help/Concepts/Kilns]] - What makes a kiln
- [[Meta/Docs Architecture]] - Documentation guidelines
