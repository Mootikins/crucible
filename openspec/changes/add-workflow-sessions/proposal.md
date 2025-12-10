# Workflow Sessions: Logging, Resumption, and Codification

## Why

Agent sessions generate valuable knowledge: decisions made, tools used, errors encountered, and patterns that emerge. Currently this knowledge is:

- **Lost after sessions end** - No persistent record
- **Not resumable** - Can't continue from where you left off
- **Not learnable** - No way to extract patterns or improve

This change adds:
1. **Session logging** in readable markdown format
2. **Session resumption** from saved state
3. **Workflow codification** - extract workflow definitions from sessions
4. **RL case generation** - learn from failures and interventions

## Relationship to Workflow Markup

**`workflow-markup`** defines how to write and execute workflows (definition → execution).

**`workflow-sessions`** defines how to log and learn from sessions (execution → learning).

They're complementary:
- Run a workflow → logs to session format
- Analyze session logs → codify into new workflows
- Refine workflows → better sessions → better patterns

## What Changes

### Session Log Format

Sessions are logged as markdown with structured frontmatter:

```markdown
---
type: session
session_id: 2025-12-05-feature-impl
started: 2025-12-05T14:32:15Z
status: active | completed | failed | paused
channel: dev
participants:
  - orchestrator@1.0
  - researcher@1.2
model: claude-opus-4-5-20251101
tokens_used: 12400
subagents:
  - session: 2025-12-05-research-subtask
    agent: researcher@1.2
    model: claude-sonnet-4-20250514
    tokens: 3200
resume_point: phase-2
---

# Feature Implementation

## Research Phase @orchestrator #dev

Investigating existing patterns for workflow logging.

**Tool calls:**
- `grep "session" crates/` → 12 matches
- `read crates/crucible-core/src/session.rs` → 245 lines

> [!subagent] @researcher@1.2 (claude-sonnet-4-20250514)
> Delegated deep research on logging formats.
> → [[2025-12-05-research-subtask]]
> Summary: Recommends structured markdown with frontmatter.

> [!decision]
> Use heading-per-phase structure with callout types.

---

## Design Phase @orchestrator #dev

Based on research, designing session format.

> [!error]
> First attempt had circular reference. Fixed by separating metadata.

> [!user]
> "Use frontmatter instead of HTML comments" - confirmed approach.
```

### Structural Elements

**Frontmatter** - Session metadata:
- `session_id` - Unique identifier
- `started`/`ended` - Timestamps
- `status` - Session state
- `channel` - Primary communication context
- `participants` - Agents involved with versions
- `model` - Primary model used
- `tokens_used` - Running total
- `subagents` - Nested sessions with links
- `resume_point` - Where to continue from

**Heading-per-phase** - Group related work:
- `## Phase Name @agent #channel`
- Each phase is a resumable checkpoint
- Natural structure for codification

**Tool calls** - Lists of operations:
```markdown
**Tool calls:**
- `tool_name args` → result_summary
- `another_tool` → outcome
```

**Callout types** - Special annotations:
- `[!subagent]` - Delegated work with link to nested session
- `[!decision]` - Key conclusions/choices made
- `[!error]` - Failures that needed handling
- `[!user]` - Human intervention/input

### Subagent Modes

Three modes for handling nested agent work:

**1. Inline** - Subagent work rendered directly in parent:
```markdown
> [!subagent] @researcher@1.2
> mode: inline
> [full subagent session content here]
```

**2. Link** - Separate file, wikilink reference:
```markdown
> [!subagent] @researcher@1.2
> mode: link
> [[2025-12-05-research-subtask]]
```

**3. Embed** - Separate file, transclusion:
```markdown
> [!subagent] @researcher@1.2
> mode: embed
> ![[2025-12-05-research-subtask]]
```

### Session Resumption

When resuming a paused/incomplete session:

1. Load session markdown
2. Parse frontmatter for `resume_point`
3. Reconstruct context from phases up to resume point
4. Continue with agent having full context

CLI:
```bash
cru session resume sessions/2025-12-05-feature-impl.md
```

### Workflow Codification

Extract workflow definitions from sessions:

**Progressive approach:**
1. **Auto-extract** - Parse phases as workflow steps
2. **Agent-refine** - Point agent at session to clean up
3. **User-confirm** - Interactive review before saving

```bash
# Auto-extract workflow from session
cru session codify sessions/my-session.md --output workflows/new-workflow.md

# With agent refinement
cru session codify sessions/my-session.md --refine

# Interactive mode
cru session codify sessions/my-session.md --interactive
```

The codification process itself is a workflow - customizable via Rune.

### RL Case Generation

Generate learning cases from sessions:

**From failures:**
```bash
cru session learn sessions/failed-deploy.md --type puzzle
```

