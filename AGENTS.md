# 🤖 AI Agent Guide for Crucible

> Instructions for AI agents (Claude, Codex, etc.) working on the Crucible codebase

This file provides essential information for AI agents to understand and contribute to the Crucible knowledge management system effectively.

## 🎯 Project Overview

**Crucible** is a knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. It promotes **linked thinking** - the seamless connection and evolution of ideas across time and context.

## 🏗️ Architecture

### Core Components
- **Rust Core** (`crates/crucible-core/`): Business logic, CRDT operations, document management
- **Tauri Backend** (`crates/crucible-tauri/`): Desktop app commands, IPC, system integration
- **Svelte Frontend** (`packages/web/`): UI components, user interactions, real-time updates
- **MCP Integration** (`crates/crucible-mcp/`): AI agent tools and protocol handling

### Key Technologies
- **Rust**: Core performance-critical components
- **Tauri**: Desktop application framework
- **Svelte 5**: Modern reactive frontend
- **Yjs**: CRDT for real-time collaboration
- **DuckDB**: Embedded database with vector search
- **Rune**: Plugin scripting language

## 📁 Project Structure

```
crucible/
├── crates/
│   ├── crucible-core/           # Core Rust business logic
│   ├── crucible-tauri/          # Tauri desktop backend
│   └── crucible-mcp/            # MCP server for AI integration
├── packages/
│   └── web/                     # Svelte frontend application
├── docs/                        # Human documentation
│   └── ARCHITECTURE.md          # System architecture details
└── CLAUDE.md                    # This file - AI agent instructions
```

## 🔧 Development Guidelines

### Code Style
- **Rust**: Use `snake_case` for functions/variables, `PascalCase` for types
- **TypeScript/Svelte**: Use `camelCase` for variables, `PascalCase` for components
- **Error Handling**: Use `Result<T, E>` in Rust, proper error boundaries in Svelte
- **Documentation**: Add comments for complex logic, maintain clear commit messages

### File Organization
- Keep related functionality in appropriate crates/packages
- Use clear, descriptive file and function names
- Follow established patterns in existing code
- Maintain separation between core logic, UI, and external integrations

### Testing
- Write unit tests for core functionality
- Include integration tests for component interactions
- Test error conditions and edge cases
- Use descriptive test names that explain the scenario

## 🚀 Common Tasks

### Adding New Features
1. Identify the appropriate location (core, backend, frontend)
2. Follow existing patterns and conventions
3. Add comprehensive tests
4. Update relevant documentation
5. Consider performance implications

### Fixing Bugs
1. Reproduce the issue with minimal test case
2. Identify root cause through debugging
3. Implement fix with proper error handling
4. Add tests to prevent regression
5. Verify fix doesn't break existing functionality

### Code Review
1. Check for consistency with project patterns
2. Verify error handling is comprehensive
3. Ensure tests provide adequate coverage
4. Confirm documentation is updated if needed
5. Consider performance and security implications

## 🔍 Code Analysis Patterns

### Search Commands
- Use `grep -r "pattern" --include="*.rs"` for Rust code
- Use `grep -r "pattern" --include="*.ts" --include="*.svelte"` for frontend code
- Search in both `crates/` and `packages/` directories

### Understanding Code Flow
1. Start from entry points (main.rs, App.svelte)
2. Follow function calls and data flow
3. Look for error handling patterns
4. Check tests for expected behavior examples

## ⚡ Performance Considerations

### Rust Code
- Use efficient data structures (HashMap, Vec)
- Minimize allocations in hot paths
- Leverage Rust's zero-cost abstractions
- Consider async/await for I/O operations

### Frontend Code
- Use Svelte's reactivity efficiently
- Implement virtual scrolling for large lists
- Optimize bundle size through tree-shaking
- Cache expensive computations

## 🔐 Security Guidelines

- Validate all user inputs
- Use parameterized queries for database operations
- Sanitize data before rendering
- Follow principle of least privilege for plugin system
- Keep dependencies updated and review security advisories

## 📋 Quality Checklist

Before submitting changes:
- [ ] Code follows project style guidelines
- [ ] Tests pass and provide good coverage
- [ ] Error handling is comprehensive
- [ ] Documentation is updated if needed
- [ ] Performance impact is considered
- [ ] Security implications are reviewed
- [ ] No console.log or debug code left in
- [ ] Commit messages are clear and descriptive

## 🤖 Agent-Specific Instructions

### For Claude Code
- Use the Task tool for complex multi-step operations
- Leverage TodoWrite for tracking progress
- Use appropriate tools (Read, Edit, Bash, Grep) based on operation type
- Follow conventional commit format when creating commits

### For Codex/GitHub Copilot
- Focus on specific code generation tasks
- Follow existing patterns and conventions
- Generate code that is idiomatic for the target language
- Include appropriate error handling and documentation

### General Guidelines
- Always consider the broader context of changes
- Maintain consistency with existing codebase
- Ask for clarification if requirements are ambiguous
- Prioritize correctness over cleverness

## 🔗 Key Resources

- **[Architecture Documentation](./docs/ARCHITECTURE.md)**: Detailed system architecture
- **[Crucible README](./README.md)**: Project overview and getting started
- **[Rust Documentation](https://doc.rust-lang.org/)**: Rust language reference
- **[Svelte Documentation](https://svelte.dev/docs)**: Svelte framework reference
- **[Tauri Documentation](https://tauri.app/)**: Tauri framework reference

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*