# Crate Structure

## New Crate: `crucible-grammar-test`

```
crates/crucible-grammar-test/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library root
│   ├── harness.rs        # Test runner
│   ├── grammar.rs        # Grammar loading/building
│   ├── scoring.rs        # Result evaluation
│   ├── executor.rs       # Tool execution (wraps MCP bridge)
│   └── api.rs            # llama-server API client
├── grammars/
│   └── tools.gbnf        # The grammar file
└── test_cases/
    ├── read_file.toml
    ├── edit_file.toml
    └── search_code.toml
```

## Dependencies

```toml
[dependencies]
crucible-core = { path = "../crucible-core" }
crucible-tools = { path = "../crucible-tools" }  # MCP tools
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
```

## CLI Interface

```bash
# Run all test cases, constrained mode
crucible-grammar-test --mode constrained

# Run specific test, unconstrained (baseline)
crucible-grammar-test --mode unconstrained --test read_file

# Live execution mode
crucible-grammar-test --mode constrained --live

# Output JSON results
crucible-grammar-test --output results.json
```

## Core Types

```rust
/// Test case definition
struct TestCase {
    name: String,
    prompt: String,
    expected_tool: String,
    expected_params: HashMap<String, Value>,
    setup: Option<SetupStep>,      // for live mode
    verification: Option<String>,   // for live mode
}

/// Test result
struct TestResult {
    case: String,
    mode: Mode,
    parsed: bool,
    tool_correct: bool,
    params_correct: bool,
    task_success: Option<bool>,
    raw_output: String,
    latency_ms: u64,
}

/// Generation mode
enum Mode {
    Constrained { grammar: String },
    Unconstrained,
}
```

## Integration Points

1. **MCP Bridge** - Use existing `crucible-tools` via new `ToolExecutor` impl
2. **API Client** - Direct HTTP to llama.krohnos.io/v1/chat/completions
3. **Grammar** - Load from file, pass as `grammar` field (or `response_format` fallback)
