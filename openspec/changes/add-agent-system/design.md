# Design Document: In-Project Agent System

## Context

After ACP integration proves the value of context injection for external agents, the next step is building an internal agent orchestration system. This enables task decomposition, specialized agents, and lays groundwork for Agent-to-Agent (A2A) communication with parallel execution and inter-agent channels.

**Current State:**
- ACP integration allows external agents (claude-code, gemini-cli) to interact with kiln
- Tool system provides 6 MCP-compatible tools for knowledge access
- No mechanism for task decomposition or subagent orchestration

**Stakeholders:**
- End users: Need complex tasks broken into manageable subtasks
- Primary agents: Need ability to spawn specialized subagents
- Developers: Need extensible agent system for future A2A features

**Constraints:**
- Must maintain plaintext-first philosophy (markdown definitions)
- Session history must be storable in kiln for learning/debugging
- Maximum depth of 2 levels (User → Primary → Subagents) for MVP
- Sequential execution for MVP (concurrent execution is future work)

## Goals / Non-Goals

### Goals
1. **Agent Definition**: Markdown files with frontmatter (similar to Claude Code slash commands)
2. **Task Decomposition**: Primary agent spawns specialized subagents
3. **Session Storage**: Markdown-based session history with wikilink parent/child tracking
4. **Permission Inheritance**: Subagents cannot exceed parent permissions
5. **Observability**: Users see abbreviated subagent progress
6. **Extensibility**: Design for future A2A, concurrency, custom tools per agent

### Non-Goals
1. ❌ Deep nesting - Maximum depth 2 for MVP (no subagent-spawning-subagent)
2. ❌ Parallel execution - Sequential for MVP, provider limits are future work
3. ❌ Agent-specific tools - Use existing 6 MCP tools, custom tools are future
4. ❌ Session persistence across reboots - Sessions are write-once for MVP
5. ❌ Agent metrics/analytics - Focus on basic observability first

## Decisions

### Decision 1: Agent Definition Format

**Choice**: Markdown files with YAML frontmatter in `.crucible/agents/`

**Format**:
```markdown
---
name: code-reviewer
description: Reviews code for bugs, performance issues, and style violations
model: sonnet  # or haiku for faster/cheaper agents
permissions:
  - FilesystemRead
  - SemanticSearch
enabled: true
---

# Code Reviewer Agent

You are a specialized code review agent with deep expertise in software engineering best practices.

## Your Responsibilities
- Identify bugs and potential runtime errors
- Suggest performance optimizations
- Enforce code style consistency
- Highlight security vulnerabilities

## Guidelines
- Be concise but thorough
- Provide specific line references when possible
- Suggest concrete improvements
- Prioritize critical issues over style nitpicks

## Tools Available
You have access to:
- `read_note` - Read file contents
- `list_notes` - List files in directories
- `semantic_search` - Find related code/documentation

## Output Format
Structure your review as:
1. **Summary**: High-level assessment
2. **Critical Issues**: Bugs, security, performance
3. **Suggestions**: Improvements and best practices
4. **Positive Notes**: What's done well
```

**Why**:
- **Familiar Format**: Users already understand markdown + frontmatter (kiln notes use this)
- **Human-Readable**: Agents are easy to read, edit, and version control
- **Composable**: Frontmatter handles metadata, markdown handles instructions
- **Extensible**: Easy to add new frontmatter fields as system evolves
- **Similar to Claude Code**: Users familiar with `.claude/commands/` will understand this

**Alternatives Considered**:
- JSON/YAML only: Less human-readable, loses rich formatting for instructions
- Code-based (Rust traits): Requires compilation, not user-extensible
- Embedded in database: Violates plaintext-first philosophy
- Single monolithic config file: Harder to manage multiple agents

**References**: Claude Code `.claude/commands/`, Gemini `.gemini/agents/`

### Decision 2: Agent Execution Model

**Choice**: Separate LLM calls with isolated context (stateless subagents)

