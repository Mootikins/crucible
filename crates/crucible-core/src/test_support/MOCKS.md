# Mock Implementations for Testing

This module provides in-memory mock implementations of core traits for deterministic tests.

## Available Mocks

### MockEventEmitter

`MockEventEmitter<E>` (`src/test_support/mocks/event_emitter.rs`) implements `EventEmitter`. It
records each event it receives, so a test can inspect what the code under test emitted.

**Features:**

- Records every emitted event in order
- Counts emits, cancels, failures and handler errors in `MockEventEmitterStats`
- Injects an error, a cancel, handler errors or an unavailable state through `MockEmitterBehavior`

**Usage:**

```rust
use crucible_core::test_support::mocks::MockEventEmitter;

let emitter: MockEventEmitter<MyEvent> = MockEventEmitter::new();

// Inspect what the code under test emitted
assert_eq!(emitter.event_count(), 0);
let events = emitter.emitted_events();
let last = emitter.last_event();

// Configure failures
emitter.set_cancel_events(true);
emitter.set_unavailable(true);

// Clear recorded events and counters between cases
emitter.reset();
```

## Best Practices

- Call `reset()` between cases in one test so that the counters start at zero.
- Assert on `stats()` to verify how many times the code under test emitted.
- Use `set_error` or `add_handler_error` to test the error path of the caller.

## History

`MockStorage` and `MockHashingAlgorithm` were deleted in Consolidation Plan batch B17. They
had no caller.
