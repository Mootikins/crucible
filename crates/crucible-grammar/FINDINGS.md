# Grammar-Constrained Generation Findings

## Summary

Testing grammar-constrained generation for tool calling on `qwen3-14b-ud-q8_k_xl` at `https://llama.krohnos.io/`.

## Critical Discovery: Chat Template + Grammar Interaction

**The problem is NOT grammar constraints alone - it's the interaction between chat template and grammar.**

### Evidence

Using `/v1/chat/completions` (with chat template):
- Model's top token: `<think>` with logprob 0.0 (probability ~100%)
- Grammar masks `<think>`, forcing redistribution
- After redistribution: `git` wins over `read`

Using `/v1/completions` (no chat template):
- First token: ` read` with logprob -0.002 (probability ~99.8%)
- Grammar + completions: `read(path="README.md")` ✓ CORRECT

```bash
# Chat completions + grammar = WRONG
curl .../v1/chat/completions -d '{"grammar": "...", ...}'
# Result: git(args="status --porcelain")

# Text completions + grammar = CORRECT
curl .../v1/completions -d '{"prompt": "...", "grammar": "..."}'
# Result: read(path="README.md")
```

### Root Cause

Qwen3's chat template injects a `<think>` token with logprob 0.0 at the start of generation. When grammar constraints mask this token, the probability mass redistributes in unexpected ways, causing `git` to become more probable than `read`.

### Solution: API-Level Thinking Toggle

**Use `chat_template_kwargs` to disable thinking per-request:**

```json
{
  "model": "qwen3-14b-ud-q8_k_xl",
  "messages": [...],
  "grammar": "...",
  "chat_template_kwargs": {"enable_thinking": false}
}
```

