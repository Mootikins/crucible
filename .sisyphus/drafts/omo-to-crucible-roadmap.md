# Strategic Roadmap: Oh-My-OpenCode Patterns → Crucible

## Executive Summary

**The Insight**: Oh-My-OpenCode's value isn't in its tools (LSP, AST-grep) - those are commodities. Its value is in **workflow orchestration patterns** that make agents work like disciplined developers. Crucible's Rust+Lua architecture is uniquely positioned to implement these patterns with better performance, security, and composability than Bun/TypeScript.

**The Vision**: Crucible becomes the **extensible foundation** for agentic coding workflows, where:
- **Rust** handles performance-critical infrastructure (LSP, search, sessions)
- **Lua** defines workflow logic (FSMs, agent personalities, delegation rules)
- **OIL** provides composable UI building blocks

This mirrors the game engine pattern: C++/Rust for the engine, Lua for game logic.

---

## Part 1: What's UNIQUE in OMO (→ Lua Plugin)

These patterns are OMO's secret sauce. They should become a **default Crucible Lua plugin** demonstrating best practices.

### 1.1 Agent Personality System

**The Pattern**: Named agents with distinct identities, constraints, and behaviors.

```
┌─────────────────────────────────────────────────────────────────┐
│                    OMO Agent Hierarchy                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   PROMETHEUS (Planner)                                          │
│   ├── Identity: "Strategic consultant, NOT implementer"         │
│   ├── Constraints: Can ONLY write .md files                     │
│   ├── Outputs: .sisyphus/plans/*.md, .sisyphus/drafts/*.md     │
│   └── Consults: Metis (gap analysis), Momus (validation)       │
│                                                                 │
│   SISYPHUS (Primary Orchestrator)                               │
│   ├── Identity: "The boulder-roller who never quits"            │
│   ├── Behavior: Delegates via categories, tracks todos          │
│   └── FSM: Idle → Working → Delegating → Verifying → Done      │
│                                                                 │
│   ORACLE (Strategic Advisor)                                    │
│   ├── Identity: "High-IQ consultant for hard problems"          │
│   ├── Trigger: 3 failures, architecture questions               │
│   └── Model: GPT 5.2 (reasoning specialist)                     │
│                                                                 │
│   LIBRARIAN (Research)                                          │
│   ├── Identity: "Documentation and codebase archaeologist"      │
│   └── Output: Synthesized findings, code references             │
│                                                                 │
│   EXPLORE (Fast Search)                                         │
│   ├── Identity: "Blazing fast contextual grep"                  │
│   └── Model: Cheap/fast (Haiku-class)                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Lua Implementation Sketch**:
```lua
-- plugins/crucible-agents/agents/prometheus.lua
return {
  name = "prometheus",
  identity = "Strategic planning consultant",
  
  -- Hard constraints enforced by hooks
  constraints = {
    can_write = { "%.md$" },  -- Only markdown
    cannot_write = { "%.rs$", "%.lua$", "%.ts$" },
    output_dirs = { ".sisyphus/plans/", ".sisyphus/drafts/" },
  },
  
  -- Personality injected into system prompt
  personality = [[
    YOU ARE A PLANNER. YOU ARE NOT AN IMPLEMENTER.
    When user says "do X", interpret as "create a work plan for X".
    Your only outputs: Questions, Research requests, Work plans.
  ]],
  
  -- FSM states
  states = {
    interview = { next = { "planning", "researching" } },
    researching = { next = { "interview", "planning" } },
    planning = { next = { "validating" } },
    validating = { next = { "complete", "planning" } },  -- Momus loop
    complete = { final = true },
  },
  
  -- Consults other agents
  consults = { "metis", "momus", "librarian", "explore" },
}
```

**Why This Matters**: Agent personalities create **predictable behavior**. Users learn what each agent does. The system becomes legible.

---

### 1.2 Category-Based Task Routing

**The Pattern**: Tasks are classified into categories, each with:
- Model selection (cheap vs expensive)
- Prompt appends (context injection)
- Capability constraints

```
┌────────────────────────────────────────────────────────────────┐
│                    Category Routing                             │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  delegate_task(category="visual-engineering", ...)             │
│       │                                                        │
│       ▼                                                        │
│  ┌─────────────────────────────────────────────────────┐      │
│  │ Category: visual-engineering                         │      │
│  │ Model: gemini-3-pro (visual specialist)              │      │
│  │ Prompt Append:                                       │      │
│  │   "Design-first mindset. Bold aesthetic choices.     │      │
│  │    Distinctive typography. High-impact animations."  │      │
│  └─────────────────────────────────────────────────────┘      │
│                                                                │
│  delegate_task(category="quick", ...)                          │
│       │                                                        │
│       ▼                                                        │
│  ┌─────────────────────────────────────────────────────┐      │
│  │ Category: quick                                      │      │
│  │ Model: claude-haiku (cheap/fast)                     │      │
│  │ Prompt Append:                                       │      │
│  │   "CALLER WARNING: Less capable model.               │      │
│  │    Your prompt MUST be EXHAUSTIVELY EXPLICIT."       │      │
│  │   (Forces orchestrator to write better prompts)      │      │
│  └─────────────────────────────────────────────────────┘      │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