**Flow**:
```rust
// Primary agent during task execution
primary_agent.execute(user_query) {
    // Decides to spawn subagent
    let result = spawn_agent(AgentSpawnRequest {
        agent_name: "code-reviewer",
        task_description: "Review the authentication module for security issues",
        context: None,  // No shared context
        permissions: PermissionSet::from_parent(self.permissions).constrain(&[FilesystemRead]),
    }).await?;

    // Gets back structured markdown result
    // result.content = markdown with review findings
    // result.metadata = execution time, model used, token count

    // Incorporates result into own reasoning
    continue_execution_with(result);
}
```

**Why**:
- **Clean Isolation**: Each subagent has fresh context (no pollution from parent)
- **Predictable**: Subagents are pure functions (task → result)
- **Scalable**: Easy to parallelize in future (no shared state)
- **Debuggable**: Each subagent execution is standalone, can be replayed
- **Similar to Claude Code**: Matches Task tool pattern

**Alternatives Considered**:
- Shared context (same session): Complex state management, hard to isolate failures
- In-process prompting: Less clear separation, harder to debug
- Persistent subagent sessions: Added complexity for MVP

**Trade-offs**:
- Pro: Clean, stateless, easy to reason about
- Con: Higher token usage (no context sharing)
- Mitigation: Subagents only get task description + kiln read access (minimal context)

### Decision 3: Session Storage with Wikilinks

**Choice**: Markdown files in `.crucible/sessions/` with wikilink-based parent/child tracking

**Structure**:
```
.crucible/sessions/
├── 2025-01-20-refactor-auth-module/
│   ├── session.md              # Primary agent conversation
│   ├── code-reviewer-auth.md   # [[code-reviewer-auth]] linked from session.md
│   ├── test-generator-auth.md  # [[test-generator-auth]] linked from session.md
│   └── metadata.json           # Session-level metadata
```

**session.md** (primary agent):
```markdown
---
session_id: 2025-01-20-refactor-auth-module
started_at: 2025-01-20T14:32:00Z
primary_agent: claude-sonnet-4
model: sonnet
status: completed
---

# User Query
Refactor the authentication module for better security

# Agent Reasoning
I'll break this into two tasks:
1. Security review of current implementation
2. Generate comprehensive test coverage

## Security Review
Spawning code-reviewer agent: [[code-reviewer-auth]]

The code reviewer found 3 critical issues...

## Test Coverage
Spawning test-generator agent: [[test-generator-auth]]

The test generator created...
```

**code-reviewer-auth.md** (subagent):
```markdown
---
subagent_of: 2025-01-20-refactor-auth-module
agent_name: code-reviewer
spawned_at: 2025-01-20T14:32:15Z
completed_at: 2025-01-20T14:32:47Z
model: sonnet
tokens: 2847
---

# Task
Review the authentication module for security issues

# Findings

## Critical Issues
1. **Password Hashing**: Uses deprecated MD5...
2. **Session Tokens**: Not cryptographically secure...
3. **SQL Injection**: Direct string concatenation in...

## Recommendations
...
```

**Why**:
- **Wikilinks are Native**: Crucible already uses wikilinks for note relationships
- **Visual Clarity**: Users can see parent/child structure by following links
- **Storable in Kiln**: Sessions become part of knowledge base
- **Debuggable**: Each agent execution is a standalone markdown file
- **Future Learning**: Can build RLHF datasets from successful sessions
- **Similar to Gemini**: Matches Gemini's session storage pattern

**Alternatives Considered**:
- JSON structure files: Less human-readable, harder to navigate
- Database-only: Loses plaintext-first benefits
- Nested directories: Less discoverable than wikilinks
- Parent ID in frontmatter only: Wikilinks provide bidirectional navigation

### Decision 4: Agent Registry and Discovery

**Choice**: Scan `.crucible/agents/` and `~/.config/crucible/agents/` at startup

