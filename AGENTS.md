# 🤖 Crucible AI Agents Documentation

> AI agents working on the Crucible codebase through linked thinking

This documentation is for AI agents (like Claude, GPT, etc.) that are working on the Crucible project. It provides context, guidelines, and tools to help AI assistants understand and contribute to the codebase effectively through **linked thinking** - the seamless connection and evolution of ideas across time and context.

## 🧠 Linked Thinking in Agent Workflows

Crucible promotes **linked thinking** through:
- **Contextual Connections**: Agents maintain awareness of related concepts and dependencies
- **Evolutionary Development**: Ideas and code evolve through iterative refinement
- **Cross-Reference Awareness**: Agents understand how changes affect related components
- **Temporal Context**: Agents consider the history and future implications of changes

## Purpose

The `.agents` folder contains:

- **Commands**: Common operations AI agents can perform on the codebase
- **Hooks**: Event-driven triggers for automated code maintenance
- **Context**: Shared knowledge about the project structure and conventions
- **Tools**: Reusable utilities for code analysis and generation

## Directory Structure

```
.agents/
├── tools/             # AI agent tools and guides
│   ├── codebase-analysis.md    # Code analysis patterns
│   └── development-workflows.md # Development workflows
├── contexts/          # Project context for AI agents
│   └── project-context.md      # Essential project information
├── workflows/         # Detailed workflow guides
│   └── feature-development.md  # Feature development process
└── config/            # Agent configuration
    ├── mcp.json       # MCP server config
    └── agents.json    # Agent definitions

specs/                 # Technical specifications organized by tech stack
├── rust-core/         # Core Rust business logic specs
│   └── sprint-1/      # Sprint-based implementation phases
├── tauri-backend/     # Tauri desktop backend specs
│   └── sprint-1/
├── svelte-frontend/   # Svelte UI component specs
│   └── sprint-1/
├── database/          # Database and persistence specs
│   └── sprint-2/
├── plugin-system/     # Plugin and extensibility specs
│   └── sprint-3/
├── mcp-integration/   # MCP and agent integration specs
│   └── sprint-4/
├── code-generation/   # Agent code generation specifications
│   ├── agent-specifications.md
│   └── workflow-specifications.md
├── data-specs/        # Data schemas and type definitions
│   ├── document-schema.json
│   ├── embeddings-schema.json
│   ├── canvas-schema.json
│   ├── document-types.ts
│   └── embeddings-types.ts
├── sprint-{1,2,3,4}/  # Sprint-based roadmap phases
└── GAP_ANALYSIS_COMPREHENSIVE.md  # Implementation gap analysis
```

## Available Tools

### Codebase Analysis (`tools/codebase-analysis.md`)
- Search patterns for Rust, TypeScript, and Svelte code
- Common analysis tasks and workflows
- File type patterns and conventions
- Understanding project structure

### Development Workflows (`tools/development-workflows.md`)
- Code generation workflows
- Testing strategies
- Documentation processes
- Refactoring approaches
- Debugging techniques

### Project Context (`contexts/project-context.md`)
- Essential project information
- Tech stack overview
- Architecture patterns
- Development guidelines
- Common code patterns

### Feature Development (`workflows/feature-development.md`)
- Step-by-step feature development process
- Planning and implementation phases
- Testing and documentation workflows
- Quality checklists and common pitfalls

## Technical Specifications

### Tech Stack Specifications (`specs/`)
- **Rust Core** (`rust-core/`): Document CRDT operations, business logic
- **Tauri Backend** (`tauri-backend/`): Desktop app commands and IPC
- **Svelte Frontend** (`svelte-frontend/`): UI components and stores
- **Database Layer** (`database/`): PGlite schemas and vector operations
- **Plugin System** (`plugin-system/`): Rune runtime and extensibility
- **MCP Integration** (`mcp-integration/`): AI agent tools and protocols
- **Code Generation** (`code-generation/`): Agent specifications for automated code generation
- **A2A Protocol** (`sprint-4/a2a-protocol-*.md`): Agent-to-agent communication protocols

### Data Specifications (`specs/data-specs/`)
- **JSON Schemas**: API validation and documentation
- **Zod Types**: TypeScript runtime validation
- **Document Schema**: Hierarchical node structure
- **Embeddings Schema**: Vector search specifications
- **Canvas Schema**: Spatial positioning and connections

### Sprint Phases (`specs/sprint-{1,2,3,4}/`)
- **Sprint 1**: Foundation (CRDT + Basic UI)
- **Sprint 2**: Persistence & UI Polish
- **Sprint 3**: Canvas & Properties
- **Sprint 4**: Intelligence Layer