**Default Categories**:
| Category | Model Class | Use Case |
|----------|-------------|----------|
| `visual-engineering` | Gemini (visual) | UI/UX, frontend, design |
| `quick` | Haiku (cheap) | Trivial tasks, single-file changes |
| `ultrabrain` | o1/GPT-5.2 (reasoning) | Complex logic, architecture |
| `artistry` | Opus (creative) | Novel ideas, creative work |
| `writing` | Sonnet (prose) | Documentation, READMEs |
| `unspecified-low` | Sonnet | Moderate unclassified work |
| `unspecified-high` | Opus | Substantial unclassified work |

**Lua Implementation Sketch**:
```lua
-- plugins/crucible-agents/categories.lua
return {
  ["visual-engineering"] = {
    model = "gemini/gemini-2.0-flash",
    prompt_append = [[
      <Category_Context>
      Design-first mindset. Bold aesthetic choices over safe defaults.
      Distinctive typography. High-impact animations.
      </Category_Context>
    ]],
  },
  
  ["quick"] = {
    model = "anthropic/claude-3-haiku",
    prompt_append = [[
      <Caller_Warning>
      THIS USES A LESS CAPABLE MODEL.
      Your prompt MUST be EXHAUSTIVELY EXPLICIT.
      </Caller_Warning>
    ]],
    -- This is clever: warns the ORCHESTRATOR to write better prompts
  },
  
  -- Users can define custom categories
  ["my-domain"] = {
    model = "openai/gpt-4o",
    prompt_append = "You are a domain expert in X...",
  },
}
```

---

### 1.3 Ralph Loop / Boulder State (FSM Workflows)

**The Pattern**: Agent runs in a loop until completion, with:
- State persistence (survives restarts)
- Iteration tracking
- Completion detection
- Automatic continuation

```
┌────────────────────────────────────────────────────────────────┐
│                    Ralph Loop FSM                               │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│   .sisyphus/ralph-loop.local.md (state file)                   │
│   ┌──────────────────────────────────────────────────────┐    │
│   │ active: true                                          │    │
│   │ iteration: 7                                          │    │
│   │ max_iterations: 100                                   │    │
│   │ completion_promise: "DONE"                            │    │
│   │ prompt: "Implement auth system"                       │    │
│   │ session_id: "ses_abc123"                              │    │
│   │ ultrawork: true                                       │    │
│   └──────────────────────────────────────────────────────┘    │
│                                                                │
│   State Transitions:                                           │
│                                                                │
│   ┌─────┐    start     ┌─────────┐    idle     ┌──────────┐  │
│   │IDLE │ ──────────▶ │ WORKING │ ──────────▶ │ CHECKING │  │
│   └─────┘              └─────────┘              └──────────┘  │
│      ▲                      ▲                       │         │
│      │                      │     has todos         │         │
│      │                      └───────────────────────┘         │
│      │                                              │         │
│      │                      no todos / "DONE"       ▼         │
│      │                                         ┌────────┐     │
│      └─────────────────────────────────────────│COMPLETE│     │
│                                                └────────┘     │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

**Lua Implementation Sketch**:
```lua
-- plugins/crucible-agents/workflows/ralph_loop.lua
local M = {}

