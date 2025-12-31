# System Prompt Template Design

**Date**: 2024-12-30
**Status**: Implemented
**Goal**: Size-aware system prompts that optimize small model reliability

## Problem

Small models (< 4B) hit tool loops when prompted to output JSON or perform tasks they could handle directly. The current hardcoded system prompt in `create_internal_agent()` only explains HOW to use tools, not WHEN.

**Evidence**: granite-micro-3b and granite-tiny-7b hit `MaxDepthError` on JSON output prompts, cycling through tool calls instead of responding directly.

## Design

### Size Detection

Auto-detect model size from name, with thresholds:

```
Small:  < 4B   (3b patterns - need heavy guidance)
Medium: 4-30B  (4b, 7b, 8b, 14b, 20b patterns)
Large:  > 30B  (32b, 70b, etc.)
```

Parse with regex: `/(\d+)b/i` → extract number → categorize.

### Size-Based Prompts

**Small models** need explicit guidance:
```
You are a helpful assistant.

## When to Use Tools
- ONLY use tools when the user asks for file operations, searches, or commands
- For questions, math, JSON, formatting: respond directly WITHOUT tools
- Do NOT call tools for: definitions, explanations, code generation, data formatting

## Available Tools (use sparingly)
- read_file, glob, grep (read-only operations)
```

**Medium models** get standard guidance:
```
You are a helpful assistant with access to workspace tools.

## Tool Usage
- Use tools when tasks require file operations or system interaction
- For simple questions and formatting: respond directly
- Available: read_file, write_file, edit_file, bash, glob, grep
```

**Large models** get minimal prompts:
```
You are a helpful assistant with workspace tools available.
```

### Size-Based Tool Filtering

| Size | Tools Available |
|------|-----------------|
| Small | read_file, glob, grep (read-only) |
| Medium | All tools |
| Large | All tools |

This reduces confusion for small models by limiting options.

### Prompt Hierarchy (existing LayeredPromptBuilder)

```
Priority 100: Base prompt (size-aware)
Priority 200: AGENTS.md / CLAUDE.md (project rules)
Priority 300: Agent card system prompt
Priority 400: User customization
Priority 500: Dynamic context
```

**Files to check** (Zed-compatible):
- `AGENTS.md`, `CLAUDE.md` (already supported)
- `.rules`, `.cursorrules` (add support)
- `.github/copilot-instructions.md` (add support)

**Deduplication**: These files often contain similar/identical content. Avoid bloating context:

1. **Hash-based dedup**: Hash each file's content, skip if already seen
2. **First-match wins**: Check in priority order, stop at first match (Zed approach)
3. **Diff detection**: If files differ by <10%, warn and use first

Recommended: **First-match wins** (simplest, matches Zed behavior)

```
Check order: .rules → .cursorrules → AGENTS.md → CLAUDE.md → .github/copilot-instructions.md
Load first one found, skip rest.
```

### Behavior Hooks (existing event system)

Prompt content is hierarchical injection. Behavior modification uses events:
- `PreToolCall` - intercept/block tool decisions
- `PreLlmCall` - adjust parameters
- Scripts react to events, no new config files needed

## Implementation

### Files to Modify

1. **`crates/crucible-core/src/prompts/`** (new module)
   - `mod.rs` - size detection, base prompts
   - `templates.rs` - size-specific prompt templates

2. **`crates/crucible-cli/src/factories/agent.rs`**
   - Wire `LayeredPromptBuilder` into `create_internal_agent()`
   - Add size detection
   - Filter tools based on size

3. **`crates/crucible-rig/src/workspace_tools.rs`**
   - Add `read_only_tools()` method for small models

4. **`crates/crucible-context/src/layered_prompt.rs`**
   - Add `.rules` file support

### Size Detection Function

```rust
pub enum ModelSize {
    Small,   // < 4B
    Medium,  // 4-30B
    Large,   // > 30B
}

pub fn detect_model_size(model_name: &str) -> ModelSize {
    let re = regex::Regex::new(r"(\d+)[bB]").unwrap();
    if let Some(caps) = re.captures(model_name) {
        let size: u32 = caps[1].parse().unwrap_or(0);
        match size {
            0..=3 => ModelSize::Small,   // < 4B needs heavy guidance
            4..=30 => ModelSize::Medium, // 4-30B
            _ => ModelSize::Large,       // > 30B
        }
    } else {
        ModelSize::Medium // default to medium if can't detect
    }
}
```

## Future Enhancements

1. **Model tracking** - Log VT results to JSONL, build model cards with scores
2. **Dynamic tool loading** - Semantic match query → tools, expand during session
3. **Query-based filtering** - Embed tool descriptions, include relevant subset
4. **Model annotations** - Store quirks/notes per model in config

## Test Models

**Small (< 4B, needs heavy guidance):**
- granite-micro-3b

**Medium (4-30B):**
- qwen3-4b, granite-tiny-7b, deepseek-r1-qwen3-8b, qwen3-14b

**Large (> 30B, minimal prompting):**
- qwen2.5-coder-32b, qwen3-32b, M2.1 IQ2_M (78GB)

## Success Criteria

1. granite-micro-3b passes VT full suite (including JSON test)
2. No tool loops on simple prompts
3. LayeredPromptBuilder used for all internal agents
4. .rules files loaded when present
