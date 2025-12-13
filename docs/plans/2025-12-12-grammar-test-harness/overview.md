# Grammar Test Harness - Overview

## Goal

Test whether GBNF-constrained generation improves tool-calling accuracy for local models (7B-32B range). Compare constrained vs unconstrained generation on identical prompts.

## Hypothesis

Grammar constraints can substitute for model size/training by making tool calls deterministic - the model only needs to predict *which* tool and *what* values, not *how* to format them.

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  Test Harness   │────▶│  llama-server        │────▶│  Tool Executor  │
│  (Rust binary)  │     │  (via llama-swap)    │     │  (MCP bridge)   │
└─────────────────┘     └──────────────────────┘     └─────────────────┘
        │                        │                          │
   test_cases/*.toml       grammar.gbnf              mock | live
```

## Metrics

1. **Parse rate** - Did output match grammar? (always 100% with grammar)
2. **Tool selection** - Correct tool for the task?
3. **Parameter accuracy** - Correct values extracted?
4. **Task completion** - (live mode) Did the operation succeed?

## Scope

- Fixed tool set: L0 (read/write/edit/ls) + L1 (git/rg)
- Single endpoint: llama.krohnos.io
- Models: Start with Qwen3-Coder, QwQ, DeepSeek-R1

## Non-goals (for now)

- Dynamic tool loading
- Rune integration
- Multi-turn conversations
- Training/fine-tuning
