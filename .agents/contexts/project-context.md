# Project Context

Essential context about the Crucible project for AI agents.

## Project Overview

**Crucible** is a next-generation knowledge management system that combines:
- Infinite zoom interface (like Workflowy)
- Extensibility (like Obsidian)
- AI agent integration
- Real-time collaboration

## Tech Stack

### Backend
- **Rust** with Tauri for desktop app
- **Yrs** for CRDT-based real-time sync
- **PGlite** with pgvector for local database
- **Rune** for plugin scripting

### Frontend
- **Svelte 5** with runes
- **TypeScript** for type safety
- **Vite** for build tooling
- **@xenova/transformers** for AI features

### Development
- **pnpm** for package management
- **GitHub Actions** for CI/CD
- **Cargo** for Rust dependencies

## Architecture

### Core Crates
- `crucible-core`: Core data structures and CRDT logic
- `crucible-tauri`: Desktop application with Tauri
- `crucible-mcp`: MCP server for AI agent integration
- `crucible-plugins`: Rune-based plugin runtime

### Frontend Packages
- `desktop`: Main Svelte application
- `web`: Limited web version
- `shared`: Common TypeScript utilities

## Key Concepts

### Document System
- Hierarchical document structure
- CRDT-based real-time sync
- Property system for metadata
- Canvas mode for spatial organization

### Agent System
- MCP integration for external AI agents
- Plugin system for custom behaviors
- Event-driven architecture
- Sandboxed execution environment

### Data Flow
1. User interactions → Tauri commands
2. Commands → Core logic
3. Core logic → CRDT updates
4. CRDT updates → Frontend state
5. Frontend state → UI updates

## Development Guidelines

### Code Style
- Rust: snake_case, comprehensive error handling
- TypeScript: camelCase, strict typing
- Svelte: Component-based, reactive patterns

### Testing
- Unit tests for all public APIs
- Integration tests for workflows
- E2E tests for user journeys
- Property-based tests for complex logic

### Documentation
- Doc comments for all public functions
- README files for each package
- Architecture diagrams for complex systems
- API documentation with examples

## Common Patterns

### Error Handling
```rust
#[derive(Debug, thiserror::Error)]
pub enum CrucibleError {
    #[error("Document not found: {0}")]
    DocumentNotFound(uuid::Uuid),
    // ...
}

pub type Result<T> = std::result::Result<T, CrucibleError>;
```

### Component Structure
```svelte
<script lang="ts">
  // Component logic
</script>

<!-- Template -->

<style>
  /* Component styles */
</style>
```

### CRDT Operations
```rust
pub fn transact<F, R>(&self, f: F) -> Result<R>
where
    F: FnOnce(&mut Transact) -> Result<R>,
{
    let mut txn = self.doc.transact_mut();
    f(&mut txn)
}
```
