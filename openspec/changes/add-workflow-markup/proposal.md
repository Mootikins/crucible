# Add Workflow Markup: Prose-Friendly DAG Workflows in Markdown

## Why

Crucible aims to be a platform where **knowledge and workflows are unified** in markdown documents. Current workflow systems require either:
- **Code-based DSLs** (YAML, JSON) that separate workflows from documentation
- **Visual tools** (flow diagrams) that can't be versioned or AI-generated easily
- **Prose descriptions** that lack formal structure for execution

This creates friction between **documenting** what a workflow does and **executing** it. Teams maintain parallel artifacts: prose documentation for humans, code definitions for machines.

### The Critical Insight: Workflows Are Documentation

The best workflow documentation reads like a story:
- "The @architect reviews the config → creates database schema"
- "Multiple @developer agents in #dev-channel take tech_spec → write code → submit PRs"
- "The system checks for changes → if found, regenerates embeddings → stores in database"

This prose **already contains the DAG structure**. We just need lightweight markup that:
1. **Blends seamlessly** into natural writing
2. **Encodes formal semantics** for parsing into executable DAGs
3. **Stores execution traces** compactly for later analysis

### Why Markdown + Toon-Format

**Markdown Structure as DAG**:
- Document headings define workflow nodes/phases
- Heading hierarchy creates parent-child relationships
- Special markup inside headings/prose defines edges and metadata

**Toon-Format for Session Recording**:
- Execution traces are uniform arrays of structured data
- Perfect use case for toon-format's 40% token compression
- Embedded as code blocks within workflow markdown
- Agents can query execution history efficiently

This enables:
- **Write once, run anywhere**: Same document is human-readable AND agent-executable
- **Self-documenting workflows**: Execution traces stored inline with workflow definition
- **Version control**: Workflows and their history tracked in git
- **AI-native**: Agents read, write, and modify workflows in natural language

## What Changes

**NEW CAPABILITY: Prose-Friendly Workflow Markup**

### Core Markup Syntax

Special symbols that blend into prose:

- `@agent-name` - Agent/actor invocation (who performs the step)
- `#room` or `#channel` - Communication context/namespace
- `->` - Data flow between steps (directed edge)
- `::type` - Optional type annotations for data
- `?` suffix - Conditional/optional steps
- `!` suffix - Critical/required steps with failure handling

### Document Structure as DAG

**Headings Define Nodes**:
```markdown
## Phase 1: Planning @product-manager
Reviews requirements and creates specification.

## Phase 2: Design @architect
Takes specification → creates architecture diagram.
```

**Heading Hierarchy**:
```markdown
## Build Pipeline
Main pipeline orchestrator.

### Quick Filter
Checks file hash → skips if unchanged.

### Parse
Transforms markdown → AST.
```
Parent-child relationships create sub-DAG structure.

### Data Flow Notation

**Simple Flow**:
```markdown
## Process File @parser
Reads file.md → parses content → outputs AST
```

**Typed Flow**:
```markdown
## Generate Schema @architect
Takes project_config::Config → creates schema.sql::SQL
```

**Conditional Flow**:
```markdown
## Check Cache ?
If hash_changed → proceed to Parse
Otherwise → skip to next file
```

**Multi-Output Flow**:
```markdown
## Design Phase @architect
Takes product_spec → creates:
- architecture_diagram
- tech_spec::Markdown
- database_schema::SQL
```

### Session Recording with Toon-Format

After workflow execution, session markers are embedded:

````markdown
## Phase 1: Planning @product-manager
Reviews requirements in #planning-room and creates specification.

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 Planning,product-manager,planning-room,requirements.md,product_spec.md,4500,success,2340
```

---

## Phase 2: Design @architect
Takes specification → creates architecture diagram.

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 Design,architect,design-room,product_spec.md,arch_diagram.svg,6200,success,3100
```
````

**Benefits**:
- Execution history stored inline with workflow definition
- Compact representation (~40% fewer tokens vs JSON)
- Agents can query: "Show me all failed steps in #dev-channel"
- Session replay: regenerate workflow state from markers

### Example: Complete Workflow Document

