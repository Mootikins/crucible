---
title: Your First Kiln
description: Create and configure a new knowledge base from scratch
tags:
  - guide
  - beginner
order: 2
---

# Your First Kiln

This guide walks you through creating a new kiln from scratch. By the end, you'll have a working knowledge base ready for notes.

## What is a Kiln?

A kiln is simply a directory containing markdown files. Crucible processes these files into a searchable knowledge graph. Think of it like an Obsidian vault or a folder of notes.

## Creating Your Kiln

### 1. Choose a Location

Pick where your notes will live:

```bash
# Personal notes
mkdir -p ~/Documents/my-kiln

# Project documentation
mkdir -p ~/projects/my-project/docs

# Shared team knowledge
mkdir -p /shared/team-knowledge
```

### 2. Initialize the Directory

Create a basic structure:

```bash
cd ~/Documents/my-kiln

# Create some starter folders
mkdir -p Notes Projects Reference
```

### 3. Create Your First Note

Create `Notes/Welcome.md`:

```markdown
---
title: Welcome
description: My first note
tags:
  - meta
---

# Welcome to My Kiln

This is my knowledge base. Here I'll store notes, ideas, and references.

## Getting Started

- [[Notes/Ideas]] - Capture thoughts
- [[Projects/Current]] - Active work
- [[Reference/Index]] - Reference materials
```

### 4. Configure Crucible

Set your kiln path:

```bash
# Option 1: Environment variable
export CRUCIBLE_KILN_PATH=~/Documents/my-kiln

# Option 2: Config file
mkdir -p ~/.config/crucible
cat > ~/.config/crucible/config.toml << 'EOF'
kiln_path = "/home/user/Documents/my-kiln"

[embedding]
provider = "fastembed"

[cli]
show_progress = true
EOF
```

### 5. Process Your Notes

```bash
cru process
```

You should see:
```
Processing 1 files through pipeline...
Pipeline processing complete!
   Processed: 1 files
```

### 6. Verify Setup

```bash
cru stats
```

You should see your kiln statistics.

## Building Your Structure

### Start Simple

Don't over-organize from the start:

```
my-kiln/
  Notes/          # Day-to-day notes
  Projects/       # Active work
  Reference/      # Stable reference material
```

### Let Structure Emerge

As you add notes, patterns will emerge. Then you can:
- Add more folders as needed
- Create index notes (MOCs)
- Adopt an organizational system like [[Organization Styles/PARA|PARA]]

### Link Liberally

The power of a kiln is in connections:

```markdown
This idea relates to [[Other Note]].

See also:
- [[Related Concept]]
- [[Another Idea#specific-section]]
```

## Adding Metadata

### Frontmatter

Every note benefits from frontmatter:

```yaml
---
title: Project Alpha
description: Main project documentation
tags:
  - project
  - active
created: 2024-01-15
---
```

### Tags

Use tags for cross-cutting organization:

```markdown
#status/active #priority/high #project/alpha
```

## Processing Workflow

### Initial Processing

First run processes everything:
```bash
cru process
```

### Incremental Updates

Subsequent runs only process changes:
```bash
cru process
```

### Watch Mode

Auto-process on file changes:
```bash
cru process --watch
```

## Using Your Kiln

### Search

Find content semantically:
```bash
cru chat "Find notes about project planning"
```

### Chat

Explore with AI:
```bash
cru chat
```

### Stats

Check kiln health:
```bash
cru stats
```

## Next Steps

Now that you have a working kiln:

1. **Add more notes** - Start capturing ideas
2. **Create connections** - Link related notes with wikilinks
3. **Choose a structure** - See [[Organization Styles/Index]]
4. **Learn commands** - Read [[Basic Commands]]
5. **Configure agents** - Explore [[Help/Config/agents]]

## Common Questions

### Can I use existing notes?

Yes! Point Crucible at any folder with markdown files.

### Do I need frontmatter?

It's recommended but not required. Frontmatter enables property search and better metadata.

### How do I backup?

Your kiln is just files. Use any backup method:
- Git for version control
- Cloud sync (Dropbox, iCloud)
- Regular file backups

### Can I use Obsidian too?

Yes! Crucible is compatible with Obsidian vaults. Just point to the same directory.

## See Also

- [[Getting Started]] - Installation and setup
- [[Basic Commands]] - Essential CLI commands
- [[Organization Styles/Index]] - Structuring your kiln
- `:h frontmatter` - Metadata format
