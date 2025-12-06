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
