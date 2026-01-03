# Preventing XML-Style Tool Calls in Small LLMs

**Date**: 2026-01-02
**Status**: Research Complete
**Authors**: Research synthesis from Zed, OpenCode, Claude Code, llama.cpp, vLLM, Hermes analysis

## Executive Summary

Small LLMs (< 30B parameters) often output XML-style tool calls (like `<tool_call>` or `<function=bash>`) as text instead of using native function calling APIs. This research identifies:

1. **Root Cause**: Training data shape - many models were fine-tuned on Hermes-style XML tool calling templates
2. **Crucible's Current Approach**: Already has system prompt guidance AND client-side XML parsing fallback
3. **Industry Patterns**: Most tools use multi-layered defenses (prompts + parsing + normalization)
4. **Key Insight**: XML format may actually be MORE reliable for small models than JSON function calling

### Key Findings

1. **Crucible already implements the best practice**: XML parsing fallback in `xml_tool_parser.rs`
2. **System prompts help but don't prevent**: Models trained on XML will revert under pressure
3. **Small models prefer XML over JSON**: Research shows 7B models generate valid XML more reliably
4. **Per-model customization is essential**: Qwen, Llama, Granite, Phi all use different formats

---

## Part 1: What Crucible Currently Does

### System Prompts (`crates/crucible-core/src/prompts/templates.rs`)

Crucible already includes explicit anti-XML guidance:

```rust
pub const SMALL_MODEL_PROMPT: &str = r#"You are a helpful assistant.

Only use tools when the task requires file/system operations.
For questions and formatting: respond directly without tools.

IMPORTANT:
- After using tools, provide a final answer. Do not keep calling tools repeatedly.
- Use ONLY the native function calling format. NEVER output <tool_call>, <function>, or XML-style tool invocations as text."#;
```

### XML Fallback Parser (`crates/crucible-rig/src/xml_tool_parser.rs`)

Crucible has a robust XML tool parser that:
- Detects potential XML tool calls in streaming output
- Parses multiple formats: `<function=name>` and `<tool_call>` wrappers
- Handles incomplete/malformed XML gracefully
- Buffers during streaming to avoid emitting partial XML
- Converts parsed calls to native `ChatToolCall` format

### Handle Integration (`crates/crucible-rig/src/handle.rs`)

The `RigAgentHandle` streaming handler:
1. Buffers text when XML is detected
2. Attempts parsing when complete
3. Emits tool calls via `ChatChunk::tool_calls`
4. Suppresses raw XML from reaching the user

---

## Part 2: Industry Approaches

### Zed Editor

**Approach**: Trust native function calling, provide clear tool schema

Key prompt patterns:
- "Make sure to adhere to the tools schema"
- "Provide every required argument"
- "DO NOT use tools to access items already in context"
- "Use only the tools that are currently available"

Zed does NOT include explicit anti-XML guidance because they primarily support frontier models (GPT-4, Claude) that reliably use native function calling.

