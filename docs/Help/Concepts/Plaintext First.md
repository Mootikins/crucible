---
description: Why Crucible uses markdown and keeps your files local
status: implemented
tags:
  - concept
  - philosophy
  - markdown
---

# Plaintext First

Crucible is built on a simple principle: **your notes are just files**. Markdown files in folders on your computer. No database required, no cloud dependency, no proprietary format.

## Why Plaintext

### Portability

Your notes work everywhere:
- Any text editor (VS Code, Vim, Obsidian, Notepad)
- Any operating system
- Any device
- Any decade - markdown will outlive any app

### Ownership

Your files live on your machine:
- No company holds your data hostage
- No subscription to access your notes
- No internet required to write and read
- You control backups and sync

### Simplicity

Files are simple:
- No migration when tools change
- No export/import workflows
- Version control with Git
- Scriptable with standard tools

## What Crucible Adds

Crucible enhances your files without changing them:

| Your Files | Crucible Cache |
|------------|----------------|
| Source of truth | Derived data |
| Portable markdown | Local database |
| Always accessible | Rebuilt anytime |

The database (SQLite by default) is a cache. Delete it and Crucible rebuilds it from your files. Your notes are never modified unless you ask.

## The Markdown Contract

Crucible commits to:

1. **Never require proprietary format** - Standard markdown works
2. **Never lock you in** - All features work with plain files
3. **Never modify without consent** - Your files, your control
4. **Always be rebuildable** - Cache from source, not source from cache

## Wikilinks and Extensions

Crucible uses `[[wikilinks]]` - a widely-supported markdown extension. These work in Obsidian, Logseq, Foam, and many other tools.

Other extensions (frontmatter, block references) are also standard across the PKM ecosystem.

## See Also

- [[Help/Concepts/Kilns]] - What makes a kiln
- [[Help/Frontmatter]] - YAML metadata (standard format)
- [[Help/Wikilinks]] - Link syntax (widely supported)
