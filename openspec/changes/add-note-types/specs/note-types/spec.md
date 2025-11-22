# Note Types Specification

## Overview

This specification defines a type system for Crucible notes, enabling structured metadata schemas, templates, and type-aware querying. Every note can have a `type` field that determines expected metadata structure, similar to AnyType's object/relation system and Obsidian's Dataview pattern.

## ADDED Requirements

### Requirement: Parse Type Definitions from Markdown

The system SHALL parse type definition files from `.crucible/types/*.md`, extracting metadata schemas, relation definitions, and template configurations.

#### Scenario: Parse valid type definition
- **GIVEN** markdown file at `.crucible/types/book.md` with frontmatter:
  ```yaml
  type_id: book
  name: Book
  relations:
    title:
      type: text
      required: true
    rating:
      type: number
      min: 1
      max: 5
  ```
- **WHEN** type registry loads definitions
- **THEN** system SHALL create Type object with id "book"
- **AND** SHALL create relation "title" with type Text, required=true
- **AND** SHALL create relation "rating" with type Number, min=1, max=5
- **AND** type SHALL be available for use

#### Scenario: Parse type with all metadata
- **GIVEN** type definition with: type_id, name, description, icon, color, relations, templates
- **WHEN** parser processes definition
- **THEN** system SHALL extract all fields
- **AND** icon SHALL be single emoji or text string
- **AND** color SHALL be valid CSS color or named color
- **AND** templates SHALL be array of template names
- **AND** relations SHALL be map of field_name → schema

#### Scenario: Invalid type definition syntax
- **GIVEN** type definition with malformed YAML frontmatter
- **WHEN** parser processes file
- **THEN** system SHALL return parse error
- **AND** error SHALL include file path and line number
- **AND** type SHALL NOT be registered
- **AND** system SHALL log error for debugging

#### Scenario: Type with no relations
- **GIVEN** type definition with only type_id and name
- **WHEN** type registry loads definition
- **THEN** system SHALL create valid type
- **AND** type SHALL have empty relations map
- **AND** type SHALL use default template (no frontmatter)
- **AND** notes of this type SHALL behave like basic notes

### Requirement: Validate Relation Schemas

The system SHALL validate relation field definitions, ensuring data types, constraints, and options are well-formed and consistent.

#### Scenario: Text relation with pattern validation
- **GIVEN** relation definition:
  ```yaml
  isbn:
    type: text
    pattern: '^[\d-]{10,17}$'
  ```
- **WHEN** system validates schema
- **THEN** pattern SHALL be valid regex
- **AND** pattern SHALL be tested for ReDoS vulnerabilities
- **AND** relation type SHALL be Text
- **AND** validation function SHALL be created

#### Scenario: Number relation with min/max bounds
- **GIVEN** relation definition:
  ```yaml
  rating:
    type: number
    min: 1
    max: 5
    step: 0.5
  ```
- **WHEN** system validates schema
- **THEN** min SHALL be less than max
- **AND** step SHALL be positive
- **AND** default value (if provided) SHALL be within bounds
- **AND** default SHALL be aligned to step

#### Scenario: Enum relation with options
- **GIVEN** relation definition:
  ```yaml
  status:
    type: enum
    options: [todo, in-progress, done]
    default: todo
  ```
- **WHEN** system validates schema
- **THEN** options SHALL be non-empty array
- **AND** options SHALL contain unique values
- **AND** default (if provided) SHALL be in options
- **AND** enum values SHALL be stored as strings

#### Scenario: Link relation with target type
- **GIVEN** relation definition:
  ```yaml
  author:
    type: link
    target_type: person
    required: true
  ```
- **WHEN** system validates schema
- **THEN** target_type SHALL reference valid type (or "note" for any)
- **AND** link SHALL resolve to note ID or [[wikilink]]
- **AND** target note SHALL have specified type (if enforced)
- **AND** required=true SHALL prevent empty links

#### Scenario: List relation with item type
- **GIVEN** relation definition:
  ```yaml
  genres:
    type: list
    item_type: text
    options: [fiction, non-fiction, sci-fi]
  ```
- **WHEN** system validates schema
- **THEN** item_type SHALL be valid data type
- **AND** options (if provided) SHALL constrain each item
- **AND** list SHALL accept array in YAML frontmatter
- **AND** empty list SHALL be valid unless required=true

#### Scenario: Date relation with default value
- **GIVEN** relation definition:
  ```yaml
  created_date:
    type: date
    default: today
  ```
