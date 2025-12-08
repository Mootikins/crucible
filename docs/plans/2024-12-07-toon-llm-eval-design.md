# TOON LLM Evaluation Design

**Date:** 2024-12-07
**Goal:** Test how well local Ollama models can read/write TOON format to evaluate the value of building a JSON→TOON converter.

## Overview

TOON (Token-Oriented Object Notation) is a compact, human-readable encoding designed for LLM prompts. This test suite evaluates LLM capabilities with TOON to answer:

1. Can LLMs reliably parse TOON and convert to JSON? (expected: yes)
2. Can LLMs reliably write valid TOON from JSON? (expected: needs examples)
3. What prompt strategies work best for TOON generation?

## Test Location

- **Integration tests:** `crates/crucible-llm/tests/toon_eval.rs`
- **Modules:** `crates/crucible-llm/tests/toon_eval/*.rs`
- **Report output:** `target/toon_eval_report.md`

## Configuration

All configuration via environment variables (no hardcoded infra):

- `OLLAMA_BASE_URL` - Ollama endpoint (default: `http://localhost:11434`)
- `TOON_EVAL_MODEL` - Model to test (default: `qwen3:8b`)

## Modular Prompt System

Toggleable prompt components that combine into test configurations:

### Components

| Component | Description |
|-----------|-------------|
| `SpecGrammar` | ABNF grammar excerpt from TOON spec |
| `SpecRules` | Quoting, escaping, indentation rules |
| `ExampleSimple` | Flat object JSON→TOON pair |
| `ExampleNested` | Nested object example |
| `ExampleTabular` | Array with `{field,list}:` syntax |
| `ExampleMixed` | Complex mixed arrays |
| `TaskJsonToToon` | "Convert this JSON to TOON format" |
| `TaskToonToJson` | "Convert this TOON to JSON format" |
| `TaskToonQuery` | "Answer this question about the TOON data" |

### Configurations

| Config | Components |
|--------|------------|
| `ZeroShot` | Task only |
| `SpecOnly` | Task + grammar/rules |
| `FewShot(n)` | Task + n examples |
| `SpecPlusFewShot(n)` | Task + spec + n examples |
| `Full` | Everything |

## Test Fixtures

### Complexity Ladder

1. **Primitives & flat objects** - `{"name": "Ada", "age": 30}`
2. **Nested objects** - 2-3 levels deep
3. **Simple arrays** - `{"tags": ["a", "b", "c"]}`
4. **Tabular arrays** - Uniform objects (TOON's sweet spot)
5. **Mixed arrays** - Heterogeneous elements
6. **Real-world** - MCP tool responses, API payloads
7. **Edge cases** - Quoting, escaping, special values

### TOON-Specific Features

- Tabular syntax `{id,name,qty}:`
- Key folding `a.b.c: 1`
- Delimiter variations (comma, tab, pipe)
- Empty arrays `[0]:`

## Validation & Error Categorization

### Validation Flow

1. LLM produces output
2. Parse with `toon-format` crate
3. Decode to `serde_json::Value`
4. Compare structure to expected JSON

### Error Categories

| Category | Description |
|----------|-------------|
| `InvalidSyntax` | toon-format parse failure |
| `MissingField` | Expected field not present |
| `ExtraField` | Unexpected field present |
| `WrongType` | Field has wrong JSON type |
| `WrongArrayLength` | Array size mismatch |
| `ValueMismatch` | Right structure, wrong value |

## Report Format

Markdown report with:

1. **Summary table** - Pass/fail rates by direction and config
2. **Error analysis** - Breakdown by error category
3. **Detailed results** - Per-test outcomes

## Dependencies

- `toon-format` - Official Rust TOON implementation (spec v2.0)
- Existing `crucible-llm` infrastructure for Ollama calls

## Success Criteria

- TOON→JSON: >90% success rate (zero-shot)
- JSON→TOON with examples: >70% success rate
- Clear error categorization for failures
- Report enables informed decision on JSON→TOON converter value
