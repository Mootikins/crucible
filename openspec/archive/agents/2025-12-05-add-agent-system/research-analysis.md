# Agent System Research Analysis

This document analyzes modern agent frameworks (AutoGPT, CrewAI, MetaGPT, LangGraph, AutoGen, Reflexion) to identify potential gaps in our in-project agent system specification.

## Research Summary

### Frameworks Analyzed

1. **AutoGPT**: Solo, goal-driven automation with autonomous task decomposition
2. **CrewAI**: Role-based teams with sequential/hierarchical task assignment
3. **MetaGPT**: Software company simulation with specialized roles (PM, architect, engineer, QA)
4. **LangGraph**: Graph-based multi-agent workflows with multiple architectural patterns
5. **Microsoft AutoGen**: Asynchronous messaging, distributed agents, cross-language support
6. **Reflexion**: Self-reflection and verbal reinforcement learning from task feedback
7. **ReAct**: Reason + Act pattern with intermixed thinking and action steps

### Common Architectural Patterns

1. **Supervisor Pattern**: One supervisor delegates tasks to specialized sub-agents
2. **Swarm Pattern**: Agents dynamically pass control based on expertise areas
3. **Tool Calling Pattern**: Controller agent treats other agents as tools
4. **Network Pattern**: Divide-and-conquer with task routing to expert agents
5. **Reflection Pattern**: Agents critique and refine their own outputs iteratively

## Gap Analysis

### ✅ Features We Already Have

Our current spec includes:
- ✅ Agent definitions via markdown + frontmatter (similar to Claude Code)
- ✅ Task decomposition (primary → subagents)
- ✅ Permission inheritance and validation
- ✅ Session storage with wikilinks (similar to Gemini)
- ✅ Sequential execution queue (designed for future concurrency)
- ✅ Progress observability (abbreviated updates to user)
- ✅ Depth limiting (max 2 levels)
- ✅ Agent registry and discovery
- ✅ Model selection per agent (sonnet/haiku/opus)
- ✅ Basic spawning and result collection

### 🔴 MAJOR GAPS (High Priority)

#### 1. Memory Systems

**What's Missing:**
- **Working Memory**: Shared context/scratch space across agents in a session
- **Episodic Memory**: Historical record of past agent executions and outcomes
- **Semantic Memory**: Learned knowledge from successful patterns
- **Procedural Memory**: Learned procedures/workflows from experience

**Why It Matters:**
- Current design: Subagents are stateless (only get task description)
- Research shows: Memory enables agents to learn from past experiences and avoid repeating mistakes
- Frameworks like MIRIX show 6 distinct memory types working together

**Potential Solutions:**
- **MVP+1**: Add session-scoped working memory (shared key-value store per session)
- **Future**: Episodic memory via RAG over past session markdown files
- **Future**: Semantic memory via vector DB of successful agent patterns

**Recommendation for Spec:**
- ✅ **Keep stateless for MVP** (simpler, more predictable)
- 📝 **Add design note** acknowledging memory as post-MVP enhancement
- 📝 **Add "Working Memory" to future extensibility section** in design.md

---

#### 2. Reflection & Self-Critique

**What's Missing:**
- **Self-Evaluation**: Agents don't critique their own outputs
- **Iterative Refinement**: No automatic retry with improved approach
- **Learning from Failures**: No mechanism to improve based on past errors
- **Reflection Loops**: No ReAct/Reflexion-style think→act→reflect cycles

**Why It Matters:**
- Research shows: Reflexion improved GPT-4 from 80% → 91% on coding tasks
- Current design: Agents execute once and return result (no self-improvement)
- Industry pattern: Reflection is considered a foundational agentic pattern (Andrew Ng)

**Potential Solutions:**
- **MVP**: Add optional `max_retries` parameter to agent spawn
- **MVP+1**: Add `self_evaluate()` step after execution with LLM-based critique
- **Future**: Full Reflexion pattern with episodic memory of reflections