M.state_file = ".sisyphus/ralph-loop.local.md"

function M.start(prompt, opts)
  local state = {
    active = true,
    iteration = 0,
    max_iterations = opts.max_iterations or 100,
    completion_promise = opts.completion_promise or "DONE",
    prompt = prompt,
    started_at = os.date("!%Y-%m-%dT%H:%M:%SZ"),
  }
  M.write_state(state)
  return state
end

-- Handler: session:idle event
-- @handler event="session:idle"
function M.on_idle(ctx, event)
  local state = M.read_state()
  if not state or not state.active then return end
  
  -- Check for completion
  local transcript = crucible.get_transcript(state.session_id)
  if transcript:match(state.completion_promise) then
    state.active = false
    M.write_state(state)
    crucible.toast("Ralph loop complete!")
    return
  end
  
  -- Check for incomplete todos
  local todos = crucible.search({ query = "- [ ]", type = "text" })
  if #todos > 0 then
    state.iteration = state.iteration + 1
    M.write_state(state)
    
    -- Continue working
    return {
      inject = "Continue working. Mark tasks complete when done. Do not stop until all tasks are done."
    }
  end
end

return M
```

---

### 1.4 Todo Continuation Enforcer

**The Pattern**: On session idle, check for incomplete todos and force continuation.

This is the **"keeps Sisyphus rolling that boulder"** mechanism.

```lua
-- plugins/crucible-agents/hooks/todo_enforcer.lua