- **WHEN** system validates schema
- **THEN** default "today" SHALL resolve to current date
- **AND** default "now" SHALL resolve to current datetime
- **AND** default SHALL accept ISO 8601 format: "2025-11-22"
- **AND** invalid default SHALL fail validation

### Requirement: Type Registry and Discovery

The system SHALL maintain a registry of available types, discover type definitions from configured directories, and provide type lookup by ID.

#### Scenario: Load types at startup
- **GIVEN** `.crucible/types/` directory with type definition files
- **WHEN** system initializes
- **THEN** registry SHALL scan directory for `*.md` files
- **AND** SHALL parse each file as type definition
- **AND** SHALL validate schemas
- **AND** SHALL register valid types by type_id
- **AND** invalid types SHALL be logged but not block startup

#### Scenario: Type registry lookup
- **GIVEN** registered types: note, book, movie, meeting
- **WHEN** user queries registry.get("book")
- **THEN** system SHALL return Type object for "book"
- **AND** object SHALL include: name, description, icon, color, relations, templates
- **WHEN** user queries registry.get("nonexistent")
- **THEN** system SHALL return None or error
- **AND** NOT panic or crash

#### Scenario: List all available types
- **GIVEN** registered types in registry
- **WHEN** user requests type list
- **THEN** system SHALL return all type IDs
- **AND** SHALL include type name and description
- **AND** SHALL indicate builtin vs custom types
- **AND** SHALL sort alphabetically by name

#### Scenario: Type override by kiln scope
- **GIVEN** system type at `~/.config/crucible/types/book.md`
- **AND** kiln type at `.crucible/types/book.md`
- **WHEN** registry loads types
- **THEN** kiln type SHALL take precedence
- **AND** system SHALL log override
- **AND** `cru types show book` SHALL show kiln definition

#### Scenario: Type hot-reload on file change
- **GIVEN** file watcher monitoring `.crucible/types/`
- **WHEN** type definition file is modified
- **THEN** system SHALL detect change
- **AND** SHALL reload type definition
- **AND** SHALL re-validate schema
- **AND** existing notes SHALL use new schema on next access

### Requirement: Create Typed Notes from Templates

The system SHALL create new notes with specified type, populate frontmatter from relation defaults, and expand template content with user-provided values.

#### Scenario: Create note with type
- **GIVEN** registered type "book" with relations
- **WHEN** user runs `cru new book "The Three-Body Problem"`
- **THEN** system SHALL create new markdown file
- **AND** frontmatter SHALL include `type: book`
- **AND** frontmatter SHALL include `title: "The Three-Body Problem"`
- **AND** other relations SHALL use default values or be empty
- **AND** file name SHALL be derived from title (slugified)

#### Scenario: Create note with template
- **GIVEN** type "book" with template "detailed-review"
- **AND** template at `.crucible/types/book/templates/detailed-review.md`
- **WHEN** user runs `cru new book --template detailed-review "Dune"`
- **THEN** system SHALL load template file
- **AND** SHALL replace `{{title}}` with "Dune"
- **AND** SHALL populate frontmatter with defaults
- **AND** SHALL preserve template structure and headings

#### Scenario: Template placeholder expansion
- **GIVEN** template with placeholders:
  ```markdown
  ---
  type: meeting
  meeting_date: {{date}}
  title: {{title}}
  ---
  # {{title}}
  Date: {{date:format:long}}
  ```
- **WHEN** user creates note with title "Sprint Planning"
- **THEN** `{{title}}` SHALL be replaced with "Sprint Planning"
- **AND** `{{date}}` SHALL be replaced with current date (ISO format)
- **AND** `{{date:format:long}}` SHALL use formatted date (e.g., "November 22, 2025")
- **AND** unknown placeholders SHALL be left as-is or removed (configurable)

#### Scenario: Create note without template
- **GIVEN** type "book" with no default template
- **WHEN** user runs `cru new book "New Book"`
- **THEN** system SHALL create note with frontmatter only
- **AND** frontmatter SHALL include type and title
- **AND** markdown body SHALL be empty
- **AND** user can fill in content manually

#### Scenario: Template with required relations
- **GIVEN** type with required relation: `author: {type: link, required: true}`
- **WHEN** user creates note without specifying author
- **THEN** system SHALL create note successfully
- **AND** SHALL emit warning about missing required field
- **AND** frontmatter SHALL include `author: ` (empty, for user to fill)
- **AND** validation SHALL fail until user provides value