````markdown
# E-Commerce Pipeline

This workflow handles order processing from cart to fulfillment.

## Validate Cart @cart-service #orders
Checks inventory and pricing for items in cart.
cart_items::CartItem[] → validated_cart::ValidatedCart

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 ValidateCart,cart-service,orders,cart_123.json,validated_cart_123.json,150,success,420
 ValidateCart,cart-service,orders,cart_124.json,validated_cart_124.json,180,success,450
 ValidateCart,cart-service,orders,cart_125.json,error_invalid_item.json,90,failed,310
```

## Process Payment @payment-service #payments !
Critical step: Charges payment method.
validated_cart → payment_confirmation::PaymentReceipt

```session-toon
execution[2]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 ProcessPayment,payment-service,payments,validated_cart_123.json,receipt_123.json,850,success,520
 ProcessPayment,payment-service,payments,validated_cart_124.json,receipt_124.json,920,success,540
```

## Ship Order @fulfillment-service #warehouse
Creates shipping label and notifies carrier.
payment_confirmation → tracking_number::String
````

### Integration with Existing Systems

**Parser Integration**:
- New `WorkflowExtension` implements `SyntaxExtension` trait
- Parses markdown → extracts DAG structure
- Priority ~70 (after basic markdown, before specialized syntax)
- Outputs `NoteContent::Workflow(WorkflowNode)` blocks

**Toon-Format Storage**:
- `session-toon` code blocks parsed by `ToonExtension`
- Stored as `NoteContent::SessionMarker(ToonData)`
- Indexed for fast querying by agent, phase, status, channel
- Can be aggregated across multiple workflow runs

**Pipeline Integration**:
- Workflows stored as markdown notes (standard processing)
- Session markers updated during workflow execution
- File watcher detects changes → re-indexes workflows
- Merkle diff efficiently handles session marker additions

## Impact

### Affected Specs

- **workflow-markup** (NEW) - Complete workflow markup specification
- **parser** (extends) - Add workflow parsing to extension system
- **toon-integration** (NEW) - Toon-format for session storage
- **agent-system** (reference) - Agents execute and modify workflows
- **meta-systems** (reference) - Workflow engines as plugins

### Affected Code

**New Components**:
- `crates/crucible-parser/src/extensions/workflow.rs` - NEW - Workflow markup parser
  - Parse heading hierarchy → DAG nodes
  - Extract `@agent`, `#channel`, `->` flow notation
  - Build `WorkflowGraph` data structure
  - Handle conditional/critical markers (`?`, `!`)
- `crates/crucible-parser/src/extensions/toon_format.rs` - NEW - Toon session marker parser
  - Parse `session-toon` code blocks
  - Validate toon-format syntax
  - Convert to structured `SessionMarker` objects
  - Index for fast querying
- `crates/crucible-core/src/workflow/` - NEW - Workflow domain types
  - `graph.rs` - `WorkflowGraph`, `WorkflowNode`, `WorkflowEdge` types
  - `execution.rs` - Workflow execution engine (basic interpreter)
  - `session.rs` - `SessionMarker`, execution trace storage
  - `query.rs` - Query session markers by agent/channel/status
- `crates/crucible-core/src/parser/content.rs` - MODIFY - Add new content types
  - `NoteContent::Workflow(WorkflowNode)` - Workflow definitions
  - `NoteContent::SessionMarker(SessionData)` - Execution traces

**Integration Points**:
- `crates/crucible-parser/src/extensions.rs` - Register `WorkflowExtension`, `ToonExtension`
- `crates/crucible-pipeline/` - Workflow notes processed like other markdown
- `crates/crucible-db/` - Index session markers for querying
- `crates/crucible-agents/` (future) - Execute workflows, write session markers

**Dependencies Added**:
- `toon-format = "0.1"` - Rust toon-format parser/encoder
- `petgraph = "0.6"` - DAG data structure and algorithms
- `daggy = "0.8"` - Alternative: simpler DAG library (evaluate both)

### Implementation Strategy

**Phase 1: Toon-Format Foundation (Week 1)**
- Add `toon-format` dependency
- Create `ToonExtension` parser for `session-toon` blocks
- Define `SessionMarker` domain types
- Test parsing and encoding session markers

