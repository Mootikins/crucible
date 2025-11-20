## ADDED Requirements

### Requirement: Path-Based Tool System ✅ COMPLETE
The system SHALL provide a comprehensive tool system that enables agents to access and manipulate knowledge within the kiln using filesystem paths, supporting flexible user organization patterns.

**Implementation Status**:
- **Architecture**: ✅ MCP-compatible using rmcp 0.9.0 with `#[tool_router]` and `#[tool]` macros
- **Tools**: ✅ 10 focused tools (refined from 25+ legacy tools, 1,189 lines removed)
- **Categories**:
  - **NoteTools** (6): ✅ create_note, read_note, read_metadata, update_note, delete_note, list_notes
  - **SearchTools** (3): ✅ text_search, property_search, semantic_search
  - **KilnTools** (1): ✅ get_kiln_info
- **Validation**: ✅ Parameters<T> wrapper with schemars::JsonSchema for automatic schema generation
- **Dependency Injection**: ✅ Proper dependency injection via core traits
- **Permission System**: ⏳ DEFERRED - User approval for write operations planned for future implementation

**Design Rationale**:
- Filesystem paths map naturally to MCP resources, shell operations, and file management
- Supports diverse organizational patterns (by project, date, topic, etc.)
- Line range support enables efficient handling of large files
- Separate metadata reads prevent unnecessary content loading

#### Scenario: Agent reads note by path
- **WHEN** agent requests note content using relative path from kiln root
- **THEN** system SHALL return full note content or specified line range
- **AND** agent MAY request only metadata without loading content
- **AND** system SHALL support partial reads via start_line and end_line parameters

#### Scenario: Efficient metadata access
- **WHEN** agent calls `read_metadata` with path
- **THEN** system SHALL return frontmatter properties and structural statistics
- **AND** SHALL NOT load full note content
- **AND** response SHALL include word count, heading count, wikilink count, etc.

### Requirement: Knowledge Access Tools
The system SHALL provide tools for reading, listing, and discovering kiln content using filesystem paths.

#### Scenario: Read note content with line ranges
- **WHEN** agent calls `read_note` with path and optional line range
- **THEN** system SHALL return requested content in structured format
- **AND** SHALL support full file read, first N lines, last N lines, or range
- **AND** response SHALL include total_lines and lines_returned counts
- **AND** content SHALL be suitable for agent processing and analysis

#### Scenario: Read metadata without content
- **WHEN** agent calls `read_metadata` with path
- **THEN** system SHALL parse frontmatter and return properties
- **AND** SHALL return structural statistics (word count, headings, wikilinks)
- **AND** SHALL NOT load or return full note content
- **AND** operation SHALL be fast enough for bulk metadata queries

#### Scenario: List notes in directory
- **WHEN** agent calls `list_notes` with optional folder parameter
- **THEN** system SHALL return all markdown files in specified scope
- **AND** results SHALL include paths, word counts, and modification dates
- **AND** MAY optionally include frontmatter properties
- **AND** SHALL support recursive and non-recursive listing

#### Scenario: Fast text search
- **WHEN** agent calls `text_search` with query string
- **THEN** system SHALL use ripgrep for fast full-text search
- **AND** SHALL return matching lines with context
- **AND** SHALL support case-sensitive and case-insensitive modes
- **AND** MAY limit search to specific folder

#### Scenario: Property and tag search
- **WHEN** agent calls `property_search` with property filters
- **THEN** system SHALL search frontmatter properties across all notes
- **AND** SHALL support AND logic for multiple properties
- **AND** SHALL support OR logic for array values (e.g., tags)
- **AND** tags SHALL be treated as frontmatter properties

#### Scenario: Semantic search discovery
- **WHEN** agent calls `semantic_search` with natural language query
- **THEN** system SHALL return ranked results based on embedding similarity
- **AND** results SHALL include relevance scores and content snippets
- **AND** MAY filter by frontmatter properties before semantic search
- **AND** SHALL work across entire accessible kiln scope

### Requirement: Knowledge Manipulation Tools
The system SHALL provide tools for creating, updating, and deleting notes with appropriate permission controls.

#### Scenario: Create note with frontmatter
- **WHEN** agent calls `create_note` with path, content, and optional frontmatter
- **THEN** system SHALL prompt user for approval before creation
- **AND** upon approval SHALL create note at specified path
- **AND** IF frontmatter provided, SHALL serialize as YAML frontmatter block
- **AND** SHALL return success with path and word count statistics

