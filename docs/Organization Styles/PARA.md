---
title: PARA Method
description: Organizing by actionability - Projects, Areas, Resources, Archive
tags:
  - organization
  - para
  - productivity
---

# PARA Method

PARA is a productivity-focused organizational system created by Tiago Forte. It organizes information by **actionability** rather than by topic.

## The Four Categories

### Projects
**Active efforts with defined outcomes and deadlines.**

Projects have:
- A specific goal or outcome
- A deadline (even if flexible)
- Multiple tasks to complete
- A clear definition of "done"

**Examples:**
```
Projects/
  Launch Website Redesign/
  Q4 Sales Report/
  Move to New Apartment/
  Learn Spanish Basics/
```

### Areas
**Ongoing responsibilities without end dates.**

Areas represent:
- Roles you maintain (parent, manager, homeowner)
- Standards you uphold (health, finances, career)
- Recurring concerns that need attention

**Examples:**
```
Areas/
  Health/
  Finances/
  Home/
  Career Development/
  Team Management/
```

### Resources
**Reference material for current or future use.**

Resources include:
- Topics of interest
- Reference materials
- Collected knowledge
- Research for potential future projects

**Examples:**
```
Resources/
  Cooking Recipes/
  Programming Languages/
  Travel Destinations/
  Book Notes/
```

### Archive
**Inactive items from the other three categories.**

The archive stores:
- Completed projects
- Responsibilities you no longer hold
- Resources no longer relevant

**Examples:**
```
Archive/
  2023 Projects/
  Old Job Materials/
  Deprecated Research/
```

## PARA in Practice

### Folder Structure

A typical PARA kiln might look like:

```
my-kiln/
  1 Projects/
    Launch Product/
    Write Book/
  2 Areas/
    Health/
    Finances/
  3 Resources/
    Programming/
    Design/
  4 Archive/
    2023/
    Old Projects/
```

The numbers (1-4) are optional but help maintain sort order.

### Key Principles

**1. Organize by actionability, not topic**

A note about marketing goes in `Projects/Product Launch/` if it's for an active project, not in `Resources/Marketing/`.

**2. Projects are temporary, Areas are permanent**

If something has a deadline, it's a Project. If it's ongoing, it's an Area.

**3. Archive aggressively**

When a project ends or an area is no longer your responsibility, move it to Archive. This keeps active folders focused.

**4. Resources support action**

Resources exist to support Projects and Areas. If something isn't useful for current work, it might belong in Archive.

## PARA with Crucible

### Using Tags

Complement folder structure with tags:

```yaml
---
tags:
  - project/product-launch
  - area/marketing
  - status/active
---
```

### Using Wikilinks

Create hub notes for each category:

```markdown
# Active Projects

- [[Projects/Product Launch/Index]]
- [[Projects/Q4 Report/Index]]

# Current Areas

- [[Areas/Team Management/Index]]
- [[Areas/Health/Index]]
```

### Semantic Search

PARA works well with semantic search because notes are grouped by context (what you're working on) rather than abstract topics.

## Tradeoffs

**Strengths:**
- Clear distinction between active and inactive
- Easy to find what needs attention
- Natural archiving workflow
- Reduces decision fatigue

**Challenges:**
- Notes about the same topic may be scattered
- Requires maintenance as projects complete
- Area boundaries can be fuzzy
- May create many small folders

## When to Use PARA

PARA works best when you:
- Juggle multiple active projects
- Need clear boundaries between work-in-progress and reference
- Want to focus on actionable items
- Have regular reviews to maintain organization

## Combining with Other Methods

PARA combines well with:
- **Zettelkasten** within Resources for interconnected notes
- **Johnny Decimal** for numbering within categories
- **MOCs** for navigation across the hierarchy

## See Also

- [[Index]] - Overview of all organization styles
- [[Zettelkasten]] - Atomic linked notes approach
- [[Choosing Your Structure]] - Help deciding which method to use
- `:h tags` - Using tags for cross-cutting organization
