# Rune Attribute Macros: Type-Hinted Tool Examples

This document shows how Rune attribute macros (defined in Rust, used in Rune scripts) require explicit type hints for tool schema generation, since Rune is dynamically typed.

## Core Concept

Unlike Rust procedural macros which can infer types from function signatures, **Rune attribute macros must explicitly specify types** in the attribute parameters because:

1. Rune is dynamically typed - function signatures don't carry type information
2. MCP/agent tools need JSON schemas with explicit types
3. The attribute macro processes this at Rune compile-time to generate schemas

## Pattern: Type Hints in Attribute Metadata

```rust
// In Rust: Define the attribute macro handler
// crates/crucible-rune/src/attribute_macros/tool.rs

pub fn tool_attribute_macro(
    cx: &mut MacroContext,
    stream: &TokenStream,
) -> rune::compile::Result<TokenStream> {
    // Parse attribute args to extract:
    // - desc: tool description
    // - category: optional category
    // - params: hash with type hints and descriptions
    // - returns: optional return type hint

    // Generate:
    // 1. JSON schema from type hints
    // 2. Tool registration code
    // 3. Parameter validation logic

    // Transform the function to register itself
    // ...
}
```

## Example 1: Basic Tool with Type Hints

```rune
// plugins/note_tools.rn
use crucible::tools;

/// In Rune scripts, types are EXPLICITLY declared in the attribute
#[tool(
    desc = "Creates a new note with title and content",
    category = "file",
    tags = ["note", "create"],
    params = {
        "title": {
            "type": "string",
            "description": "Note title"
        },
        "content": {
            "type": "string",
            "description": "Note content"
        }
    },
    returns = "string"
)]
pub fn create_note(title, content) {
    // Function parameters have no type annotations (Rune is dynamically typed)
    // Types are validated at runtime based on the schema above
    let note_id = tools::fs::create_file(`notes/${title}.md`, content);
    `Created note with id: ${note_id}`
}
```

Generated JSON Schema:
```json
{
  "name": "create_note",
  "description": "Creates a new note with title and content",
  "category": "file",
  "tags": ["note", "create"],
  "inputSchema": {
    "type": "object",
    "properties": {
      "title": {
        "type": "string",
        "description": "Note title"
      },
      "content": {
        "type": "string",
        "description": "Note content"
      }
    },
    "required": ["title", "content"]
  },
  "outputSchema": {
    "type": "string"
  }
}
```

## Example 2: Optional Parameters with Defaults

```rune
// plugins/search_tools.rn
use crucible::query;

#[tool(
    desc = "Searches for notes matching a query with pagination",
    category = "search",
    tags = ["search", "notes", "pagination"],
    params = {
        "query": {
            "type": "string",
            "description": "Search query text"
        },
        "limit": {
            "type": "number?",  // ? suffix indicates optional
            "description": "Maximum number of results",
            "default": 10
        },
        "offset": {
            "type": "number?",
            "description": "Number of results to skip",
            "default": 0
        },
        "include_archived": {
            "type": "boolean?",
            "description": "Include archived notes",
            "default": false
        }
    },
    returns = "array"  // Array of note objects
)]
pub fn search_notes(query, limit, offset, include_archived) {
    // Default values are applied before function execution
    // limit, offset, and include_archived will have defaults if not provided

    query::from("notes")
        .search(query)
        .limit(limit)
        .offset(offset)
        .filter(|note| include_archived || !note.archived)
        .collect()
}
```

Generated Schema:
```json
{
  "name": "search_notes",
  "description": "Searches for notes matching a query with pagination",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {"type": "string", "description": "Search query text"},
      "limit": {"type": ["number", "null"], "description": "Maximum number of results", "default": 10},
      "offset": {"type": ["number", "null"], "description": "Number of results to skip", "default": 0},
      "include_archived": {"type": ["boolean", "null"], "description": "Include archived notes", "default": false}
    },
    "required": ["query"]
  },
  "outputSchema": {"type": "array"}
}
```

## Example 3: Complex Nested Types

