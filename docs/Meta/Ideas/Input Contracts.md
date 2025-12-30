---
tags:
  - idea
  - contracts
  - typing
  - workflows
---

# Input Contracts

Typed input declarations in frontmatter. Inspired by Racket-style contracts.

## Core Idea

Frontmatter declares inputs. Outputs are implicit (markdown, plaintext, or binary/scriptable).

```yaml
---
type: workflow
inputs:
  source: "[[note]]"           # note reference
  format: csv | json           # enum
  threshold: number >= 0.5     # soft constraint
---
```

The workflow IS its own schema. No separate schema notes needed.

## Why Inputs Only

Outputs don't need declaration—they're always one of:
1. **markdown** — creates/modifies a note
2. **plaintext** — returns text
3. **binary/scriptable** — MCP tools, skills, side effects

The system infers output type from what actually happens.

## Type Syntax

| Syntax | Meaning | Strictness |
|--------|---------|------------|
| `"[[note]]"` | Note reference | Hard |
| `"[[folder/*]]"` | Notes in folder | Hard |
| `string`, `number`, `boolean` | Primitives | Hard |
| `foo \| bar \| baz` | Enum | Hard |
| `number >= 0.5` | Criteria | Soft |
| `string ~ /pattern/` | Regex match | Soft |
| `any` | Accept anything | Escape hatch |

Soft constraints warn but don't fail. Scripts can override.

## Progressive Complexity

| Level | Frontmatter |
|-------|-------------|
| 0 | None |
| 1 | Metadata (tags, title) |
| 2 | Input contracts |
| 3 | Attached script for custom validation |
| 4 | Composite workflows (DAG of contracted units) |

## Existing Foundation

The docs kiln already has typed frontmatter:

- `type: agent` with `tools`, `model`, `temperature`
- `type: workflow` with inline `-> output` syntax
- `type: handlers` for event plugins

Gap: no explicit `inputs:` field yet.

## Future Extensions

- **Agent inputs** — extend contracts to agent cards
- **Contract inference** — replay sessions, suggest types from observed behavior
- **Blame tracking** — trace failures to specific input violations

## Related

- [[Help/Extending/Agent Cards]]
- [[Help/Workflows/Workflow Syntax]]
- [[Help/Extending/Markdown Handlers]]
