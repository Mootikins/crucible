---
date: 2025-12-11T15:30:00Z
researcher: Claude
topic: "Chat UX Improvements - Status Line, Utility Commands, @-File References"
tags: [research, codebase, chat, reedline, status-line, slash-commands, file-references]
status: complete
---

# Research: Chat UX Improvements

## Research Question

Prioritize and understand the implementation requirements for chat UX improvements in Crucible:
1. Phase 1: Status Line (mode + background progress)
2. Phase 2: Utility Commands (`/clear`, `/reset`, `/export`, `/files`, `/context`)
3. Phase 3: @-File References (parse, fuzzy search, inject context)

## Summary

| Phase | Complexity | Dependencies | Recommendation |
|-------|------------|--------------|----------------|
| **Phase 1: Status Line** | Medium | LiveProgress exists but unused | **Start here** - infrastructure exists |
| **Phase 2: Utility Commands** | Low-Medium | Needs ChatContext extension | High value, incremental |
| **Phase 3: @-File References** | High | Needs new input parser + fuzzy lib | Defer - largest scope |

**Key Finding:** Phase 1 has the best effort-to-value ratio. `LiveProgress` already exists in `progress.rs` but is not integrated with `ChatSession`. Wiring it through requires minimal changes.

---

## Phase 1: Status Line

### Current State

**Infrastructure exists but is unused:**
- `BackgroundProgress` (`progress.rs:17-94`) - Thread-safe atomic counters
- `LiveProgress` (`progress.rs:160-291`) - ANSI cursor manipulation for status above prompt
- Created in `chat.rs:179` but passed as `_live_progress` (unused parameter)

### Architecture Gap

```
chat.rs:99    → Creates BackgroundProgress
chat.rs:179   → Creates LiveProgress (passes to run_interactive_session)
chat.rs:204   → run_interactive_session() calls session.run()
session.rs    → ChatSession::run() has NO access to progress
```

### Implementation Approach

**Option A: Integrate LiveProgress (Recommended)**
- Modify `ChatSession` to hold `Option<Arc<BackgroundProgress>>`
- Pass `LiveProgress` to session (already partially wired)
- Real-time updates via existing Crossterm pattern

**Option B: Prompt-based status (Simpler)**
- Poll `BackgroundProgress::status_string()` each loop iteration
- Include in prompt indicator (no real-time updates)
- Updates only visible when user presses enter

### Target Display
```
[plan] ● Indexing: 45/300 files
> _
```

### Files to Modify
| File | Change |
|------|--------|
| `session.rs:78-84` | Add `progress: Option<Arc<BackgroundProgress>>` to struct |
| `session.rs:148` | Update `run()` signature to accept LiveProgress |
| `session.rs:174` | Add status line render before prompt |
| `chat.rs:235` | Pass LiveProgress to session.run() |

### Effort: Low-Medium (infrastructure exists)

---

## Phase 2: Utility Commands

### Command Infrastructure

**Well-designed trait-based system:**
```rust
#[async_trait]
pub trait CommandHandler: Send + Sync {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()>;
}
```

**Registration pattern:**
```rust
SlashCommandRegistryBuilder::default()
    .command("name", Arc::new(Handler), "description")
    .build();
```

### Command Analysis

| Command | State Access Needed | Exists? | Implementation |
|---------|---------------------|---------|----------------|
| `/clear` | None (terminal op) | ✅ Ready | Crossterm `Clear(ClearType::All)` |
| `/reset` | History clear | ❌ Need trait method | Extend ChatContext |
| `/export [file]` | History read | ❌ Need trait method | Extend ChatContext |
| `/files` | File tracking | ❌ Not tracked | New tracking system |
| `/context` | Token stats | ❌ Need trait method | Extend ChatContext |

### ChatContext Extensions Needed

```rust
// Add to crucible-core/src/traits/chat.rs
pub trait ChatContext: Send {
    // Existing methods...

    // New for utility commands:
    fn get_history(&self) -> Vec<HistoryMessage>;       // For /export
    fn clear_history(&mut self);                         // For /reset
    fn get_session_state(&self) -> ConversationState;   // For /context
}
```

### `/files` - Special Case

**No file tracking exists.** Options:
1. Track files in `ChatSession` when tool calls contain paths
2. Parse tool call results for file operations
3. Add tracking to `ToolExecutor`

### Implementation Order (by effort)
1. `/clear` - No state needed, just crossterm
2. `/context` - Read ConversationState (add getter)
3. `/reset` - Clear history (add method)
4. `/export` - Read history + file I/O
5. `/files` - Requires new tracking system

### Files to Modify
| File | Change |
|------|--------|
| `handlers.rs` | Add 5 new handler structs |
| `session.rs:92-136` | Register new commands |
| `crucible-core/src/traits/chat.rs` | Extend ChatContext trait |
| `context.rs` | Implement new ChatContext methods |
| `acp/client.rs` | Expose underlying session state |

### Effort: Low-Medium (except /files which is Medium-High)

---

## Phase 3: @-File References

### Current State

**No existing @ parsing.** Related systems:
- Config file refs (`{file:path}`) exist but different syntax
- Semantic search works but doesn't resolve direct file paths
- `KnowledgeRepository.get_note_by_name()` can resolve files

### Implementation Components

