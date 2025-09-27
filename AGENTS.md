# 🤖 Crucible AI Agents Documentation

> AI agents working on the Crucible codebase

This documentation is for AI agents (like Claude, GPT, etc.) that are working on the Crucible project. It provides context, guidelines, and tools to help AI assistants understand and contribute to the codebase effectively.

## Purpose

The `.agents` folder contains:

- **Commands**: Common operations AI agents can perform on the codebase
- **Hooks**: Event-driven triggers for automated code maintenance
- **Context**: Shared knowledge about the project structure and conventions
- **Tools**: Reusable utilities for code analysis and generation

## Directory Structure

```
.agents/
├── commands/           # Agent commands
│   ├── search.rn      # Search codebase
│   ├── generate.rn    # Generate code
│   ├── refactor.rn    # Refactor code
│   └── test.rn        # Generate tests
├── hooks/             # Event hooks
│   ├── on-file-create.rn   # Triggered on file creation
│   ├── on-file-update.rn   # Triggered on file updates
│   └── on-commit.rn        # Triggered on git commits
├── tools/             # Reusable tools
│   ├── analysis.rn    # Code analysis utilities
│   ├── docs.rn        # Documentation tools
│   └── testing.rn     # Testing utilities
├── contexts/          # Context definitions
│   ├── project.rn     # Project context
│   └── codebase.rn    # Codebase context
└── config/            # Agent configuration
    ├── mcp.json       # MCP server config
    └── agents.json    # Agent definitions
```

## Commands

### Code Search Command (`commands/search.rn`)
```rune
// Search the codebase for specific patterns or functionality
pub fn search_code(query: String, file_types: Vec<String>) -> Vec<CodeMatch> {
    // Search through source files for patterns
}

pub struct CodeMatch {
    pub file_path: String,
    pub line_number: i64,
    pub content: String,
    pub context: String,
}
```

### Code Generation Command (`commands/generate.rn`)
```rune
// Generate new code based on specifications
pub fn generate_code(template: String, context: Map<String, Any>) -> GeneratedCode {
    // Generate code following project conventions
}

pub struct GeneratedCode {
    pub content: String,
    pub file_path: String,
    pub dependencies: Vec<String>,
}
```

### Refactor Command (`commands/refactor.rn`)
```rune
// Refactor existing code while maintaining functionality
pub fn refactor(file_path: String, changes: Map<String, Any>) -> RefactorResult {
    // Apply refactoring changes
}
```

### Test Generation Command (`commands/test.rn`)
```rune
// Generate tests for existing code
pub fn generate_tests(file_path: String, test_type: String) -> TestSuite {
    // Generate comprehensive test coverage
}
```

## Hooks

### On File Create Hook (`hooks/on-file-create.rn`)
```rune
// Automatically process new files
pub fn on_file_create(file_path: String, content: String) -> Vec<Action> {
    // Add to git tracking
    // Run linting
    // Generate documentation
    // Update imports
}
```

### On File Update Hook (`hooks/on-file-update.rn`)
```rune
// Process file changes
pub fn on_file_update(file_path: String, changes: Map<String, Any>) -> Vec<Action> {
    // Update related files
    // Refresh dependencies
    // Run tests
    // Update documentation
}
```

### On Commit Hook (`hooks/on-commit.rn`)
```rune
// Process git commits
pub fn on_commit(commit_hash: String, files: Vec<String>) -> Vec<Action> {
    // Update changelog
    // Run full test suite
    // Check for breaking changes
    // Update version numbers
}
```

## Tools

### Code Analysis Tool (`tools/analysis.rn`)
```rune
// Analyze code patterns and dependencies
pub fn analyze_dependencies(file_path: String) -> DependencyGraph {
    // Parse imports and dependencies
}

pub fn find_code_smells(file_path: String) -> Vec<CodeSmell> {
    // Detect potential issues
}
```

### Documentation Tool (`tools/docs.rn`)
```rune
// Generate and maintain documentation
pub fn generate_docs(file_path: String) -> Documentation {
    // Extract comments and generate docs
}

pub fn update_readme(changes: Map<String, Any>) -> String {
    // Update README based on changes
}
```

### Testing Tool (`tools/testing.rn`)
```rune
// Generate and run tests
pub fn generate_test_cases(function: String) -> Vec<TestCase> {
    // Generate comprehensive test cases
}

pub fn run_tests(file_path: String) -> TestResults {
    // Execute test suite
}
```

## Context Management

### Project Context (`contexts/project.rn`)
```rune
// Project-wide context and conventions
pub struct ProjectContext {
    pub name: String,
    pub tech_stack: Vec<String>,
    pub conventions: Map<String, Any>,
    pub file_structure: Map<String, String>,
    pub dependencies: Vec<Dependency>,
}
```

### Codebase Context (`contexts/codebase.rn`)
```rune
// Current codebase state
pub struct CodebaseContext {
    pub files: Vec<FileInfo>,
    pub dependencies: DependencyGraph,
    pub test_coverage: Map<String, f64>,
    pub recent_changes: Vec<Change>,
}
```

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

### Basic Agent Interaction
```rune
// Search for code patterns
let results = search_code("async function", vec!["*.rs", "*.ts"]);

// Generate new component following conventions
let component = generate_code("svelte_component", context);

// Analyze dependencies
let deps = analyze_dependencies("src/main.rs");
```

### Hook Implementation
```rune
// Auto-format and lint when creating files
pub fn on_file_create(file_path: String, content: String) -> Vec<Action> {
    let mut actions = Vec::new();
    
    if file_path.ends_with(".rs") {
        actions.push(Action::RunCommand {
            command: "cargo fmt".to_string(),
            args: vec![file_path.clone()]
        });
        actions.push(Action::RunCommand {
            command: "cargo clippy".to_string(),
            args: vec![file_path]
        });
    }
    
    actions
}
```

## Development

### Adding New Commands
1. Create a new `.rn` file in `commands/`
2. Implement the command function following project conventions
3. Register in `config/mcp.json`
4. Test with AI agent interactions

### Adding New Hooks
1. Create a new `.rn` file in `hooks/`
2. Implement the hook function for codebase events
3. Register in `config/agents.json`
4. Test with relevant file/git events

### Adding New Tools
1. Create a new `.rn` file in `tools/`
2. Implement utility functions for code analysis
3. Import in commands/hooks as needed
4. Document usage patterns

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

---

*This agent system is designed to help AI assistants work effectively with the Crucible codebase. Use these tools and conventions to maintain code quality and project consistency.*