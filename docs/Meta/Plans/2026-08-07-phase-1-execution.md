---
tags: [plan, execution, launch]
---

# Phase 1 Execution — The Stranger's First Ten Minutes

> Task-by-task execution guide for Phase 1 of [[2026-08-06-public-launch-readiness]].
> Line references verified against master `adcafe528` on 2026-08-07.

## What Phase 0 already closed

Re-verification against master before writing this doc:

- **§1.1 is done.** All four first-run bugs were fixed in Phase 0 (`e4e834b64`):
  the wizard fires for bare `cru` *and* interactive `cru chat`
  (`main.rs:76` `wants_first_run_setup`), the post-wizard config is reloaded in
  the same invocation (`main.rs:119-121`), `chat_preflight.rs:36-37` now passes
  `config.resolved_kiln_path()` into `discover_kiln` and persists the answer
  (`:90`), and `init.rs:161-182` registers the chosen provider globally via
  `register_llm_provider_in_config`. No task here; approval criterion 1 still
  applies at the end.
- **§1.4 is half done.** The ranking fix landed (`provider_detect.rs:209`
  sorts credential-backed providers first) and the false doc comment was
  rewritten (`:66-72`). Still open: the dead `match` over a constant
  (`:79-161`) and the unprobed `available: true` on the Ollama entry.

## Remaining tasks

Order: T1 and T2 are independent; T3 is independent of both; T4 (discoverability)
last because it edits the messages T1 and T2 create.

---

### T1 — Block on zero providers before accepting input (§1.2)

**Defect.** `providers_listed` is emitted with no emptiness guard
(`crucible-daemon/src/server/session/mod.rs:211-218`); the TUI no-ops on an
empty list (`crucible-cli/src/tui/oil/chat_app/message_handlers.rs:318-324`).
A user with zero providers gets a normal prompt, types, and receives
`agent turn error: <raw transport error>` (`stream.rs`).

**Design.**

1. **Canonical message, one place.** A helper in `crucible-cli` (used by both
   the preflight and `cru doctor`) producing the zero-provider remedy text.
   Must name all three: `cru auth login`, `ollama serve` (if a local model is
   wanted), and `cru doctor`.
2. **CLI preflight guard.** In `commands/chat.rs::execute_chat_command`, when
   the session will use the internal agent (`agent_name.is_none()` — `-a`
   always selects an ACP agent), call `DaemonClient::list_providers`
   (`rpc_client/client/agent.rs:309`, RPC `providers.list`) before entering the
   TUI or oneshot runner. Empty list → bail with the canonical message.
   This is presentation of a daemon-provided fact, not duplicated business
   logic; web can consume the same RPC.
3. **TUI notification fallback.** `ChatAppMsg::ProvidersListed` with an empty
   vec → surface a warning notification with the same remedies (covers resume
   and any path that skips preflight) instead of silently doing nothing.
4. **Doctor alignment.** `doctor.rs:92` currently recommends `cru config init`
   for zero providers — replace with the canonical helper output.

**Tests (write first).**

- Unit: the canonical message names `cru auth login`, `ollama serve`, and
  `cru doctor`.
- Unit (TUI): `ProvidersListed(vec![])` adds a warning notification;
  non-empty list still sets `current_provider` and adds no notification.
- Integration: preflight against a daemon with no configured providers and a
  scrubbed env exits with the canonical message before any TUI frame.
  (Hermeticity: `TestDaemon` with child-scoped env, not `set_var`.)

**Commit.** `feat(cli): block chat startup when no providers are configured`

---

### T2 — Cold-start daemon failure: seconds, not 51, and say why (§1.3)

**Defects** (all in current master):

- `rpc_client/client/mod.rs:163-173`: 50ms doubling ×10 = 51.15s of silence.
- `:221-227`: the spawned daemon's stdout **and stderr go to `Stdio::null()`**
  — the real failure reason is discarded. Same pattern at the second spawn
  site, `commands/daemon.rs:119-121` (`cru daemon start` background fork).
- `main.rs:195-202`: `daemon serve` is not a stdio command, so its default log
  level is `OFF` — even with stderr captured, tracing emits nothing.
- `cru daemon logs` is advertised (`cli/mod.rs:375` long_about) but has no
  subcommand (`commands/daemon.rs:14-42`).
- `commands/daemon.rs:50`: `Restart { wait: _ }` discards the flag
  (`restart_daemon` hardcodes `wait = true` at `:197`).

**Design.**

1. **Log destination.** `crucible_core::config::crucible_home().join("daemon.log")`
   (honors `CRUCIBLE_HOME`, so tests stay hermetic). Both spawn sites open it
   append-mode and pass it as `Stdio` for stdout+stderr; fall back to
   `Stdio::null()` if the file can't be opened (spawn must not fail because
   logging can't).
