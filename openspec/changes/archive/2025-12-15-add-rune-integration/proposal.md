# Rune Integration: Scripting and Extension Layer

## Status: Placeholder

This is a placeholder spec documenting the future Rune integration. Details will be fleshed out when implementation begins.

## Why

Crucible needs an extension layer that is:
- **Safe** - Sandboxed execution, no arbitrary system access
- **Fast** - Near-native performance for hot paths
- **Portable** - Can run in VMs, federated agents, A2A contexts
- **Expressive** - Rich enough for complex workflows and behaviors

Rune (https://rune-rs.github.io/) provides:
- Rust-like syntax, easy for Rust developers
- Async-first runtime
- Compile to VM bytecode (portable)
- Strong sandboxing guarantees
- Excellent Rust interop

## What Rune Enables

### Custom Workflows
```rune
pub fn codify_session(session) {
    let phases = session.phases();
    let workflow = Workflow::new();

    for phase in phases {
        workflow.add_step(phase.name, phase.agent);
    }

    workflow.optimize();
    workflow
}
```

### Custom Callout Handlers
```rune
pub fn handle_callout(callout) {
    match callout.type {
        "decision" => log_decision(callout),
        "error" => trigger_alert(callout),
        "custom" => custom_handler(callout),
    }
}
```

### Federated Execution
Rune macros can be published to remote VMs, enabling:
- A2A protocol compatibility
- Distributed workflow execution
- Custom agent behaviors across federation

### State Graphs
Define agent behaviors as state machines:
```rune
pub fn agent_behavior() -> StateMachine {
    StateMachine::new()
        .state("idle", idle_handler)
        .state("working", working_handler)
        .state("waiting", waiting_handler)
        .transition("idle", "working", on_task)
        .transition("working", "idle", on_complete)
}
```

## Design Principle

**Build Rust as "Rune-shaped"** - Design Rust APIs to map cleanly to Rune bindings:

```rust
// Rust API
pub fn process_session(session: &SessionLog) -> Result<Workflow> {
    // ...
}

// Maps directly to Rune
pub fn process_session(session) {
    // Same logic, Rune syntax
}
```

This enables:
- Local fast path in Rust
- Customization in Rune scripts
- Federation via Rune macros in VMs

## Integration Points

### Workflow System
- Custom codification workflows
- Custom session handlers
- Workflow optimization plugins

### Agent System
- Custom agent behaviors
- State machine definitions
- Tool implementations

### Parser Extensions
- Custom syntax handlers
- Callout processors
- Frontmatter validators

### Meta-Systems
- Plugin architecture
- Hook system
- Event handlers

## Implementation Notes

### Phase 1: Foundation
- Add `rune` dependency
- Create Rune runtime wrapper
- Define core type bindings

### Phase 2: Workflow Bindings
- Bind Session, Phase, Callout types
- Bind Workflow, WorkflowStep types
- Create codification example

### Phase 3: Agent Bindings
- Bind AgentCard, AgentHandle types
- Bind tool system
- Create behavior example

### Phase 4: Federation
- VM bytecode compilation
- Remote execution protocol
- A2A compatibility layer

## Open Questions

1. **Rune version** - Which version to target? (latest stable)
2. **Binding strategy** - Manual or macro-based? (start manual)
3. **Script location** - Where do user scripts live? (`KILN/.crucible/scripts/`?)
4. **Hot reload** - Support script changes without restart?
5. **Permission model** - What can scripts access?

## References

- Rune Language: https://rune-rs.github.io/
- Rune Book: https://rune-rs.github.io/book/
- A2A Protocol: TBD

## Future Specs

Detailed specs will be created for:
- `rune-runtime` - Core runtime and sandboxing
- `rune-bindings` - Type bindings for Crucible types
- `rune-workflows` - Workflow scripting API
- `rune-agents` - Agent behavior scripting
- `rune-federation` - Distributed execution

---

## Amendment: Session, Inbox, and Context APIs

*Added via add-session-daemon proposal*

### Session Module (`session::*`)

Rune API for managing concurrent agent sessions:

```rune
// Start a new session in a worktree
let session = session::start("wt/feat-auth", Agent::Acp("claude-code"))?;

// Send message to session
session.send("Implement the auth flow")?;

// List active sessions
let sessions = session::list()?;

// Get session by ID or number
let s = session::get(2)?;

// Stop a session
session::stop(session)?;

// Start ephemeral session (for aggregation tasks)
let temp = session::start_ephemeral(Agent::Internal("ollama"))?;
```

### Inbox Module (`inbox::*`)

Rune API for HITL notifications:

```rune
// Send notification to human
inbox::send(Message {
    msg_type: MessageType::TaskComplete,
    title: "Auth implementation done",
    body: Some("All tests passing"),
})?;

// Wait for human response (blocking)
let response = inbox::wait_for(session, MessageType::DecisionNeeded)?;

// Query inbox
let unread = inbox::list(unread_only: true)?;

// Mark as read
inbox::mark_read(message.id)?;
```

### Context Module (`context::*`)

Rune API for context stack manipulation:

```rune
// Pop entries from context
context::pop(1)?;  // Remove last entry
context::pop(3)?;  // Remove last 3 entries

// Checkpoints
context::checkpoint("before-refactor")?;
context::rollback("before-refactor")?;

// Reset and summarize
context::reset()?;  // Clear all except system prompt
let summary = context::summarize()?;  // LLM-generated summary

// Replace top with summary
context::replace_top("Previous attempt failed: connection refused")?;
```

### Workflow Orchestration Examples

**Retry with context control:**
```rune
pub async fn implement_with_retry(session, task, max_retries) {
    let mut attempts = 0;
    context::checkpoint("task-start")?;

    loop {
        let result = session.send(task).await;

        match result {
            Ok(output) => return Ok(output),
            Err(e) if attempts < max_retries => {
                attempts += 1;
                context::rollback("task-start")?;
                context::push(f"Attempt {attempts} failed: {e}. Trying different approach.")?;
            }
            Err(e) => {
                inbox::send(Message::error(f"Failed after {max_retries} attempts: {e}"))?;
                return Err(e);
            }
        }
    }
}
```

**Multi-session aggregation:**
```rune
pub async fn aggregate_research(sessions) {
    // Wait for all sessions to complete
    let outputs = [];
    for session in sessions {
        let msg = inbox::wait_for(session, MessageType::TaskComplete)?;
        outputs.push(msg.body);
    }

    // Use ephemeral session to synthesize
    let summarizer = session::start_ephemeral(Agent::Internal("ollama"))?;
    summarizer.send(f"Summarize these research findings:\n{outputs.join('\n---\n')}")?
}
```

**Workflow with HITL gates:**
```rune
pub async fn feature_workflow(feature_name) {
    // Planning phase
    let plan_session = session::start(f"wt/plan-{feature_name}", Agent::Acp("claude-code"))?;
    plan_session.send("Create implementation plan")?;

    // Wait for human approval
    inbox::send(Message::approval_required("Plan ready for review"))?;
    let approved = inbox::wait_for_approval()?;

    if !approved {
        context::reset()?;
        return Err("Plan rejected");
    }

    // Implementation phase
    let impl_session = session::start(f"wt/impl-{feature_name}", Agent::Acp("claude-code"))?;
    // ...
}
```

### Markdown Workflow Syntax

Context directives in markdown workflows:

```markdown
3. **Implement** @ claude-code
   - checkpoint: "implementation-start"
   - Follow the plan
   - On tool failure: pop 1, inject error, continue
   - On approach failure: rollback to checkpoint, retry (max 3)
   - On spiral: reset context, restart step with summary
```

Retry loop syntax:

```markdown
4. **Test** @ ollama (retry 3x with exponential backoff)
   - Run test suite
   - On failure: reset context, inject failure summary
   - On success: notify human with coverage report
```