Outputs a "puzzle" scenario that can be used for:
- LoRA fine-tuning
- Prompt engineering examples
- Role-play training scenarios
- State graph behaviors (Rune)

**From interventions:**
Track `[!user]` callouts as correction signals.

### Token-Aware Delegation

For large sessions that need proxy handling:

1. Estimate session tokens
2. If over threshold, split by phases
3. Route to appropriate models:
   - Fast model for routine phases
   - Strong model for key decisions
4. Different handling for direct vs proxied work

### TOON Index Layer

Session markdown is source of truth. TOON is derived index:

```
Session Markdown (verbose, readable)
         ↓
    TOON Index (compact, queryable)
         ↓
    Fast queries, aggregations
```

Sync happens on session write/update.

## Impact

### Affected Specs

- **workflow-sessions** (NEW) - Session logging and codification
- **workflow-markup** (reference) - Workflow definitions from codification
- **agent-system** (reference) - Agents write sessions
- **rune-integration** (future) - Custom codification workflows

### Affected Code

**New Components:**
- `crates/crucible-core/src/session/` - Session domain types
  - `log.rs` - SessionLog, Phase, ToolCall, Callout
  - `frontmatter.rs` - Session metadata schema
  - `resumption.rs` - Resume state management
- `crates/crucible-cli/src/commands/session.rs` - Session CLI
  - `resume` - Continue paused session
  - `codify` - Extract workflow
  - `learn` - Generate RL cases

**Integration Points:**
- Chat session writes to session log format
- Parser extracts session structure
- TOON indexer derives compact index

### User-Facing Impact

- Sessions are saved automatically as markdown
- Can resume interrupted work
- Can extract patterns into reusable workflows
- Can learn from failures to improve agents

## Design Decisions

1. **Markdown over TOON for sessions** - Readability and agent comprehension trump token efficiency for logs. TOON is for derived indexes.

2. **Heading-per-phase** - Natural checkpoints for resumption and codification. Phases map to workflow steps.

3. **Callouts for metadata** - Obsidian-compatible, renders nicely, parseable.

4. **Three subagent modes** - Flexibility for different use cases without changing format.

5. **Progressive codification** - D approach: auto → agent → user. Each layer is optional.

6. **Meta-workflow** - Codification itself is a workflow, customizable via Rune.

## Future Work

### Rune Integration
- Custom codification workflows
- User-defined callout handlers
- Federated session sync (A2A)

### Task Lists
- Research how OpenCode/Claude Code handle task state
- Determine serialization: frontmatter vs code block vs TOON
- Implement task tool that logs to session format

### RL Pipeline
- Puzzle scenario format
- LoRA training data export
- Prompt engineering examples

## Questions for Review

1. **Task list serialization** - Where should in-progress tasks live during a session? Frontmatter field? Dedicated code block? Separate file?

2. **Session file location** - `sessions/` folder? `KILN/.crucible/sessions/`? Configurable?

3. **Auto-save frequency** - After each phase? Each tool call? Configurable threshold?

4. **Subagent isolation** - Should subagents inherit parent's channel by default?

---

## Amendment: Session Daemon Integration

*Added via add-session-daemon proposal*

### Concurrent Session Support

Sessions integrate with the session daemon for concurrent access:

**Session Registry:**
- Active sessions register with daemon on start
- Registry tracks: session_id, worktree, agent_type, agent_name, status
- Sessions deregister on close or timeout

**Session Navigation:**
- `/sessions` - List all active sessions in current kiln
- `/goto <n>` - Switch to session by number
- `/next`, `/prev` - Rotate through sessions

### Inbox System (HITL)

Sessions can send messages to human inbox:

**New callout types:**
- `[!decision_needed]` - Agent blocked, needs human choice
- `[!approval_required]` - Agent wants to do something risky
- `[!task_complete]` - Finished assigned work
- `[!error]` - Something went wrong (existing, now indexed)

**Agent API:**
```rust
ctx.notify("Task complete: implemented auth flow")?;
ctx.request_decision("Which auth strategy?", &["JWT", "Session", "OAuth"])?;
ctx.request_approval("About to delete 47 files")?;
```

**Inbox is a view:**
- Messages written to session logs (existing callout format)
- Inbox queries aggregate actionable items across sessions
- Full history in logs, queryable/purgeable per-session

### Status Bar

Single-line display at bottom of chat:
```
[1] main/claude  [2] feat/auth/ollama  [3] test/gemini  |  2  /sessions
```

Shows: active sessions, unread inbox count, help hint.

### V1 Scope

- Single kiln concurrency (daemon per kiln)
- Sessions scoped to kiln
- Future: centralized session storage in personal kiln