### Requirement: Validate Frontmatter Against Type Schema

The system SHALL validate note frontmatter against type relation schemas, emitting warnings for missing required fields, type mismatches, and constraint violations.

#### Scenario: Validate note with correct frontmatter
- **GIVEN** note with frontmatter:
  ```yaml
  type: book
  title: "Dune"
  rating: 5
  genre: [sci-fi, fiction]
  ```
- **AND** type "book" with matching relations
- **WHEN** parser validates frontmatter
- **THEN** validation SHALL pass
- **AND** no warnings SHALL be emitted
- **AND** note SHALL be indexed with all relation values

#### Scenario: Missing required field
- **GIVEN** type with required relation `title: {type: text, required: true}`
- **AND** note frontmatter without `title` field
- **WHEN** parser validates frontmatter
- **THEN** system SHALL emit warning
- **AND** warning SHALL indicate field name and type
- **AND** note SHALL still be parsed (non-fatal)
- **AND** query results MAY exclude note with missing required field

#### Scenario: Type mismatch in relation value
- **GIVEN** type with relation `rating: {type: number, min: 1, max: 5}`
- **AND** note frontmatter with `rating: "five stars"` (string, not number)
- **WHEN** parser validates frontmatter
- **THEN** system SHALL emit warning about type mismatch
- **AND** SHALL indicate expected type (number) vs actual (string)
- **AND** value SHALL be stored as-is (graceful degradation)
- **AND** queries on rating SHALL handle type coercion or skip

#### Scenario: Constraint violation (out of bounds)
- **GIVEN** type with relation `rating: {type: number, min: 1, max: 5}`
- **AND** note frontmatter with `rating: 10`
- **WHEN** parser validates frontmatter
- **THEN** system SHALL emit warning about constraint violation
- **AND** SHALL indicate valid range (1-5)
- **AND** value SHALL be stored but marked invalid
- **AND** user SHALL be notified to fix value

#### Scenario: Enum value not in options
- **GIVEN** type with relation `status: {type: enum, options: [todo, in-progress, done]}`
- **AND** note frontmatter with `status: completed`
- **WHEN** parser validates frontmatter
- **THEN** system SHALL emit warning
- **AND** SHALL suggest valid options: todo, in-progress, done
- **AND** value SHALL be stored as-is (permissive mode)
- **AND** strict mode (if enabled) SHALL reject value

#### Scenario: Graceful handling of untyped notes
- **GIVEN** note without `type` field in frontmatter
- **WHEN** parser processes note
- **THEN** system SHALL treat as type "note" (default)
- **AND** no validation SHALL be performed
- **AND** all frontmatter fields SHALL be accepted
- **AND** note SHALL be queryable by arbitrary fields

### Requirement: Query Notes by Type and Relations

The system SHALL extend query language to filter notes by type and relation values, supporting equality, comparison, and list membership operators.

#### Scenario: Query all notes of a type
- **GIVEN** notes with types: book, movie, meeting, note
- **WHEN** user queries `type:book`
- **THEN** system SHALL return only notes where type=book
- **AND** SHALL use indexed type field (fast lookup)
- **AND** results SHALL be ordered by relevance or modified date

#### Scenario: Query by relation equality
- **GIVEN** book notes with various ratings
- **WHEN** user queries `type:book AND rating:5`
- **THEN** system SHALL return books where rating equals 5
- **AND** SHALL handle both integer and float comparison
- **AND** notes with rating=5.0 SHALL match rating:5

#### Scenario: Query by relation comparison
- **GIVEN** book notes with ratings 1-5
- **WHEN** user queries `type:book AND rating:>4`
- **THEN** system SHALL return books where rating > 4 (i.e., 4.5, 5)
- **AND** SHALL support operators: `>`, `<`, `>=`, `<=`, `!=`
- **AND** non-numeric ratings SHALL be skipped

#### Scenario: Query by relation range
- **GIVEN** paper notes with publication years
- **WHEN** user queries `type:paper AND year:[2020 TO 2025]`
- **THEN** system SHALL return papers where 2020 <= year <= 2025
- **AND** range syntax SHALL be inclusive on both ends
- **AND** SHALL support date ranges: `read_date:[2025-01-01 TO 2025-12-31]`

