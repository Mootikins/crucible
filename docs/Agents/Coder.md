---
description: Code-focused analysis and generation agent
tags:
  - agent
  - example
  - coding
type: agent-card
---

# Coder Agent

A specialized agent for working with code snippets, technical documentation, and programming-related notes.

## Purpose

The Coder agent excels at:
- Finding code examples in your notes
- Explaining technical concepts
- Generating code based on your patterns
- Reviewing code snippets

## Agent Card

```yaml
name: Coder
description: Code-focused analysis and generation
version: 1.0.0

capabilities:
  - semantic_search
  - text_search
  - read_note
  - create_note

system_prompt: |
  You are a coding assistant with access to the user's technical notes.

  Your role is to:
  1. Find relevant code examples and patterns
  2. Explain technical concepts clearly
  3. Generate code following the user's established patterns
  4. Help debug and review code snippets

  When generating code:
  - Follow patterns found in existing notes
  - Include brief explanations
  - Cite any examples you reference

  When explaining:
  - Use code examples when helpful
  - Reference relevant notes with [[wikilinks]]
  - Break down complex concepts

tools:
  - semantic_search
  - text_search
  - read_note
  - create_note

preferences:
  code_style: infer_from_notes
  explanation_depth: detailed
  include_examples: true
```

## Usage

In chat mode, invoke the coder:

```
/agent coder

How do I implement error handling in this project?
```

The agent will:
1. Search for existing error handling patterns
2. Read relevant code examples
3. Explain the approach
4. Generate code if requested

## Example Session

```
You: Show me how to use the parser in my codebase

Coder: I'll find examples of parser usage in your notes.

[Searches for "parser" and related terms]

Based on [[Crucible Parser Usage]] and [[Parsing Examples]], here's how
the parser is typically used:

```rust
use crucible_parser::parse_note;

let content = std::fs::read_to_string("note.md")?;
let parsed = parse_note(&content)?;

// Access frontmatter
if let Some(fm) = parsed.frontmatter {
    println!("Title: {}", fm.title);
}

// Access wikilinks
for link in parsed.wikilinks {
    println!("Links to: {}", link.target);
}
```

This pattern is used in:
- [[Processing Pipeline]] - for batch processing
- [[Search Implementation]] - for indexing

Want me to show a more specific use case?
```

## Modes

### Read-Only (Plan Mode)

Default mode. Agent can search and explain but not create files.

### Write Mode (Act Mode)

Enable with `/act` to allow creating notes:

```
/act

Create a note documenting this new API pattern we discussed
```

## Customization

**For specific language:**
```yaml
preferences:
  primary_language: rust
  secondary_languages: [python, typescript]
```

**For documentation:**
```yaml
preferences:
  explanation_depth: thorough
  include_rationale: true
```

## When to Use

Use the Coder agent when you:
- Need to find code examples in your notes
- Want to understand technical concepts
- Are generating code following existing patterns
- Need help debugging

## Limitations

- Works with notes, not actual codebases
- Cannot execute code
- Best with well-documented technical notes
- May not know project-specific conventions without notes

## See Also

- [[Researcher]] - General research agent
- [[Reviewer]] - Review and feedback agent
- [[Help/Extending/Agent Cards]] - Creating custom agents
