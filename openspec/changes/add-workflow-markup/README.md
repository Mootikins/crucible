# Workflow Markup for Crucible

**Status**: 📝 Proposal
**Created**: 2025-11-22
**Author**: System Design
**Related**: [toon-format/toon](https://github.com/toon-format/toon)

## Quick Summary

Enable workflows to be written in natural markdown prose with lightweight syntax (`@agent`, `#channel`, `->`), then executed as DAGs with session traces stored compactly using toon-format.

## The Vision

Workflows should read like documentation:

```markdown
## Validate Cart @cart-service #orders
Checks inventory and pricing for items in cart.
cart_items → validated_cart::ValidatedCart

## Process Payment @payment-service #payments !
Critical: Charges payment method.
validated_cart → payment_confirmation
```

After execution, session markers are embedded:

````markdown
## Validate Cart @cart-service #orders
...

```session-toon
execution[3]{phase,agent,channel,status,duration_ms}:
 ValidateCart,cart-service,orders,success,150
 ValidateCart,cart-service,orders,success,180
 ValidateCart,cart-service,orders,failed,90
```
````

## Key Innovations

1. **Prose-First Workflows**: Markup blends seamlessly into natural writing
2. **Heading Hierarchy = DAG**: Document structure defines workflow structure
3. **Toon-Format Storage**: 40% token reduction for execution traces
4. **Self-Documenting**: Workflow definition and execution history unified
5. **AI-Native**: Agents read, write, and analyze workflows efficiently

## Markup Reference

| Syntax | Meaning | Example |
|--------|---------|---------|
| `@agent` | Agent/actor who performs step | `@cart-service` |
| `#channel` | Communication context/namespace | `#orders`, `#production` |
| `->` | Data flow between steps | `input → output` |
| `::Type` | Type annotation (optional) | `config::Config` |
| `!` | Critical step (failure halts) | `Deploy Production !` |
| `?` | Optional step (failure skips) | `Deploy Staging ?` |

## Data Flow Patterns

**Simple flow**:
```markdown
file.md → parsed_content → enriched_data → database
```

**Typed flow**:
```markdown
project_config::Config → schema.sql::SQL → deployed_db::Database
```

**Multi-output**:
```markdown
Takes input → creates:
- output_a
- output_b::Type
- output_c
```

**Conditional**:
```markdown
If hash_changed → proceed to Parse
Otherwise → skip to next file
```

## Files in This Change

```
add-workflow-markup/
├── README.md (this file)
├── proposal.md - Full rationale and implementation plan
├── specs/
│   └── workflow-markup/
│       └── spec.md - Formal requirements (GIVEN-WHEN-THEN)
└── examples/
    ├── simple-pipeline.md - Basic sequential workflow
    ├── ecommerce-order.md - Parallel branches, conditional logic
    └── software-development.md - Complex hierarchical workflow
```

## Example Use Cases

### 1. Data Pipeline
Track ETL jobs with session markers showing which data sources succeeded/failed.

### 2. DevOps Deployment
Document deployment steps as workflow, record each environment deployment with metrics.

### 3. Agent Collaboration
Multiple AI agents working together, each in their own `#channel`, with data flowing between them.

### 4. Research Workflow
Literature review → PDF download → citation extraction → network analysis, all tracked.

## Integration Points

### Parser System
- `WorkflowExtension` - Parses headings, `@agent`, `#channel`, `->` notation
- `ToonExtension` - Parses `session-toon` code blocks
- Priority 70-75 (after basic markdown, before specialized extensions)

### Storage Layer
- Session markers indexed in SurrealDB
- Queryable by: agent, channel, phase, status, time range
- Aggregations: avg duration, failure rates, token usage

### Execution Engine
- Topological sort of DAG nodes
- Parallel branch execution (where possible)
- Critical step failure handling (`!`)
- Optional step skipping (`?`)
- Session marker writing after each step

## CLI Commands (Proposed)

```bash
# Execute workflow
cru workflow run workflows/deploy.md

# View execution history
cru workflow history workflows/deploy.md

# Query sessions across all workflows
cru workflow sessions --agent architect --status failed

# Validate workflow without execution
cru workflow validate workflows/deploy.md
```

## Toon-Format Benefits

**Token Efficiency**:
- JSON execution log: ~2400 tokens
- Toon-format log: ~1440 tokens (~40% reduction)

**Use Cases for Toon**:
- Storing execution traces compactly
- Reducing token costs when AI analyzes workflow history
- Efficient serialization for large-scale workflow runs
- Human-readable tabular format for session markers

**Session Marker Schema**:
```
execution[N]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 Phase1,agent1,channel1,input1.md,output1.md,1200,success,450
 Phase1,agent1,channel1,input2.md,output2.md,980,success,420
 Phase2,agent2,channel2,output1.md,final.md,3400,success,890
```

## Why This Matters for Crucible

### Unified Knowledge + Workflows
Crucible is about **plaintext-first knowledge management**. Workflows are knowledge. By embedding workflows in markdown:
- Workflows are searchable via semantic search
- Workflow execution history becomes knowledge
- Agents can discover and reference workflows
- Version control tracks workflow evolution

### AI Agent Integration
Crucible is **agent-ready**. Prose-friendly workflows:
- Agents can read workflows in natural language
- Agents can generate workflows from descriptions
- Agents can modify workflows without DSL knowledge
- Agents can analyze failures efficiently (toon-format)

### Local-First, Git-Friendly
- Workflows are markdown files (diffable, mergeable)
- Session markers are append-only (no conflicts)
- No external workflow engine required
- Runs entirely on local machine

## Next Steps

1. **Review** proposal and spec for completeness
2. **Evaluate** DAG libraries (petgraph vs daggy)
3. **Prototype** basic parser in `crucible-parser`
4. **Integrate** toon-format Rust crate
5. **Implement** minimal execution engine
6. **Test** with example workflows
7. **Document** markup syntax and best practices

## Open Questions

See [proposal.md](./proposal.md) for full list of design questions, including:
- Should we use petgraph or daggy for DAG representation?
- Should execution be sync or async by default?
- Should session markers be append-only or updatable?
- Should we enforce type checking on data flows?

## Related Work

- **Toon-Format**: https://github.com/toon-format/toon - Token-efficient JSON alternative
- **Meta-Systems**: Crucible plugin architecture for custom workflow engines
- **Agent System**: AI agents that execute and generate workflows
- **Parser Extensions**: Markdown syntax extension system

## Contributing

This is a proposal! Feedback welcome on:
- Syntax design (is `@`, `#`, `->` intuitive?)
- Session marker schema (what fields are essential?)
- Execution semantics (how should `!` and `?` behave?)
- Use cases (what workflows would you build?)

---

**Status Legend**:
- 📝 Proposal - Under review
- 🚧 In Progress - Implementation started
- ✅ Implemented - Code complete
- 📦 Shipped - In release
