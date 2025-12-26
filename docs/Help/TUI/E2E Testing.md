---
description: How to write and run end-to-end tests for the TUI using expectrl
tags:
  - tui
  - testing
  - development
  - reference
status: implemented
---

# TUI E2E Testing

Crucible includes an expectrl-based test harness for end-to-end TUI testing. This enables PTY-based testing with real terminal emulation, supporting multi-turn interaction verification.

## Overview

The test harness spawns the `cru` binary in a pseudo-terminal (PTY), allowing tests to:

- Send keystrokes and text input
- Wait for expected output patterns
- Verify UI behavior across multi-turn interactions
- Test the full TUI stack (rendering, input handling, agent communication)

## Architecture

```
+------------------+
|  Test Case       |
|  (Rust test)     |
+--------+---------+
         |
         v
+------------------+
|  TuiTestSession  |-- Spawns `cru chat` in PTY
|  (expectrl)      |-- Sends keystrokes
|                  |-- Captures output
+--------+---------+
         |
         v
+------------------+
|  Assertions      |-- Pattern matching
|                  |-- Regex expectations
+------------------+
```

## Running Tests

Build the binary first (tests require a compiled `cru`):

```bash
cargo build --release
```

Run the TUI e2e tests (they're ignored by default since they require a built binary):

```bash
cargo test -p crucible-cli tui_e2e -- --ignored
```

Run a specific test:

```bash
cargo test -p crucible-cli smoke_help -- --ignored
```

## Test Harness API

### TuiTestConfig

Configuration for test sessions:

```rust
let config = TuiTestConfig {
    subcommand: "chat".to_string(),
    args: vec!["--no-splash".to_string()],
    env: vec![("MY_VAR".to_string(), "value".to_string())],
    timeout: Duration::from_secs(10),
    cols: 80,
    rows: 24,
    ..Default::default()
};
```

### TuiTestBuilder

Fluent builder for test sessions:

```rust
let mut session = TuiTestBuilder::new()
    .command("chat")
    .timeout(15)
    .env("DEBUG", "1")
    .spawn()
    .expect("Failed to spawn");
```

### TuiTestSession

The main session type for interacting with the TUI:

| Method | Description |
|--------|-------------|
| `spawn(config)` | Create session with config |
| `spawn_chat()` | Create session for `cru chat` |
| `expect(pattern)` | Wait for text pattern |
| `expect_regex(pattern)` | Wait for regex pattern |
| `send(text)` | Send raw text |
| `send_line(text)` | Send text with Enter |
| `send_key(key)` | Send special key |
| `send_control(char)` | Send Ctrl+char |
| `expect_eof()` | Wait for process exit |
| `wait(duration)` | Fixed delay |
| `output_contains(pattern)` | Non-blocking check |

### Key Enum

Special keys for TUI interaction:

```rust
pub enum Key {
    Up, Down, Left, Right,
    Enter, Escape, Tab,
    Backspace, Delete,
    Home, End,
    PageUp, PageDown,
    F(u8),  // F1-F12
}
```

## Writing Tests

### Basic Test Pattern

```rust
#[test]
#[ignore = "requires built binary"]
fn my_tui_test() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn");

    // Wait for initial render
    session.wait(Duration::from_secs(1));

    // Interact with TUI
    session.send("Hello").expect("Failed to send");
    session.send_key(Key::Enter).expect("Failed to send Enter");

    // Verify behavior
    session.wait(Duration::from_millis(500));

    // Clean exit
    session.send_control('c').expect("Failed to exit");
}
```

### Testing Slash Commands

```rust
#[test]
#[ignore = "requires built binary"]
fn test_help_command() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    // Trigger command popup
    session.send("/").expect("Failed to send /");
    session.wait(Duration::from_millis(200));

    // Navigate and select
    session.send("help").expect("Failed to type help");
    session.send_key(Key::Enter).expect("Failed to confirm");

    // Clean exit
    session.send_control('c').expect("Failed to exit");
}
```

### Multi-Turn Conversations

For tests that require actual agent responses:

```rust
#[test]
#[ignore = "requires built binary and ACP agent"]
fn test_conversation() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)  // Longer for LLM responses
        .spawn()
        .expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    // Turn 1
    session.send_line("Say hello").expect("Failed to send");
    session.wait(Duration::from_secs(10));

    // Turn 2
    session.send_line("Now say goodbye").expect("Failed to send");
    session.wait(Duration::from_secs(10));

    session.send_control('c').expect("Failed to exit");
}
```

## Test Categories

The test suite is organized into categories:

| Category | Purpose | Examples |
|----------|---------|----------|
| Smoke | Basic startup/shutdown | `smoke_version`, `smoke_help` |
| Navigation | Key sequences, popups | `chat_popup_navigation` |
| Input | Text entry, backspace | `chat_input_typing` |
| Commands | Slash command behavior | `chat_help_command` |
| Multi-turn | Full conversation flows | `chat_multiturn_basic` |
| Stress | Rapid input handling | `chat_rapid_input` |

## Future Capabilities

The harness is designed to support future enhancements:

### Timestamped Recording

The `OutputChunk` type captures output with timestamps for:

- **Flicker detection** - Identify rapid redraws
- **Frame diffing** - Compare render states
- **Performance profiling** - Measure render latency

### VTE Parsing

Future versions may integrate VTE parsing for:

- Escape sequence analysis
- Cursor position tracking
- Color/style verification

## Tips

**Start with smoke tests**: Verify basic `--help` and `--version` work before testing the full TUI.

**Use generous timeouts**: Agent responses vary in timing. Use 10-30 second timeouts for conversation tests.

**Clean exit**: Always send Ctrl+C at the end of tests to avoid orphaned processes.

**Mark tests as ignored**: TUI tests require a built binary and may need infrastructure. Use `#[ignore = "reason"]`.

**Test incrementally**: Build up complex interactions step by step, verifying each stage.

## File Locations

| File | Purpose |
|------|---------|
| `crates/crucible-cli/tests/tui_e2e_harness.rs` | Test harness implementation |
| `crates/crucible-cli/tests/tui_e2e_tests.rs` | Test cases |

## See Also

- [[Help/TUI/Index|TUI Reference]] - TUI architecture overview
- [[Help/TUI/Component Architecture]] - Widget system details
- [[Help/Rune/Testing Plugins]] - Testing Rune plugins