This was added in [llama.cpp PR #13196](https://github.com/ggml-org/llama.cpp/pull/13196).

### Test Results

| Config | Result |
|--------|--------|
| `enable_thinking: true` + grammar | `git(args="...")` ✗ WRONG TOOL |
| `enable_thinking: false` + grammar | `read(path="README.md")` ✓ CORRECT |

When thinking is disabled:
- No `<think>` token is injected
- Grammar doesn't need to mask anything
- Correct tool is selected

### Alternative Mitigations

If `chat_template_kwargs` is not available:
1. **Use `/v1/completions` endpoint** instead of `/v1/chat/completions`
2. Build prompts manually without chat template
3. Or use models without built-in thinking tokens

## Key Findings (Chat Completions Mode)

### 1. Grammar Constraints Improve Syntax, Hurt Semantics

| Metric | Constrained | Unconstrained |
|--------|-------------|---------------|
| Parse Rate | 90% | 40% |
| Tool Accuracy | 30% | 40% |
| Param Accuracy | 30% | 10% |
| Latency | 731-1621ms | 6691-10305ms |

### 2. Probability Distortion

When grammar constraints are applied, the model's probability distribution is renormalized over only the valid tokens. This can **completely change** which tool is selected.

Example - "Read the README.md file":
- **Unconstrained**: `read(path="README.md")` ✓ correct
- **Constrained**: `git(args="status --porcelain")` ✗ wrong tool

The model *knows* the right answer but the grammar forces it down a different path.

### 3. Git Bias

In constrained mode, the model heavily favors `git` commands over other tools:
- read_readme → git(args="status --porcelain")
- read_with_path → git(args="status --porcelain")
- list_directory → git instead of ls
- search_todo → git(args="grep ...") in infinite loop

### 4. Speed Benefits

Grammar constraints are ~5-10x faster because they reduce the search space:
- Constrained: 731-1621ms
- Unconstrained: 6691-10305ms

### 5. Infinite Loops

Some prompts cause the constrained model to get stuck repeating patterns:
```
git(args="grep --include-regex --include-regex --include-regex...")
```

This happens when the grammar allows repetitive patterns that the model gets stuck in.

## Implications

### Grammar Constraints Are NOT a Silver Bullet

The original hypothesis was that grammar constraints could help smaller models "stay on rails" for tool calling. The data shows this is **partially true but misleading**:

- ✓ Syntax is always valid (when it terminates)
- ✗ Tool selection is often WORSE
- ✗ Model knowledge is distorted, not enhanced

### When Grammar Constraints Help

1. **Single-tool scenarios**: When only one tool is valid, constraints work perfectly
2. **Speed-critical applications**: 5-10x faster generation
3. **Post-processing reduction**: No need to parse/validate syntax

### When Grammar Constraints Hurt

1. **Multi-tool selection**: The probability renormalization favors unexpected tools
2. **Long outputs**: Risk of infinite loops or repetitive patterns
3. **Semantic accuracy**: The model may "know" the right answer but be forced elsewhere

## Conclusion

**Grammar-constrained generation works well for tool calling when thinking mode is disabled.**

The key insight: For thinking models like Qwen3, use `chat_template_kwargs: {"enable_thinking": false}` to prevent the `<think>` token from interfering with grammar constraints.

## Recommended Usage

```rust
// Using the crucible-grammar API client
let request = CompletionRequest::new(model, messages)
    .with_grammar(grammar)
    .without_thinking();  // <-- Critical for Qwen3
```

```json
// Raw JSON API
{
  "model": "qwen3-14b-ud-q8_k_xl",
  "messages": [...],
  "grammar": "...",
  "chat_template_kwargs": {"enable_thinking": false}
}
```

## Tool Call Format Comparison

### Test Setup

Tested three tool call formats with **Granite Micro 3B** (dense, non-thinking):
- **Structured**: `tool(param="value", param2="value")` - Full parameter decomposition
- **Passthrough**: `tool(args="...")` - CLI args passed through
- **Raw CLI**: `command args` - Directly executable shell commands

### Results (December 2024)

| Format | Accuracy | Avg Tokens | Score |
|--------|----------|------------|-------|
| passthrough | **80%** | 7.6 | 4/5 |
| structured | **80%** | 8.8 | 4/5 |
| raw_cli | 60% | **6.0** | 3/5 |

### Per-Prompt Breakdown

| Prompt | Structured | Passthrough | Raw CLI |
|--------|------------|-------------|---------|
| Search TODO | ✓ 9 tok | ✓ 6 tok | ✓ 3 tok |
| Find .rs files | ~ 7 tok | ✓ 7 tok | ✗ 12 tok |
| Read README | ✓ 7 tok | ✓ 7 tok | ✓ 4 tok |
| List directory | ✓ 5 tok | ~ 6 tok | ~ 4 tok |
| Search 'impl Scorer' | ✓ 16 tok | ✓ 12 tok | ✓ 7 tok |

### Key Insights

1. **Raw CLI is most token-efficient** but least reliable - the model sometimes picks the wrong command (e.g., `rg --files-with-matches` instead of `fd` for file finding)

2. **Passthrough offers the best balance** - nearly as token-efficient as raw CLI with much better tool selection accuracy

3. **Structured is verbose but reliable** - the model knows the parameter schemas but generates more tokens

4. **Models leverage training data** - commands like `rg`, `fd`, `cat`, `ls` work well because models have seen them in training

5. **Tool name matters** - using exact CLI names (`rg` not `grep`, `fd` not `find`) improves accuracy since the model has stronger priors for well-documented tools

### Grammar Definitions

```gbnf
# Structured (verbose, accurate)
rg ::= "rg(pattern=\"" pattern "\"" rg-opts? ")"
rg-opts ::= ", path=\"" path "\""

# Passthrough (balanced)
root ::= tool "(args=\"" args "\")"
tool ::= "rg" | "fd" | "cat" | "ls"
args ::= [^"]+

# Raw CLI (efficient, less reliable)
root ::= command " " args
command ::= "rg" | "fd" | "cat" | "ls"
args ::= [^\n]+
```

### Recommendation

For small models (3B parameters):
- Use **passthrough format** for the best accuracy/efficiency tradeoff
- Use **raw CLI format** only when token budget is critical and you can handle tool misselection
- Use **structured format** when you need exact parameter extraction for downstream processing

## MCP JSON Call/Response Flow Testing

### Test Setup

Full call/response flows using rmcp's `Tool` type with:
- **Structured tools**: Individual parameters (pattern, path, extension, etc.)
- **Passthrough tools**: Single `args` parameter
- **With/without grammar constraints**

### Full 2×2 Matrix Results (December 2024)

| Variant | Success | Avg Tokens | Parsed |
|---------|---------|------------|--------|
| Structured+Grammar | 3/4 (75%) | 26.2 | 4/4 |
| **Structured-NoGrammar** | **4/4 (100%)** | 22.8 | 4/4 |
| Passthrough+Grammar | 2/4 (50%) | 26.5 | 4/4 |
| Passthrough-NoGrammar | 3/4 (75%) | **17.0** | 4/4 |

### Per-Prompt Breakdown

| Prompt | Struct+G | Struct-NoG | Pass+G | Pass-NoG |
|--------|----------|------------|--------|----------|
| Search TODO | ✓ 33 | ✓ 21 | ~ 18 | ✓ 17 |
| Find .rs files | ~ 20 | ✓ 26 | ~ 30 | ✓ 15 |
| Read Cargo.toml | ✓ 27 | ✓ 18 | ✓ 31 | ~ 19 |
| List src/ | ✓ 25 | ✓ 26 | ✓ 27 | ✓ 17 |

Legend: ✓=success, ~=parsed but execution failed

### SURPRISING FINDING: Grammar Hurts More Than Helps!

**Without grammar, the model performs BETTER:**
- Structured-NoGrammar: 4/4 success
- Structured+Grammar: 3/4 success

**Why?** The grammar constraint causes:
1. **Malformed JSON**: Model outputs `{"name": rg}` instead of `{"name": "rg"}`
2. **Extra whitespace/tabs**: Grammar allows `ws` which model abuses
3. **Higher token count**: 26.2 avg (grammar) vs 22.8 avg (no grammar)

Without grammar, Granite Micro 3B naturally produces valid JSON:
```json
{"name": "rg", "arguments": {"pattern": "TODO"}}
```

With grammar, it produces malformed output requiring lenient parsing:
```json
{"name": 	rg, 	"arguments":	{		"pattern":	"TODO" 	}}
```

### Key Insight: Semantic Guidance

**Structured schemas provide semantic guidance that passthrough lacks.**

When the model sees:
```json
{"pattern": "...", "path": "..."}
```

It knows to put the search term in `pattern`. But with:
```json
{"args": "..."}
```

The model has no semantic cue and may generate incorrect args.

### Model Output Quirks

The Granite Micro 3B model generates:
1. **Unquoted string values**: `{"name": rg, ...}` instead of `{"name": "rg", ...}`
2. **Excessive whitespace/tabs**: Extra formatting in JSON
3. **Wrong args for passthrough**: `{"args": "--text"}` instead of `{"args": "TODO"}`

The parser handles #1 and #2 with `fix_unquoted_values()`, but #3 requires structured schemas.

### Execution Flow

```
User: "Search for TODO comments"
     ↓
Model: {"name": rg, "arguments": {"pattern": "TODO", "path": "."}}
     ↓
Parser: ToolCallParams { name: "rg", arguments: {...} }
     ↓
Executor: rg TODO .
     ↓
Result: CallToolResult { content: [...], is_error: false }
```

### Code Example

```rust
use crucible_grammar::mcp::{CliTools, tools_to_system_prompt, tools_to_grammar,
                            parse_tool_call, execute_tool_call};

// Define tools
let tools = CliTools::all();
let system_prompt = tools_to_system_prompt(&tools);
let grammar = tools_to_grammar(&tools);

// Generate tool call with grammar constraint
let response = llm.complete(prompt, grammar).await?;

// Parse and execute
if let Some(tool_call) = parse_tool_call(&response) {
    let result = execute_tool_call(&tool_call).await;
}
```

### Final Recommendations

1. **Use structured schemas WITHOUT grammar** - Best accuracy (100%) and reasonable token count
2. **Skip grammar constraints** for Granite-class models - they naturally produce valid JSON
3. **Use passthrough-NoGrammar** when token budget is critical (17 avg tokens)
4. **Always add lenient parsing** - even without grammar, some edge cases produce malformed JSON
5. **Grammar is counterproductive** for JSON generation on small instruct models

### When Grammar IS Useful

Grammar constraints may still be valuable for:
- **Non-JSON formats** (raw CLI commands, custom DSLs)
- **Larger models** that may ramble without constraints
- **Strict output validation** when you can't afford ANY parsing errors
- **Non-instruct models** that don't have JSON training

## Tool Naming Variants: General vs Unix

### Overview

Tested three tool naming approaches with Granite Micro 3B:

1. **General**: Abstract names (Read, Search, List, Find) with smart defaults and pagination
2. **UnixRaw**: CLI names (cat, rg, ls, fd) - minimal schema, no limits
3. **UnixEnhanced**: CLI names with pagination params (head, tail, max_count, etc.)

### A/B Test Results (December 2024)

| Variant | Accuracy | Avg Tokens | Avg Latency | Schema Tokens |
|---------|----------|------------|-------------|---------------|
| General | 6/6 (100%) | 20.7 | 416ms | ~453 |
| **UnixRaw** | 6/6 (100%) | **17.2** | **330ms** | **~169** |
| UnixEnhanced | 6/6 (100%) | 18.8 | 380ms | ~437 |

### Key Findings

1. **All variants achieved 100% accuracy** on clear prompts - the model correctly selected the right tool every time

2. **UnixRaw is most efficient**:
   - 17% fewer tokens than General (17.2 vs 20.7)
   - 21% faster latency (330ms vs 416ms)
   - 63% smaller schema (169 vs 453 tokens)

3. **General tools handle ambiguity better**:
   - "Find errors in the code" → General=Search ✓, UnixRaw=REFUSED
   - "Get the test files" → General=Find, UnixRaw=ls (different interpretations)

4. **Schema size matters for context**:
   - UnixRaw: ~169 tokens (smallest)
   - UnixEnhanced: ~437 tokens
   - General: ~453 tokens (largest)

### Pros/Cons Summary

| Aspect | General | UnixRaw | UnixEnhanced |
|--------|---------|---------|--------------|
| Model familiarity | Low | **High** | **High** |
| Token efficiency | Medium | **High** | Medium |
| Output safety | **High** | Low | **High** |
| Schema complexity | High | **Low** | Medium |
| Ambiguity handling | **Good** | Poor | Good |

### Recommendations

1. **Use UnixRaw for prototyping** - smallest schema, fastest, model knows these tools
2. **Use UnixEnhanced for production** - familiar names + safe pagination defaults
3. **Use General for custom agents** - clearer semantics, better ambiguity handling
4. **Consider token budget** - UnixRaw saves ~280 tokens vs General in every prompt

### When Each Variant Excels

**UnixRaw** (`cat`, `rg`, `ls`, `fd`):
- Quick prototyping
- Small file operations
- Token-constrained contexts
- When you trust the model's Unix knowledge

**UnixEnhanced** (Unix names + pagination):
- Production agents
- Large codebase exploration
- Preventing context blowup
- Token-conscious applications

**General** (`Read`, `Search`, `List`, `Find`):
- Custom/internal agents
- Ambiguous user requests
- Multi-modal contexts
- When semantics matter more than efficiency

## Schema Detail Level Comparison

### Overview

Tested three schema detail levels with Qwen3 14B:

1. **Minimal**: Just tool name and required params, 2-word descriptions (~104 tokens)
2. **Standard**: Descriptions with param explanations (~437 tokens)
3. **Detailed**: Rich descriptions with WHEN TO USE/WHEN NOT TO USE sections, examples (~1050 tokens)

### Results (Qwen3 14B)

| Schema Detail | Accuracy | Avg Tokens | Schema Size |
|--------------|----------|------------|-------------|
| Minimal | 4/4 (100%) | 17.8 | ~104 tokens |
| Standard | 4/4 (100%) | 22.8 | ~437 tokens |
| Detailed | 4/4 (100%) | 20.0 | ~1050 tokens |

**Key Finding**: All detail levels achieve the same accuracy with larger models. Minimal is most token-efficient for output, but uses 10x less context.

## System Prompt Style Comparison

### Styles Tested

1. **Minimal**: "Tools: cat, rg, ls, fd. Respond with JSON tool call."
2. **Standard**: Tool descriptions + schemas + "Respond with JSON: {name, arguments}"
3. **Detailed**: Decision guide table + examples + "ALWAYS respond with valid JSON" + markdown formatting
4. **JsonFocused**: "OUTPUT FORMAT: You MUST respond with ONLY valid JSON" + explicit schema

### Results (Qwen3 14B)

| Prompt Style | Accuracy | Avg Tokens | Notes |
|--------------|----------|------------|-------|
| Minimal | **0/4 (0%)** | 20.2 | Wrong JSON format! |
| Standard | 4/4 (100%) | 22.8 | Reliable |
| **Detailed** | **4/4 (100%)** | **16.0** | Best efficiency! |
| JsonFocused | 4/4 (100%) | 25.0 | Verbose |

### Critical Finding: Prompt Format Matters More Than Schema Detail

**Minimal prompt failed completely** because the model output:
```json
{"tool": "rg", "args": ["TODO"]}
```
Instead of expected:
```json
{"name": "rg", "arguments": {"pattern": "TODO"}}
```

Without explicit format examples, the model invents its own JSON structure.

### Recommendations for Larger Models (14B+)

1. **Use Detailed prompt style** - Best accuracy AND lowest output tokens
2. **Schema detail doesn't matter for accuracy** - Use Minimal to save context
3. **Always include format examples** - Prevents JSON format drift
4. **Decision guide tables work well** - Clear mapping of task→tool

### Model Size Observations

| Model | Behavior |
|-------|----------|
| 3B (Granite) | Works with all prompts, needs clearer schemas |
| 14B (Qwen3) | Works best with Detailed prompt, robust to schema detail |
| 4B (Qwen3-Thinking) | Reasoning model, works but 20x slower and 15x more tokens |
| 8B (DeepSeek-R1) | Reasoning model, works with markdown-wrapped JSON output |

## Thinking/Reasoning Models for Tool Calling

### Key Discovery: Thinking Models Are NOT Ideal for Tool Calling

Tested dedicated thinking models (Qwen3-4B-Thinking-2507, DeepSeek-R1-0528-Qwen3-8B) against their instruct counterparts.

### API Response Structure

Thinking models return structured responses with separate fields:
```json
{
  "choices": [{
    "message": {
      "reasoning_content": "Okay, the user wants me to read README.md...",
      "content": "{\"name\": \"cat\", \"arguments\": {\"file\": \"README.md\"}}"
    }
  }]
}
```

- `reasoning_content`: Chain-of-thought reasoning (can be 300-600 tokens)
- `content`: Actual tool call JSON

### Qwen3-4B-Thinking vs Qwen3-4B-Instruct

| Metric | Instruct | Thinking |
|--------|----------|----------|
| Accuracy | **100%** (6/6) | 50-67% (3-4/6) |
| Avg Tokens | **17-21** | 370-385 |
| Avg Latency | **~400ms** | ~10,000ms |
| Output | Direct JSON | Reasoning + JSON |

**The 4B thinking model fails on ~40% of tool calls** because:
1. It uses 500+ tokens on reasoning before outputting the tool call
2. With limited max_tokens, reasoning runs out before the JSON is generated
3. Setting max_tokens=1000+ fixes this but wastes tokens

### DeepSeek-R1-0528-Qwen3-8B Observations

- Uses ~430 tokens per tool call (1800+ chars of reasoning)
- Wraps JSON in markdown code blocks: ` ```json ... ``` `
- Our parser handles this by finding `{` and `}` boundaries
- Slower than instruct models (~15s vs ~500ms)

### When Thinking Models Make Sense

❌ **DON'T use for simple tool routing** - No reasoning benefit, just overhead
❌ **DON'T use when latency matters** - 10-20x slower than instruct
❌ **DON'T use when token-constrained** - Need 500-1000 tokens per call

✅ **DO use for complex multi-step planning** - When reasoning matters
✅ **DO use for ambiguous requests** - Better disambiguation
✅ **DO use when you need the reasoning trace** - Debugging, explainability

### Recommendation

**For tool calling, use instruct models with `enable_thinking: false`:**

```rust
let request = CompletionRequest::new(model, messages)
    .without_thinking();  // Prevents thinking overhead
```

Thinking models are optimized for complex reasoning tasks, not simple function dispatch. Use them for the problems they're designed for.

## Standard Tool Calling Formats

For production integration, use these canonical formats:

### OpenAI Format
```json
// Tool definition
{"type": "function", "function": {"name": "tool", "parameters": {...}}}

// Model response
{"tool_calls": [{"id": "call_*", "function": {"name": "tool", "arguments": "{...}"}}]}
```

### Anthropic Format
```json
// Tool definition
{"name": "tool", "input_schema": {...}}

// Model response (tool_use block)
{"type": "tool_use", "id": "toolu_*", "name": "tool", "input": {...}}
```

### MCP Format (rmcp)
```json
// Tool definition
{"name": "tool", "inputSchema": {...}}
```

**Note**: Our test format `{"name": "tool", "arguments": {...}}` is similar to MCP/Anthropic but simplified.

## Future Work

1. **Performance testing**: Benchmark grammar-constrained vs JSON Schema mode
2. **Grammar improvements**: More restrictive grammars to prevent infinite loops
3. **Model comparisons**: Test with non-thinking models (Llama3, etc.)
4. **Tool variant testing**: More ambiguous prompt scenarios
5. **Production format integration**: Align with OpenAI/Anthropic wire formats

## Test Harness

The `crucible-grammar` crate provides a test harness for these experiments:

```bash
# Quick test
./target/release/crucible-grammar --quick "Read README.md" --builtin-grammar

# Run test suite
./target/release/crucible-grammar --suite test_cases/basic.toml --builtin-grammar

# JSON output for analysis
./target/release/crucible-grammar --suite test_cases/basic.toml --builtin-grammar --json
```
