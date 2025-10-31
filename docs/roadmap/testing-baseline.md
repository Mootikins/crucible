# Testing Baseline – 2025-10-30

Command: `cargo test --workspace --no-run`

Outcome:
- Build succeeded, but emitted extensive warnings (primarily from `crucible-surrealdb`).
- Common warning themes:
  - Unused imports and variables.
  - Dead code in embedding/kiln helpers.
  - Redundant comparisons (`>= 0` on unsigned durations).
- No failing crates; integration binaries compiled without running.

Notes & Follow-ups:
- Capture representative warning examples (e.g., `crates/crucible-surrealdb/src/kiln_integration.rs` unused `RelationalDB` import) for later cleanup.
- Future phases should drive warning count down as modules are simplified or retired.
- When re-running this baseline later, compare warning volume to confirm improvements.
