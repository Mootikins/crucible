---
tags: [help, configuration, agents]
---

# Rules Files

Rules files provide project-specific instructions to AI agents working in your codebase. Crucible loads these files automatically and includes them in the system prompt.

## Supported Files

By default, Crucible searches for these files (in order):

1. `AGENTS.md` - Industry standard, recommended
2. `.rules` - Zed-compatible
3. `.github/copilot-instructions.md` - GitHub Copilot compatible

Other common files you can add to your config:
- `CLAUDE.md` - Claude Code compatible
- `.cursorrules` - Cursor compatible

## Hierarchical Loading

Crucible loads rules files **hierarchically** from the git root down to your workspace directory. Files closer to your workspace have higher priority and can override parent rules.

**Example:** If you have:
```
/repo/AGENTS.md           # Repo-wide rules
/repo/src/AGENTS.md       # Source-specific rules
/repo/src/module/AGENTS.md # Module-specific rules
```

All three files are loaded, with `/repo/src/module/AGENTS.md` having highest priority (its rules take precedence).

## Configuration

Customize which files to search for in your `config.toml`:

```toml
[context]
rules_files = ["AGENTS.md", ".rules", ".github/copilot-instructions.md"]
```

To add CLAUDE.md or .cursorrules:

```toml
[context]
rules_files = ["AGENTS.md", "CLAUDE.md", ".rules", ".cursorrules"]
```

## Writing Effective Rules

### Do

- Be specific about coding conventions
- Explain project-specific patterns
- List files/directories agents should know about
- Describe testing requirements

### Don't

- Repeat generic instructions (agents already know how to code)
- Include sensitive information (these files are often committed)
- Make rules too long (agents have context limits)

## Example AGENTS.md

```markdown
# Project Rules

## Architecture
- Use repository pattern for data access
- Services go in `src/services/`
- Keep controllers thin, logic in services

## Testing
- All new code needs tests
- Use `pytest` with fixtures in `conftest.py`
- Mock external APIs in tests

## Conventions
- Use snake_case for Python
- Docstrings on all public functions
- Type hints required
```

## See Also

- [[Configuration]] - Full config reference
- [[Help/Extending/Internal Agent]] - How agents use rules files
