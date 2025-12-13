---
title: Johnny Decimal
description: Numbered category system for predictable organization
tags:
  - organization
  - johnny-decimal
  - hierarchy
---

# Johnny Decimal

Johnny Decimal is a hierarchical organization system that uses decimal numbering to create a fixed, predictable structure. Every item has a unique identifier, making it easy to reference, find, and discuss.

## Core Concepts

### Areas (10-99)

The system divides your kiln into 10 broad areas, numbered 10-19, 20-29, etc.

```
10-19 Administration
20-29 Finance
30-39 Projects
40-49 Clients
50-59 Marketing
```

You can only have **10 areas** (10-99). This constraint forces you to think carefully about high-level categories.

### Categories (X1-X9)

Within each area, you have up to 10 categories, numbered X1-X9.

```
20-29 Finance
  21 Invoices
  22 Expenses
  23 Tax Documents
  24 Bank Statements
```

### Items (XX.XX)

Individual items get unique IDs within their category:

```
21 Invoices
  21.01 Invoice Template
  21.02 2024 Q1 Invoices
  21.03 2024 Q2 Invoices
```

## Example Structure

A complete Johnny Decimal system might look like:

```
10-19 Administration
  11 Company Info
    11.01 Contact Directory
    11.02 Office Locations
  12 Policies
    12.01 HR Handbook
    12.02 Security Policy

20-29 Finance
  21 Invoices
    21.01 Invoice Template
    21.02 Outstanding Invoices
  22 Expenses
    22.01 Expense Policy
    22.02 2024 Expenses

30-39 Projects
  31 Active Projects
    31.01 Website Redesign
    31.02 Mobile App
  32 Archived Projects
    32.01 2023 Projects
```

## Key Principles

### 1. Fixed Structure

Once you define your areas and categories, they rarely change. This stability means everyone knows where to find things.

### 2. Unique Identifiers

Every item has a unique ID (e.g., `21.03`). You can reference items by their ID in conversations, emails, and notes.

### 3. Maximum 10 Categories per Area

This constraint prevents category sprawl and forces thoughtful organization.

### 4. No Nesting Beyond Three Levels

You have Area → Category → Item. No deeper nesting. If you need more structure within an item, use wikilinks or tags.

## Johnny Decimal in Crucible

### Folder Structure

```
my-kiln/
  10-19 Administration/
    11 Company Info/
      11.01 Contact Directory.md
      11.02 Office Locations.md
  20-29 Finance/
    21 Invoices/
      21.01 Invoice Template.md
```

### Frontmatter

Include the ID in frontmatter for searchability:

```yaml
---
title: Invoice Template
jd-id: "21.01"
jd-area: Finance
jd-category: Invoices
tags:
  - jd/21
  - template
---
```

### Using Wikilinks with IDs

Reference items by their ID:

```markdown
See the [[21.01 Invoice Template]] for the current format.
```

### Creating an Index

Maintain a master index note:

```markdown
# Johnny Decimal Index

## 10-19 Administration
- [[11 Company Info]]
  - [[11.01 Contact Directory]]
  - [[11.02 Office Locations]]

## 20-29 Finance
- [[21 Invoices]]
  - [[21.01 Invoice Template]]
```

## Planning Your System

Before implementing, plan your areas carefully. Ask:

1. What are the 5-10 major domains in my life/work?
2. What categories exist within each domain?
3. How stable are these categories?

Example planning worksheet:

```
Area 10-19: _______________
  11: _______________
  12: _______________

Area 20-29: _______________
  21: _______________
  22: _______________
```

## Tradeoffs

**Strengths:**
- Predictable locations for everything
- Easy to cite and reference (just use the ID)
- Works well for teams with shared systems
- Forces disciplined categorization

**Challenges:**
- Rigid structure is harder to refactor
- Initial setup requires careful planning
- Can feel constraining for creative work
- Cross-cutting topics don't fit neatly

## When to Use Johnny Decimal

Johnny Decimal works best when you:
- Need stable, predictable organization
- Work in teams that need shared reference systems
- Manage large archives or documentation
- Value being able to cite items by ID
- Have clearly defined domains that rarely change

## Combining with Other Methods

Johnny Decimal combines well with:
- **MOCs** for navigation across areas
- **Zettelkasten** for atomic notes within items
- **Tags** for cross-cutting organization

## See Also

- [[Index]] - Overview of all organization styles
- [[PARA]] - Organizing by actionability
- [[Choosing Your Structure]] - Help deciding which method to use