**Phase 2: Workflow Parser (Week 2)**
- Implement `WorkflowExtension`
- Parse heading hierarchy → DAG nodes
- Extract `@agent`, `#channel`, `->` notation
- Build `WorkflowGraph` from parsed structure
- Handle edge cases (nested headings, complex flows)

**Phase 3: Basic Execution Engine (Week 3)**
- Implement simple workflow interpreter
- Execute each node in topological order
- Support basic data passing between nodes
- Write session markers after execution
- Handle failures gracefully

**Phase 4: Querying & Integration (Week 4)**
- Index session markers in SurrealDB
- Implement session marker queries
- Integrate with file watcher for auto-reloading
- Add CLI commands: `cru workflow run`, `cru workflow history`
- Create example workflow documents

### User-Facing Impact

**Immediate Benefits**:
- Write workflows in natural prose, no DSL required
- Execution history stored inline with workflow definition
- AI agents can read, modify, and execute workflows
- Version control for workflows and their execution traces
- Compact session markers reduce token costs for analysis

**Long-Term Vision**:
- Workflows become first-class knowledge artifacts
- Agents compose workflows from prose descriptions
- Session markers enable workflow optimization (identify bottlenecks)
- Community shares workflow patterns as markdown notes
- Workflows and documentation are unified, never drift

**Example Use Cases**:

```
Research Pipeline:
# Literature Review Workflow

## Search Papers @researcher #research
Queries arXiv for papers matching keywords → paper_list::JSON

## Download PDFs @fetcher #research
Downloads papers → stores in papers/ folder

## Extract Citations @parser #research
Parses PDFs → extracts citations → citation_graph::Graph

## Analyze Network @analyst #research
Finds clusters and key papers → ranking::Markdown
```

```
DevOps Pipeline:
# Deployment Workflow

## Run Tests @ci-service #testing !
Critical: All tests must pass.
code_changes → test_results::JUnit

## Build Image @docker-service #build
Creates container image → image_tag::String

## Deploy Staging @k8s-service #staging ?
Optional: Deploy to staging if branch is main.
image_tag → staging_url::URL

## Deploy Production @k8s-service #production !
Critical: Requires manual approval.
staging_url → production_url::URL
```

### Security Considerations

**Workflow Execution**:
- Workflows are declarative, not imperative code
- Execution engine controls what operations are allowed
- Agent invocations (`@agent`) go through standard tool system
- Channel access (`#channel`) controlled by permissions

**Session Marker Integrity**:
- Session markers are append-only (past executions immutable)
- Agents can read markers but only append new ones
- Markers include execution metadata for audit trail
- No sensitive data stored in markers (only references to artifacts)

### Timeline
- **Week 1**: Toon-format integration and session markers
- **Week 2**: Workflow parser and DAG extraction
- **Week 3**: Basic execution engine
- **Week 4**: Querying, CLI, examples
- **Estimated effort**: 4 weeks for production-ready workflow system

### Dependencies
- Toon-format Rust library (mature, stable)
- DAG library (petgraph or daggy, both mature)
- Parser extension system (existing)
- Session marker storage (SurrealDB, existing)

### Future Extensions

**Advanced Flow Control**:
- Parallel execution: `@agent1, @agent2 →` (fork)
- Loops: `while condition → step`
- Error handling: `on_error → fallback_step`

**Cross-Document Workflows**:
- `[[Other Workflow]]#Phase 2` - Reference steps in other documents
- Compose workflows from reusable components

**Live Workflow Execution**:
- Real-time session marker updates
- Progress tracking in CLI
- Workflow visualization in future GUI

**Agent-Generated Workflows**:
- "Create a deployment workflow for this project"
- Agent writes workflow markdown with proper markup
- User reviews and executes

## Questions for Review

1. Should we support more complex flow control (loops, conditionals) in initial version?
2. Should session markers be append-only, or allow updates for long-running phases?
3. Should we use petgraph or daggy for DAG representation?
4. Should workflow execution be synchronous or async by default?
5. Should we support inline workflow definitions (non-heading based) for small workflows?
