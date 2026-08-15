---
title: Frontmatter
description: YAML metadata format for Crucible notes
tags:
  - help
  - syntax
  - metadata
---

# Frontmatter

Frontmatter is metadata at the start of a note — YAML enclosed by `---` delimiters, or TOML enclosed by `+++` delimiters.

## Basic Format

```yaml
---
description: A brief description of this note
tags:
  - tag1
  - tag2
---
```

### TOML Frontmatter

TOML frontmatter is fully supported as an alternative, using `+++` delimiters:

<!-- crucible:not-config -->
```toml
+++
description = "A brief description of this note"
tags = ["tag1", "tag2"]
+++
```

## Required Fields

For dev-kiln documentation, these fields are required:

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Display title for the note |
| `description` | string | Brief summary (1-2 sentences) |
| `tags` | list | Categorization tags |

`title` and `tags` are extracted and indexed (titles participate in wikilink
resolution; tags are searchable via `property_search`). `description` is only
read as a typed field for task files and workflow notes; on ordinary notes it
is stored as a generic property like any other.

## Conventional Fields

These fields are conventions carried over from Obsidian-style vaults. Crucible
stores them as **generic queryable properties** (available via
`property_search`), but their special semantics are **not implemented**:

| Field | Type | Convention | Status |
|-------|------|------------|--------|
| `order` | number | Sort order within a folder | Stored only — nothing sorts by it |
| `aliases` | list | Alternative names for wikilink resolution | Stored only — wikilink resolution does not consult aliases |
| `created` | date | Creation timestamp | Stored only — no date handling |
| `modified` | date | Last modification timestamp | Stored only — no date handling |

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

## See Also

- [[Tags]] - Tag syntax and conventions
- [[Wikilinks]] - Internal linking