**Registry Structure**:
```rust
pub struct AgentRegistry {
    agents: HashMap<String, AgentDefinition>,
    system_agents_path: PathBuf,  // ~/.config/crucible/agents/
    project_agents_path: PathBuf, // .crucible/agents/
}

pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub model: String,  // "sonnet" | "haiku" | "opus"
    pub permissions: Vec<Permission>,
    pub prompt: String,  // Markdown content after frontmatter
    pub enabled: bool,
    pub source_path: PathBuf,
}

impl AgentRegistry {
    pub async fn discover() -> Result<Self> {
        let mut agents = HashMap::new();

        // Load system agents first
        load_agents_from(&system_path, &mut agents)?;

        // Load project agents (can override system agents)
        load_agents_from(&project_path, &mut agents)?;

        Ok(Self { agents, ... })
    }

    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name).filter(|a| a.enabled)
    }

    pub fn list_agents(&self) -> Vec<&AgentDefinition> {
        self.agents.values()
            .filter(|a| a.enabled)
            .collect()
    }
}
```

**Why**:
- **Automatic Discovery**: No manual registration needed
- **Project Override**: Project agents can override system agents (like git config)
- **Simple Implementation**: Just scan directories and parse markdown
- **Similar to OpenSpec**: Follows same discovery pattern as specs/changes

**Alternatives Considered**:
- Manual registration: Extra step, error-prone
- Database storage: Violates plaintext-first
- Single global location: Can't have project-specific agents

### Decision 5: Permission Inheritance Model

**Choice**: Subagents inherit parent's maximum permissions, can be further constrained

**Implementation**:
```rust
pub enum Permission {
    FilesystemRead,
    FilesystemWrite,
    SemanticSearch,
    DatabaseRead,
    DatabaseWrite,
    NetworkAccess,
}

pub struct PermissionSet {
    permissions: HashSet<Permission>,
}

impl PermissionSet {
    /// Create from parent, capping at parent's max permissions
    pub fn from_parent(parent: &PermissionSet) -> PermissionSetBuilder {
        PermissionSetBuilder {
            max_permissions: parent.permissions.clone(),
            requested_permissions: HashSet::new(),
        }
    }

    /// Check if this set allows a specific permission
    pub fn allows(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }
}

pub struct PermissionSetBuilder {
    max_permissions: HashSet<Permission>,
    requested_permissions: HashSet<Permission>,
}

impl PermissionSetBuilder {
    /// Constrain to subset of parent permissions
    pub fn constrain(mut self, permissions: &[Permission]) -> Result<PermissionSet> {
        for perm in permissions {
            if !self.max_permissions.contains(perm) {
                return Err(anyhow::anyhow!(
                    "Subagent requested {:?} but parent doesn't have it", perm
                ));
            }
            self.requested_permissions.insert(perm.clone());
        }

        Ok(PermissionSet {
            permissions: self.requested_permissions,
        })
    }
}
```

**Default Permissions for Subagents**:
- Always have: `FilesystemRead`, `SemanticSearch` (kiln read access)
- Inherited if parent has: `FilesystemWrite`, `DatabaseWrite`, `NetworkAccess`
- Can be explicitly constrained to subset

**Why**:
- **Security by Default**: Subagents can't escalate privileges
- **Least Privilege**: Spawn read-only agents even if parent can write
- **Explicit Grants**: Clear declaration of what each agent can do
- **Audit Trail**: Permission grants are logged in session metadata

**Alternatives Considered**:
- All subagents inherit all permissions: Too permissive, security risk
- All subagents have no permissions: Too restrictive, breaks functionality
- User prompts for each permission: Too much friction

### Decision 6: Execution Queue (Sequential for MVP)

**Choice**: Simple FIFO queue with sequential execution, designed for future concurrency

