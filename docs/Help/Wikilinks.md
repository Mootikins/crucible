---
title: Wikilinks
description: Comprehensive reference for wikilink syntax and resolution
tags:
  - reference
  - syntax
---

# Wikilinks

Wikilinks are Obsidian-style links that connect notes within your kiln. They use double bracket syntax `[[...]]` and support various forms of references including aliases, headings, and block references.

## Basic Syntax

### Simple Link

```markdown
[[Note Name]]
```

Links to a note named "Note Name.md". The display text matches the target note name.

### Link with Alias

```markdown
[[Note Name|display text]]
```

Links to "Note Name.md" but displays "display text" to the user.

### Link with Heading Reference

```markdown
[[Note Name#Heading]]
```

Links to a specific heading within "Note Name.md".

### Link with Block Reference

```markdown
[[Note Name#^block-id]]
```

Block references start with `^` after the `#` symbol. The reference is parsed and stored on the link, but block IDs are not extracted or resolved — the link resolves to the note itself (see [[Help/Block References]]).

### Combined: Heading with Alias

```markdown
[[Note Name#Section|Display Text]]
```

Links to a heading within a note, but displays custom text.

## Embed Syntax

> [!warning] Not transcluded
> The `!` prefix parses (the link is flagged as an embed), but **transclusion is not implemented**. An embed renders as an ordinary link to the target note — no content is inlined, for whole notes, headings, or blocks.

### Basic Embed

```markdown
![[Note Name]]
```

Parsed as an embed of "Note Name"; currently rendered as a link.

### Embed with Heading

```markdown
![[Note Name#Heading]]
```

The heading reference is captured on the parsed link; no heading-scoped content is embedded.

### Embed with Block Reference

```markdown
![[Note Name#^block-id]]
```

The block reference is captured on the parsed link; block IDs are not extracted or resolved (see [[Help/Block References]]).

## Path Syntax

Wikilinks support hierarchical paths for notes organized in folders:

```markdown
[[Folder/Subfolder/Note]]
```

**Examples:**

```markdown
[[Help/Wikilinks]]              → docs/Help/Wikilinks.md
[[Organization Styles/PARA]]    → docs/Organization Styles/PARA.md
```

## Resolution Algorithm

### 1. Parsing Phase

The parser uses a regex to extract wikilinks:

```rust
Regex::new(r"(!?)\[\[([^\]]+)\]\]")
```

### 2. Component Extraction

The content inside brackets is parsed in the following order:

1. **Alias separation**: Split on `|` → `(target_part, alias)`
2. **Reference extraction**: Split `target_part` on `#` → `(target, ref_part)`
3. **Reference type detection**: If `ref_part` starts with `^`: Block reference, otherwise: Heading reference

### 3. Code Block Exclusion

Wikilinks inside code blocks are **not parsed**.

### 4. Link Resolution

The daemon's link index resolves each link target deterministically, in this order:

1. **Exact extension-less path**: `[[notes/async]]` → `notes/async.md` (a full path with extension also matches)
2. **Unique title match**: exactly one note whose title matches
3. **Unique file-stem match**: exactly one note whose filename stem matches
4. **Ambiguous stem** (2+ notes share the stem): a deterministic winner is chosen — shortest path, then lexicographic — and the link is flagged ambiguous

No match leaves the link dangling (unresolved). There is **no fuzzy-matching stage**; resolution is exact and deterministic so that backlinks and rename-rewrites are safe.

## Edge Cases

### Special Characters

Wikilinks support various special characters in note names:

```markdown
[[note-with-dashes]]
[[note_with_underscores]]
[[note with spaces]]
[[note.with.dots]]
```

### Empty Wikilinks

```markdown
[[]]
```

Empty wikilinks are parsed but may be ignored.

### Unclosed Wikilinks

```markdown
[[broken
```

Unclosed wikilinks are **not parsed**.

### Multiple Wikilinks on Same Line

```markdown
Multiple links: [[first]] and [[second]] and [[third]]
```

All wikilinks on the same line are parsed independently.

## Escaping

There is **no escape mechanism** for wikilink syntax. If you need to display literal `[[` and `]]`:

1. Use inline code: `` `[[not a link]]` ``
2. Use HTML entities: `&#91;&#91;not a link&#93;&#93;`
3. Place in a code block

## See Also

- `:h frontmatter` - YAML metadata format
- `:h tags` - Tag system and nested tags
- `:h block-references` - Block ID syntax and usage
