---
description: What a kiln is and how it differs from a folder or vault
status: implemented
tags:
  - concept
  - fundamentals
---

# Kilns

A **kiln** is a directory of markdown files that Crucible treats as a connected knowledge base. The name comes from the idea of a place where raw materials are transformed into something more useful - your notes become a searchable, interconnected knowledge graph.

## What Makes a Kiln

A kiln is simply a folder containing:

- **Markdown files** (.md) - Your notes, ideas, documents
- **Config.toml** (optional) - Configuration for this kiln
- **Any folder structure** - Organize however you like

That's it. No special database, no proprietary format, no lock-in.

## Kiln vs Folder

Any folder with markdown files can be a kiln. The difference is what you do with it:

| Folder | Kiln |
|--------|------|
| Just files | Connected knowledge |
| Text search only | Semantic search |
| Manual organization | Wikilink-based graph |
| Static content | AI-assisted discovery |

When you run Crucible commands in a folder, it becomes a kiln.

## Kiln vs Vault

If you're coming from Obsidian, a kiln is similar to a vault:

- Both are folders of markdown
- Both use `[[wikilinks]]`
- Both have configuration files

The difference is philosophy:

- **Obsidian vaults** require Obsidian to get full value
- **Kilns** work with any text editor - Crucible just adds capabilities

Your files are always portable. You can open a kiln in Obsidian, VS Code, or plain Vim.

## Creating a Kiln

Any folder becomes a kiln when you use Crucible:

```bash
# Process files and start exploring
cru

# Or explicitly initialize
cru init
```

See [[Guides/Your First Kiln]] for detailed setup.

## See Also

- [[Help/Concepts/Plaintext First]] - Why markdown matters
- [[Help/Concepts/The Knowledge Graph]] - How kilns become connected
- [[Guides/Getting Started]] - Your first steps with Crucible