**Implementation**:
```rust
pub struct ExecutionQueue {
    queue: VecDeque<AgentTask>,
    current: Option<AgentTask>,
    max_concurrent: usize,  // 1 for MVP, configurable later
}

pub struct AgentTask {
    pub agent_name: String,
    pub task_description: String,
    pub permissions: PermissionSet,
    pub parent_session_id: String,
    pub result_tx: oneshot::Sender<AgentResult>,
}

impl ExecutionQueue {
    pub async fn spawn_agent(&mut self, task: AgentTask) -> AgentResult {
        // For MVP: wait for current task to finish
        if self.current.is_some() {
            self.queue.push_back(task);
            // Block until result arrives
            task.result_tx.await?
        } else {
            self.execute_task(task).await
        }
    }

    async fn execute_task(&mut self, task: AgentTask) -> Result<AgentResult> {
        self.current = Some(task);

        // Load agent definition
        let agent_def = self.registry.get_agent(&task.agent_name)?;

        // Create LLM session with agent's prompt + task
        let session = create_llm_session(agent_def, &task).await?;

        // Execute and get markdown result
        let result = session.execute().await?;

        // Save to session folder
        self.save_subagent_result(&task, &result).await?;

        self.current = None;

        // Process next in queue
        if let Some(next_task) = self.queue.pop_front() {
            tokio::spawn(async move {
                self.execute_task(next_task).await
            });
        }

        Ok(result)
    }
}
```

**Future Concurrency Design** (not MVP):
```rust
pub struct ProviderLimits {
    pub local: usize,      // 1 (single local model at a time)
    pub anthropic: usize,  // 3 (API rate limits)
    pub openai: usize,     // 5 (higher limits)
}
```

**Why**:
- **Simple for MVP**: No concurrency complexity, easy to debug
- **Designed for Future**: Queue structure enables adding concurrency later
- **Provider-Aware**: Architecture supports per-provider limits
- **Graceful Degradation**: If agent fails, queue continues

**Alternatives Considered**:
- No queue (synchronous only): Harder to add concurrency later
- Full concurrency from start: Premature optimization, added complexity
- Thread pool: Overkill for sequential MVP

### Decision 7: Observability and Progress Reporting

**Choice**: Abbreviated progress updates visible to user, full details in session files

**User-Visible Output**:
```
User: Refactor the authentication module

Primary Agent: I'll break this into security review and test generation.

→ Running code-reviewer agent...
  Found 3 critical security issues

→ Running test-generator agent...
  Created 47 test cases

Primary Agent: I've completed the refactor. The security review found...
```

**Implementation**:
```rust
pub trait ProgressObserver {
    fn on_agent_spawn(&self, agent_name: &str, task_summary: &str);
    fn on_agent_complete(&self, agent_name: &str, result_summary: &str);
    fn on_agent_error(&self, agent_name: &str, error: &str);
}

pub struct CliObserver {
    output: io::Stdout,
}

impl ProgressObserver for CliObserver {
    fn on_agent_spawn(&self, agent_name: &str, task_summary: &str) {
        writeln!(self.output, "→ Running {} agent...", agent_name)?;
    }

    fn on_agent_complete(&self, agent_name: &str, result_summary: &str) {
        writeln!(self.output, "  {}", result_summary)?;
    }
}
```

**Result Summary Extraction**:
Subagents include a `## Summary` section in their markdown output:
```markdown
## Summary
Found 3 critical security issues: password hashing (MD5), insecure session tokens, SQL injection risk
```

Observer extracts first 100 chars of summary for abbreviated display.

**Why**:
- **User Awareness**: Users know what's happening without overwhelming details
- **Similar to Claude Code/Gemini**: Matches familiar UX patterns
- **Debuggable**: Full details in session markdown files
- **Async-Friendly**: Observer pattern decouples progress from execution

**Alternatives Considered**:
- No progress updates: Users don't know what's happening (poor UX)
- Full streaming output: Too verbose, overwhelming
- Silent with final summary only: Feels unresponsive for long tasks

### Decision 8: Depth Limiting (Max 2 Levels)

**Choice**: Hard limit of 2 levels (User → Primary → Subagents) enforced in spawner

