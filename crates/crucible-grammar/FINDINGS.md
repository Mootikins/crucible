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

## Future Work

1. **Performance testing**: Benchmark grammar-constrained vs JSON Schema mode
2. **Grammar improvements**: More restrictive grammars to prevent infinite loops
3. **Model comparisons**: Test with non-thinking models (Llama3, etc.)

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
