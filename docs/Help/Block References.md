---
title: Block References
description: Reference for block reference link syntax — parsing status and planned behavior
status: partial
tags:
  - reference
  - syntax
---

# Block References

Block references are Obsidian's mechanism for linking to a specific paragraph or list item inside a note, via `^id` markers and `[[Note#^id]]` links.

> [!warning] Implementation status
> Only the **link syntax** is implemented. The parser recognizes `[[Note#^id]]` and captures the block reference on the parsed link (`WikiLink.block_ref`). Nothing else on this page exists yet: `^id` markers are **not extracted** from note content, block IDs are **not resolved** to blocks, `![[Note#^id]]` does **not** transclude anything (embeds render as ordinary links), and there is no `auto_block_ids` option. Sections below are marked accordingly.

## Link Syntax (implemented: parsing only)

A wikilink whose fragment starts with `^` is parsed as a block reference rather than a heading reference:

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

The parser stores the target and the `important-point` block ref on the link. Resolution stops at the note: the link resolves to `Other Note`, and the block ref is carried as metadata that nothing currently consumes.

## Heading References vs Block References

| Heading Reference | Block Reference |
|-------------------|-----------------|
| `[[Note#Heading]]` | `[[Note#^block-id]]` |
| Links to section | Links to specific block |
| Uses heading text | Uses explicit ID |
| May break if heading changes | Stable if ID preserved |

Both parse; neither fragment is resolved to a location inside the target note today.

## Not Implemented

Everything below describes Obsidian's behavior, kept here as the design target. **None of it exists in Crucible yet.**

### Authoring block IDs

```markdown
This is an important paragraph. ^important-point

- List item two ^key-item
```

The parser does not extract trailing `^id` markers from paragraphs or list items; they remain plain text in the note.

### Embedding blocks

```markdown
![[Other Note#^important-point]]
```

The `!` embed prefix parses (see [[Help/Wikilinks]]), but there is no transclusion — the embed renders as a regular link to the note, not the block's content.

### Automatic block IDs

There is no `auto_block_ids` frontmatter option and no content-hash ID generation.

### Block ID → content mapping

No block-level index exists; searching for `^id` patterns is just a text search over note content.

## See Also

- `:h wikilinks` - Full wikilink syntax
- `:h frontmatter` - Note metadata
- [[Help/Wikilinks]] - Linking syntax reference