#### Scenario: Query by list membership
- **GIVEN** book notes with genre arrays
- **WHEN** user queries `type:book AND genre:sci-fi`
- **THEN** system SHALL return books where "sci-fi" is in genre list
- **AND** SHALL handle list fields automatically
- **AND** SHALL support multiple values: `genre:(sci-fi OR fantasy)`

#### Scenario: Query by link relation
- **GIVEN** book notes with author links to person notes
- **WHEN** user queries `type:book AND author:[[Ted Chiang]]`
- **THEN** system SHALL return books where author links to "Ted Chiang" note
- **AND** SHALL resolve wikilink to note ID
- **AND** SHALL support multiple authors (OR logic)

#### Scenario: Query with missing relation
- **GIVEN** book notes, some with rating, some without
- **WHEN** user queries `type:book AND rating:*`
- **THEN** system SHALL return books that have rating field (any value)
- **WHEN** user queries `type:book AND !rating:*`
- **THEN** system SHALL return books without rating field

#### Scenario: Combine type and text search
- **GIVEN** mixed note types
- **WHEN** user queries `type:book AND "science fiction"`
- **THEN** system SHALL return book notes containing "science fiction" in content
- **AND** SHALL combine type filter (indexed) with text search
- **AND** type filter SHALL apply first (narrowing scope)

### Requirement: Aggregate Queries on Typed Notes

The system SHALL support aggregate functions (count, sum, avg, min, max) over relation values for typed notes.

#### Scenario: Count notes by type
- **GIVEN** notes of various types
- **WHEN** user queries `type:book --aggregate count`
- **THEN** system SHALL return count of book notes
- **AND** SHALL be efficient (use index)

#### Scenario: Average numeric relation
- **GIVEN** book notes with ratings: 3, 4, 5, 5, 4
- **WHEN** user queries `type:book --aggregate avg(rating)`
- **THEN** system SHALL return average: 4.2
- **AND** SHALL skip notes without rating field
- **AND** SHALL skip non-numeric values

#### Scenario: Group by relation value
- **GIVEN** book notes with various genres
- **WHEN** user queries `type:book --group-by genre --aggregate count`
- **THEN** system SHALL return counts per genre:
  - sci-fi: 12
  - fiction: 23
  - fantasy: 8
- **AND** SHALL handle multi-valued list fields (count each value)

#### Scenario: Min and max on date relations
- **GIVEN** book notes with read_date values
- **WHEN** user queries `type:book --aggregate min(read_date), max(read_date)`
- **THEN** system SHALL return earliest and latest dates
- **AND** SHALL parse dates correctly
- **AND** SHALL skip notes without read_date

### Requirement: Type-Aware CLI Commands

The system SHALL provide CLI commands for managing types, creating typed notes, and querying by type.

#### Scenario: List available types
- **GIVEN** registered types in registry
- **WHEN** user runs `cru types list`
- **THEN** system SHALL display table of types:
  - Type ID, Name, Description, Icon, Template Count
- **AND** SHALL indicate builtin vs custom types
- **AND** SHALL sort alphabetically

#### Scenario: Show type definition
- **GIVEN** registered type "book"
- **WHEN** user runs `cru types show book`
- **THEN** system SHALL display:
  - Type metadata (id, name, description, icon, color)
  - Relations table (name, type, required, default, constraints)
  - Available templates
  - Example usage
- **AND** output SHALL be human-readable (formatted)

#### Scenario: Validate type definition
- **GIVEN** type definition file at `.crucible/types/movie.md`
- **WHEN** user runs `cru types validate movie`
- **THEN** system SHALL parse definition
- **AND** SHALL validate all relation schemas
- **AND** SHALL check for errors: invalid regex, inconsistent constraints
- **AND** SHALL report validation errors with line numbers
- **AND** exit code SHALL be 0 if valid, non-zero if invalid

#### Scenario: Create new type definition
- **GIVEN** user wants to create type "recipe"
- **WHEN** user runs `cru types create recipe --interactive`
- **THEN** system SHALL prompt for:
  - Type name, description, icon
  - Relation definitions (name, type, required, etc.)
- **AND** SHALL generate type definition file
- **AND** SHALL validate generated schema
- **AND** SHALL register type in registry

#### Scenario: Create typed note via CLI
- **GIVEN** registered type "book"
- **WHEN** user runs `cru new book "Project Hail Mary"`
- **THEN** system SHALL create note file
- **AND** file path SHALL be `books/project-hail-mary.md` (or configured location)
- **AND** frontmatter SHALL include type and title
- **AND** system SHALL open in editor if `--edit` flag provided

