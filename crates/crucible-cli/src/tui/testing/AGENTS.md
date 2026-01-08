# TUI Testing Infrastructure

> Instructions for AI agents working on Crucible TUI tests

## Quick Start

```rust
use crate::tui::testing::{Harness, fixtures::sessions, TEST_WIDTH, TEST_HEIGHT};
use crossterm::event::KeyCode;

#[test]
fn my_feature_works() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);
    h.keys("hello");
    assert_eq!(h.input_text(), "hello");
    insta::assert_snapshot!(h.render());
}
```

## Test Infrastructure Overview

### Core Components

| Component | Purpose | When to Use |
|-----------|---------|-------------|
| `Harness` | Full TUI simulation | Interactive tests, snapshots, e2e flows |
| `TestStateBuilder` | Isolated state construction | Unit tests of rendering logic |
| `fixtures::sessions` | Conversation histories | Tests needing chat context |
| `fixtures::registries` | Popup item lists | Tests needing commands/agents |
| `fixtures::events` | Streaming event sequences | Tests simulating LLM responses |

### Standard Dimensions

**Always use the exported constants:**
```rust
use crate::tui::testing::{TEST_WIDTH, TEST_HEIGHT};

let h = Harness::new(TEST_WIDTH, TEST_HEIGHT);
```

**Never define local constants:**
```rust
// BAD - creates maintenance burden
const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;
```

## Harness API Reference

### Setup Methods

```rust
// Basic harness
let h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

// With conversation history
let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
    .with_session(sessions::basic_exchange());

// With popup pre-populated
let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
    .with_popup_items(PopupKind::Command, registries::standard_commands());
```

### Input Simulation

```rust
h.key(KeyCode::Enter);              // Single key
h.keys("hello world");              // Type string
h.key_ctrl('c');                    // Ctrl+C
h.key_alt('x');                     // Alt+X
h.key_with_modifiers(KeyCode::Up, KeyModifiers::SHIFT);
```

### Event Injection

```rust
use fixtures::events;

// Streaming response
h.events(events::streaming_chunks("Hello from LLM"));

// Tool call lifecycle
h.events(events::tool_lifecycle("read_file", r#"{"path": "foo.txt"}"#, "file contents"));
```

### State Accessors

```rust
h.input_text()           // Current input buffer
h.cursor_position()      // Cursor position in input
h.has_popup()            // Is popup visible?
h.popup_query()          // Current filter text
h.popup_selected()       // Selected item index
h.conversation_len()     // Number of conversation items
h.has_error()            // Is error displayed?
h.error()                // Error message if any
```

### Rendering

```rust
h.render()               // Full terminal as string (for snapshots)
h.render_input()         // Just input area
h.render_terminal()      // Raw Terminal<TestBackend>
```

## Test Patterns

### Pattern 1: Popup Interaction

```rust
#[test]
fn command_popup_filters() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
        .with_popup_items(PopupKind::Command, registries::standard_commands());

    h.keys("sea");

    assert_eq!(h.popup_query(), Some("sea"));
    assert_snapshot!(h.render());
}
```

### Pattern 2: Conversation Display

```rust
#[test]
fn shows_conversation_history() {
    let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
        .with_session(sessions::multi_turn());

    assert_eq!(h.conversation_len(), 4);
    assert_snapshot!(h.render());
}
```

### Pattern 3: Streaming Response

```rust
#[test]
fn streaming_updates_display() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

    h.events(events::streaming_chunks("Hello, I am responding..."));

    assert!(!h.has_error());
    assert_snapshot!(h.render());
}
```

### Pattern 4: Tool Call Lifecycle

```rust
#[test]
fn tool_call_shows_progress() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

    h.event(events::tool_call_event("search", r#"{"query": "test"}"#));
    assert_snapshot!("tool_running", h.render());

    h.event(events::tool_completed_event("search", "Found 3 results"));
    assert_snapshot!("tool_completed", h.render());
}
```

### Pattern 5: Multi-Step E2E Flow

