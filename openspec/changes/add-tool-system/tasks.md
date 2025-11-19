## 1. Foundation and Core Architecture
- [ ] 1.1 Define tool system architecture and interfaces
- [ ] 1.2 Design kiln-agnostic note reference system
- [ ] 1.3 Implement permission model foundation
- [ ] 1.4 Create tool registration and discovery system
- [ ] 1.5 Define structured JSON result formats

## 2. Knowledge Access Tools
- [ ] 2.1 Implement `read_note` tool (note name/wikilink access)
- [ ] 2.2 Implement `list_notes` tool (directory navigation)
- [ ] 2.3 Implement `search_notes` tool (semantic search integration)
- [ ] 2.4 Implement `get_note_metadata` tool (tags, properties, links)
- [ ] 2.5 Implement `find_related_notes` tool (backlink/forwardlink discovery)
- [ ] 2.6 Add error handling and validation for note access

## 3. Knowledge Manipulation Tools
- [ ] 3.1 Implement `create_note` tool with permission prompts
- [ ] 3.2 Implement `update_note` tool with permission prompts
- [ ] 3.3 Implement `delete_note` tool with permission prompts
- [ ] 3.4 Implement `add_tag` and `remove_tag` tools
- [ ] 3.5 Implement `create_wikilink` and `remove_wikilink` tools
- [ ] 3.6 Add batch operation support for multiple notes

## 4. Metadata and Administrative Tools
- [ ] 4.1 Implement `list_tags` tool (hierarchical tag browsing)
- [ ] 4.2 Implement `get_kiln_stats` tool (note counts, indexing status)
- [ ] 4.3 Implement `rebuild_index` tool (search index management)
- [ ] 4.4 Implement `export_notes` tool (backup/migration support)
- [ ] 4.5 Implement `validate_kiln` tool (integrity checks)

## 5. Permission and Security System
- [ ] 5.1 Implement directory scope validation
- [ ] 5.2 Create user permission prompts and approval system
- [ ] 5.3 Implement auto-approve toggles and settings persistence
- [ ] 5.4 Add permission audit logging
- [ ] 5.5 Create permission management CLI commands

## 6. ACP Integration Layer
- [ ] 6.1 Create ACP tool bridge (agent calls → native tools)
- [ ] 6.2 Implement tool registration for ACP agent startup
- [ ] 6.3 Add permission flow integration with ACP sessions
- [ ] 6.4 Create tool discovery interface for agents
- [ ] 6.5 Add error handling and timeout management

## 7. Backend Implementations
- [ ] 7.1 Implement file-based storage tool backends
- [ ] 7.2 Implement SurrealDB storage tool backends
- [ ] 7.3 Add storage backend abstraction layer
- [ ] 7.4 Implement note reference resolution (name → path/storage)
- [ ] 7.5 Add caching layer for frequently accessed notes

## 8. Testing and Validation
- [ ] 8.1 Write unit tests for all tool implementations
- [ ] 8.2 Create integration tests with ACP client
- [ ] 8.3 Add permission system security tests
- [ ] 8.4 Test kiln-agnostic behavior across storage backends
- [ ] 8.5 Performance testing for tool operations
- [ ] 8.6 End-to-end testing with agent workflows

## 9. Documentation and Examples
- [ ] 9.1 Create tool API documentation
- [ ] 9.2 Write permission system guide
- [ ] 9.3 Create agent integration examples
- [ ] 9.4 Add troubleshooting guide for common issues
- [ ] 9.5 Document kiln reference patterns and conventions

## 10. Quality Assurance
- [ ] 10.1 Code review and style validation
- [ ] 10.2 Security audit of permission system
- [ ] 10.3 Performance profiling and optimization
- [ ] 10.4 Integration testing with existing CLI commands
- [ ] 10.5 User acceptance testing with real kiln data