1. **Input Parser** (NEW)
   - Regex: `@[\w\-./]+(?:\.md)?`
   - Extract file hints from user input
   - Handle quoted paths: `@"path with spaces.md"`

2. **File Resolver** (NEW)
   - Try exact path match first
   - Fuzzy search fallback (needs new dependency)
   - Use `get_note_by_name()` for resolution

3. **Fuzzy Matcher** (NEW DEPENDENCY)
   - Options: `fuzzy-matcher`, `nucleo`
   - Match against `list_notes()` results

4. **Context Injection** (MODIFY)
   - Extend `ContextEnricher` to handle file refs
   - Priority: file refs > semantic search
   - Format as context blocks

5. **Interactive Picker** (NEW)
   - Trigger on `@` input
   - Reedline menu or custom UI
   - Return selected file path

### Integration Points
| Component | File | Integration |
|-----------|------|-------------|
| Input parsing | `session.rs` | Before sending to agent |
| File resolution | NEW `file_resolver.rs` | Uses KilnContext |
| Context injection | `acp/enricher.rs` | Extend `enrich_context()` |
| UI picker | NEW `file_picker.rs` | Reedline integration |

### Files to Create/Modify
| File | Change |
|------|--------|
| `chat/input_parser.rs` | NEW - Parse @references |
| `chat/file_resolver.rs` | NEW - Resolve paths |
| `chat/file_picker.rs` | NEW - Interactive selection |
| `acp/enricher.rs` | Extend for file refs |
| `session.rs` | Wire parsing into input flow |
| `Cargo.toml` | Add `fuzzy-matcher` or `nucleo` |

### Effort: High (4-5 new modules, new dependency)

---

## Code References

### Phase 1 (Status Line)
- `crates/crucible-cli/src/progress.rs:17-291` - BackgroundProgress & LiveProgress
- `crates/crucible-cli/src/commands/chat.rs:77-179` - Progress creation
- `crates/crucible-cli/src/chat/session.rs:148-271` - Session loop
- `crates/crucible-cli/src/chat/mode_ext.rs:52-98` - Mode display utilities

### Phase 2 (Utility Commands)
- `crates/crucible-cli/src/chat/handlers.rs:1-214` - Existing handlers
- `crates/crucible-cli/src/chat/slash_registry.rs:1-372` - Registry
- `crates/crucible-cli/src/chat/context.rs:1-239` - CliChatContext
- `crates/crucible-core/src/traits/chat.rs` - ChatContext trait
- `crates/crucible-acp/src/chat.rs:94-137` - ConversationState
- `crates/crucible-acp/src/history.rs:89-167` - ConversationHistory

### Phase 3 (@-File References)
- `crates/crucible-surrealdb/src/kiln_integration/repository.rs:148-167` - KnowledgeRepository
- `crates/crucible-cli/src/acp/enricher.rs` - Context enrichment
- `crates/crucible-config/src/includes.rs:173-195` - Path resolution utilities

### Display Layer
- `crates/crucible-cli/src/chat/display.rs:1-595` - Terminal rendering
- `crates/crucible-cli/src/formatting/markdown_renderer.rs` - Markdown

---

## Architecture Insights

### Strengths
1. **Clean trait separation** - `CommandHandler`, `ChatContext`, `AgentHandle`
2. **Builder pattern** for registry - Easy command registration
3. **LiveProgress exists** - Just needs integration
4. **Repository pattern** - `get_note_by_name()` enables file resolution

### Gaps
1. **No file modification tracking** - `/files` requires new system
2. **ChatContext too narrow** - Doesn't expose history/state
3. **No session persistence** - History lost on exit
4. **Token counting is approximate** - Character-based only

### Design Decisions

**Status Line:** Use prompt-based status if real-time isn't critical. LiveProgress integration adds complexity for marginal benefit.

**Commands:** Extend ChatContext trait rather than exposing internal types. Maintains abstraction boundaries.

**@-Files:** Consider two modes:
- `/file <path>` command for explicit includes (simpler)
- `@filename` inline parsing for natural language (complex)

---

## Prioritization Recommendation

### Immediate (Low Effort, High Value)
1. **`/clear`** - One handler, no state, immediate utility
2. **Status line in prompt** - Poll progress each iteration, minimal changes

### Short Term (Medium Effort)
3. **`/context`** - Extend ChatContext, expose token stats
4. **`/reset`** - Add clear_history(), simple

### Medium Term (Requires Design)
5. **`/export`** - History access + file I/O, need format decision
6. **LiveProgress integration** - Real-time status (optional polish)

### Deferred (High Effort)
7. **`/files`** - Requires file tracking infrastructure
8. **@-File references** - Largest scope, multiple new modules

---

## Open Questions

1. **Export format?** Markdown, JSON, or both?
2. **File tracking granularity?** Just paths or include operations (read/write/delete)?
3. **@-file UX?** Interactive picker vs explicit `/file` command?
4. **Real-time status?** Is prompt-based polling sufficient?

---

## Next Steps

1. Implement `/clear` handler (5 min)
2. Add status to prompt indicator (30 min)
3. Extend ChatContext for `/context` (1 hr)
4. Implement `/reset` (30 min)
5. Design file tracking for `/files` (needs spec)
6. Design @-file parsing (needs spec)
