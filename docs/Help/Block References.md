---
title: Block References
description: Reference for block ID syntax and embedding
tags:
  - reference
  - syntax
---

# Block References

Block references allow you to link to and embed specific paragraphs, lists, or other content blocks within notes.

## Creating Block IDs

Add a block ID to any paragraph or list item:

```markdown
This is an important paragraph. ^important-point

- List item one
- List item two ^key-item
- List item three
```

Block IDs:
- Start with `^`
- Appear at the end of the line
- Use lowercase letters, numbers, and hyphens
- Must be unique within the note

## Linking to Blocks

### Within the same note

```markdown
As mentioned in [[#^important-point]], we need to focus.
```

### In another note

```markdown
See [[Other Note#^important-point]] for details.
```

### With alias

```markdown
See [[Other Note#^important-point|the key insight]] for details.
```

## Embedding Blocks

Embed a specific block using the `!` prefix:

```markdown
![[Other Note#^important-point]]
```

This transcludes just that block, not the entire note.

## Block ID Best Practices

### Naming

Choose descriptive, stable IDs:

```markdown
The main thesis of this paper is... ^thesis

Key finding: productivity increased 40% ^finding-productivity

Decision: We will use React for the frontend ^decision-frontend
```

### Avoid

- Auto-generated IDs that might conflict
- IDs that describe position (`^paragraph-3`)
- Overly long IDs

## Heading References vs Block References

| Heading Reference | Block Reference |
|-------------------|-----------------|
| `[[Note#Heading]]` | `[[Note#^block-id]]` |
| Links to section | Links to specific block |
| Uses heading text | Uses explicit ID |
| May break if heading changes | Stable if ID preserved |

## Finding Block References

Search for notes with block IDs:

```json
{
  "query": "\\^[a-z][a-z0-9-]*$"
}
```

## Parser Implementation

Block IDs are parsed during markdown processing:

**Location:** `crates/crucible-core/src/parser/types/blocks.rs`

The parser:
1. Identifies blocks (paragraphs, lists, etc.)
2. Extracts trailing `^id` patterns
3. Stores block ID → content mapping
4. Enables block-level linking and embedding

## Automatic Block IDs

Crucible can generate block IDs automatically for content hashing:

```yaml
---
auto_block_ids: true
---
```

Generated IDs use content hashes and are stable as long as content doesn't change.

## Use Cases

### Citing specific points

```markdown
In [[Research Paper#^methodology]], the authors describe...
```

### Building arguments

```markdown
Given that:
1. [[Premise One#^main-point]]
2. [[Premise Two#^conclusion]]

Therefore...
```

### Creating excerpts

```markdown
# Key Quotes

![[Book Notes#^quote-1]]

![[Book Notes#^quote-2]]
```

### Stable references

When heading text might change, use block IDs for stability:

```markdown
This relates to [[API Design#^rate-limiting-decision]]
```

## See Also

- `:h wikilinks` - Full wikilink syntax
- `:h frontmatter` - Note metadata
- [[Help/Wikilinks]] - Linking syntax reference
