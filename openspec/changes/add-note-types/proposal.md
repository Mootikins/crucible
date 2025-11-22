# Add Note Types: Structured Metadata and Templates

## Why

Crucible is designed for **plaintext-first knowledge management**, but not all notes are created equal. In practice, users organize knowledge into different categories with different metadata needs:

- **Book reviews** need: title, author, rating, genre, read_date
- **Movie ratings** need: title, director, rating, release_year, watched_date
- **Meeting notes** need: date, attendees, agenda_items, action_items
- **Research papers** need: title, authors, journal, year, doi, citations
- **Tasks** need: status, priority, due_date, assignee, tags
- **People/Contacts** need: email, phone, organization, role, last_contact

Currently, users can add arbitrary YAML frontmatter to any note, but there's no system for:
1. **Defining types** with expected metadata schemas
2. **Templates** that pre-populate frontmatter for new notes of a type
3. **Querying by type** to find all notes of a specific category
4. **Validating metadata** against type schemas
5. **Type-aware search** that understands semantic differences between types

### The Critical Insight: Types Enable Structure Without Sacrificing Flexibility

The best knowledge management systems support **both** freeform notes **and** structured data. Types provide this bridge:
- Every note is still just markdown + frontmatter (plaintext-first)
- The `type` metadata field determines expected structure
- Types define schemas, but don't enforce rigidly (graceful degradation)
- Users can query: "show me all books rated 5 stars" or "meetings from Q4 2024"

This is the same philosophy as **AnyType** (objects have types with relations/properties), **Obsidian Dataview** (YAML frontmatter with queries), and **Notion** (databases with properties).

### Why This Matters for AI Agents

AI agents benefit enormously from typed notes:
- "Create a book review for The Three-Body Problem" → agent knows to use book template
- "Show me all contacts in aerospace industry" → agent queries people notes with industry filter
- "Find papers published after 2020 with >100 citations" → semantic query on paper type
- "Create meeting notes for today's sync" → auto-populate date, suggest attendees from past meetings

Types give agents **semantic understanding** of note structure, enabling richer interactions.

## What Changes

**NEW CAPABILITY: Note Types with Metadata Schemas**

### Core Concepts

**Note Types**:
- Every note has a `type` metadata field (defaults to "note")
- Types are defined in special `.crucible/types/*.md` files
- Types specify expected metadata fields (relations/properties)
- Types can have multiple templates for different use cases

**Metadata Relations**:
- Fields that describe properties of a note (like database columns)
- Can link to other notes (e.g., `author: [[Jane Doe]]`)
- Support various data types: text, number, date, boolean, list, link
- Optional vs required fields
- Default values

**Templates**:
- Pre-populated markdown files with frontmatter
- Stored in `.crucible/types/{type}/templates/*.md`
- Can include placeholder text, example content
- Multiple templates per type (e.g., "Quick Meeting", "Detailed Meeting")

**Type-Based Queries**:
- Query language extensions: `type:book AND rating:>4`
- Semantic search aware of type schemas
- Aggregate queries: `avg(rating) WHERE type:movie`
- Filter by relation values

### Type Definition Format

Types are defined as markdown files with YAML frontmatter:

```markdown
---
type_id: book
name: Book
description: Book reviews and reading notes
icon: 📚
color: blue
relations:
  title:
    type: text
    required: true
    description: Book title
  author:
    type: link
    required: true
    description: Author (link to person note)
    target_type: person
  rating:
    type: number
    min: 1
    max: 5
    description: Rating out of 5 stars
  genre:
    type: list
    options: [fiction, non-fiction, sci-fi, fantasy, mystery, biography, technical]
    description: Book genre(s)
  read_date:
    type: date
    description: Date finished reading
  isbn:
    type: text
    pattern: '^[\d-]{10,17}$'
    description: ISBN number
  pages:
    type: number
    description: Number of pages
  status:
    type: enum
    options: [to-read, reading, finished, abandoned]
    default: to-read
templates:
  - quick-review
  - detailed-analysis
---

# Book Type

This type is used for book reviews, reading notes, and book metadata tracking.

## Usage

Create a new book note with:
```bash
cru new book "The Three-Body Problem"
```

Or specify a template:
```bash
cru new book --template detailed-analysis "Project Hail Mary"
```

## Example Queries

Find all 5-star sci-fi books:
```
type:book AND genre:sci-fi AND rating:5
```

List unfinished books:
```
type:book AND status:reading
```

Books by author:
```
type:book AND author:[[Ted Chiang]]
```
```

### Template Format

Templates are markdown files with placeholder frontmatter:

```markdown
---
type: book
title: {{title}}
author:
rating:
genre: []
read_date:
isbn:
pages:
status: to-read
tags: [books, reading]
---

# {{title}}

## Summary

Brief summary of the book...

## Key Takeaways

-
-
-

## Quotes

>

## My Thoughts

## Related
-
```