**Recommendation for Spec:**
- 📝 **Add "Reflection Capability" requirement** (optional for MVP, enabled per agent)
- 📝 **Add frontmatter field**: `enable_reflection: true/false`
- 📝 **Add scenario**: "Agent self-evaluates and retries on failure"

---

#### 3. Human-in-the-Loop Approval Gates

**What's Missing:**
- **Mid-Execution Approval**: No way for agents to ask user for approval during execution
- **Action Gates**: No approval before destructive/irreversible actions
- **Return of Control**: Users can't modify parameters before agent executes
- **Co-Planning**: No collaborative planning between user and agent

**Why It Matters:**
- Industry consensus (Google VP): "You wouldn't want a system that can do this fully without a human in the loop"
- Current design: Permissions are pre-granted at spawn time (no runtime approval)
- Use case: Code reviewer might want to ask user "Should I check for SQL injection?" mid-review

**Potential Solutions:**
- **MVP**: Add `requires_approval` flag to agent frontmatter for write operations
- **MVP+1**: Add `request_approval(action, context)` tool for agents to call
- **Future**: Full co-planning with user feedback loops

**Recommendation for Spec:**
- 📝 **Add "Human Approval Gates" requirement**
- 📝 **Add frontmatter field**: `requires_approval: true/false` (per action type)
- 📝 **Add scenario**: "Agent requests approval for destructive action"
- 📝 **Add scenario**: "User modifies agent parameters at approval gate"

---

### 🟡 MODERATE GAPS (Medium Priority)

#### 4. Task Planning & Decomposition

**What's Missing:**
- **Explicit Planning Phase**: Agents don't create visible plans before execution
- **Task Dependencies**: No way to specify "task B needs task A's output"
- **Conditional Execution**: No if/then branching in workflows
- **Failure Recovery**: No retry strategies or fallback plans

**Why It Matters:**
- Research shows: Explicit planning improves task success rates
- Current design: Primary agent implicitly plans via LLM reasoning (not visible)
- Frameworks like MetaGPT explicitly model planning as separate phase

**Potential Solutions:**
- **MVP**: Primary agent outputs "plan" section before spawning subagents
- **MVP+1**: Add `PlannerAgent` system agent that creates execution plans
- **Future**: Task dependency DAG with conditional execution

**Recommendation for Spec:**
- ✅ **Keep implicit planning for MVP** (primary agent handles via LLM)
- 📝 **Add "Planning Visibility" to design.md** as post-MVP enhancement
- 📝 **Add system agent**: `planner.md` that creates structured task plans

---

#### 5. Agent Communication Patterns

**What's Missing:**
- **Peer-to-Peer**: Subagents can't communicate with each other (only parent)
- **Async Messaging**: All communication is synchronous spawn/return
- **Shared State**: No message bus or shared context between agents
- **Event System**: No pub/sub for agent coordination

**Why It Matters:**
- Research shows: AutoGen's async messaging enables distributed agent networks
- Current design: Only parent→child (spawn) and child→parent (return) communication
- Use case: Two subagents might want to collaborate on same subtask

**Potential Solutions:**
- **Post-MVP**: Add message bus for agent-to-agent communication
- **A2A Phase**: Full async channels with pub/sub (already planned)

**Recommendation for Spec:**
- ✅ **Acknowledge in design.md**: "Communication limited to parent↔child for MVP"
- ✅ **Already planned**: A2A communication is explicitly noted as future work
- 📝 **Add clarification**: "Peer-to-peer deferred to A2A phase"

---

#### 6. Observability & Debugging

**What's Missing:**
- **Agent Traces**: No visualization of agent execution flow
- **Performance Metrics**: No per-agent latency/token usage in real-time
- **Debugging Tools**: No step-through, breakpoints, or replay
- **Error Attribution**: Hard to trace errors to specific agent in chain

