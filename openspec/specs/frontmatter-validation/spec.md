# frontmatter-validation Specification

## Purpose
TBD - created by archiving change enhance-parsing-and-markdown-syntax. Update Purpose after archive.
## Requirements
### Requirement: Template-Based Frontmatter Processing
The system SHALL provide flexible frontmatter processing with template-based guidance and soft validation that maintains Obsidian compatibility.

#### Scenario: Template-based parsing
- **WHEN** parsing document frontmatter
- **THEN** the system SHALL extract all YAML frontmatter content
- **AND** apply template-based field recognition
- **AND** provide gentle suggestions for common field patterns
- **AND** preserve all user-defined fields without restrictions

#### Scenario: Common field recognition
- **WHEN** processing frontmatter
- **THEN** the system SHALL recognize standard fields (title, date, tags, author)
- **AND** identify Crucible-specific fields (agent, relationships, metadata)
- **AND** suggest field completion for common patterns
- **AND** maintain compatibility with Obsidian field conventions

#### Scenario: Soft validation guidance
- **WHEN** providing validation feedback
- **THEN** the system SHALL offer suggestions rather than enforce rules
- **AND** highlight potential typos in common field names
- **AND** provide template examples for document types
- **AND** allow users to override or ignore suggestions

### Requirement: Frontmatter Template System
The system SHALL support user-defined templates for common document types with optional field guidance and auto-completion.

#### Scenario: Template definition and discovery
- **WHEN** managing document templates
- **THEN** the system SHALL support user-defined template files
- **AND** provide template discovery in configurable directories
- **AND** allow template inheritance and composition
- **AND** support template metadata and categorization

#### Scenario: Template application and guidance
- **WHEN** creating or editing documents
- **THEN** the system SHALL suggest appropriate templates based on content
- **AND** provide field completion from template definitions
- **AND** populate default values for optional fields
- **AND** maintain template-document associations

#### Scenario: Template evolution and maintenance
- **WHEN** updating template definitions
- **THEN** the system SHALL track template usage and relationships
- **AND** provide template migration guidance
- **AND** support template versioning and rollback
- **AND** maintain backward compatibility with existing documents

### Requirement: Frontmatter Error Detection and Assistance
The system SHALL provide helpful error detection for syntax issues while maintaining maximum flexibility for content structure.

#### Scenario: YAML syntax validation
- **WHEN** parsing frontmatter YAML
- **THEN** the system SHALL detect YAML syntax errors
- **AND** provide specific error location and description
- **AND** suggest corrections for common YAML mistakes
- **AND** maintain frontmatter content when possible during error recovery

#### Scenario: Field type assistance
- **WHEN** analyzing frontmatter fields
- **THEN** the system SHALL identify potential type mismatches
- **AND** suggest format corrections for dates and structured data
- **AND** provide examples for complex field types
- **AND** allow users to accept or ignore type suggestions

#### Scenario: Relationship validation
- **WHEN** processing document relationships
- **THEN** the system SHALL detect broken wikilinks and references
- **AND** identify circular transclusion references
- **AND** provide relationship repair suggestions
- **AND** maintain relationship integrity metrics

### Requirement: Flexible Metadata Extraction
The system SHALL extract and index frontmatter metadata while supporting arbitrary user-defined fields and structures.

#### Scenario: Comprehensive metadata extraction
- **WHEN** processing document frontmatter
- **THEN** the system SHALL extract all YAML fields regardless of structure
- **AND** preserve nested objects and arrays
- **AND** maintain data types and formatting
- **AND** support custom field parsing strategies

#### Scenario: Metadata indexing and search
- **WHEN** building document indexes
- **THEN** the system SHALL index common frontmatter fields
- **AND** support custom field indexing configurations
- **AND** provide metadata-based search capabilities
- **AND** maintain field frequency and usage statistics

#### Scenario: Custom field support
- **WHEN** processing user-defined fields
- **THEN** the system SHALL support arbitrary field names and structures
- **AND** maintain field ordering and formatting
- **AND** provide field usage analytics
- **AND** support field-based document organization

