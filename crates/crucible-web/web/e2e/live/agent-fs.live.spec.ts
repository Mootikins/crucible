import { test, expect } from '@playwright/test';
import { existsSync, readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { readHeroState } from './hero-state';
import { AGENT_FS_WRITE } from './hero-script';
import { findTuiTestBinary, runTuiLeg } from './tui-leg-runner';
import { appReady, openSessionsList } from '../helpers/nav';

/**
 * The flagship full-flow journey: new session → agent responds → agent
 * AFFECTS THE FILESYSTEM via a real `write_file` tool call, through BOTH
 * consoles, fully deterministic via the fake Ollama server (hero-setup).
 *
 * Both legs prove the same shape: the daemon dispatches `write_file` through
 * the default-deny permission gate (write_file is not in the safe-tool
 * allowlist — `is_safe()` in agent_manager/mod.rs), blocks on a real
 * `interaction_requested` event, the console renders it as a permission
 * prompt, and approving it lets the tool run and the file land on disk.
 *
 * PART A (TUI leg) spawns the ignored Rust test `hero::agent_fs_leg_tui_write`
 * (crates/crucible-cli/tests/tui_e2e_tests/hero.rs), which drives its own
 * FRESH `cru chat` session end-to-end (prompt → permission modal → `y` →
 * reply → file-on-disk assertion) — this spec only asserts the leg passed
 * and double-checks the resulting file.
 *
 * PART B (web leg) drives the real browser UI: New Session → send the
 * trigger prompt → the real `PermissionInteraction` card renders inline →
 * click Allow → the reply renders → the file lands on disk.
 *
 * Both legs write into `state.kilnDir`, but by different routes, and that is
 * worth knowing when this breaks: a web-created session's tool `workspace` is
 * the registered PROJECT root — `SessionPanel.handleCreateSession` passes
 * `workspace: project.path` explicitly — while a plain `cru chat` (no
 * `--workspace`) defaults its workspace to the kiln. hero-setup registers the
 * kiln dir as the project, so the two coincide here, as they do for a real
 * user working in one directory. If the harness ever registers a project that
 * is NOT the kiln, the web leg's file moves and the TUI leg's does not.
 *
 * Requires a real `cru` (CRU_BIN or target/debug/cru) + the built TUI test
 * binary; otherwise hero-setup writes { skip:true } and this skips cleanly.
 */

test.describe.configure({ mode: 'serial' });

test('agent writes a file: TUI leg then web leg, both via a real permission approval', async ({ page }, testInfo) => {
  const state = readHeroState();
  test.skip(state.skip, `hero setup skipped: ${state.reason ?? 'unknown'}`);

  const tuiBin = findTuiTestBinary();
  test.skip(
    !tuiBin,
    'tui_e2e_tests binary not built — run `cargo test -p crucible-cli --test tui_e2e_tests --no-run`',
  );

  const kilnDir = state.kilnDir!;
  const framesDir = path.join(testInfo.outputDir, 'tui-frames');
  mkdirSync(framesDir, { recursive: true });

  const legEnv: NodeJS.ProcessEnv = {
    ...process.env,
    ...(state.childEnv ?? {}),
    CRU_BIN: state.cruBin!,
    HERO_KILN: kilnDir,
    HERO_ARTIFACT: framesDir,
    RUST_LOG: 'warn',
  };

  // ── PART A — TUI console: fresh session, tool call, real permission prompt ──
  const leg = await runTuiLeg(tuiBin!, 'hero::agent_fs_leg_tui_write', legEnv, 120_000);
  expect(leg.code, `tui leg failed:\n${leg.out}`).toBe(0);
  expect(leg.out, `tui leg did not actually run:\n${leg.out}`).toContain('1 passed');

  const tuiNotePath = path.join(kilnDir, AGENT_FS_WRITE.tui.path);
  expect(readFileSync(tuiNotePath, 'utf-8')).toBe(AGENT_FS_WRITE.tui.content);

  // ── PART B — web console: New Session, tool call, real permission prompt ──
  await page.goto(state.baseURL!);
  // App ready, then Navigator into Sessions scope — `new-session-button` only
  // exists there. Both via e2e/helpers/nav.ts, the same path the mock tier
  // takes (e2e/new-session-chat-tab.spec.ts); asserting the testid inline is
  // what let this spec rot past the Navigator refactor.
  await appReady(page);
  await openSessionsList(page);

  // New Session opens a DRAFT surface with the center composer — nothing hits
  // the daemon until the first message, which creates the session and swaps the
  // draft tab for a real chat tab (`chat-input`). This spec used to reach
  // straight for `chat-input`, which only exists after that swap.
  await page.getByTestId('new-session-button').click();
  await expect(page.getByTestId('composer-input')).toBeVisible({ timeout: 15_000 });

  // Bind the draft to the registered project. The composer defaults to
  // "Session folder" — the daemon's per-session scratch dir — and this spec is
  // specifically about the project-rooted workspace, so choose it explicitly
  // rather than depending on whatever the default happens to be.
  await page.getByTestId('composer-project').click();
  await page
    .getByTestId('composer-project-popout')
    .getByText(path.basename(kilnDir), { exact: true })
    .click();

  await page.getByTestId('composer-input').fill(`Please ${AGENT_FS_WRITE.web.trigger} to create the note.`);
  await page.getByTestId('composer-send').click();
  // No "input is enabled" wait here: sending starts the turn, which disables
  // the composer until it completes. The permission card below IS the signal.

  // The daemon blocks on a real interaction_requested event before the tool
  // runs; the web console renders the real PermissionInteraction card inline
  // in the chat (crucible-web/src/events.rs normalize_interaction flattens
  // the wire shape so tool_name renders here).
  await expect(page.getByText('Permission Required')).toBeVisible({ timeout: 30_000 });
  // Tool activity indicator: the permission card names the real tool in its
  // action chip.
  await expect(page.getByTestId('perm-action-chip')).toHaveText('write_file');

  await page.getByRole('button', { name: 'Allow' }).click();
  await expect(page.getByText('Permission Required')).toHaveCount(0);

  await expect(
    page.getByTestId('message-assistant').filter({ hasText: AGENT_FS_WRITE.web.replyAfterTool }).first(),
  ).toBeVisible({ timeout: 30_000 });

  const webNotePath = path.join(kilnDir, AGENT_FS_WRITE.web.path);
  await expect.poll(() => existsSync(webNotePath), { timeout: 15_000 }).toBe(true);
  expect(readFileSync(webNotePath, 'utf-8')).toBe(AGENT_FS_WRITE.web.content);
});