**Implementation**:
```rust
pub struct AgentSpawnRequest {
    pub agent_name: String,
    pub task_description: String,
    pub permissions: PermissionSet,
    pub parent_session_id: String,
    pub depth: usize,  // 0 = primary, 1 = subagent
}

pub async fn spawn_agent(request: AgentSpawnRequest) -> Result<AgentResult> {
    // Enforce depth limit
    if request.depth >= 2 {
        return Err(anyhow::anyhow!(
            "Maximum agent depth (2) exceeded. Subagents cannot spawn their own subagents."
        ));
    }

    // Increment depth for spawned agent
    let child_depth = request.depth + 1;

    // ... rest of spawning logic
}
```

**Why**:
- **Prevents Runaway**: No infinite recursion or accidental deep nesting
- **Simplifies MVP**: Easier to reason about and debug
- **Clear Mental Model**: User → Primary → Specialist is intuitive
- **Future Extension**: Can increase limit after A2A patterns emerge

**Alternatives Considered**:
- No limit: Risk of runaway agent spawning
- Depth 3+: Added complexity without clear use cases yet
- Dynamic based on task: Too complex to determine automatically

### Decision 9: Optional Reflection for Self-Improvement

**Choice**: Support optional reflection via frontmatter flag, disabled by default

**Implementation**:
```yaml
---
name: code-reviewer
description: Reviews code for bugs and security issues
model: sonnet
enable_reflection: true
max_retries: 2
reflection_criteria: |
  - Did I identify all critical bugs?
  - Did I provide specific line references?
  - Did I suggest concrete fixes?
  - Did I check for security vulnerabilities?
permissions:
  - FilesystemRead
  - SemanticSearch
---
```

```rust
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub model: String,
    pub permissions: Vec<Permission>,
    pub prompt: String,
    pub enabled: bool,

    // Reflection fields
    pub enable_reflection: bool,
    pub max_retries: usize,  // default 1
    pub reflection_criteria: Option<String>,
}

async fn execute_with_reflection(
    agent: &AgentDefinition,
    task: &str,
) -> Result<AgentResult> {
    let mut attempts = 0;
    let mut best_result = None;

    while attempts <= agent.max_retries {
        // Execute agent
        let result = execute_agent(agent, task).await?;

        // If reflection disabled, return immediately
        if !agent.enable_reflection {
            return Ok(result);
        }

        // Self-evaluate using reflection criteria
        let evaluation = self_evaluate(&result, &agent.reflection_criteria).await?;

        // If passes evaluation, return
        if evaluation.passed {
            return Ok(result.with_reflection_history(evaluation));
        }

        // Store best attempt
        if best_result.is_none() || evaluation.score > best_result.score {
            best_result = Some((result, evaluation));
        }

        attempts += 1;

        // Refine approach based on critique
        task = refine_task_with_critique(task, &evaluation.critique).await?;
    }

    // Return best attempt with reflection metadata
    Ok(best_result.with_metadata("reflection_failed", true))
}
```

**Why**:
- **Research-Backed**: Reflexion improved GPT-4 from 80% → 91% on coding benchmarks
- **Industry Pattern**: Reflection is foundational agentic pattern (Andrew Ng, DeepLearning.AI)
- **Optional**: Not all agents need reflection (cost vs. quality tradeoff)
- **Simple Start**: Self-critique via LLM, no complex episodic memory needed for MVP
- **Quality Improvement**: Catches errors before returning to parent

**Alternatives Considered**:
- Always-on reflection: Too expensive for simple tasks (2-3x token cost)
- No reflection: Misses proven quality improvement pattern
- Complex Reflexion implementation: Requires episodic memory, premature for MVP
- Fixed retry count: Not configurable per agent type

**Trade-offs**:
- **Pro**: Significantly improves output quality for complex tasks (10-15% better)
- **Pro**: Catches errors before parent agent receives result
- **Pro**: Self-correcting agents reduce manual iteration
- **Con**: 2-3x token cost per agent (runs multiple iterations)
- **Con**: Increased latency (sequential retries)
- **Mitigation**: Optional flag, configurable max_retries, disabled by default

**References**:
- Reflexion paper: https://arxiv.org/abs/2303.11366
- LangChain reflection agents: https://blog.langchain.com/reflection-agents/