### Example: Movie Rating Type

```markdown
---
type_id: movie
name: Movie
description: Movie reviews and watchlist
icon: 🎬
color: purple
relations:
  title:
    type: text
    required: true
  director:
    type: link
    target_type: person
  rating:
    type: number
    min: 0
    max: 10
    step: 0.5
  release_year:
    type: number
    min: 1888
    max: 2100
  watched_date:
    type: date
  genre:
    type: list
    options: [action, comedy, drama, horror, sci-fi, documentary, animation]
  runtime_minutes:
    type: number
  imdb_id:
    type: text
    pattern: '^tt[\d]{7,8}$'
  status:
    type: enum
    options: [watchlist, watched, rewatched]
    default: watchlist
templates:
  - quick-rating
  - detailed-review
---

# Movie Type

Track movies you've watched and want to watch.
```

### Example: Meeting Notes Type

```markdown
---
type_id: meeting
name: Meeting
description: Meeting notes and action items
icon: 🗓️
color: green
relations:
  meeting_date:
    type: date
    required: true
    default: today
  attendees:
    type: list
    item_type: link
    target_type: person
    description: People who attended
  agenda_items:
    type: list
    item_type: text
  action_items:
    type: list
    item_type: text
  meeting_type:
    type: enum
    options: [standup, planning, review, retrospective, sync, brainstorm]
  duration_minutes:
    type: number
  next_meeting:
    type: date
templates:
  - standup
  - planning-meeting
  - retrospective
---

# Meeting Type

Structured meeting notes with action items.
```

### Type Registry and Discovery

**Type Registry** (`.crucible/types/registry.yaml`):
```yaml
types:
  note:
    builtin: true
    description: Default note type
  book:
    path: types/book.md
    enabled: true
  movie:
    path: types/movie.md
    enabled: true
  meeting:
    path: types/meeting.md
    enabled: true
  person:
    path: types/person.md
    enabled: true
  paper:
    path: types/paper.md
    enabled: true

# Type inheritance (future)
# task:
#   extends: note
#   path: types/task.md
```

### CLI Integration

**Create typed notes**:
```bash
# Create new book note
cru new book "The Three-Body Problem"

# Create with template
cru new meeting --template standup "Daily Standup 2025-11-22"

# Create and open in editor
cru new movie --edit "Interstellar"

# List available types
cru types list

# Show type definition
cru types show book

# Validate type definition
cru types validate book

# Create new type from template
cru types create movie-series --based-on movie
```

**Query by type**:
```bash
# Find all books
cru query "type:book"

# Find 5-star books
cru query "type:book AND rating:5"

# Find unwatched movies
cru query "type:movie AND status:watchlist"

# Aggregate queries
cru query "type:book" --aggregate "avg(rating)"

# Export to JSON/CSV
cru query "type:book" --format json > books.json
```

### Integration with Existing Systems

**Parser Integration**:
- Parse type definitions from `.crucible/types/*.md`
- Validate YAML frontmatter against type schemas
- Graceful handling of missing/invalid fields (warnings, not errors)
- Type-aware content suggestions

**Database Integration**:
- Index `type` field for fast filtering
- Index relation fields for querying
- Support type-aware search (e.g., semantic search within book notes only)
- Aggregate queries over typed notes

**Agent Integration**:
- Agents discover available types: "What types of notes exist?"
- Agents create typed notes: "Create a book review for..."
- Agents query by type: "Show me all meetings from last week"
- Agents suggest types: "This looks like a book review, should I add type:book?"

**Template Expansion**:
- `{{title}}` replaced with user-provided title
- `{{date}}` replaced with current date
- `{{today}}` for date fields
- `{{user}}` for current user (if configured)
- Template functions: `{{date:+7d}}` (7 days from now)

## Impact

### Affected Specs

- **note-types** (NEW) - Complete type system specification
- **parser** (extends) - Parse and validate type definitions
- **query-system** (extends) - Type-aware querying
- **agent-system** (reference) - Agents use types for semantic understanding
- **templates** (NEW) - Template system for typed notes

### Affected Code

**New Components**:
- `crates/crucible-core/src/types/` - NEW - Type system core
  - `definition.rs` - Type definition parsing and validation
  - `registry.rs` - Type registry, discovery, loading
  - `schema.rs` - Relation schemas, data types, validation
  - `template.rs` - Template loading and expansion
- `crates/crucible-core/src/types/relations.rs` - NEW - Relation types
  - Text, Number, Date, Boolean, List, Link, Enum
  - Validation logic for each type
  - Default value generation
- `crates/crucible-parser/src/frontmatter/` - MODIFY - Type-aware frontmatter parsing
  - Validate frontmatter against type schema
  - Emit warnings for missing required fields
  - Suggest fields based on type
