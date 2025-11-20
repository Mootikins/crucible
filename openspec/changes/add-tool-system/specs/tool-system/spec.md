## ADDED Requirements

### Requirement: Kiln-Agnostic Tool System ✅ IMPLEMENTED
The system SHALL provide a comprehensive tool system that enables agents to access and manipulate knowledge within the kiln using note names and wikilinks, independent of underlying storage implementation.

**Implementation Status**:
- **Architecture**: MCP-compatible using rmcp 0.9.0 with `#[tool_router]` and `#[tool]` macros
- **Tools**: 11 focused tools consolidated from 25+ previous tools
- **Categories**:
  - **NoteTools** (5): create_note, read_note, update_note, delete_note, list_notes
  - **SearchTools** (4): semantic_search, text_search, metadata_search, tag_search
  - **KilnTools** (2): get_roots, get_stats
- **Validation**: Parameters<T> wrapper with schemars::JsonSchema for automatic schema generation
- **Dependency Injection**: Proper dependency injection via core traits

#### Scenario: Agent reads note by name
- **WHEN** agent requests note content using note name or wikilink
- **THEN** system SHALL locate and return note content regardless of storage backend
- **AND** note resolution SHALL follow Obsidian-style reference patterns
- **AND** system SHALL handle name conflicts and disambiguation

#### Scenario: Storage backend abstraction
- **WHEN** tools access notes
- **THEN** storage implementation SHALL be transparent to the agent
- **AND** tools SHALL work with file-based or database storage
- **AND** future storage backends SHALL work without tool changes

### Requirement: Knowledge Access Tools
The system SHALL provide tools for reading, listing, and discovering kiln content using natural note references.

#### Scenario: Read note content
- **WHEN** agent calls `read_note` with note name or wikilink
- **THEN** system SHALL return full note content in structured format
- **AND** metadata SHALL include title, tags, creation date, and backlinks
- **AND** content SHALL be suitable for agent processing and analysis

#### Scenario: List notes in context
- **WHEN** agent calls `list_notes` with optional directory context
- **THEN** system SHALL return available notes within scope
- **AND** results SHALL include note names, titles, and preview snippets
- **AND** results SHALL respect permission boundaries

#### Scenario: Semantic search discovery
- **WHEN** agent calls `semantic_search` with query and optional filters
- **THEN** system SHALL return ranked results based on semantic similarity
- **AND** results SHALL include relevance scores and content snippets
- **AND** search SHALL work across entire accessible kiln scope
- **AND** agent SHALL discover notes beyond auto-enriched context

#### Scenario: Find related knowledge
- **WHEN** agent calls `find_related_notes` with note reference
- **THEN** system SHALL return notes linked by wikilinks or semantic similarity
- **AND** results SHALL include bidirectional link relationships
- **AND** discovery SHALL surface potentially relevant but unconnected notes

### Requirement: Knowledge Manipulation Tools
The system SHALL provide tools for creating, updating, and deleting notes with appropriate permission controls.

#### Scenario: Create new note with permission
- **WHEN** agent calls `create_note` with content and metadata
- **THEN** system SHALL prompt user for approval before creation
- **AND** upon approval SHALL create note with proper file naming
- **AND** SHALL return success confirmation with note reference

#### Scenario: Update existing note
- **WHEN** agent calls `update_note` with note reference and changes
- **THEN** system SHALL prompt user for approval before modification
- **AND** SHALL preserve note history and metadata
- **AND** SHALL return confirmation of changes made

#### Scenario: Delete note safely
- **WHEN** agent calls `delete_note` with note reference
- **THEN** system SHALL require explicit user confirmation
- **AND** SHALL warn about incoming links before deletion
- **AND** SHALL provide backup option before deletion

### Requirement: Metadata and Property Management
The system SHALL provide tools for managing note metadata, tags, and relationships.

#### Scenario: Tag management
- **WHEN** agent calls `add_tag` or `remove_tag` with note and tag
- **THEN** system SHALL update tag assignments with permission prompts
- **AND** SHALL maintain hierarchical tag relationships
- **AND** SHALL update search indexes appropriately

#### Scenario: Wikilink management
- **WHEN** agent creates or removes wikilinks between notes
- **THEN** system SHALL validate target note existence
- **AND** SHALL update bidirectional link tracking
- **AND** SHALL maintain link graph integrity

#### Scenario: Metadata queries
- **WHEN** agent calls `get_note_metadata` with note reference
- **THEN** system SHALL return comprehensive metadata
- **AND** SHALL include tags, links, creation dates, and custom properties
- **AND** SHALL provide statistical information about note connections

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
The system SHALL provide tools for kiln management, indexing, and system status.

#### Scenario: Kiln statistics and health
- **WHEN** agent calls `get_kiln_stats`
- **THEN** system SHALL return note counts, indexing status, and storage information
- **AND** SHALL identify any issues requiring attention
- **AND** SHALL provide performance metrics

#### Scenario: Index management
- **WHEN** agent calls `rebuild_index` or similar administrative functions
- **THEN** system SHALL require explicit permission for disruptive operations
- **AND** SHALL provide progress feedback during operations
- **AND** SHALL validate index integrity after completion

#### Scenario: System validation
- **WHEN** agent calls `validate_kiln`
- **THEN** system SHALL check for data integrity issues
- **AND** SHALL report broken links, orphaned notes, or corruption
- **AND** SHALL suggest corrective actions

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

### Requirement: Tool Executor Integration
The existing `ToolExecutor` trait SHALL be enhanced to support kiln-specific tools and agent integration patterns.

#### Scenario: Native tool execution
- **WHEN** ACP client bridges agent tool calls to native execution
- **THEN** `ToolExecutor` implementations SHALL handle kiln operations
- **AND** execution SHALL maintain security boundaries and permissions
- **AND** results SHALL be formatted for agent consumption

#### Scenario: Tool registration
- **WHEN** system initializes or tools are added
- **THEN** kiln tools SHALL be registered with `ToolExecutor` registry
- **AND** metadata SHALL include tool categories and permission requirements
- **AND** discovery SHALL support both static and dynamic registration patterns

## REMOVED Requirements

### Requirement: File System Tool Dependencies
**Reason**: Agents should not directly access filesystem paths as this creates storage dependencies and violates kiln-agnostic principles.

**Migration**: All file access tools SHALL use note names and wikilinks, with storage backend abstraction handling path resolution.