```rune
// plugins/task_tools.rn
use crucible::entity;
use crucible::query;

#[tool(
    desc = "Creates a task with dependencies and metadata",
    category = "tasks",
    tags = ["tasks", "project", "dependencies"],
    params = {
        "title": {
            "type": "string",
            "description": "Task title"
        },
        "project": {
            "type": "string?",
            "description": "Project name to associate with"
        },
        "priority": {
            "type": "number",
            "description": "Priority level (1-10)",
            "default": 5,
            "minimum": 1,
            "maximum": 10
        },
        "dependencies": {
            "type": "array",
            "description": "Array of task IDs this task depends on",
            "items": {"type": "string"},
            "default": []
        },
        "metadata": {
            "type": "object?",
            "description": "Additional custom metadata",
            "properties": {
                "tags": {"type": "array", "items": {"type": "string"}},
                "estimate_hours": {"type": "number"},
                "assignee": {"type": "string"}
            }
        }
    },
    returns = {
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "created_at": {"type": "string", "format": "date-time"},
            "status": {"type": "string", "enum": ["pending", "active", "completed"]}
        }
    }
)]
pub fn create_task(title, project, priority, dependencies, metadata) {
    // Validate dependencies exist
    for dep_id in dependencies {
        if !entity::exists(dep_id) {
            throw `Dependency task ${dep_id} not found`;
        }
    }

    let task = entity::create("task", #{
        title: title,
        project: project,
        priority: priority,
        dependencies: dependencies,
        metadata: metadata ?? #{},
        status: "pending",
        created_at: chrono::now()
    });

    task
}
```

## Example 4: Graph Query Tool with Type Validation

```rune
// plugins/graph_tools.rn
use crucible::query;

#[tool(
    desc = "Find all entities connected to a starting entity within N hops",
    category = "graph",
    tags = ["graph", "traversal", "relationships"],
    params = {
        "entity_id": {
            "type": "string",
            "description": "Starting entity ID"
        },
        "max_hops": {
            "type": "number",
            "description": "Maximum traversal depth",
            "default": 2,
            "minimum": 1,
            "maximum": 5
        },
        "relation_types": {
            "type": "array?",
            "description": "Filter by specific relation types",
            "items": {
                "type": "string",
                "enum": ["links_to", "references", "depends_on", "child_of"]
            }
        },
        "direction": {
            "type": "string",
            "description": "Traversal direction",
            "enum": ["outbound", "inbound", "both"],
            "default": "both"
        }
    },
    returns = {
        "type": "object",
        "properties": {
            "nodes": {"type": "array", "items": {"type": "object"}},
            "edges": {"type": "array", "items": {"type": "object"}},
            "depth_map": {"type": "object"}
        }
    }
)]
pub fn find_connected_entities(entity_id, max_hops, relation_types, direction) {
    let visited = #{};
    let nodes = [];
    let edges = [];
    let depth_map = #{};

    fn traverse(current_id, depth) {
        if depth > max_hops || visited.contains_key(current_id) {
            return;
        }

        visited[current_id] = true;
        depth_map[current_id] = depth;

        let entity = query::by_id(current_id);
        nodes.push(entity);

        // Get relations based on direction
        let relations = match direction {
            "outbound" => entity.outbound_relations(),
            "inbound" => entity.inbound_relations(),
            "both" => entity.all_relations(),
        };

        // Filter by relation types if specified
        if relation_types {
            relations = relations.filter(|r| relation_types.contains(r.type));
        }

        for relation in relations {
            edges.push(relation);
            traverse(relation.target_id, depth + 1);
        }
    }

    traverse(entity_id, 0);

    #{ nodes, edges, depth_map }
}
```

## Example 5: Agent-Friendly Analysis Tool

```rune
// plugins/analysis_tools.rn
use crucible::query;
use crucible::analysis;

#[tool(
    desc = "Analyzes note clusters to identify knowledge gaps and suggest connections",
    category = "analysis",
    tags = ["analysis", "clustering", "recommendations", "agent"],
    params = {
        "focus_tags": {
            "type": "array",
            "description": "Tags to focus analysis on",
            "items": {"type": "string"},
            "minItems": 1
        },
        "similarity_threshold": {
            "type": "number",
            "description": "Minimum similarity score (0-1)",
            "default": 0.7,
            "minimum": 0.0,
            "maximum": 1.0
        },
        "include_suggestions": {
            "type": "boolean",
            "description": "Include AI-generated connection suggestions",
            "default": true
        },
        "max_suggestions": {
            "type": "number?",
            "description": "Maximum suggestions to generate",
            "default": 10
        }
    },
    returns = {
        "type": "object",
        "properties": {
            "clusters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "notes": {"type": "array"},
                        "centroid_tags": {"type": "array"},
                        "density": {"type": "number"}
                    }
                }
            },
            "gaps": {
                "type": "array",
                "description": "Identified knowledge gaps",
                "items": {
                    "type": "object",
                    "properties": {
                        "area": {"type": "string"},
                        "missing_connections": {"type": "number"},
                        "suggested_topics": {"type": "array"}
                    }
                }
            },
            "suggestions": {
                "type": "array?",
                "items": {
                    "type": "object",
                    "properties": {
                        "from_note": {"type": "string"},
                        "to_note": {"type": "string"},
                        "reason": {"type": "string"},
                        "confidence": {"type": "number"}
                    }
                }
            }
        }
    }
)]
pub fn analyze_knowledge_clusters(
    focus_tags,
    similarity_threshold,
    include_suggestions,
    max_suggestions
) {
    // Find all notes with focus tags
    let notes = query::from("notes")
        .where(|n| n.tags.contains_any(focus_tags))
        .collect();

    // Cluster by semantic similarity
    let clusters = analysis::cluster_by_similarity(
        notes,
        similarity_threshold
    );

    // Identify gaps (sparse areas between dense clusters)
    let gaps = analysis::identify_knowledge_gaps(clusters);

    // Generate connection suggestions if requested
    let suggestions = if include_suggestions {
        analysis::suggest_connections(
            clusters,
            max_suggestions ?? 10
        )
    } else {
        None
    };

    #{
        clusters: clusters,
        gaps: gaps,
        suggestions: suggestions
    }
}
```