#### Scenario: Query typed notes via CLI
- **GIVEN** book notes in database
- **WHEN** user runs `cru query "type:book AND rating:5"`
- **THEN** system SHALL display matching notes
- **AND** output SHALL show: title, path, relevant relation values
- **AND** SHALL support output formats: table, json, csv
- **WHEN** user runs `cru query "type:book" --format json`
- **THEN** output SHALL be valid JSON array of note objects

### Requirement: Type-Aware Agent Interactions

The system SHALL enable AI agents to discover types, create typed notes, and query with type awareness.

#### Scenario: Agent discovers available types
- **GIVEN** registered types in system
- **WHEN** agent queries "What types of notes exist?"
- **THEN** agent tool SHALL return list of types with descriptions
- **AND** agent SHALL understand semantic meaning of each type
- **AND** agent can suggest appropriate type for user content

#### Scenario: Agent creates typed note
- **GIVEN** user request "Create a book review for Dune"
- **WHEN** agent processes request
- **THEN** agent SHALL recognize "book review" → type:book
- **AND** SHALL invoke create_note tool with type="book", title="Dune"
- **AND** SHALL prompt user for required fields (author, rating)
- **AND** note SHALL be created with proper frontmatter

#### Scenario: Agent queries typed notes
- **GIVEN** user request "Show me all 5-star sci-fi books"
- **WHEN** agent processes request
- **THEN** agent SHALL construct query: `type:book AND genre:sci-fi AND rating:5`
- **AND** SHALL execute query via search tool
- **AND** SHALL present results to user with titles and summaries

#### Scenario: Agent suggests type for untyped note
- **GIVEN** existing note with content mentioning book title, author, rating
- **WHEN** agent analyzes note
- **THEN** agent SHALL suggest: "This looks like a book review. Convert to type:book?"
- **AND** if user confirms, SHALL add type field to frontmatter
- **AND** SHALL infer relation values from content (title, author, etc.)

## CHANGED Requirements

(None - this is a new feature with no modifications to existing specs)

## REMOVED Requirements

(None - no existing functionality removed)

## Dependencies

### Internal Dependencies
- `crucible-parser` - Parse type definitions and validate frontmatter
- `crucible-core` - Type domain types, relation schemas, template expansion
- `crucible-db` - Index type field and relation values
- `crucible-cli` - Type management and typed note creation commands

### External Dependencies
- `jsonschema = "0.17"` - Schema validation for relation definitions
- `chrono = "0.4"` (existing) - Date parsing and validation
- `regex = "1.10"` (existing) - Pattern validation for text relations
- `handlebars = "5.1"` OR `tera = "1.19"` - Template expansion (evaluate both)

## Open Questions

1. **Schema Format**: Should type schemas use YAML, JSON Schema, or custom DSL?
   - **Recommendation**: YAML in frontmatter for simplicity, JSON Schema for advanced features (v2)

2. **Validation Strictness**: Should invalid frontmatter be rejected or accepted with warnings?
   - **Recommendation**: Permissive by default (warnings), strict mode opt-in via config

3. **Template Engine**: Should we use Handlebars, Tera, or simple string replacement?
   - **Recommendation**: Simple replacement for v1 (`{{var}}`), full templating in v2

4. **Type Inheritance**: Should types support inheritance (extends) in v1?
   - **Recommendation**: Defer to v2, start with flat type system

5. **Computed Relations**: Should we support computed fields (e.g., `days_since: today - created_date`)?
   - **Recommendation**: Defer to v2, start with static fields only

6. **Migration Tools**: Should we provide tools to convert untyped notes to typed?
   - **Recommendation**: Yes, add `cru types migrate` command in Phase 4

## Future Enhancements

### Type Inheritance
```yaml
type_id: task
extends: note
relations:
  status:
    type: enum
    options: [todo, in-progress, done]
  priority:
    type: enum
    options: [low, medium, high]
```

### Computed Relations
```yaml
days_since_read:
  type: computed
  formula: "today - read_date"
  value_type: number
```

### Relation Constraints
```yaml
due_date:
  type: date
  validate: "due_date > created_date"
  error: "Due date must be after creation date"
```

### Bidirectional Links
When `book.author → person`, automatically create `person.books → [book]`

### Type Migration Assistant
```bash
cru types migrate "books/*.md" --to book --infer-metadata --preview
```

### Auto Type Detection
Agent analyzes note content and suggests type based on structure and keywords.