### Decision 10: Human-in-the-Loop Approval Gates

**Choice**: Support approval requests via frontmatter configuration and runtime tool

**Implementation**:
```yaml
---
name: refactoring-assistant
description: Refactors code for better structure and maintainability
model: sonnet
requires_approval:
  - FilesystemWrite  # Always ask before writing files
  - DatabaseWrite    # Always ask before DB changes
approval_timeout: 300  # 5 minutes, then abort
permissions:
  - FilesystemRead
  - FilesystemWrite
  - SemanticSearch
---
```

```rust
pub struct AgentDefinition {
    // ... existing fields
    pub requires_approval: Vec<Permission>,
    pub approval_timeout: u64,  // seconds
}

// Agents can also request approval at runtime
async fn request_approval(
    action: &str,
    impact: &str,
    reversible: bool,
) -> Result<ApprovalResponse> {
    let prompt = format!(
        "Agent requests approval:\n\
         Action: {}\n\
         Impact: {}\n\
         Reversible: {}\n\n\
         [A]pprove / [D]eny / [M]odify parameters?",
        action, impact, reversible
    );

    let response = prompt_user_with_timeout(prompt, approval_timeout).await?;

    match response {
        UserResponse::Approve => Ok(ApprovalResponse::Approved),
        UserResponse::Deny(reason) => Ok(ApprovalResponse::Denied(reason)),
        UserResponse::Modify(params) => {
            // Validate modified params
            validate_params(params)?;
            Ok(ApprovalResponse::ApprovedWithModifications(params))
        }
        UserResponse::Timeout => Err(anyhow!("Approval timeout")),
    }
}

// Pre-execution approval check
async fn execute_agent(agent: &AgentDefinition, task: &str) -> Result<AgentResult> {
    // Check if agent needs approval for any permissions
    for permission in &agent.permissions {
        if agent.requires_approval.contains(permission) {
            let approved = request_approval(
                &format!("Execute {} with {:?}", agent.name, permission),
                task,
                permission.is_reversible(),
            ).await?;

            if !approved.is_approved() {
                return Err(anyhow!("User denied approval"));
            }
        }
    }

    // ... execute agent
}
```

**Why**:
- **Industry Consensus**: Google, OpenAI emphasize HITL as essential for production agents
- **User Safety**: Prevents destructive actions without oversight
- **Return of Control**: Users can modify parameters before execution (more nuanced than yes/no)
- **Flexibility**: Configured at agent level + runtime requests for dynamic needs
- **Compliance**: Audit trail of approvals/denials for governance

**Alternatives Considered**:
- No approval gates: Too risky for write operations, violates user agency
- Always prompt: Too much friction for read-only tasks
- Permission-only system: Too coarse-grained, can't differentiate between reads/writes
- Auto-approve after timeout: Too risky, could execute destructive actions unattended