## Type System Reference

### Supported Type Strings

```rune
// Basic types
"string"    // String value
"number"    // Numeric value (int or float)
"boolean"   // true/false
"null"      // null value

// Optional types (append ?)
"string?"   // String or null
"number?"   // Number or null
"boolean?"  // Boolean or null

// Collections
"array"     // Array of items
"object"    // Object/hash map

// With constraints
{
    "type": "string",
    "minLength": 3,
    "maxLength": 100,
    "pattern": "^[a-zA-Z0-9_]+$"
}

{
    "type": "number",
    "minimum": 0,
    "maximum": 100,
    "multipleOf": 5
}

{
    "type": "array",
    "items": {"type": "string"},
    "minItems": 1,
    "maxItems": 50,
    "uniqueItems": true
}

{
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "age": {"type": "number"}
    },
    "required": ["name"],
    "additionalProperties": false
}

// Enums
{
    "type": "string",
    "enum": ["option1", "option2", "option3"]
}
```

## Key Differences: Rust Macro vs Rune Attribute Macro

### Rust Procedural Macro (`#[rune_tool]` on Rust functions)
```rust
// Types INFERRED from function signature
#[rune_tool(desc = "Example")]
pub fn example(title: String, count: Option<i32>) -> Result<String, String> {
    // Rust compiler knows: title is String, count is Option<i32>
}
```

### Rune Attribute Macro (`#[tool]` in Rune scripts)
```rune
// Types EXPLICITLY declared in attribute
#[tool(
    desc = "Example",
    params = {
        "title": {"type": "string"},      // Must specify!
        "count": {"type": "number?"}      // Must specify!
    },
    returns = "string"
)]
pub fn example(title, count) {
    // Rune runtime validates against declared types
}
```

## Registration Pattern

```rust
// In Rust: Register the attribute macro
// crates/crucible-rune/src/lib.rs

pub fn register_crucible_macros(context: &mut rune::Context) -> Result<(), ContextError> {
    let mut module = rune::Module::new();

    // Register the #[tool] attribute macro
    module.attribute_macro(&["tool"], tool_attribute_macro)?;

    // Register other attribute macros
    module.attribute_macro(&["entity"], entity_attribute_macro)?;
    module.attribute_macro(&["filter"], filter_attribute_macro)?;
    module.attribute_macro(&["computed"], computed_attribute_macro)?;

    context.install(&module)?;
    Ok(())
}
```

Then in Rune scripts, these attributes become available as first-class language features, but **always require explicit type hints** for schema generation.

## Advantages of This Approach

1. **Explicit Type Safety**: Type hints in metadata catch errors early
2. **Self-Documenting**: Parameter types and descriptions in one place
3. **MCP Compatible**: Generates perfect JSON schemas for agents
4. **Validation Built-in**: Runtime validates args against schema
5. **Plugin-Friendly**: Plugin authors see clear type expectations
6. **LSP Support**: IDEs can autocomplete based on schema
7. **Versioning**: Schema changes are explicit and trackable

## Implementation Notes

- The Rust attribute macro handler parses the params hash
- Generates JSON Schema and stores it in a registry
- Transforms the function to validate parameters on entry
- Registers the tool with MCP server automatically
- All type checking happens at Rune compile-time or runtime, not Rust compile-time
