---
description: Define workflows using natural prose syntax
status: planned
tags:
  - workflows
  - markup
  - syntax
---

# Workflow Markup

> [!warning] Workflow markup syntax is not yet implemented.

Workflow markup lets you define automated processes using natural language with embedded structure.

## Basic Syntax

Workflows are written in prose with embedded commands:

```markdown
# My Workflow

First, search for notes tagged with #inbox.
Then, for each note, analyze the content and suggest categories.
Finally, move processed notes to the appropriate folder.
```

The parser understands:
- Action verbs (search, create, move, update)
- References (notes, folders, tags)
- Conditions (if, when, for each)
- Sequences (first, then, finally)

## Structure Elements

### Steps

Each paragraph is a step:

```markdown
Search for all notes in the Inbox folder.

For each note found, extract the main topic.

Create a summary note with links to all processed notes.
```

### Conditions

Use natural conditionals:

```markdown
If the note has no tags, suggest tags based on content.

When a note is older than 30 days, mark it for review.
```

### Loops

Process multiple items:

```markdown
For each project note, check for incomplete tasks.

Process all notes modified today.
```

## Variables and Context

Workflows maintain context between steps:

```markdown
Find all meeting notes from this week.
(This creates a list of notes)

For each meeting note, extract action items.
(Operates on the list from previous step)

Create a task list combining all action items.
(Uses accumulated results)
```

## Integration with Kiln

### Searching

```markdown
Search semantically for "project updates".
Find notes tagged with #important and #urgent.
Get all notes in the Projects/ folder.
```

### Creating

```markdown
Create a new note called "Weekly Summary" in Summaries/.
Add today's date to the frontmatter.
Include links to all notes found.
```

### Updating

```markdown
Update the status to "reviewed".
Add the tag #processed.
Set the reviewed_at property to now.
```

## Example Workflows

### Inbox Processing

```markdown
# Inbox Processing

Find all notes in the Inbox folder.

For each note:
  - Analyze the content to determine the topic
  - Suggest appropriate tags
  - Recommend a destination folder

After processing, create a summary of changes.
```

### Weekly Review

```markdown
# Weekly Review

Gather all notes modified in the last 7 days.
Group them by their folder.

For each group, create a section in the review:
  - List the notes with brief descriptions
  - Highlight any notes marked important
  - Note any incomplete tasks

Save the review to Reviews/[today's date].md.
```

## Relation to YAML Workflows

> [!warning] The YAML workflow format is being replaced by prose markup.

The current `Help/Extending/Workflow Authoring` describes YAML-based workflows. The prose markup format is intended to be more natural and readable while providing the same capabilities.

## See Also

- [[Help/Workflows/Index]] - Workflow overview
- [[Help/Core/Sessions]] - Session tracking (orthogonal to workflows)
- [[Help/Extending/Workflow Authoring]] - Current YAML format