**Trade-offs**:
- **Pro**: User maintains control over sensitive operations
- **Pro**: Clear audit trail for compliance
- **Pro**: Return of control enables collaborative refinement
- **Con**: Adds latency to agent execution (wait for human)
- **Con**: Requires user to be present (can't run fully unattended)
- **Mitigation**: Configurable per agent, timeout aborts (fail safe), approval caching for session

**References**:
- Microsoft Magentic-UI: https://www.microsoft.com/en-us/research/wp-content/uploads/2025/07/magentic-ui-report.pdf
- OpenAI Agents SDK HITL: https://openai.github.io/openai-agents-js/guides/human-in-the-loop/

### Decision 11: Enhanced Observability with Distributed Tracing

**Choice**: Add trace IDs, parent chain tracking, and session trace visualization

**Implementation**:
```rust
pub struct AgentSpawnRequest {
    pub agent_name: String,
    pub task_description: String,
    pub permissions: PermissionSet,
    pub parent_session_id: String,
    pub depth: usize,

    // Tracing fields
    pub trace_id: Uuid,  // Unique per spawn
    pub parent_chain: Vec<String>,  // ["user", "primary-agent", "code-reviewer"]
}

pub struct AgentResult {
    pub content: String,
    pub metadata: AgentMetadata,
    pub status: ExecutionStatus,

    // Tracing
    pub trace_id: Uuid,
    pub parent_chain: Vec<String>,
}

// Session metadata enhancement
pub struct SessionMetadata {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub primary_agent: String,

    // Tracing
    pub execution_trace: Vec<TraceEntry>,
}

pub struct TraceEntry {
    pub trace_id: Uuid,
    pub agent_name: String,
    pub parent_chain: Vec<String>,
    pub spawned_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: u64,
    pub status: ExecutionStatus,
    pub error: Option<String>,
}

// CLI command for visualization
async fn trace_session(session_id: &str) -> Result<()> {
    let metadata = load_session_metadata(session_id)?;

    // Build tree from trace entries
    let tree = build_trace_tree(&metadata.execution_trace);

    // ASCII visualization
    println!("Session: {}", session_id);
    println!("Duration: {}ms", metadata.duration_ms());
    println!("\nExecution Trace:");
    print_trace_tree(&tree, 0);

    /*
    Output example:
    Session: 2025-01-20-refactor-auth
    Duration: 12,847ms

    Execution Trace:
    ├─ [primary] claude-sonnet-4 (8,234ms) ✓
    │  ├─ [code-reviewer] code-reviewer (2,156ms) ✓
    │  ├─ [test-generator] test-generator (1,987ms) ✓
    │  └─ [documentation-writer] documentation-writer (470ms) ✗ Error: Timeout
    */
}
```

**Why**:
- **Debugging**: Multi-agent workflows are hard to debug without execution flow visibility
- **Error Attribution**: Trace IDs enable correlating errors to specific agent in chain
- **Performance**: Parent chain shows where bottlenecks occur
- **Industry Standard**: AutoGen uses OpenTelemetry, distributed tracing is common pattern
- **Post-Mortem**: Session traces enable understanding what happened after failures

**Alternatives Considered**:
- No tracing: Hard to debug multi-agent failures, poor developer experience
- Log-only tracing: Not structured, hard to query and visualize
- Full OpenTelemetry: Overkill for MVP, can add later
- Trace to database only: Loses plaintext-first benefits

**Trade-offs**:
- **Pro**: Significantly improves debugging experience
- **Pro**: Enables performance optimization (find slow agents)
- **Pro**: Audit trail for understanding agent behavior
- **Con**: Small metadata overhead per agent spawn
- **Con**: Additional CLI command to implement
- **Mitigation**: Minimal overhead (just UUIDs and timestamps), trace command is optional

**References**:
- AutoGen observability: https://microsoft.github.io/autogen/0.2/docs/Use-Cases/agent_chat/
- Distributed tracing patterns: https://opentelemetry.io/docs/concepts/observability-primer/

## Risks / Trade-offs

### Risk 1: Token Usage with Separate LLM Calls

**Risk**: Each subagent is a separate LLM call, increasing token costs

**Mitigation**:
- Subagents only get task description (no full conversation history)
- Use `haiku` model for simple tasks (cheaper, faster)
- Cache agent prompts (frontmatter markdown reused across calls)
- Monitor token usage per session, expose in metadata

**Impact**: Medium - Acceptable for MVP, optimize in post-MVP

### Risk 2: Sequential Execution Performance

**Risk**: Multiple subagents run sequentially, slowing down complex tasks

**Mitigation**:
- Design queue for future concurrency (provider limits already planned)
- Most tasks won't need >3 subagents in MVP
- Users can see progress (not blocked waiting silently)
- Post-MVP: Add parallel execution with provider-specific limits

**Impact**: Low - Acceptable for MVP, clear path to improvement

### Risk 3: Agent Definition Validation

**Risk**: User-created agents may have invalid frontmatter or permissions

**Mitigation**:
- Strict validation on registry load (fail fast with clear errors)
- Schema validation for frontmatter (serde with #[serde(deny_unknown_fields)])
- `cru agents validate` command to check definitions
- Clear error messages: "Agent 'foo' has invalid permission 'WriteDatabase' (did you mean 'DatabaseWrite'?)"

**Impact**: Low - Good validation prevents runtime issues

### Risk 4: Session Storage Growth

**Risk**: Many sessions accumulate in `.crucible/sessions/`, filling disk

**Mitigation**:
- Sessions are write-once (no runaway growth from edits)
- Users can delete old sessions (plaintext files, easy to manage)
- Post-MVP: Add `cru sessions clean --older-than 30d` command
- Sessions are valuable (learning data), retention is feature not bug

**Impact**: Low - Growth is linear with usage, manageable

### Risk 5: Depth Limit Too Restrictive

**Risk**: Users need depth >2 for complex workflows

**Mitigation**:
- Start conservative (depth 2), increase if needed
- Design supports increasing limit (just change constant)
- Alternative: Primary agent can spawn multiple rounds of subagents sequentially
- A2A patterns may change depth requirements entirely

**Impact**: Low - Can adjust based on real usage patterns

## Migration Plan

### Phase 1: Core Agent System (Week 1)
1. Implement `crucible-agents` crate
2. Agent definition parsing and validation
3. Agent registry with discovery
4. Permission inheritance system
5. Basic spawning (no queue yet)

### Phase 2: Session Management & Queue (Week 2)
1. Session folder creation and markdown storage
2. Wikilink-based parent/child tracking
3. Execution queue with sequential processing
4. Progress observer for CLI output
5. Session metadata collection

### Phase 3: CLI Integration (Week 3)
1. Add `cru agents list` command
2. Add `cru agents validate` command
3. Integrate spawning into chat mode
4. Add default system agents (code-reviewer, test-generator, documentation-writer)
5. Testing with real workloads

### Phase 4: Documentation & Polish (Week 4)
1. Write agent creation guide
2. Example agent definitions
3. Session navigation documentation
4. Performance testing and optimization
5. User testing and feedback

### Future Phases (Post-MVP)
- Parallel execution with provider limits
- Agent-to-Agent (A2A) communication channels
- Custom tools per agent (beyond 6 MCP tools)
- Session persistence across CLI restarts
- Agent metrics and analytics
- Per-MCP-server specialized agents

## Open Questions

None for MVP - all architectural decisions made based on clarifications.

## Success Metrics

### MVP Success Criteria
- [ ] Users can define agents in `.crucible/agents/` with frontmatter
- [ ] `cru agents list` shows available agents (system + project)
- [ ] Primary agent can spawn subagent and receive markdown result
- [ ] Sessions saved to `.crucible/sessions/` with wikilinks
- [ ] Subagents cannot exceed parent permissions
- [ ] Depth limit (2) enforced with clear error
- [ ] Users see abbreviated progress updates
- [ ] Sequential queue processes multiple subagents correctly

### Quality Gates
- [ ] Agent definition validation catches all invalid frontmatter
- [ ] Permission inheritance prevents privilege escalation
- [ ] Session storage creates valid markdown with proper wikilinks
- [ ] Observer updates appear in real-time (< 100ms delay)
- [ ] Queue handles agent failures gracefully (no crash)

### User Experience Goals
- [ ] Agent creation is intuitive (similar to slash commands)
- [ ] Session navigation via wikilinks is smooth
- [ ] Progress updates are informative but not overwhelming
- [ ] Error messages are actionable and helpful
- [ ] Documentation covers common agent patterns

## References

- **Claude Code**: `.claude/commands/` pattern for slash command definitions
- **Gemini Code Assist**: Session storage and subagent progress reporting
- **OpenSpec**: Discovery pattern for specs/changes (reuse for agents)
- **openspec/changes/add-tool-system/**: Permission system patterns
- **openspec/changes/rework-cli-acp-chat/**: CLI integration patterns
- **Agent-to-Agent (A2A)**: Future direction for parallel execution and channels
