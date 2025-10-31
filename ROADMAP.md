# Crucible Roadmap

Local-first knowledge management means the CLI and future desktop app must share a simple, testable core. This roadmap breaks the refactor into incremental phases so the business logic lives behind clean interfaces while presentation layers stay thin.

---

## Phase 0 – Baseline & Record
- Run `cargo test --workspace --no-run` and note which crates build tests; capture the current state in `docs/roadmap/testing-baseline.md`.
- Summarize the existing CLI boot flow (config → command modules → globals) in `README.md` so we have a before/after snapshot.
- Create `PERSONAL.md` (or update it) with a checklist of these phases for quick tracking.

## Phase 1 – Remove Dead Artifacts
- Delete the unused workspace-root `tests/*.rs`, the Phase 8 harnesses, and `tests/test-kiln/`. Keep any reusable markdown examples under `examples/`.
- Drop legacy docs that describe the old orchestrators (e.g. `README_PHASE8_INTEGRATION_TESTS.md`, `*_SUMMARY.md`) and point links at the new baseline note.
- Update `CONTRIBUTING.md` to clarify that integration tests live inside workspace crates, preventing future regressions.

## Phase 2 – Local Test Utilities
- Expose shared fixtures in `crucible_core::test_support`, then add lightweight `tests::support` modules per crate (starting with the CLI) that wrap CLI- or UI-specific helpers.
- Port existing CLI integration tests to use the shared module inside the crate, removing custom env/tempfile plumbing.
- Add smoke tests to each module to prove the helpers behave and avoid coupling between crates.

## Phase 3 – Command Dependency Injection
- For each CLI command (start with `search`, then `fuzzy`, `semantic`, `note`, `config`):
  - Introduce a small `CommandService` struct with a `run` method that accepts the relevant inputs, returning results without touching global state.
  - Inject dependencies such as the tool executor or kiln repository via traits exposed by the crate’s `tests::support` module.
  - Add missing test coverage via TDD before removing the old helpers (query validation, empty results, note edge cases, etc.).
- Ensure `main.rs` constructs each command service once per invocation and hands in the shared dependencies.

## Phase 4 – Tool Manager Simplification
- Replace the `static mut` singleton in `CrucibleToolManager` with a `ToolExecutor` implementation backed by `OnceCell` or an owned struct.
- Migrate all imports of `execute_tool_global`/`ensure_initialized_global` to depend on the injected trait.
- Update tests to use the mock executor so tool-loading can be exercised deterministically.

## Phase 5 – CliApp Core (No “Services”)
- Create a `CliApp` builder that loads configuration once, initializes shared facilities (tool executor, kiln repository, optional watcher), and exposes handles for commands + REPL.
- Move watcher startup (`kiln_processor::ensure_watcher_running`) behind this app so it can be bypassed in tests.
- Adjust `main.rs` to construct the app and delegate command execution through it, isolating presentation (argument parsing, terminal IO) from core logic.

## Phase 6 – Kiln & Data Access Cleanup
- Replace `kiln_processor`’s external binary invocation with an in-process kiln updater that uses the shared repository.
- Create a `KilnRepository` abstraction for reading metadata/embeddings; the CLI uses it via dependency injection, and the desktop app can reuse it later.
- Add focused tests around metadata refresh and embedding updates using the fixtures.

## Phase 7 – REPL Alignment
- Refactor `commands::repl::execute` to receive the already-initialized `CliApp` components rather than rebuilding its own state.
- Cover REPL parsing edge cases (whitespace normalization, quoted args, history limits) with new tests that mock tool execution and kiln access.
- Strip any redundant configuration loading or global access from REPL modules.

## Phase 8 – Focused Integration Tests
- Introduce `crates/integration-tests` to exercise end-to-end scenarios using the new app core (search + note workflow, tool execution via mock, REPL startup).
- Reuse the per-crate support modules to keep tests fast and predictable; avoid custom runners or emoji logging so failures are easy to read.

## Phase 9 – Documentation Refresh
- Update `README.md`, `CLAUDE.md`, and `CONTRIBUTING.md` to describe the new architecture: config → `CliApp` core → CLI/REPL UI layers.
- Write `docs/testing.md` outlining where unit, integration, and manual tests live, with references to the fixture crate.
- Add a short “Future Work” section in `PERSONAL.md` for desktop-app hooks or eventual CI tasks.

## Phase 10 – Final Sanity
- Run `cargo fmt`, `cargo clippy --all-targets`, and `cargo test --workspace` to verify the refactor.
- Confirm no references remain to deleted Phase 8 code or global tool helpers.
- Archive the roadmap note with completion dates so the evolution stays documented.

---

Following these steps keeps the internal architecture portable while the CLI (and later desktop UI) focus purely on presentation. Each phase is intentionally small, making it easy to pause or regroup between milestones.
