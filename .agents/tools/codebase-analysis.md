# Codebase Analysis Tools

Tools and patterns for AI agents to analyze the Crucible codebase.

## Code Search Patterns

### Rust Code Analysis
- **Function definitions**: `fn\s+\w+\s*\(`
- **Struct definitions**: `struct\s+\w+`
- **Module declarations**: `pub\s+mod\s+\w+`
- **Error types**: `#\[derive\(.*Error.*\)\]`
- **Async functions**: `async\s+fn\s+\w+`

### TypeScript/JavaScript Analysis
- **Component definitions**: `export\s+(default\s+)?function\s+\w+`
- **Svelte components**: `<script\s+lang="ts">`
- **Type definitions**: `interface\s+\w+|type\s+\w+`
- **Hook usage**: `use[A-Z]\w+`

### Configuration Files
- **Cargo.toml**: Dependencies, workspace members
- **package.json**: Scripts, dependencies, workspaces
- **vite.config.ts**: Build configuration
- **svelte.config.js**: Svelte configuration

## Common Tasks

### Finding Related Code
1. Search for function/type definitions
2. Find usage patterns across files
3. Identify dependency relationships
4. Locate test files

### Understanding Project Structure
1. Check workspace configuration
2. Identify crate/package boundaries
3. Understand build processes
4. Locate entry points

### Code Quality Checks
1. Look for TODO/FIXME comments
2. Check for error handling patterns
3. Verify documentation coverage
4. Identify potential refactoring opportunities

## File Type Patterns

### Rust Files (`.rs`)
- Focus on: `lib.rs`, `main.rs`, `mod.rs`
- Look for: Error types, public APIs, tests
- Patterns: `pub use`, `#[cfg(test)]`, `Result<T>`

### TypeScript Files (`.ts`, `.tsx`)
- Focus on: Type definitions, utility functions
- Look for: Exports, interfaces, type guards
- Patterns: `export`, `interface`, `type`

### Svelte Files (`.svelte`)
- Focus on: Component structure, props, stores
- Look for: `<script>`, `export let`, `$:`
- Patterns: Component lifecycle, reactivity

### Configuration Files
- Focus on: Dependencies, build settings
- Look for: Version constraints, feature flags
- Patterns: Workspace definitions, script commands
