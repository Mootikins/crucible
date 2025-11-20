# Implementation Tasks

## 1. Core Infrastructure

### 1.1 Create crucible-agents Crate
- [ ] 1.1.1 Create `crates/crucible-agents/` directory structure
- [ ] 1.1.2 Add Cargo.toml with dependencies (serde, tokio, anyhow, schemars)
- [ ] 1.1.3 Create module structure (lib.rs, registry.rs, definition.rs, etc.)
- [ ] 1.1.4 Add crate to workspace Cargo.toml

### 1.2 Agent Definition Parsing
- [ ] 1.2.1 Define `AgentDefinition` struct with serde frontmatter parsing
- [ ] 1.2.2 Implement frontmatter validation (required fields: name, description, model, permissions)
- [ ] 1.2.3 Add markdown content extraction (everything after frontmatter)
- [ ] 1.2.4 Write unit tests for valid and invalid agent definitions
- [ ] 1.2.5 Add helpful error messages for common validation failures

### 1.3 Permission System
- [ ] 1.3.1 Define `Permission` enum (FilesystemRead, FilesystemWrite, SemanticSearch, etc.)
- [ ] 1.3.2 Implement `PermissionSet` with HashSet storage
- [ ] 1.3.3 Implement `PermissionSetBuilder` with from_parent() and constrain()
- [ ] 1.3.4 Add validation to prevent permission escalation
- [ ] 1.3.5 Write tests for permission inheritance and constraints
- [ ] 1.3.6 Ensure default kiln read permissions always granted