#### Scenario: Update note content or frontmatter
- **WHEN** agent calls `update_note` with path and content or frontmatter
- **THEN** system SHALL prompt user for approval before modification
- **AND** MAY update content only, frontmatter only, or both
- **AND** IF frontmatter provided, SHALL replace existing frontmatter entirely
- **AND** SHALL return confirmation of which fields were updated

#### Scenario: Delete note with backlink warning
- **WHEN** agent calls `delete_note` with path
- **THEN** system SHALL require explicit user confirmation
- **AND** SHALL detect and warn about incoming wikilinks
- **AND** SHALL include backlink count in approval prompt
- **AND** upon approval SHALL delete file from filesystem

### Requirement: Frontmatter-Based Metadata Management
The system SHALL manage note metadata through frontmatter properties, treating tags as first-class properties.

#### Scenario: Frontmatter as single source of truth
- **WHEN** agent needs to add/remove tags or update properties
- **THEN** agent SHALL use `update_note` with frontmatter parameter
- **AND** frontmatter SHALL be serialized as YAML
- **AND** tags SHALL be stored as array property in frontmatter
- **AND** system SHALL support arbitrary JSON-compatible properties

#### Scenario: Metadata-only updates
- **WHEN** agent calls `update_note` with frontmatter but no content
- **THEN** system SHALL update frontmatter without modifying note content
- **AND** SHALL preserve existing content exactly
- **AND** SHALL replace frontmatter block entirely

#### Scenario: Wikilink detection
- **WHEN** system reads note content
- **THEN** parser SHALL automatically detect wikilinks in content
- **AND** wikilink information SHALL be included in metadata response
- **AND** agents SHALL modify wikilinks by updating note content
- **AND** backlink tracking SHALL be handled by storage layer

### Requirement: Permission and Safety Model
The system SHALL implement a permission model that ensures users maintain control over their kiln content.

#### Scenario: Default read access within scope
- **WHEN** agent accesses notes within current working directory
- **THEN** access SHALL be granted without additional prompts
- **AND** scope SHALL be limited to directory tree where CLI was invoked

#### Scenario: Write permission prompts
- **WHEN** agent attempts to modify kiln content
- **THEN** system SHALL prompt user with clear description of changes
- **AND** user SHALL have option to approve, deny, or enable auto-approve
- **AND** approval settings SHALL persist for session duration

#### Scenario: Directory scope expansion
- **WHEN** agent requests access outside current directory
- **THEN** system SHALL require explicit user approval
- **AND** approval SHALL be remembered for session
- **AND** system SHALL provide context about requested access

### Requirement: Administrative and System Tools
The system SHALL provide tools for kiln information and status reporting.

#### Scenario: Kiln information and statistics
- **WHEN** agent calls `get_kiln_info` with optional detailed flag
- **THEN** system SHALL return kiln root URIs for MCP resource access
- **AND** SHALL return basic statistics (note count, total size, word count)
- **AND** IF detailed flag set, SHALL include indexing status and health metrics
- **AND** response SHALL be fast enough for frequent polling

### Requirement: Agent Integration and Discovery
The system SHALL provide standardized interfaces for agent tool discovery and usage.

#### Scenario: Tool discovery on startup
- **WHEN** ACP agent initializes connection
- **THEN** system SHALL provide list of available tools with descriptions
- **AND** each tool SHALL include parameter schemas and usage examples
- **AND** tools SHALL be categorized by function (access, manipulate, admin)

#### Scenario: Structured tool results
- **WHEN** any tool executes
- **THEN** results SHALL be returned in structured JSON format
- **AND** results SHALL include success/failure status and error information
- **AND** results SHALL be suitable for automated agent processing

#### Scenario: Error handling and timeouts
- **WHEN** tool operations encounter errors or exceed time limits
- **THEN** system SHALL provide clear error messages and recovery suggestions
- **AND** timeouts SHALL prevent agent blocking
- **AND** partial results SHALL be returned when possible

## MODIFIED Requirements

None - This is a greenfield implementation using rmcp library patterns.

## REMOVED Requirements

### Requirement: Note Name Resolution System
**Reason**: After analysis of use cases and MCP patterns, filesystem paths provide better integration with existing tools (shell, MCP resources, file operations). Wikilink resolution remains internal to parser for backlink tracking.

**Decision**: Tools use relative filesystem paths from kiln root. User organizational patterns (folders, naming) are respected rather than abstracted.