2. **Give the daemon something to say.** In `main.rs`, treat
   `daemon serve` / `daemon start --foreground` like stdio commands for level
   purposes: default `WARN` to stderr. Startup errors (config parse, bind
   failure) already reach stderr via anyhow's error print; this adds runtime
   warnings.
3. **Cap the backoff.** Delay doubles from 50ms but caps at 1s, 8 attempts:
   50+100+200+400+800+1000+1000+1000 ≈ 4.6s total.
4. **Failure message carries the cause.** On exhaustion, read the last ~15
   lines of `daemon.log` and append them to the error, keeping the existing
   `Try: cru daemon stop && cru daemon start` line and adding
   `cru daemon logs`.
5. **Implement `cru daemon logs`.** `-n/--lines N` (default 50) printing the
   tail of the file; a clear message when the file doesn't exist yet.
6. **Honor `Restart { wait }`.** Pass it through to `start_daemon(false, wait, …)`.

**Tests (write first).**

- Unit: tail-N helper on a tempfile (exact lines, short files, missing file).
- Unit: backoff schedule sums to < 6s (extract the delay sequence as a pure
  function so the test doesn't sleep).
- Integration: spawn with a deliberately corrupted `--config`; assert failure
  arrives in < 10s and the message contains the daemon's actual error text.
  (This is approval criterion 3 verbatim — `#[ignore]`d if it needs a real
  binary, with the prerequisite in the reason string.)
- Unit: `daemon logs` output for a seeded log file.

**Commit.** `fix(daemon): surface cold-start failures fast with the real cause`

---

### T3 — Finish `provider_detect` (§1.4 remainder)

**Defect.** `provider_detect.rs:79` fixes `provider_backend = BackendType::Ollama`,
so the `match` arms for OpenAI/Anthropic/OpenRouter/ZAI (`:103-154`) are dead
code (the post-match re-detection at `:163-202` compensates only partially),
and the Ollama entry asserts `available: true` with no probe (`:97`).

**Design.**

1. **Delete the dead match.** Restructure `detect_providers` as a flat sweep:
   each chat-capable backend with a credential (env or store, via the existing
   `has_api_key_with_source`) gets an entry — this un-kills OpenRouter and
   Z.AI, which today are only detectable through the constant's dead arms.
2. **Probe Ollama.** Cheap TCP connect to the resolved endpoint
   (`OLLAMA_HOST` > config endpoint > default) with a ~300ms timeout.
   `available` reflects the probe; an unreachable Ollama stays listed with
   `reason` saying it didn't answer, so the wizard can still offer it.
   Detection runs only in interactive setup (`cru init`, wizard), so one
   300ms probe is acceptable; keep the "no HTTP probes" doc honest by
   updating it (it becomes "no probes except a TCP dial to Ollama").
3. **`-y` picks availability.** `commands/init.rs` non-interactive mode takes
   the first *available* provider, not `providers[0]` blindly. With the T3
   sort already in place these usually coincide; the probe makes the
   dead-Ollama case explicit.

**Tests (write first).**

- With only `ANTHROPIC_API_KEY` present (via `EnvVarGuard` — this test
  genuinely exercises env reading — or injected lookup), detection includes
  Anthropic ranked above unprobed/unavailable Ollama, and `-y` selection is
  Anthropic. (Approval criterion 4.)
- OpenRouter and ZAI keys are detected (they are not today — regression
  proof for the dead-match deletion).
- Probe helper: connect to a bound listener → `true`; closed port → `false`
  within the timeout.

**Commit.** `fix(cli): detect all credentialed providers and probe ollama`

---

### T4 — Make `cru doctor` discoverable (§1.5)

- Add `doctor`, `search`, and `setup` rows to the README command table
  (`README.md:161-190` region).
- The three first-run failure paths must each mention `cru doctor`:
  1. daemon-connect failure — done in T2's message,
  2. zero providers — done in T1's canonical message,
  3. config-parse failure — `crucible-core/src/config/config/cli_app.rs`
     parse-error path (~`:275-288`): append `Try: cru doctor`.
- Test: config-parse failure text names `cru doctor`.

**Commit.** `docs: make cru doctor reachable from every first-run failure`

---

## Approval criteria (from the launch plan, Phase 1)

1. **1.1** — wizard choices take effect same-invocation; no kiln re-prompt from
   another directory. *(Fixed in Phase 0 — re-verify manually.)*
2. **1.2** — zero providers ⇒ `cru chat` blocks pre-input naming both remedies.
3. **1.3** — corrupted config ⇒ failure < 10s including the daemon's actual
   error text.
4. **1.4** — only `ANTHROPIC_API_KEY` ⇒ `cru init -y` configures Anthropic.
5. **1.5** — README lists `doctor`; all three failure paths mention it.
6. **End to end** — a human stranger reaches a grounded chat turn. *(Requires
   an actual other human; out of scope for this execution.)*

## Finishing

Branch `phase-1-first-ten-minutes`, one commit per task, `just ci` before the
final commit, code review, fast-forward merge.