**Why It Matters:**
- Research shows: AutoGen has built-in OpenTelemetry for observability
- Current design: Only abbreviated progress updates + post-hoc session files
- Use case: Developer wants to see why subagent failed mid-workflow

**Potential Solutions:**
- **MVP**: Add structured logging with agent trace IDs
- **MVP+1**: Add `cru sessions trace {session-id}` command for visual flow
- **Future**: OpenTelemetry integration for distributed tracing

**Recommendation for Spec:**
- 📝 **Add "Agent Tracing" requirement** for structured logs
- 📝 **Add scenario**: "Developer views agent execution trace"
- 📝 **Add session metadata**: Include trace ID and parent chain

---

### 🟢 MINOR GAPS (Low Priority / Future)

#### 7. Agent Roles & Team Patterns

**Gap**: No explicit role system (just descriptions), no hierarchical teams
**Status**: Partially covered via agent descriptions and model selection
**Recommendation**: ✅ Adequate for MVP, add explicit roles in post-MVP

---

#### 8. Custom Tools Per Agent

**Gap**: All agents share same 6 MCP tools, can't define agent-specific tools
**Status**: Explicitly deferred ("Don't worry about specific tools for now")
**Recommendation**: ✅ Already planned as future work

---

#### 9. Cost Tracking & Resource Management

**Gap**: No per-agent token counting, cost attribution, or resource limits
**Status**: Basic metadata collection (token count) exists
**Recommendation**: 📝 Enhance metadata to include cost estimates per model

---

#### 10. Cross-Session Learning

**Gap**: Agents don't learn from past sessions (no prompt refinement)
**Status**: Related to memory/episodic systems
**Recommendation**: ✅ Deferred to post-MVP memory work

---

## Recommendations by Priority

### 🔴 HIGH PRIORITY: Add to Current Spec

1. **Reflection Capability**
   - Add `enable_reflection: true/false` to agent frontmatter
   - Add `max_retries: N` for self-improvement loops
   - Add scenario: Agent self-evaluates output and retries

2. **Human Approval Gates**
   - Add `requires_approval: true/false` to frontmatter
   - Add `request_approval(action, reason)` tool for agents
   - Add scenarios for approval flows and return of control

3. **Enhanced Observability**
   - Add trace IDs to session metadata
   - Add parent chain tracking for error attribution
   - Add `cru sessions trace` command (or defer to MVP+1)

4. **Working Memory (Optional)**
   - Add session-scoped key-value store for shared context
   - Enable agents to read/write shared state within session
   - Keep stateless by default, opt-in via frontmatter

### 🟡 MEDIUM PRIORITY: Document as Future Work

5. **Episodic Memory System**
   - RAG over past session markdown files
   - Learn from successful/failed patterns

6. **Planning Visualization**
   - Explicit planning phase before execution
   - Task dependency modeling

7. **Advanced Communication**
   - Peer-to-peer messaging
   - Async channels (A2A phase)

### 🟢 LOW PRIORITY: Acknowledge in Design

8. **Cost Tracking**
   - Enhance metadata with cost estimates

9. **Custom Tools**
   - Already noted as future work

10. **Cross-Session Learning**
    - Agent prompt refinement based on outcomes

---

## Proposed Spec Updates

### 1. Add to `specs/agent-system/spec.md`

#### New Requirement: Agent Reflection and Self-Improvement

```markdown
### Requirement: Agent Reflection and Self-Improvement
The system SHALL support optional reflection capabilities enabling agents to self-evaluate outputs and retry with improved approaches.

#### Scenario: Agent with reflection enabled
- **WHEN** agent definition has `enable_reflection: true` in frontmatter
- **THEN** agent SHALL self-evaluate output before returning to parent
- **AND** if evaluation indicates failure, agent SHALL retry with refined approach
- **AND** maximum retries SHALL be limited by `max_retries` (default 1)

#### Scenario: Reflection loop with improvement
- **WHEN** agent completes execution with reflection enabled
- **THEN** agent SHALL critique own output via LLM-based evaluation
- **AND** if critique identifies issues, agent SHALL generate improved output
- **AND** reflection history SHALL be included in result metadata

#### Scenario: Reflection disabled (default)
- **WHEN** agent definition omits `enable_reflection` or sets to false
- **THEN** agent SHALL execute once and return result immediately
- **AND** no self-evaluation SHALL occur
```

