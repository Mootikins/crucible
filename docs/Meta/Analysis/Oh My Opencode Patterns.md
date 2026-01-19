---
title: Patterns from oh-my-opencode
date: 2026-01-01
tags: [research, patterns, agents, context-management]
source: https://github.com/code-yeongyu/oh-my-opencode
---

# Patterns from oh-my-opencode for Crucible

Research synthesis of useful patterns from the oh-my-opencode repository (OpenCode plugin for Claude Code).

## Overview

oh-my-opencode is an OpenCode plugin implementing advanced agent orchestration with:
- Multi-model agent coordination (Claude, GPT, Gemini)
- 12 LSP tools for IDE-quality code intelligence
- AST-Grep for semantic code search
- 22 lifecycle hooks for context management
- Preemptive session compaction

## 1. Multi-Agent Orchestration System

**Pattern**: Hierarchical agent delegation with specialized roles

| Agent Role | Purpose | Model |
|------------|---------|-------|
| Sisyphus | Primary orchestrator - classifies intent, delegates | Claude Opus |
| Oracle | Strategic reasoning consultant for complex decisions | GPT 5.2 |
| Explore | Fast parallel codebase search | Claude Haiku |
| Librarian | Multi-repo documentation research | Claude Sonnet |
| Frontend Engineer | UI/UX specialist | Gemini |
| Document Writer | Technical documentation | Claude Sonnet |
| Multimodal Looker | PDF/image analysis | Claude Sonnet |

### Intent Classification (Phase 0)

Before any action, classify requests into categories:
- **Skill Match** → Invoke skill immediately
- **Trivial** → Direct execution
- **Explicit** → Clear requirements, proceed
- **Exploratory** → Research first
- **Open-ended** → Clarify scope
- **GitHub Work** → Specific workflows
- **Ambiguous** → Ask for clarification

### Delegation Framework

Mandatory 7-section prompt structure when delegating:
1. TASK - What to do
2. EXPECTED OUTCOME - Success criteria
3. REQUIRED SKILLS - Capabilities needed
4. REQUIRED TOOLS - Which tools to use
5. MUST DO - Non-negotiable requirements
6. MUST NOT DO - Explicit prohibitions
7. CONTEXT - Background information

### Failure Recovery Protocol

After 3 consecutive failures:
1. Halt execution
2. Revert to known working state
3. Document attempts
4. Escalate to Oracle before involving user

## 2. Context Window Management

### Context Window Monitor Hook

Tracks token usage after each tool call:
- Display limit: 1M tokens (what users see)
- Actual limit: 200K tokens (operational)
- Warning threshold: 70% of actual usage

```
"X% used (tokens/limit), Y% remaining"
```

### Preemptive Compaction

Triggers when ALL conditions met:
- Usage ratio exceeds threshold
- Total tokens > MIN_TOKENS_FOR_COMPACTION
- Model is Claude-based
- Cooldown elapsed
- No compaction in progress
- Last message isn't already a summary

Actions:
1. Show warning toast with usage percentage
2. Execute `onBeforeSummarize` callback
3. Call session summarize API
4. After 500ms, send "Continue" prompt
5. Clean up state

### Compaction Context Injector

Injects structured prompt requiring 5 sections in summaries:
1. **User Requests** - Original statements verbatim
2. **Final Goal** - Intended end result
3. **Work Completed** - Accomplished tasks
4. **Remaining Tasks** - Pending items
5. **Constraints** - Forbidden approaches

## 3. Todo Continuation Enforcer

Autonomous task completion until all todos done.

**Trigger**: Session idle event

**Flow**:
1. Check for incomplete todos
2. Display 2-second countdown toast
3. Inject continuation prompt:
   > "Proceed without asking permission / Mark each task complete when finished / Do not stop until all tasks are done"

**Safety Checks**:
- Validate write permissions
- Check recovery state
- Respect error cooldowns
- Skip for "plan" mode agents
- Avoid interrupting background tasks

## 4. Directory Context Injection

Automatic injection of directory-specific AGENTS.md content.

**Flow**:
1. `toolExecuteBefore` - Identify files being read
2. `toolExecuteAfter` - Traverse parent directories for AGENTS.md
3. Inject content with truncation for large files
4. Session-based caching prevents duplicates
5. Clear cache on session compaction

**Skip**: Root AGENTS.md (loaded separately by system)

## 5. Rules Injection System

Conditional rule application based on file patterns.

**Process**:
1. File operations trigger `findRuleFiles()`
2. `shouldApplyRule()` evaluates metadata against file path
3. Content-based and path-based deduplication
4. Proximity-based sorting (closer rules first)

**Injection Format**:
```
[Rule: relativePath]
[Match: reason]
[rule content]
[truncation notice if needed]
```

## 6. LSP Tool Suite

12 IDE-quality tools exposed to agents:

| Tool | Purpose |
|------|---------|
| `lsp_hover` | Type info, docs, signatures |
| `lsp_goto_definition` | Navigate to definitions |
| `lsp_find_references` | Find all usages (truncated) |
| `lsp_document_symbols` | File outline |
| `lsp_workspace_symbols` | Cross-codebase symbol search |
| `lsp_diagnostics` | Errors/warnings before build |
| `lsp_prepare_rename` | Validate rename operation |
| `lsp_rename` | Workspace-wide rename |
| `lsp_code_actions` | Quick fixes, refactors |
| `lsp_code_action_resolve` | Apply code actions |
| `lsp_servers` | List available LSP servers |

## 7. AST-Grep Integration

Semantic code search beyond text matching.

**Tools**:
- `ast_grep_search` - Pattern-based AST queries
- `ast_grep_replace` - Semantic refactoring

**Binary Management**: Auto-download and cache ast-grep binary

## 8. Thinking Mode Auto-Detection

Keyword triggers automatic model upgrade.

**Flow**:
1. `detectThinkKeyword(promptText)` scans for triggers
2. If detected, `getHighVariant(currentModel.modelID)`
3. Swap model and inject thinking config

## 9. Session Notification System

Cross-platform notifications when agent finishes.

**Features**:
- Platform detection (macOS/Linux/Windows)
- Native sound paths per platform
- 1500ms idle confirmation before notification
- Skip if todos incomplete
- Max 100 tracked sessions

## 10. Keyword-Triggered Workflows

"Ultrawork" and similar keywords trigger special modes.

**First Message**: Transform message parts directly
**Subsequent**: Inject hook message

Example: "Maximum precision engaged. All agents at your disposal."

## Anti-Patterns to Avoid

From their AGENTS.md:
- High temperature for code agents
- Broad tool access without categorization
- Rushing task completion without verification
- Over-exploration when focused search would suffice
- Using npm/yarn instead of bun
- Hardcoding years in code
- Local version bumps

## Architectural Philosophy

- **"Battery included"** defaults with granular customization
- **Agent specialization** by cost/domain
- **Parallel execution** as the default
- **Proactive context management** rather than reactive

## Priority Recommendations

### Immediate Value (Low Effort, High Impact)
1. Context Window Monitor
2. Todo Continuation Enforcer
3. Compaction Context Injector

### Medium-Term (Moderate Effort)
4. LSP Tool Suite
5. AST-Grep Integration
6. Thinking Mode Detection

### Strategic (Higher Effort)
7. Multi-Agent Orchestration
8. Background Agent Execution
9. Directory Rules Injection

---

See [[Implementation Strategy]] for decisions on Rust vs scripted implementations.