## MCP Integration

### MCP Server Config (`config/mcp.json`)
```json
{
  "name": "crucible-dev-mcp",
  "version": "0.1.0",
  "description": "Crucible development MCP server for AI agents",
  "tools": [
    {
      "name": "search_codebase",
      "description": "Search the codebase for patterns",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": {"type": "string"},
          "file_types": {"type": "array", "items": {"type": "string"}}
        }
      }
    },
    {
      "name": "generate_code",
      "description": "Generate code following project conventions",
      "inputSchema": {
        "type": "object",
        "properties": {
          "template": {"type": "string"},
          "context": {"type": "object"}
        }
      }
    }
  ]
}
```

### Agent Definitions (`config/agents.json`)
```json
{
  "agents": [
    {
      "name": "code_generator",
      "description": "Generates code following project patterns",
      "commands": ["generate", "refactor"],
      "hooks": ["on-file-create", "on-file-update"],
      "tools": ["analysis", "docs"]
    },
    {
      "name": "test_automation",
      "description": "Automates test generation and maintenance",
      "commands": ["test", "analyze"],
      "hooks": ["on-commit", "on-file-update"],
      "tools": ["testing", "analysis"]
    },
    {
      "name": "documentation_bot",
      "description": "Maintains and updates documentation",
      "commands": ["docs", "update"],
      "hooks": ["on-file-create", "on-commit"],
      "tools": ["docs", "analysis"]
    }
  ]
}
```

## Usage Examples

### Code Analysis
- Use `tools/codebase-analysis.md` to understand search patterns
- Follow `tools/development-workflows.md` for common tasks
- Reference `contexts/project-context.md` for project conventions

### Feature Development
- Follow the step-by-step process in `workflows/feature-development.md`
- Use the quality checklist to ensure completeness
- Reference development workflows for specific tasks

### Code Generation
- Use MCP tools like `search_codebase` to understand existing patterns
- Follow Crucible conventions from project context
- Generate comprehensive tests and documentation
- Reference [Agent Code Generation Specs](./specs/code-generation/) for automated code generation
- Use [A2A Protocol Integration](./specs/sprint-4/a2a-protocol-feature.md) for agent-to-agent communication
- Apply linked thinking principles to maintain contextual awareness across generated code

## Development

### Adding New Tools
1. Create a new markdown file in `tools/`
2. Document patterns, workflows, or analysis techniques
3. Include practical examples and use cases
4. Reference in `AGENTS.md` and agent configurations

### Adding New Workflows
1. Create a new markdown file in `workflows/`
2. Document step-by-step processes
3. Include checklists and quality gates
4. Reference from agent configurations

### Updating Agent Configurations
1. Add new capabilities to `config/mcp.json`
2. Update agent definitions in `config/agents.json`
3. Reference new tools and workflows
4. Test with AI agent interactions

## Guidelines for AI Agents

### Code Style
- Follow Rust naming conventions (snake_case for functions/variables)
- Use TypeScript/JavaScript conventions for frontend code
- Maintain consistent error handling patterns
- Add comprehensive documentation

### Project Structure
- Keep related functionality in appropriate crates/packages
- Maintain clear separation between core, UI, and plugin code
- Follow the established directory structure
- Use workspace dependencies appropriately

### Testing
- Generate tests for new functionality
- Maintain test coverage above 80%
- Include both unit and integration tests
- Test edge cases and error conditions

### Linked Thinking Principles
- **Contextual Awareness**: Always consider how changes affect related components
- **Evolutionary Development**: Build upon existing patterns and conventions
- **Cross-Reference Maintenance**: Update related documentation and specifications
- **Temporal Consistency**: Ensure changes align with project history and future plans

## Key Resources for Agents

- **[Gap Analysis](./specs/GAP_ANALYSIS_COMPREHENSIVE.md)**: Comprehensive analysis of implementation gaps and context engineering needs
- **[Agent Specifications](./specs/code-generation/agent-specifications.md)**: Detailed specifications for AI agent code generation
- **[Workflow Specifications](./specs/code-generation/workflow-specifications.md)**: GitHub Actions-style workflows for agent operations
- **[A2A Protocol](./specs/sprint-4/a2a-protocol-feature.md)**: Agent-to-agent communication protocols

---

*This agent system is designed to help AI assistants work effectively with the Crucible codebase through linked thinking. Use these tools and conventions to maintain code quality, project consistency, and contextual awareness across all development activities.*