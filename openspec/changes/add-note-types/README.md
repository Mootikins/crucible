# Note Types: Structured Metadata and Templates

**Status**: 📝 Proposal
**Created**: 2025-11-22
**Author**: System Design
**Inspired by**: [AnyType](https://github.com/anyproto/anytype-ts), Obsidian Dataview, Notion Databases

## Quick Summary

Add a type system to Crucible where notes can have a `type` field that determines expected metadata schema, enables templates, and supports type-aware querying—all while maintaining the plaintext-first philosophy.

## The Problem

Not all notes are created equal. Users naturally organize knowledge into different categories with different metadata needs:

- **Books**: title, author, rating, genre, read_date
- **Movies**: title, director, rating, release_year, watched_date
- **Meetings**: date, attendees, agenda_items, action_items
- **Research Papers**: title, authors, journal, year, doi, citations

Currently, users can add arbitrary YAML frontmatter, but there's no system for:
- Defining expected schemas for different note types
- Creating notes from templates
- Querying notes by type and metadata
- Validating metadata consistency

## The Solution

**Note Types** bring structure to plaintext notes:

```yaml
---
type: book
title: "The Three-Body Problem"
author: [[Liu Cixin]]
rating: 5
genre: [sci-fi, hard-sci-fi]
read_date: 2025-11-15
status: finished
---

# The Three-Body Problem

Summary...
```

With type definitions:

```markdown
---
type_id: book
name: Book
icon: 📚
relations:
  title:
    type: text
    required: true
  rating:
    type: number
    min: 1
    max: 5
  genre:
    type: list
    options: [fiction, non-fiction, sci-fi, fantasy]
templates:
  - quick-review
  - detailed-analysis
---
```

## Key Features

### 1. Type Definitions

Define types in `.crucible/types/*.md` with:
- Metadata schemas (relations/properties)
- Data type validation (text, number, date, enum, list, link)
- Constraints (min/max, required, patterns)
- Multiple templates per type
- Visual metadata (icon, color)

### 2. Templates

Create notes from templates with placeholder expansion:

```bash
cru new book "Dune"
# Creates note from book template with type:book frontmatter

cru new meeting --template standup "Daily Standup"
# Uses standup template for meeting type
```

Templates support placeholders:
- `{{title}}` - User-provided title
- `{{date}}` - Current date
- `{{date:format:long}}` - Formatted date

### 3. Type-Aware Queries

Query notes by type and metadata:

```bash
# All 5-star sci-fi books
cru query "type:book AND genre:sci-fi AND rating:5"

# Movies in watchlist
cru query "type:movie AND status:watchlist"

# Meetings from last week
cru query "type:meeting AND meeting_date:>=2025-11-15"

# Average book rating
cru query "type:book" --aggregate avg(rating)
```

### 4. AI Agent Integration

Agents understand note structure semantically:

```
User: "Create a book review for Dune"
Agent: Recognizes "book review" → uses type:book
        Creates note with proper frontmatter
        Prompts for author, rating, etc.

User: "Show me all 5-star sci-fi books"
Agent: Constructs query: type:book AND genre:sci-fi AND rating:5
        Returns formatted results
```

### 5. Validation & Suggestions

- Validate frontmatter against type schema
- Warn about missing required fields
- Suggest corrections for invalid values
- Type detection for untyped notes

## Example Types

### Book Type

Perfect for:
- Reading lists and reviews
- Book clubs
- Research bibliographies
- Personal library tracking

**Relations**: title, author, rating, genre, read_date, isbn, pages, status

**Templates**: quick-review, detailed-analysis

### Movie Type

Perfect for:
- Movie watchlists
- Film criticism
- Director/actor tracking
- Personal ratings

**Relations**: title, director, rating, release_year, genre, runtime, imdb_id, cast, status

**Templates**: quick-rating, detailed-review

### Meeting Type

Perfect for:
- Team meetings
- 1:1s
- Sprint planning/retrospectives
- Action item tracking

**Relations**: meeting_date, attendees, facilitator, meeting_type, duration, agenda_items, action_items, decisions

**Templates**: standup, planning-meeting, retrospective, one-on-one

## Architecture

### Type System Components

```
crucible-core/src/types/
├── definition.rs      # Parse type definitions
├── registry.rs        # Type discovery and lookup
├── schema.rs          # Relation schemas and validation
├── template.rs        # Template loading and expansion
└── relations.rs       # Relation data types
```

### Data Flow

1. **Type Loading**: Scan `.crucible/types/*.md` → parse definitions → validate schemas → register
2. **Note Creation**: User runs `cru new {type}` → load template → expand placeholders → create note
3. **Note Parsing**: Parse note → extract frontmatter → validate against type schema → emit warnings
4. **Querying**: Parse query → filter by type → filter by relation values → return results

### Storage

- Type definitions: `.crucible/types/{type}.md`
- Templates: `.crucible/types/{type}/templates/{template}.md`
- Registry: `.crucible/types/registry.yaml`
- Notes: Normal markdown files with `type` in frontmatter

## Files in This Change

```
add-note-types/
├── README.md (this file)
├── proposal.md - Full rationale and implementation plan
├── specs/
│   └── note-types/
│       └── spec.md - Formal requirements (GIVEN-WHEN-THEN)
└── examples/
    └── types/
        ├── book.md - Book type definition
        │   └── templates/
        │       ├── quick-review.md
        │       └── detailed-analysis.md
        ├── movie.md - Movie type definition
        │   └── templates/
        │       ├── quick-rating.md
        │       └── detailed-review.md
        └── meeting.md - Meeting type definition
            └── templates/
                ├── standup.md
                ├── planning-meeting.md
                ├── retrospective.md
                └── one-on-one.md
```

## Comparison with Similar Systems

### vs. AnyType

**Similar**:
- Objects have types with relations (properties)
- Templates for creating typed objects
- Type-aware querying and views

**Different**:
- Crucible: Plaintext markdown files (not encrypted graph database)
- Crucible: Git-friendly, versionable
- Crucible: AI agent integration built-in

### vs. Obsidian Dataview

**Similar**:
- YAML frontmatter for metadata
- Query language for filtering notes
- Aggregate queries (count, avg, etc.)

**Different**:
- Crucible: Type schemas with validation
- Crucible: Templates built into type system
- Crucible: Agent-native API

### vs. Notion Databases

**Similar**:
- Properties define metadata structure
- Multiple property types (text, number, date, select)
- Database views with filters

**Different**:
- Crucible: Plaintext files, not proprietary database
- Crucible: Local-first, no cloud dependency
- Crucible: Full markdown flexibility

## Implementation Phases

### Phase 1: Type Definitions (Week 1)
- Parse type definition files
- Create type registry
- Validate relation schemas
- Basic type lookup

### Phase 2: Templates (Week 2)
- Load templates from files
- Placeholder expansion ({{var}})
- `cru new {type}` command
- Create example types (book, movie, meeting)

### Phase 3: Database Integration (Week 3)
- Index `type` field
- Index relation values
- Type-aware queries
- Aggregate functions

### Phase 4: CLI & Validation (Week 4)
- `cru types` command suite
- Frontmatter validation
- Validation warnings/errors
- Type suggestions

## Use Cases

### Personal Knowledge Management

**Book Tracking**:
1. Create book notes from template
2. Track reading status, ratings
3. Query: "What 5-star books did I read this year?"
4. Visualize reading habits over time

**Movie Watchlist**:
1. Add movies to watchlist
2. Rate after watching
3. Query: "Show unwatched movies by favorite directors"
4. Export to JSON for sharing

### Team Collaboration

**Meeting Notes**:
1. Create meeting note from template (auto-populated date)
2. Fill in attendees, agenda, notes
3. Extract action items → create task notes
4. Query: "All meetings with Alice from Q4"

**Project Documentation**:
1. Define "project" type with status, deadline, owner
2. Create project notes from template
3. Link to related meeting notes
4. Query: "Active projects due this month"

### Research & Academia

**Paper Database**:
1. Define "paper" type with authors, year, journal, citations
2. Create paper notes (manual or from BibTeX)
3. Link papers via citations
4. Query: "Papers published after 2020 with >100 citations"
5. Visualize citation network

## Security Considerations

- Type schemas are YAML only (no code execution)
- Regex patterns validated for safety (prevent ReDoS)
- Template paths validated (prevent directory traversal)
- Validation is permissive (warnings, not errors) to avoid data loss
- Types are user-defined (no system types required)

## Future Enhancements

### Type Inheritance
```yaml
type_id: task
extends: note
relations:
  status: [todo, in-progress, done]
```

### Computed Relations
```yaml
days_since_read:
  type: computed
  formula: "today - read_date"
```

### Bidirectional Links
When `book.author → person`, auto-create `person.books → [book]`

### Auto Type Detection
AI analyzes content and suggests: "This looks like a book review. Convert to type:book?"

### Type Migration
```bash
cru types migrate "books/*.md" --to book --infer-metadata
```

## Open Questions

1. Should validation be strict (reject invalid) or permissive (warn)?
   - **Recommendation**: Permissive by default, strict mode opt-in

2. Should we use simple `{{var}}` replacement or full templating (Handlebars)?
   - **Recommendation**: Simple for v1, full templating in v2

3. Should we support type inheritance in v1?
   - **Recommendation**: Defer to v2, start flat

4. Should all relation fields be indexed or only marked ones?
   - **Recommendation**: Index common types (date, number, enum), opt-in for text

## Related Work

- **AnyType**: [Types Documentation](https://doc.anytype.io/anytype-docs/getting-started/types)
- **AnyType Relations**: [Relations Documentation](https://doc.anytype.io/anytype-docs/basics/relations)
- **Obsidian Dataview**: YAML-based metadata queries
- **Notion**: Database properties and views

## Contributing

This is a proposal! Feedback welcome on:
- Type definition syntax (YAML in frontmatter OK?)
- Relation types (missing any essential types?)
- Validation approach (strict vs permissive?)
- Template syntax (placeholders sufficient?)
- Example types (what types would you use?)

---

**Status Legend**:
- 📝 Proposal - Under review
- 🚧 In Progress - Implementation started
- ✅ Implemented - Code complete
- 📦 Shipped - In release

## Sources

- [AnyType Types](https://doc.anytype.io/anytype-docs/getting-started/types)
- [AnyType Relations](https://doc.anytype.io/anytype-docs/basics/relations)
- [AnyType Templates](https://doc.anytype.io/anytype-docs/getting-started/types/templates)