- `crates/crucible-db/src/types.rs` - NEW - Type indexing and querying
  - Index `type` field
  - Index relation values
  - Type-aware query execution
- `crates/crucible-cli/src/commands/types.rs` - NEW - Type management commands
  - `cru types list`, `show`, `validate`, `create`
- `crates/crucible-cli/src/commands/new.rs` - NEW - Create typed notes
  - `cru new {type} {title}`
  - Template expansion

**Integration Points**:
- `crates/crucible-parser/` - Parse type definitions, validate frontmatter
- `crates/crucible-db/` - Index and query typed notes
- `crates/crucible-agents/` (future) - Type-aware agent interactions
- `crates/crucible-pipeline/` - Process type definition files

**Dependencies Added**:
- `jsonschema = "0.17"` - Schema validation (for relation schemas)
- `chrono = "0.4"` (already exists) - Date parsing and validation
- `regex = "1.10"` (already exists) - Pattern validation

### Implementation Strategy

**Phase 1: Type Definitions (Week 1)**
- Define Type and Relation domain types
- Implement type definition parser
- Create type registry and discovery system
- Implement basic validation

**Phase 2: Templates (Week 2)**
- Implement template loading and parsing
- Add template expansion with placeholders
- Create template registry
- Add `cru new {type}` command

**Phase 3: Database Integration (Week 3)**
- Index `type` field in SurrealDB
- Index relation values
- Implement type-aware queries
- Add query language extensions (type:, relation filters)

**Phase 4: CLI and Validation (Week 4)**
- Add `cru types` command suite
- Implement frontmatter validation against schemas
- Add type-aware suggestions
- Create default type definitions (book, movie, meeting, person)

### User-Facing Impact

**Immediate Benefits**:
- Structure notes with defined metadata schemas
- Create notes from templates quickly
- Query notes by type and metadata
- AI agents understand note structure semantically
- Validate metadata consistency across notes

**Long-Term Vision**:
- Community-shared type definitions (research paper types, project types, etc.)
- Type inheritance (task extends note, adds status/priority)
- Cross-note relations visualized in graph
- Type-aware views (kanban for tasks, calendar for events)
- Automatic type detection (AI suggests type based on content)

**Example Workflows**:

```
Book Tracking:
1. cru new book "Dune"
2. Fill in rating, genre, read_date in YAML
3. Write review in markdown body
4. Query: "type:book AND genre:sci-fi AND rating:>4"
5. Agent: "Recommend books similar to [[Dune]]"
```

```
Meeting Management:
1. cru new meeting --template standup "Daily Standup"
2. Auto-populated with today's date, team attendees
3. Fill in discussion points during meeting
4. Extract action items → create task notes
5. Query: "type:meeting AND meeting_date:>2025-11-01"
```

```
Research Paper Database:
1. Define "paper" type with author, year, journal, citations
2. Create paper notes from BibTeX (future agent plugin)
3. Link papers via [[citations]]
4. Query: "type:paper AND year:>2020 AND citations:>100"
5. Visualize citation network in graph view (future)
```

### Security Considerations

**Type Definition Validation**:
- Schemas validated at load time (malformed types rejected)
- No arbitrary code execution (YAML only)
- Regex patterns validated for safety (no ReDoS)
- File paths for templates validated (no directory traversal)

**Frontmatter Validation**:
- Invalid metadata logged as warnings (non-fatal)
- Type mismatches handled gracefully
- No data loss if type schema changes
- Backward compatibility with untyped notes

### Timeline
- **Week 1**: Type definitions and registry
- **Week 2**: Templates and expansion
- **Week 3**: Database integration and querying
- **Week 4**: CLI commands and default types
- **Estimated effort**: 4 weeks for production-ready type system

### Dependencies
- Type registry (new)
- Template system (new)
- Query language extensions (extends existing)
- Frontmatter validation (extends parser)

### Future Extensions

**Type Inheritance**:
```yaml
task:
  extends: note
  relations:
    status:
      type: enum
      options: [todo, in-progress, done]
```

**Computed Relations**:
```yaml
days_since_read:
  type: computed
  formula: "today - read_date"
```

**Relation Constraints**:
```yaml
due_date:
  type: date
  validate: "due_date > created_date"
```

**Type Migration**:
```bash
# Convert existing notes to typed notes
cru types migrate "books/*.md" --to book --infer-metadata
```

**Auto Type Detection**:
- AI analyzes note content and suggests type
- "This note mentions a book title, author, and rating. Convert to type:book?"

## Questions for Review

1. Should type definitions be YAML or something else (TOML, JSON)?
2. Should we enforce required fields strictly or just warn?
3. Should templates support more complex logic (conditionals, loops)?
4. Should we support type inheritance in v1 or defer to v2?
5. Should relation validation be strict or permissive by default?
6. Should we index all relation fields or only those marked for indexing?