-- @handler event="session:idle" priority=10
function enforce_todos(ctx, event)
  -- Skip certain agents (planners don't execute)
  local agent = ctx.session.agent
  if agent == "prometheus" or agent == "plan" then
    return
  end
  
  -- Check for incomplete checkboxes
  local content = crucible.read_file(ctx.session.transcript)
  local incomplete = content:match("%- %[ %]")
  
  if incomplete then
    crucible.toast("Incomplete todos detected. Continuing...", 2000)
    
    return {
      inject = [[
        Proceed without asking permission.
        Mark each task complete when finished.
        Do not stop until all tasks are done.
      ]]
    }
  end
end
```

---

### 1.5 Comment Checker

**The Pattern**: Prevent AI from adding excessive comments to code.

```lua
-- plugins/crucible-agents/hooks/comment_checker.lua

-- @handler event="tool:after" pattern="edit*"
function check_comments(ctx, event)
  if event.tool ~= "edit" then return end
  
  local diff = event.result.diff
  local added_comments = count_added_comments(diff)
  
  if added_comments > 3 then
    return {
      inject = [[
        WARNING: You added excessive comments.
        Code should be self-documenting.
        Remove unnecessary comments or justify each one.
      ]]
    }
  end
end
```

---

### 1.6 Context Window Monitor

**The Pattern**: Track token usage, warn at thresholds, trigger compaction.

```lua
-- plugins/crucible-agents/hooks/context_monitor.lua

local WARN_THRESHOLD = 0.70  -- 70% of limit
local COMPACT_THRESHOLD = 0.85  -- 85% triggers compaction

-- @handler event="tool:after"
function monitor_context(ctx, event)
  local usage = ctx.session.token_usage
  local limit = ctx.session.token_limit
  local ratio = usage / limit
  
  if ratio > COMPACT_THRESHOLD then
    crucible.toast(string.format("Context at %d%%. Compacting...", ratio * 100))
    crucible.session.compact(ctx.session.id)
    return
  end
  
  if ratio > WARN_THRESHOLD then
    crucible.toast(string.format("Context at %d%%. Consider compacting.", ratio * 100))
  end
end
```

---

### 1.7 Keyword Detection (Ultrawork, Think Mode)

**The Pattern**: Magic keywords trigger workflow modes.

```lua
-- plugins/crucible-agents/hooks/keyword_detector.lua

local KEYWORDS = {
  ultrawork = { "ultrawork", "ulw" },
  think = { "ultrathink", "think hard", "reason carefully" },
}

-- @handler event="user:prompt"
function detect_keywords(ctx, event)
  local prompt = event.prompt:lower()
  
  for mode, keywords in pairs(KEYWORDS) do
    for _, kw in ipairs(keywords) do
      if prompt:find(kw, 1, true) then
        return handle_mode(mode, ctx, event)
      end
    end
  end
end

function handle_mode(mode, ctx, event)
  if mode == "ultrawork" then
    -- Start ralph loop with full agent suite
    return {
      inject = [[
        Maximum precision engaged. All agents at your disposal.
        Explore → Plan → Implement → Verify.
        Do not stop until complete.
      ]],
      start_ralph_loop = true,
    }
  elseif mode == "think" then
    -- Switch to high-reasoning model
    ctx.session.model = "openai/o1"
    return { model_override = "openai/o1" }
  end
end
```

---

## Part 2: What's GENERIC (→ Rust Crates)

These are **commodity features** that should be implemented in Rust for performance/reliability, then exposed to Lua.

### 2.1 LSP Integration → `crucible-lsp`

**Why Rust**: LSP is a performance-critical protocol. Language servers can be slow. We want:
- Async communication
- Connection pooling
- Smart caching
- Timeout handling

**Scope**:
| Tool | Purpose | Priority |
|------|---------|----------|
| `lsp_goto_definition` | Navigate to definitions | HIGH |
| `lsp_find_references` | Find all usages | HIGH |
| `lsp_rename` | Workspace-wide rename | HIGH |
| `lsp_diagnostics` | Errors/warnings | HIGH |
| `lsp_hover` | Type info, docs | MEDIUM |
| `lsp_document_symbols` | File outline | MEDIUM |
| `lsp_workspace_symbols` | Cross-file search | MEDIUM |
| `lsp_code_actions` | Quick fixes | LOW |

**Lua Exposure**:
```lua
-- From Rust crate via FFI
local def = crucible.lsp.goto_definition("src/main.rs", 42, 15)
local refs = crucible.lsp.find_references("src/lib.rs", 10, 5)
local diags = crucible.lsp.diagnostics("src/main.rs", "error")
```

---

### 2.2 AST-Grep Integration → `crucible-ast-grep`

**Why Rust**: ast-grep is already a Rust binary. We can:
- Embed it directly (no subprocess)
- Cache parsed ASTs
- Batch queries

**Scope**:
| Tool | Purpose | Priority |
|------|---------|----------|
| `ast_grep_search` | Pattern-based code search | HIGH |
| `ast_grep_replace` | Semantic refactoring | MEDIUM |

**Lua Exposure**:
```lua
local matches = crucible.ast.search({
  pattern = "console.log($MSG)",
  lang = "typescript",
  paths = { "src/" },
})

crucible.ast.replace({
  pattern = "console.log($MSG)",
  rewrite = "logger.info($MSG)",
  lang = "typescript",
  dry_run = false,
})
```

---

### 2.3 Background Task Manager → `crucible-daemon` enhancement

**Why Rust**: Concurrency, task scheduling, resource management.

**Current State**: Crucible daemon already has session management. Extend with:
- Parallel session execution
- Concurrency limits per provider/model
- Task queue with priorities
- Progress tracking

**Lua Exposure**:
```lua
local task = crucible.background.spawn({
  agent = "explore",
  prompt = "Find all usages of AuthService",
  on_complete = function(result) ... end,
})

crucible.background.cancel(task.id)
local status = crucible.background.status(task.id)
```

---

### 2.4 Session Compaction → `crucible-core` enhancement

**Why Rust**: Token counting, summarization prompts, state management.

**Scope**:
- Automatic compaction triggers
- Structured summary format (preserves user requests, constraints)
- Token budget management

---

## Part 3: Trend Analysis - Where Agent Coding is Going

### 3.1 The FSM Convergence

**Observation**: Ralph loops, todo enforcers, and boulder state are all **finite state machines**.

```
┌────────────────────────────────────────────────────────────────┐
│                    The FSM Pattern                              │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  Traditional Agent:                                            │
│    User → Agent → Response → User → Agent → ...               │
│    (Stateless, no persistence, quits early)                   │
│                                                                │
│  FSM-Driven Agent:                                             │
│    User → [IDLE] → [WORKING] → [DELEGATING] → [VERIFYING]    │
│              ↑                                    │            │
│              └────────────────────────────────────┘            │
│    (Stateful, persistent, continues until done)               │
│                                                                │
│  This is what makes OMO feel "disciplined"                    │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

**Implication for Crucible**: First-class FSM support in Lua:

```lua
-- Define workflow as FSM
local auth_workflow = crucible.workflow({
  name = "implement-auth",
  
  states = {
    research = {
      on_enter = function(ctx)
        ctx.delegate("librarian", "Find auth patterns in codebase")
        ctx.delegate("explore", "Find existing auth code")
      end,
      transitions = {
        research_complete = "planning",
      },
    },
    
    planning = {
      on_enter = function(ctx)
        ctx.switch_agent("prometheus")
      end,
      transitions = {
        plan_approved = "implementing",
      },
    },
    
    implementing = {
      on_enter = function(ctx)
        ctx.switch_agent("sisyphus")
        ctx.start_ralph_loop()
      end,
      transitions = {
        todos_complete = "verifying",
      },
    },
    
    verifying = {
      on_enter = function(ctx)
        ctx.run_tests()
        ctx.run_lsp_diagnostics()
      end,
      transitions = {
        verified = "complete",
        failed = "implementing",
      },
    },
    
    complete = { final = true },
  },
})
```

### 3.2 Smart Blocks / Composable Units

**Observation**: OIL is "smart blocks for UI". Agent workflows need the same.

```
┌────────────────────────────────────────────────────────────────┐
│                    Smart Blocks Vision                          │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  OIL Blocks (UI):                                              │
│    Box, Text, Input, Button → compose into complex UIs        │
│                                                                │
│  Workflow Blocks (Agents):                                     │
│    Research, Plan, Implement, Verify → compose into workflows │
│                                                                │
│  Example:                                                      │
│                                                                │
│    workflow "add-feature" {                                    │
│      research(agents=["librarian", "explore"])                 │
│      plan(agent="prometheus", require_approval=true)           │
│      implement(agent="sisyphus", until="todos_complete")       │
│      verify(run=["tests", "lsp_diagnostics"])                  │
│    }                                                           │
│                                                                │
│  Lua makes this trivial to express                             │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### 3.3 The Rust+Lua Advantage

| Aspect | Bun/TypeScript (OMO) | Rust+Lua (Crucible) |
|--------|---------------------|---------------------|
| **Startup** | ~200ms | ~10ms |
| **Memory** | ~100MB | ~20MB |
| **Concurrency** | Good (V8) | Excellent (Tokio) |
| **Security** | NPM supply chain | Rust + sandboxed Lua |
| **Hot reload** | Needs restart | Lua hot reload |
| **Extensibility** | JS ecosystem | LuaRocks + Rust crates |
| **Performance** | JIT (good) | Native (excellent) |
| **Error handling** | Exceptions | Result types |

**The Pattern**: Game engines use C++/Rust for performance-critical systems, Lua for game logic. Crucible follows the same pattern for agent systems.

---

## Part 4: Long-Scope Roadmap

### 4.1 MUST HAVES (Core Extensibility)

| Feature | Type | Rationale | Effort |
|---------|------|-----------|--------|
| **Agent personality system** | Lua plugin | Core OMO differentiator | 1 week |
| **Category-based routing** | Lua plugin | Task → model mapping | 3 days |
| **Ralph loop FSM** | Lua plugin | Continuous work | 1 week |
| **Todo enforcer** | Lua plugin | Prevents early quit | 2 days |
| **LSP integration** | Rust crate | IDE-quality refactoring | 2-3 weeks |
| **Background tasks** | Rust (daemon) | Parallel agent execution | 1-2 weeks |

### 4.2 SHOULD HAVES (Enhanced UX)

| Feature | Type | Rationale | Effort |
|---------|------|-----------|--------|
| **AST-grep integration** | Rust crate | Semantic search | 1 week |
| **Context window monitor** | Lua hook | Token management | 2 days |
| **Session compaction** | Rust + Lua | Long conversations | 1 week |
| **Comment checker** | Lua hook | Code quality | 1 day |
| **Keyword detection** | Lua hook | Magic modes (ultrawork) | 1 day |
| **Directory context injection** | Lua hook | Auto-inject AGENTS.md | 2 days |

### 4.3 NICE TO HAVES (Polish)

| Feature | Type | Rationale | Effort |
|---------|------|-----------|--------|
| **Workflow DSL** | Lua | Composable FSMs | 2 weeks |
| **Model fallback chains** | Lua | Reliability | 3 days |
| **Session notifications** | Rust + OS | Cross-platform alerts | 1 week |
| **Auto-update checker** | Lua | Version management | 1 day |
| **Think mode detection** | Lua hook | Auto model upgrade | 1 day |

### 4.4 Implementation Order (Recommended)

```
Phase 1: Foundation (Weeks 1-2)
├── Agent personality system (Lua)
├── Category routing (Lua)
└── Todo enforcer (Lua)

Phase 2: Workflows (Weeks 3-4)
├── Ralph loop FSM (Lua)
├── Boulder state persistence (Lua)
└── Background tasks (Rust daemon enhancement)

Phase 3: Tooling (Weeks 5-7)
├── LSP integration (Rust crate)
├── AST-grep (Rust crate)
└── Context monitor (Lua)

Phase 4: Polish (Weeks 8-9)
├── Session compaction (Rust + Lua)
├── Keyword detection (Lua)
└── Directory injection (Lua)

Phase 5: Advanced (Weeks 10+)
├── Workflow DSL (Lua)
├── Model fallbacks (Lua)
└── Notifications (Rust)
```

---

## Part 5: Default Plugin Structure

```
~/.config/crucible/plugins/crucible-agents/
├── plugin.yaml
├── init.lua                    # Main entry, registers all components
│
├── agents/                     # Agent personality definitions
│   ├── prometheus.lua          # Planner (markdown-only)
│   ├── sisyphus.lua            # Primary orchestrator
│   ├── oracle.lua              # Strategic advisor
│   ├── librarian.lua           # Research specialist
│   └── explore.lua             # Fast codebase search
│
├── categories/                 # Task routing rules
│   ├── visual.lua
│   ├── quick.lua
│   ├── ultrabrain.lua
│   └── custom.lua              # User-defined categories
│
├── workflows/                  # FSM definitions
│   ├── ralph_loop.lua          # Continuous work loop
│   ├── boulder_state.lua       # Work plan tracking
│   └── ultrawork.lua           # Full-auto mode
│
├── hooks/                      # Event handlers
│   ├── todo_enforcer.lua
│   ├── comment_checker.lua
│   ├── context_monitor.lua
│   ├── keyword_detector.lua
│   └── directory_injector.lua
│
└── tools/                      # Custom tools
    └── delegate_task.lua       # Agent delegation
```

**plugin.yaml**:
```yaml
name: crucible-agents
version: 1.0.0
description: Oh-My-OpenCode patterns for Crucible

capabilities:
  - filesystem
  - shell
  - vault

dependencies:
  crucible: ">=0.1.0"

# User-configurable settings
config:
  default_orchestrator: sisyphus
  ralph_loop:
    max_iterations: 100
    completion_promise: "DONE"
  categories:
    quick:
      model: anthropic/claude-3-haiku
    visual:
      model: google/gemini-2.0-flash
```

---

## Summary

**The Core Insight**: OMO's value is in **workflow discipline**, not tools. Crucible should:

1. **Implement OMO's unique patterns as a default Lua plugin** - agent personalities, category routing, Ralph loops, todo enforcement

2. **Build generic tooling as Rust crates** - LSP, AST-grep, background tasks, session compaction

3. **Embrace the FSM paradigm** - workflows are state machines, Lua makes them trivial to express

4. **Position for the future** - "smart blocks" for agent workflows, composable units, hot-reloadable logic

**The Result**: Crucible becomes the **most extensible agent coding platform**, where Rust provides performance/security and Lua provides flexibility/accessibility.

---

*This is the game engine pattern applied to AI agents: native performance where it matters, scripting flexibility where it counts.*
