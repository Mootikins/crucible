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

Links to a specific block within "Note Name.md". Block references start with `^` after the `#` symbol.

### Combined: Heading with Alias

```markdown
[[Note Name#Section|Display Text]]
```

Links to a heading within a note, but displays custom text.

## Embed Syntax

### Basic Embed

```markdown
![[Note Name]]
```

Embeds (transcludes) the content of another note at the current location.

### Embed with Heading

```markdown
![[Note Name#Heading]]
```

Embeds only the content under a specific heading.

### Embed with Block Reference

```markdown
![[Note Name#^block-id]]
```

Embeds a specific block identified by its block ID.

## Path Syntax

Wikilinks support hierarchical paths for notes organized in folders:

```markdown
[[Folder/Subfolder/Note]]
```

**Examples:**

```markdown
[[Help/Wikilinks]]              → examples/dev-kiln/Help/Wikilinks.md
[[Organization Styles/PARA]]    → examples/dev-kiln/Organization Styles/PARA.md
```

## Resolution Algorithm

### 1. Parsing Phase

The parser (`crates/crucible-parser/src/wikilinks.rs`) uses a regex to extract wikilinks:

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

At query time, the storage layer resolves wikilinks:

1. **Exact match**: Search for note with exact title match
2. **Path match**: Search for note at exact path
3. **Fuzzy match**: Search for notes with similar names

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

## Parser Implementation

**Main extension:** `crates/crucible-parser/src/wikilinks.rs`

**markdown-it plugin:** `crates/crucible-parser/src/markdown_it/plugins/wikilink.rs`

**Type definition:** `crates/crucible-core/src/parser/types/links.rs`

**Edge case tests:** `crates/crucible-parser/tests/wikilink_edge_cases.rs`

## See Also

- `:h frontmatter` - YAML metadata format
- `:h tags` - Tag system and nested tags
- `:h block-references` - Block ID syntax and usage
