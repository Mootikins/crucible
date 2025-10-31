# Crucible Refactor Checklist

_Owner: Solo project – updated 2025-10-30_

- [x] Phase 0 — Record baseline (`cargo test --workspace --no-run`, document current CLI flow)
- [ ] Phase 1 — Remove legacy root tests and Phase 8 harnesses
- [ ] Phase 2 — Introduce `crates/test-support` fixtures
- [ ] Phase 3 — Refactor commands around injected dependencies
- [ ] Phase 4 — Replace global tool manager with injected executor
- [ ] Phase 5 — Build lightweight `CliApp` core
- [ ] Phase 6 — Simplify kiln/data access
- [ ] Phase 7 — Align REPL with new core
- [ ] Phase 8 — Create focused integration test crate
- [ ] Phase 9 — Refresh docs/testing guides
- [ ] Phase 10 — Final verification pass