#### New Requirement: Human-in-the-Loop Approval Gates

```markdown
### Requirement: Human-in-the-Loop Approval Gates
The system SHALL support approval gates for agents to request user permission before executing sensitive or irreversible actions.

#### Scenario: Agent requires approval for actions
- **WHEN** agent definition has `requires_approval: true` for specific action types
- **THEN** agent SHALL pause execution and request user approval
- **AND** user SHALL see clear description of proposed action and impact
- **AND** user SHALL have options to approve, deny, or modify parameters

#### Scenario: User approves with modifications
- **WHEN** agent requests approval and user modifies parameters
- **THEN** agent SHALL execute with modified parameters
- **AND** modifications SHALL be logged in session metadata
- **AND** agent SHALL acknowledge parameter changes in result

#### Scenario: User denies approval
- **WHEN** agent requests approval and user denies
- **THEN** agent SHALL abort action and return failure result
- **AND** denial reason SHALL be captured in result metadata
- **AND** primary agent SHALL receive denial to adjust strategy

#### Scenario: Request approval mid-execution
- **WHEN** agent calls `request_approval(action, reason)` tool during execution
- **THEN** system SHALL pause agent and prompt user
- **AND** agent execution SHALL resume after user responds
- **AND** timeout SHALL abort if no response within configured period
```

#### Enhanced Requirement: Session Metadata Collection (modify existing)

Add to existing metadata scenarios:

```markdown
#### Scenario: Trace ID and parent chain tracking
- **WHEN** subagent spawns
- **THEN** metadata SHALL include unique trace_id for request tracing
- **AND** metadata SHALL include parent_chain array showing full spawning hierarchy
- **AND** trace_id SHALL propagate to all logging and error messages
- **AND** session trace SHALL enable debugging of multi-agent failures
```

### 2. Add to `design.md`

#### New Decision: Reflection as Optional Capability

```markdown
### Decision 9: Optional Reflection for Self-Improvement

**Choice**: Support optional reflection via frontmatter flag, disabled by default

**Implementation**:
```yaml
---
name: code-reviewer
enable_reflection: true
max_retries: 2
reflection_criteria: |
  - Did I identify all critical bugs?
  - Did I provide specific line references?
  - Did I suggest concrete fixes?
---
```

**Why**:
- Research (Reflexion): Improved GPT-4 from 80% → 91% on coding tasks
- Industry pattern: Reflection is foundational agentic pattern (Andrew Ng)
- Optional: Not all agents need reflection (cost vs. quality tradeoff)
- Simple start: Self-critique via LLM, no complex episodic memory needed

**Alternatives Considered**:
- Always-on reflection: Too expensive for simple tasks
- No reflection: Misses proven quality improvement pattern
- Complex Reflexion implementation: Premature for MVP

**Trade-offs**:
- Pro: Significantly improves output quality for complex tasks
- Con: 2-3x token cost per agent (runs multiple iterations)
- Mitigation: Optional flag, configurable max_retries
```

#### New Decision: Human Approval Gates

```markdown
### Decision 10: Human-in-the-Loop Approval Gates

**Choice**: Support approval requests via frontmatter configuration and runtime tool

**Implementation**:
```yaml
---
name: refactoring-assistant
requires_approval:
  - FilesystemWrite  # Always ask before writing files
  - DatabaseWrite    # Always ask before DB changes
