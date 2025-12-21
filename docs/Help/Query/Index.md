---
description: Advanced queries for finding and filtering notes
status: planned
tags:
  - query
  - search
  - tq
---

# Query System

> [!warning] The tq query language is not yet implemented.

The query system provides a powerful language for finding notes based on complex criteria.

## Overview

While `cru search` handles simple lookups, the query system lets you:

- Combine multiple conditions
- Filter by relationships
- Aggregate results
- Transform output

## Basic Queries

```
# Find notes with tag
notes where tags contains "project"

# Find notes in folder
notes where path starts_with "Projects/"

# Find notes modified recently
notes where modified > "2024-01-01"
```

## Combining Conditions

```
# AND conditions
notes where tags contains "project" and status = "active"

# OR conditions
notes where tags contains "urgent" or priority = "high"

# Complex expressions
notes where (tags contains "project" and status = "active")
         or priority = "high"
```

## Relationship Queries

Query based on how notes connect:

```
# Notes that link to a specific note
notes where links_to "Projects/Alpha"

# Notes linked from a specific note
notes where linked_from "Index"

# Notes with many connections
notes where link_count > 10
```

## Sorting and Limiting

```
# Sort by date
notes where tags contains "meeting"
      order by modified desc

# Limit results
notes where folder = "Inbox"
      limit 10

# Offset for pagination
notes where folder = "Archive"
      offset 20 limit 10
```

## Aggregations

```
# Count notes by tag
count notes group by tags

# Most linked notes
notes order by backlink_count desc limit 10
```

## Output Formatting

```
# Select specific fields
notes where status = "active"
      select path, title, modified

# As list
notes where tags contains "todo" as list

# As table
notes where folder = "Projects" as table
```

## Examples

### Find Orphan Notes

Notes with no incoming or outgoing links:

```
notes where link_count = 0 and backlink_count = 0
```

### Recent Meeting Notes

```
notes where tags contains "meeting"
      and modified > "7 days ago"
      order by modified desc
```

### Project Overview

```
notes where path starts_with "Projects/"
      group by folder
      select folder, count(*) as note_count
```

### Stale Tasks

```
notes where tags contains "task"
      and status != "completed"
      and modified < "30 days ago"
```

## Integration

### In Chat

```
/query notes where tags contains "research"
```

### In Rune Plugins

```rune
let results = crucible::query("notes where status = 'active'")?;
for note in results {
    println("{}", note.path);
}
```

### From CLI

```bash
cru query "notes where tags contains 'important'"
```

## See Also

- [[Help/CLI/search]] - Simple search commands
- [[Search & Discovery]] - All search methods
- [[Help/Tags]] - Tag syntax for filtering