**Source**: [Zed Leaked Prompts](https://zed.dev/leaked-prompts)

### OpenCode

**Problems Encountered**:
- Qwen generates `Write` (capital) instead of `write` (lowercase)
- Granite 3.3 fails to generate tool calls entirely, outputs text only
- Context window requirements (~10K tokens) not met by default Ollama settings

**Solutions Implemented**:
1. Automatic tool name normalization (case-insensitive matching)
2. `/init` command to prime the model before tool requests
3. `/no_think` prefix to suppress reasoning and force tool output
4. Per-model compatibility matrix

**Proposed Enhancements**:
- Allow per-model tool calling format customization
- Model-specific chat templates

**Source**: [OpenCode Issue #234](https://github.com/sst/opencode/issues/234)

### Claude Code

**Approach**: Strict native format, parallel tool calls, no XML parsing needed

Key patterns:
- "Maximize use of parallel tool calls where possible"
- "If some tool calls depend on previous calls, do NOT call these tools in parallel"
- "Never use placeholders or guess missing parameters"

Claude models reliably use native function calling, so no XML fallback is needed.

**Source**: [Claude Code System Prompts](https://github.com/Piebald-AI/claude-code-system-prompts)

### Hermes Models (NousResearch)

**Defines the problem**: Hermes models TRAIN on XML-style tool calling:

```
<tools>
[tool definitions here]
</tools>

User query...

<tool_call>
{"name": "function_name", "arguments": {"key": "value"}}
</tool_call>
```

This training shapes model behavior. Even with API-level function calling, models may revert to text output.

**Source**: [Hermes Function Calling](https://github.com/NousResearch/Hermes-Function-Calling)

### vLLM / llama.cpp

**Approach**: Model-specific parsers

| Model | Parser | Format |
|-------|--------|--------|
| Llama 3.x | `llama3_json` | JSON in special tokens |
| Qwen 2.5 | `hermes` | XML-wrapped JSON |
| Qwen3-Coder | Custom | XML with XML params |
| Granite | `granite` | JSON |
| Mistral | `mistral_nemo` | JSON |

llama.cpp uses GBNF grammars to constrain output, but:
- Grammar and tool calling are mutually exclusive
- Chat template generates its own grammar for tool output

**Source**: [vLLM Tool Calling](https://docs.vllm.ai/en/latest/features/tool_calling/)

---

## Part 3: Why XML Actually Works Better for Small Models

### The JSON Constraint Problem

From [Morph LLM Documentation](https://docs.morphllm.com/guides/xml-tool-calls):

> Constrained decoding forces language models to generate outputs that conform to strict structural requirements. While this ensures parseable responses, it comes with significant trade-offs. When requiring an LLM to output valid JSON for tool calls, models spend computational "attention" ensuring JSON validity instead of focusing on code logic and correctness.

### The XML Advantage

> All 7B models tested generated valid XML correctly when prompted to use XML syntax for tool calls.

Benefits:
1. **Training data**: Most models trained on HTML/XML markup
2. **Natural structure**: Tags self-document parameter names
3. **Graceful failure**: Partial XML often still parseable
4. **No escaping hell**: No need to escape quotes/newlines in strings

### Recommended: Embrace XML for Small Models

Rather than fighting XML output, consider it a feature:
1. Keep API-level function calling enabled (uses model's trained format)
2. Parse XML output as fallback
3. Normalize tool names (case-insensitive)
4. Accept both formats transparently

---

## Part 4: Recommendations for Crucible

### Current State: Already Well-Implemented

Crucible's approach is aligned with industry best practices:

1. **System prompt guidance** - Already in `templates.rs`
2. **XML parsing fallback** - Already in `xml_tool_parser.rs`
3. **Streaming-aware buffering** - Already in `handle.rs`

### Recommended Enhancements

#### 1. Add Tool Name Normalization (Low Effort, High Impact)

```rust
// In xml_tool_parser.rs or a new module
pub fn normalize_tool_name(name: &str) -> String {
    name.to_lowercase()
        .replace('-', "_")
        .replace(' ', "_")
}
```

This handles Qwen's `Write` vs `write` issue.

#### 2. Per-Model Format Configuration (Medium Effort)

Add to agent config:

```toml
[agent.tool_calling]
format = "auto"  # auto, native, hermes, qwen3_coder
case_sensitive = false
```

#### 3. Enhance System Prompts for Specific Models (Low Effort)

For Qwen3-Coder specifically:

```rust
pub const QWEN3_CODER_PROMPT: &str = r#"You are a helpful coding assistant.

When using tools, follow the native function calling format provided by the API.
Do not wrap tool calls in XML tags or output them as text.
Tool names are case-sensitive - use exact names as provided."#;
```

#### 4. Add "Thinking" Suppression for Tool Calls (Low Effort)

Some models (Qwen3) wrap tool calls in `<think>` tags:

```rust
// In xml_tool_parser.rs
static THINK_WRAPPER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)<think>.*?</think>\s*").unwrap()
});

// Strip thinking before parsing
let text_without_thinking = THINK_WRAPPER_RE.replace_all(text, "");
```

#### 5. Model-Specific Chat Templates (Higher Effort)

For llama.cpp backends, allow specifying Jinja templates:

```toml
[agent.llama_cpp]
chat_template = "hermes"  # or path to custom .jinja
```

### Not Recommended

1. **Removing XML parser** - Many models will always output XML regardless of prompting
2. **Stricter JSON-only enforcement** - Will break small model compatibility
3. **Grammar constraints** - Incompatible with tool calling in llama.cpp

---

## Part 5: Model-Specific Quirks

### Qwen3-Instruct
- Uses Hermes-style: `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
- JSON inside XML tags
- vLLM parser: `hermes`

### Qwen3-Coder
- Uses full XML: `<function=name><parameter=key>value</parameter></function>`
- Parameters are XML elements, not JSON
- Requires custom parser (Crucible already has this!)

### Llama 3.x
- Uses JSON with special tokens: `{"name": "...", "parameters": {...}}`
- No XML wrapper
- vLLM parser: `llama3_json`

### Granite 4.0
- Native JSON function calling
- Supports parallel calls
- vLLM parser: `granite`

### Phi-4
- Unreliable tool calling
- Often generates malformed output
- Consider limiting to read-only tools

---

## Appendix: Sources

### Primary Sources
- [Zed Leaked Prompts](https://zed.dev/leaked-prompts)
- [OpenCode Issue #234](https://github.com/sst/opencode/issues/234)
- [Claude Code System Prompts](https://github.com/Piebald-AI/claude-code-system-prompts)
- [Hermes Function Calling](https://github.com/NousResearch/Hermes-Function-Calling)
- [vLLM Tool Calling](https://docs.vllm.ai/en/latest/features/tool_calling/)
- [Morph XML Tool Calls](https://docs.morphllm.com/guides/xml-tool-calls)

### Additional Resources
- [Qwen Function Calling](https://qwen.readthedocs.io/en/latest/framework/function_call.html)
- [llama.cpp GBNF Grammars](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md)
- [Goose Toolshim Finetuning](https://block.github.io/goose/blog/2025/04/11/finetuning-toolshim/)
