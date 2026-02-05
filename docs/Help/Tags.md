---
description: Reference for tag syntax and usage
tags:
  - reference
  - syntax
---

# Tags

Tags provide flexible, cross-cutting organization for your notes. Unlike folder hierarchies, tags can apply multiple categories to a single note.

## Basic Syntax

### Inline Tags

```markdown
This note is about #productivity and #creativity.
```

Tags start with `#` and continue until whitespace or punctuation.

### Frontmatter Tags

```yaml
---
tags:
  - productivity
  - creativity
  - review/needed
---
```

Tags in frontmatter don't need the `#` prefix.

## Nested Tags

Create hierarchies with forward slashes:

```markdown
#project/alpha
#project/beta
#status/active
#status/complete
```

Nested tags allow:
- Searching for all `#project/*` notes
- Filtering by specific `#project/alpha`
- Building tag taxonomies

## Tag Conventions

### Naming

- **Lowercase**: `#productivity` not `#Productivity`
- **Hyphens for spaces**: `#meeting-notes` not `#meeting_notes`
- **No special characters**: Avoid `#tag@special`

### Common Patterns

**Status tags:**
```markdown
#status/draft
#status/review
#status/published
#status/archived
```

**Type tags:**
```markdown
#type/note
#type/article
#type/reference
#type/meeting
```

**Priority tags:**
```markdown
#priority/high
#priority/medium
#priority/low
```

**Project tags:**
```markdown
#project/website-redesign
#project/q4-planning
```

## Searching by Tag

### Using property_search

```json
{
  "properties": {
    "tags": ["productivity"]
  }
}
```

Search for any of multiple tags (OR logic):
```json
{
  "properties": {
    "tags": ["urgent", "important"]
  }
}
```

### Using text_search

Find inline tags:
```json
{
  "query": "#productivity"
}
```

## Tags vs Folders

| Tags | Folders |
|------|---------|
| Multiple per note | One location per note |
| Cross-cutting | Hierarchical |
| Easy to add/remove | Moving requires links |
| Flexible | Stable structure |

**Use tags for:**
- Status (draft, published)
- Priority (high, medium, low)
- Topics that cross categories
- Temporary groupings

**Use folders for:**
- Primary organization
- Stable categories
- Physical structure

## Tags with Other Systems

### With PARA

```yaml
tags:
  - para/project
  - status/active
  - priority/high
```

### With Johnny Decimal

```yaml
tags:
  - jd/21.03
  - type/invoice
```

### With Zettelkasten

```yaml
tags:
  - zettel/permanent
  - topic/productivity
```

## Implementation

**Parser location:** `crates/crucible-core/src/parser/types/links.rs`

**Storage:** Tags are extracted to the database during processing and available via `property_search`.

## See Also

- `:h frontmatter` - YAML metadata including tags
- `:h search` - Searching by tags
- [[Organization Styles/Index]] - Using tags with organizational systems
