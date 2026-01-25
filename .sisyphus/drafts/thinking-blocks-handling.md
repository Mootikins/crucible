# Thinking Blocks Handling for Crucible

## Research Source: OpenCode (sst/opencode)

OpenCode uses **display-only filtering** - thinking blocks are stored and sent to LLM, but UI can toggle visibility.

---

## OpenCode Approach Summary

| Aspect | OpenCode Behavior |
|--------|-------------------|
| During streaming | Thinking blocks ARE shown in real-time |
| After completion | Kept visible (user can toggle with `/thinking`) |
| In subsequent requests | **Included** — not stripped from context |
| Storage | Permanent parts in session history |

### Key Files in OpenCode
- `session/processor.ts` - Streaming: `reasoning-start`, `reasoning-delta`, `reasoning-end`
- `session/message-v2.ts` - Context building: reasoning parts added to messages
- `provider/transform.ts` - Provider-specific transforms (e.g., DeepSeek's `reasoning_content` field)
- `routes/session/index.tsx` - UI toggle: `/thinking` command

---

## Design Questions for Crucible

### 1. Storage: Keep or Strip?

**Option A: Keep (OpenCode approach)**
- Store thinking blocks as message parts
- Full reasoning chain preserved for model continuity
- More tokens used in context
- Better for debugging and transparency

**Option B: Strip after display**
- Show during streaming
- Remove from conversation history after response completes
- Saves tokens in subsequent requests
- Loses reasoning chain

**Recommendation**: Keep (Option A) with configurable stripping for token-constrained scenarios.

---

### 2. UI Display: Toggle vs Always Show

**Option A: Always show (default)**
- Thinking blocks always visible during and after streaming
- Simple implementation

**Option B: Toggle (OpenCode approach)**
- Default hidden after completion
- `/thinking` or `:thinking` command toggles visibility
- `show_thinking` setting in config

**Option C: Show during streaming only**
- Visible while streaming
- Auto-collapse after response completes
- Click to expand

**Recommendation**: Option B (toggle) - matches user expectations from Claude/OpenCode.

---

### 3. Context Inclusion: Full vs Provider-Specific

**Option A: Always include as content**
```json
{
  "role": "assistant",
  "content": [
    { "type": "thinking", "text": "..." },
    { "type": "text", "text": "..." }
  ]
}
```

**Option B: Provider-specific field (OpenCode approach)**
```json
{
  "role": "assistant",
  "content": [{ "type": "text", "text": "..." }],
  "providerOptions": {
    "openaiCompatible": {
      "reasoning_content": "..."
    }
  }
}
```

**Option C: Configurable per provider**
- Anthropic: Native thinking blocks
- OpenAI: Strip or move to metadata
- DeepSeek: `reasoning_content` field
- Ollama: Strip (not supported)

**Recommendation**: Option C - provider-aware handling via capability traits.

---

## Proposed Implementation

### Phase 1: Basic Toggle (Quick Win)

1. **TUI State**: Add `show_thinking: bool` to `InkChatApp`
2. **Command**: `:thinking` or `:toggle thinking` to flip
3. **Rendering**: Filter thinking blocks from view when `show_thinking = false`
4. **Storage**: Keep all thinking blocks in session (no stripping)

### Phase 2: Provider-Aware Context (Future)

1. **Capability Trait**: `CanThink` with `supports_thinking_blocks() -> bool`
2. **Context Transform**: Before sending to LLM:
   - If provider supports thinking → include as native
   - If provider doesn't → strip or move to metadata
3. **Config**: `thinking.context_mode = "keep" | "strip" | "provider_default"`

### Phase 3: Session Persistence (Future)

1. **Markdown Format**: How to represent thinking blocks in session markdown?
   - Collapsible `<details>` block?
   - Special frontmatter?
   - Separate section?

2. **Search**: Should thinking blocks be searchable?
   - Probably yes for debugging
   - Maybe exclude from semantic search (too meta)

---

## Implementation References

### Where to Add Toggle State

```rust
// crates/crucible-cli/src/tui/oil/chat_app.rs
pub struct InkChatApp {
    // ... existing fields
    show_thinking: bool,  // NEW: Toggle thinking block visibility
}
```

### Where to Filter in Rendering

```rust
// In message rendering, filter content parts
let visible_parts: Vec<_> = message.content.iter()
    .filter(|part| {
        if let ContentPart::Thinking(_) = part {
            self.show_thinking
        } else {
            true
        }
    })
    .collect();
```

### Command Registration

```rust
// Add to command handling
":thinking" | ":toggle thinking" => {
    self.show_thinking = !self.show_thinking;
    self.notify(format!("Thinking blocks: {}", 
        if self.show_thinking { "shown" } else { "hidden" }));
}
```

---

## Open Questions

1. **Default state**: Should `show_thinking` default to true or false?
   - OpenCode: false (hidden by default after completion)
   - Claude web: true (always shown)

2. **Persistence**: Should the toggle persist across sessions?
   - Probably yes (user preference)
   - Store in user config, not session

3. **Streaming behavior**: Show thinking during streaming even if toggle is off?
   - OpenCode: Yes (only hides after completion)
   - This provides feedback that model is "working"

4. **Session markdown**: What format for thinking blocks?
   ```markdown
   <!-- Option A: HTML details -->
   <details><summary>Thinking</summary>
   ...reasoning...
   </details>

   <!-- Option B: Frontmatter-style -->
   ---thinking---
   ...reasoning...
   ---/thinking---

   <!-- Option C: Blockquote with marker -->
   > [!thinking]
   > ...reasoning...
   ```

---

## Priority

**Phase 1** (toggle) is a quick win - ~2 hours
- Just UI filtering, no backend changes
- Matches user expectations
- Can be done independently of FSM work

**Phase 2+** can wait until after stability testing.
