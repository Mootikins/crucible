---
description: YAML metadata format for Crucible notes
tags:
  - help
  - syntax
  - metadata
---

# Frontmatter

Frontmatter is YAML metadata at the start of a note, enclosed by `---` delimiters.

## Basic Format

```yaml
---
description: A brief description of this note
tags:
  - tag1
  - tag2
---
```

## Required Fields

For dev-kiln documentation, these fields are required:

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Display title for the note |
| `description` | string | Brief summary (1-2 sentences) |
| `tags` | list | Categorization tags |

## Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `order` | number | Sort order within a folder |
| `aliases` | list | Alternative names for wikilink resolution |
| `created` | date | Creation timestamp |
| `modified` | date | Last modification timestamp |

## Example

```yaml
---
description: Your first steps with Crucible
tags:
  - guide
  - beginner
order: 1
aliases:
  - quickstart
  - intro
---

# Getting Started

Your content here...
```

## Parsing

Crucible parses frontmatter using the `crucible-parser` crate:

- Implementation: `crates/crucible-parser/src/` (frontmatter parsing)
- Types: `crates/crucible-core/src/parser/types/frontmatter.rs`

## See Also

- [[Tags]] - Tag syntax and conventions
- [[Wikilinks]] - Internal linking