approval_timeout: 300  # 5 minutes
---
```

```rust
// Agent can also request approval at runtime
let approved = request_approval(
    action: "Delete 15 unused functions",
    impact: "Will modify 3 files, ~200 lines removed",
    reversible: false,
).await?;
```

**Why**:
- Industry consensus: HITL is essential for production agents (Google, OpenAI)
- User safety: Prevents destructive actions without oversight
- Return of control: Users can modify params before execution
- Flexibility: Configured at agent level + runtime requests

**Alternatives Considered**:
- No approval gates: Too risky for write operations
- Always prompt: Too much friction for read-only tasks
- Permission-only system: Too coarse-grained

**Trade-offs**:
- Pro: User maintains control over sensitive operations
- Con: Adds latency to agent execution
- Mitigation: Configurable per agent, timeout for automation
```

### 3. Add to `tasks.md`

Add to appropriate sections:

```markdown
## 3.4 Reflection System (NEW)
- [ ] 3.4.1 Define reflection_criteria in agent frontmatter schema
- [ ] 3.4.2 Implement self_evaluate() function with LLM-based critique
- [ ] 3.4.3 Add retry loop with max_retries limit
- [ ] 3.4.4 Capture reflection history in result metadata
- [ ] 3.4.5 Write tests for reflection success and failure cases
- [ ] 3.4.6 Document reflection patterns and best practices

## 4.3 Human Approval Gates (NEW)
- [ ] 4.3.1 Define requires_approval in agent frontmatter schema
- [ ] 4.3.2 Implement request_approval() tool for agents
- [ ] 4.3.3 Add approval prompt UI in CLI observer
- [ ] 4.3.4 Implement approval timeout and default behavior
- [ ] 4.3.5 Support parameter modification at approval gate
- [ ] 4.3.6 Log approval/denial in session metadata
- [ ] 4.3.7 Write tests for approval workflows (approve, deny, modify, timeout)

## 3.5 Enhanced Observability (NEW)
- [ ] 3.5.1 Generate unique trace_id per agent spawn
- [ ] 3.5.2 Track parent_chain array in metadata
- [ ] 3.5.3 Propagate trace_id to all logs and errors
- [ ] 3.5.4 Add `cru sessions trace {session-id}` command (optional MVP+1)
- [ ] 3.5.5 Implement trace visualization (ASCII tree or JSON)
```

### 4. Update `proposal.md`

Add to "What Changes" section:

```markdown
**Advanced Capabilities (NEW):**
- Optional reflection system for self-improvement (Reflexion pattern)
- Human-in-the-loop approval gates for sensitive actions
- Enhanced observability with trace IDs and parent chain tracking
- Session-scoped working memory (optional, post-MVP)
```

---

## Final Recommendations

### ✅ Add to Current Spec (Before Implementation)

1. **Reflection Capability** - Proven to improve quality by 10-15% (Reflexion research)
2. **Human Approval Gates** - Industry best practice, essential for production use
3. **Enhanced Tracing** - Critical for debugging multi-agent workflows

### 📝 Document as Future Work (Post-MVP)

4. **Episodic Memory** - Learn from past sessions (RAG over session markdown)
5. **Planning Visualization** - Explicit plan creation before execution
6. **Peer-to-Peer Communication** - Defer to A2A phase (already planned)

### ✅ Already Covered (No Changes Needed)

7. **Custom Tools Per Agent** - Explicitly noted as future work
8. **Cost Tracking** - Basic metadata exists, enhance incrementally
9. **Agent Roles** - Covered via descriptions and model selection

---

## Conclusion

Our initial spec is **solid and well-designed** for an MVP. The research identified three high-priority gaps:

1. **Reflection** - Missing proven quality improvement pattern
2. **Human Approval** - Missing production safety mechanism
3. **Observability** - Missing debugging/tracing infrastructure

Adding these three enhancements would align our system with industry best practices (AutoGen, CrewAI, Reflexion) while maintaining the simplicity and clarity of our original design.

**Recommendation**: Update spec with reflection, approval gates, and tracing before beginning implementation.