```rust
#[test]
fn full_command_flow() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
        .with_popup_items(PopupKind::Command, registries::standard_commands());

    // Step 1: Filter
    h.keys("help");
    assert_eq!(h.popup_query(), Some("help"));

    // Step 2: Select
    h.key(KeyCode::Enter);
    assert!(!h.has_popup());
    assert!(h.input_text().starts_with("/help"));

    // Step 3: Snapshot final state
    assert_snapshot!(h.render());
}
```

## Fixture Guidelines

### Sessions (`fixtures::sessions`)

Use for tests needing conversation context:

```rust
sessions::empty()              // No messages
sessions::basic_exchange()     // User + Assistant
sessions::multi_turn()         // Multiple exchanges
sessions::with_tool_calls()    // Includes tool invocations
sessions::long_conversation()  // Scroll testing
```

### Registries (`fixtures::registries`)

Use for popup/completion tests:

```rust
registries::standard_commands()  // Common slash commands
registries::minimal_commands()   // Small set for focused tests
registries::test_agents()        // Agent cards
registries::test_files()         // File references
registries::test_notes()         // Note references
```

### Events (`fixtures::events`)

Use for streaming/tool simulation:

```rust
events::streaming_chunks(text)           // Break text into chunks
events::streaming_chars(text)            // Character-by-character
events::tool_call_event(name, args)      // Tool invocation
events::tool_completed_event(name, out)  // Tool result
events::tool_lifecycle(name, args, out)  // Full sequence
```

## File Organization

```
src/tui/testing/
├── mod.rs                    # Exports, re-exports TEST_WIDTH/HEIGHT
├── harness.rs                # Core test harness
├── state_builder.rs          # TestStateBuilder + constants
├── AGENTS.md                 # This file
├── fixtures/
│   ├── mod.rs
│   ├── sessions.rs           # Conversation fixtures
│   ├── registries.rs         # Popup item fixtures
│   └── events.rs             # Event sequence fixtures
├── e2e_flow_tests.rs         # Multi-step workflow tests
├── harness_tests.rs          # Harness API tests
├── popup_snapshot_tests.rs   # Popup rendering tests
└── tool_call_tests.rs        # Tool call rendering tests
```

## Running Tests

```bash
# All TUI tests
cargo test -p crucible-cli tui::testing

# Specific test file
cargo test -p crucible-cli e2e_flow_tests

# Single test
cargo test -p crucible-cli step1_slash_opens_popup

# Update snapshots
cargo insta review
```

## Common Mistakes

### Using local constants instead of TEST_WIDTH/TEST_HEIGHT
```rust
// BAD
const WIDTH: u16 = 80;
let h = Harness::new(WIDTH, HEIGHT);

// GOOD
use crate::tui::testing::{TEST_WIDTH, TEST_HEIGHT};
let h = Harness::new(TEST_WIDTH, TEST_HEIGHT);
```

### Forgetting to make harness mutable for input
```rust
// BAD - won't compile
let h = Harness::new(TEST_WIDTH, TEST_HEIGHT);
h.keys("test");  // Error: cannot borrow as mutable

// GOOD
let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);
h.keys("test");
```

### Testing implementation details instead of behavior
```rust
// BAD - brittle, tests internals
assert_eq!(h.state.popup.as_ref().unwrap().items.len(), 3);

// GOOD - tests observable behavior
assert!(h.has_popup());
assert_snapshot!(h.render());
```

## Adding New Tests

1. **Choose the right pattern** from examples above
2. **Use existing fixtures** - check fixtures/ before creating new data
3. **Use TEST_WIDTH/TEST_HEIGHT** - never define local constants
4. **Prefer snapshots** for visual verification
5. **Test behavior, not implementation** - use Harness accessors
6. **Name tests descriptively** - `popup_closes_on_escape` not `test_esc`

## Extending Fixtures

When adding new fixture functions:

1. Add to appropriate file in `fixtures/`
2. Document with `///` comment
3. Export from `fixtures/mod.rs`
4. Add validation test if complex