### 1.4 Agent Registry
- [ ] 1.4.1 Implement `AgentRegistry` struct with HashMap storage
- [ ] 1.4.2 Add discover() method to scan system and project agent directories
- [ ] 1.4.3 Implement project override logic (project agents override system agents)
- [ ] 1.4.4 Add get_agent() method with enabled filtering
- [ ] 1.4.5 Add list_agents() method returning all enabled agents
- [ ] 1.4.6 Write tests for discovery, override, and retrieval
- [ ] 1.4.7 Handle missing directories gracefully (don't fail if no agents exist)

## 2. Agent Execution

### 2.1 Spawning Infrastructure
- [ ] 2.1.1 Define `AgentSpawnRequest` struct (agent_name, task_description, permissions, depth)
- [ ] 2.1.2 Define `AgentResult` struct (markdown content, metadata, status)
- [ ] 2.1.3 Implement spawn_agent() function with depth validation
- [ ] 2.1.4 Add depth limit enforcement (hardcoded to 2)
- [ ] 2.1.5 Write tests for depth limiting and rejection

### 2.2 LLM Session Creation
- [ ] 2.2.1 Implement create_llm_session() to construct agent prompt + task
- [ ] 2.2.2 Add context isolation (no parent history, only task description)
- [ ] 2.2.3 Configure kiln read tools for all subagents
- [ ] 2.2.4 Add model selection from agent definition
- [ ] 2.2.5 Write tests for session creation with different models

### 2.3 Execution Queue
- [ ] 2.3.1 Define `ExecutionQueue` struct with VecDeque and current task tracking
- [ ] 2.3.2 Define `AgentTask` struct with oneshot channel for results
- [ ] 2.3.3 Implement spawn_agent() to queue or execute immediately
- [ ] 2.3.4 Implement execute_task() to run agent and collect result
- [ ] 2.3.5 Add sequential processing (wait for current before starting next)
- [ ] 2.3.6 Design for future concurrency (max_concurrent parameter placeholder)
- [ ] 2.3.7 Write tests for single task, multiple tasks, and queue ordering

### 2.4 Error Handling
- [ ] 2.4.1 Implement error capture for failed subagent executions
- [ ] 2.4.2 Create error result format (frontmatter with error details)
- [ ] 2.4.3 Ensure queue continues after agent failure (graceful degradation)
- [ ] 2.4.4 Write tests for various error scenarios (timeout, invalid agent, permission error)

### 2.5 Reflection System
- [ ] 2.5.1 Add reflection fields to AgentDefinition (enable_reflection, max_retries, reflection_criteria)
- [ ] 2.5.2 Implement self_evaluate() function with LLM-based critique
- [ ] 2.5.3 Parse reflection_criteria from frontmatter (YAML multiline string)
- [ ] 2.5.4 Implement execute_with_reflection() retry loop
- [ ] 2.5.5 Implement refine_task_with_critique() to incorporate feedback
- [ ] 2.5.6 Track best attempt when max_retries exceeded
- [ ] 2.5.7 Capture reflection history in result metadata
- [ ] 2.5.8 Write tests for reflection success, failure, and max retries
- [ ] 2.5.9 Document reflection patterns and best practices

## 3. Session Management

### 3.1 Session Folder Creation
- [ ] 3.1.1 Implement create_session_folder() with timestamp and description
- [ ] 3.1.2 Add sanitization for folder names (remove special chars)
- [ ] 3.1.3 Ensure uniqueness (append counter if timestamp collision)
- [ ] 3.1.4 Create session.md for primary agent conversation
- [ ] 3.1.5 Create metadata.json for session-level data
- [ ] 3.1.6 Write tests for folder creation and naming

### 3.2 Subagent Result Storage
- [ ] 3.2.1 Implement save_subagent_result() to write markdown file
- [ ] 3.2.2 Generate unique filename ({agent-name}-{task-id}.md)
- [ ] 3.2.3 Write frontmatter with execution metadata (timestamps, model, tokens)
- [ ] 3.2.4 Write markdown content from agent result
- [ ] 3.2.5 Update parent session.md with wikilink to subagent file
- [ ] 3.2.6 Add parent session reference in subagent frontmatter
- [ ] 3.2.7 Write tests for result storage and wikilink creation

### 3.3 Session Metadata
- [ ] 3.3.1 Define metadata.json schema (session_id, timestamps, agents, tokens)
- [ ] 3.3.2 Implement metadata collection during execution
- [ ] 3.3.3 Update metadata.json incrementally as subagents complete
- [ ] 3.3.4 Add total token usage calculation
- [ ] 3.3.5 Write tests for metadata collection and serialization

### 3.4 Tracing and Observability
- [ ] 3.4.1 Add trace_id (Uuid) to AgentSpawnRequest and AgentResult
- [ ] 3.4.2 Add parent_chain (Vec<String>) to track spawning hierarchy
- [ ] 3.4.3 Generate unique trace_id on each spawn
- [ ] 3.4.4 Propagate parent_chain from parent to child (+append child name)
- [ ] 3.4.5 Add TraceEntry struct to SessionMetadata
- [ ] 3.4.6 Capture trace entries during execution (spawn, complete, error)
- [ ] 3.4.7 Include trace_id in all log messages
- [ ] 3.4.8 Write tests for trace ID generation and propagation
- [ ] 3.4.9 Write tests for parent chain tracking

## 4. Observability

### 4.1 Progress Observer
- [ ] 4.1.1 Define `ProgressObserver` trait (on_agent_spawn, on_agent_complete, on_agent_error)
- [ ] 4.1.2 Implement `CliObserver` for stdout progress updates
- [ ] 4.1.3 Add formatted output with visual indicators (→ for spawn)
- [ ] 4.1.4 Extract summary from subagent result (first 100 chars of ## Summary)
- [ ] 4.1.5 Ensure updates appear with < 100ms delay
- [ ] 4.1.6 Write tests for observer notification and formatting

### 4.2 Progress Integration
- [ ] 4.2.1 Wire observer into spawn_agent() and execute_task()
- [ ] 4.2.2 Call on_agent_spawn when spawning starts
- [ ] 4.2.3 Call on_agent_complete with abbreviated summary
- [ ] 4.2.4 Call on_agent_error on failures
- [ ] 4.2.5 Ensure thread-safety for future concurrent execution

### 4.3 Human Approval Gates
- [ ] 4.3.1 Add requires_approval and approval_timeout fields to AgentDefinition
- [ ] 4.3.2 Parse requires_approval from frontmatter (list of Permission)
- [ ] 4.3.3 Implement request_approval() function with user prompt
- [ ] 4.3.4 Add ApprovalResponse enum (Approved, Denied, ApprovedWithModifications)
- [ ] 4.3.5 Implement approval UI in CLI observer (approve/deny/modify)
- [ ] 4.3.6 Add approval timeout with configurable behavior (abort/auto-approve)
- [ ] 4.3.7 Implement parameter modification and validation at approval gate
- [ ] 4.3.8 Log approvals/denials in session metadata
- [ ] 4.3.9 Add pre-execution approval check for configured permissions
- [ ] 4.3.10 Write tests for approval workflows (approve, deny, modify, timeout)
- [ ] 4.3.11 Add diff/preview for destructive operations (FilesystemWrite, DatabaseWrite)

### 4.4 Session Trace Visualization
- [ ] 4.4.1 Implement build_trace_tree() to convert trace entries to tree structure
- [ ] 4.4.2 Implement print_trace_tree() for ASCII visualization
- [ ] 4.4.3 Add `cru sessions trace {session-id}` command
- [ ] 4.4.4 Display execution flow with timing and status
- [ ] 4.4.5 Visually distinguish failed agents in tree output
- [ ] 4.4.6 Include token usage and duration per agent in trace
- [ ] 4.4.7 Add option for JSON output (`--json` flag)
- [ ] 4.4.8 Write tests for trace tree building and visualization

## 5. CLI Integration

### 5.1 Agent Management Commands
- [ ] 5.1.1 Add `cru agents` subcommand group to CLI
- [ ] 5.1.2 Implement `cru agents list` command (table format with name, description, model, source)
- [ ] 5.1.3 Implement `cru agents show {name}` command (full details)
- [ ] 5.1.4 Implement `cru agents validate` command (all agents)
- [ ] 5.1.5 Implement `cru agents validate {name}` command (specific agent)
- [ ] 5.1.6 Add JSON output mode for scripting (`--json` flag)
- [ ] 5.1.7 Write integration tests for all commands

### 5.2 Chat Mode Integration
- [ ] 5.2.1 Add spawn_agent tool to primary agent's available tools
- [ ] 5.2.2 Wire tool invocation to crucible-agents spawning logic
- [ ] 5.2.3 Pass primary agent's permissions to spawner
- [ ] 5.2.4 Track session folder for current chat session
- [ ] 5.2.5 Ensure subagent results saved to session folder
- [ ] 5.2.6 Write integration tests for chat mode with subagents

### 5.3 Session Navigation
- [ ] 5.3.1 Add `cru sessions list` command (show available sessions)
- [ ] 5.3.2 Add `cru sessions show {session-id}` command (display session structure)
- [ ] 5.3.3 Support wikilink navigation in terminal (optional, stretch goal)
- [ ] 5.3.4 Write tests for session listing and display

## 6. Default System Agents

### 6.1 Create System Agent Definitions
- [ ] 6.1.1 Create `~/.config/crucible/agents/` directory structure
- [ ] 6.1.2 Write code-reviewer.md agent definition
- [ ] 6.1.3 Write test-generator.md agent definition
- [ ] 6.1.4 Write documentation-writer.md agent definition
- [ ] 6.1.5 Write refactoring-assistant.md agent definition
- [ ] 6.1.6 Validate all system agent definitions

### 6.2 System Agent Prompts
- [ ] 6.2.1 Write comprehensive prompt for code-reviewer (focus on bugs, performance, style)
- [ ] 6.2.2 Write comprehensive prompt for test-generator (unit tests, edge cases)
- [ ] 6.2.3 Write comprehensive prompt for documentation-writer (clear explanations)
- [ ] 6.2.4 Write comprehensive prompt for refactoring-assistant (SOLID, DRY, patterns)
- [ ] 6.2.5 Test prompts with real tasks to ensure quality

## 7. Documentation

### 7.1 User Documentation
- [ ] 7.1.1 Write agent creation guide (how to define custom agents)
- [ ] 7.1.2 Write agent frontmatter reference (all available fields)
- [ ] 7.1.3 Write permission system documentation (inheritance, constraints)
- [ ] 7.1.4 Write session navigation guide (wikilinks, folder structure)
- [ ] 7.1.5 Write troubleshooting guide (common errors and fixes)

### 7.2 Developer Documentation
- [ ] 7.2.1 Document AgentRegistry API
- [ ] 7.2.2 Document spawning API
- [ ] 7.2.3 Document session management API
- [ ] 7.2.4 Document permission system API
- [ ] 7.2.5 Add architecture diagrams to design.md (if needed)

### 7.3 Example Agents
- [ ] 7.3.1 Create example project-specific agent (e.g., kiln-specific reviewer)
- [ ] 7.3.2 Document example multi-step workflow using subagents
- [ ] 7.3.3 Create tutorial: "Building Your First Custom Agent"

## 8. Testing and Validation

### 8.1 Unit Tests
- [ ] 8.1.1 Agent definition parsing tests (valid, invalid, edge cases)
- [ ] 8.1.2 Permission system tests (inheritance, escalation, constraints)
- [ ] 8.1.3 Registry discovery tests (system, project, override)
- [ ] 8.1.4 Queue execution tests (single, multiple, ordering)
- [ ] 8.1.5 Session storage tests (folder creation, wikilinks, metadata)

### 8.2 Integration Tests
- [ ] 8.2.1 End-to-end test: spawn subagent and verify result
- [ ] 8.2.2 Test: primary agent spawns multiple subagents sequentially
- [ ] 8.2.3 Test: depth limit enforcement with clear errors
- [ ] 8.2.4 Test: permission inheritance and validation
- [ ] 8.2.5 Test: session folder structure and wikilinks
- [ ] 8.2.6 Test: CLI commands (list, show, validate)

### 8.3 Real-World Testing
- [ ] 8.3.1 Test with real code review task
- [ ] 8.3.2 Test with real test generation task
- [ ] 8.3.3 Test with multi-step workflow (research → plan → implement)
- [ ] 8.3.4 Test with user-created custom agent
- [ ] 8.3.5 Test error handling (invalid agent, permission denied, depth exceeded)

## 9. Performance and Optimization

### 9.1 Performance Baseline
- [ ] 9.1.1 Measure agent discovery time (should be < 100ms for 50 agents)
- [ ] 9.1.2 Measure spawning overhead (should be < 50ms)
- [ ] 9.1.3 Measure session storage time (should be < 10ms per file)
- [ ] 9.1.4 Profile memory usage (should be < 10MB for agent system)

### 9.2 Optimizations (if needed)
- [ ] 9.2.1 Cache parsed agent definitions (avoid re-parsing)
- [ ] 9.2.2 Lazy load agent prompts (only when spawning)
- [ ] 9.2.3 Async file I/O for session storage (non-blocking)

## 10. Future-Proofing

### 10.1 Prepare for Concurrency
- [ ] 10.1.1 Add max_concurrent parameter to ExecutionQueue (default 1)
- [ ] 10.1.2 Add provider-specific limits config structure (not implemented yet)
- [ ] 10.1.3 Ensure ProgressObserver is thread-safe
- [ ] 10.1.4 Document how to enable concurrency in future

### 10.2 Prepare for A2A Communication
- [ ] 10.2.1 Document agent communication patterns
- [ ] 10.2.2 Design session-based message passing structure (not implemented)
- [ ] 10.2.3 Consider channel-based communication for future

### 10.3 Extensibility Hooks
- [ ] 10.3.1 Add hooks for custom agent validation
- [ ] 10.3.2 Add hooks for custom session storage (beyond markdown)
- [ ] 10.3.3 Add hooks for custom observers (beyond CLI stdout)

## 11. Deployment

### 11.1 Pre-Deployment Checklist
- [ ] 11.1.1 All tests passing (unit + integration)
- [ ] 11.1.2 Documentation complete and reviewed
- [ ] 11.1.3 System agents validated and tested
- [ ] 11.1.4 Performance benchmarks meet targets
- [ ] 11.1.5 Example agents created and documented

### 11.2 Deployment Steps
- [ ] 11.2.1 Merge to main branch
- [ ] 11.2.2 Create system agent directory in installer
- [ ] 11.2.3 Update CLI help text with new commands
- [ ] 11.2.4 Release notes with agent system features
- [ ] 11.2.5 User migration guide (if applicable)

### 11.3 Post-Deployment
- [ ] 11.3.1 Monitor for user-reported issues
- [ ] 11.3.2 Collect feedback on agent system UX
- [ ] 11.3.3 Gather metrics on agent usage patterns
- [ ] 11.3.4 Plan next iteration based on feedback
