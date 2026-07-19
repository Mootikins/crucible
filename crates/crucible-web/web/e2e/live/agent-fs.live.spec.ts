import { test, expect } from '@playwright/test';
import { existsSync, readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { readHeroState } from './hero-state';
import { AGENT_FS_WRITE } from './hero-script';
import { findTuiTestBinary, runTuiLeg } from './tui-leg-runner';

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
 * IMPORTANT ASYMMETRY: a web-created session's tool `workspace` is the
 * registered PROJECT root (`home`), not the kiln (`home/.crucible`) —
 * `SessionPanel.handleCreateSession` passes `workspace: project.path`
 * explicitly, unlike a plain `cru chat`/`cru session create` (no
 * `--workspace` flag), whose session workspace defaults to the kiln. So the
 * web leg's file lands under `home/notes/agent-web.md`, while the TUI leg's
 * lands under `home/.crucible/notes/agent-tui.md` (the kiln hero-setup
 * already builds notes under). Both are derived from `state.kilnDir` below
 * (`kilnDir = path.join(home, '.crucible')`, set by hero-setup.ts).
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
  const homeDir = path.dirname(kilnDir); // hero-setup.ts: kilnDir = path.join(home, '.crucible')
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
  // App-ready signal (the center pane defaults to a "Home" welcome tab, not
  // an empty state — see e2e/new-session-chat-tab.spec.ts for the same idiom).
  await expect(page.getByTestId('new-session-button')).toBeVisible({ timeout: 20_000 });

  await page.getByTestId('new-session-button').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 15_000 });

  await page.getByTestId('chat-input').fill(`Please ${AGENT_FS_WRITE.web.trigger} to create the note.`);
  await page.getByTestId('chat-input').press('Enter');

  // The daemon blocks on a real interaction_requested event before the tool
  // runs; the web console renders the real PermissionInteraction card inline
  // in the chat (crucible-web/src/events.rs normalize_interaction flattens
  // the wire shape so tool_name renders here).
  await expect(page.getByText('Permission Required')).toBeVisible({ timeout: 30_000 });
  // Tool activity indicator: the permission card names the real tool (shown
  // twice — the "Tool:" label and the raw-args fallback block).
  await expect(page.getByText('write_file').first()).toBeVisible();

  await page.getByRole('button', { name: 'Allow' }).click();
  await expect(page.getByText('Permission Required')).toHaveCount(0);

  await expect(
    page.getByTestId('message-assistant').filter({ hasText: AGENT_FS_WRITE.web.replyAfterTool }).first(),
  ).toBeVisible({ timeout: 30_000 });

  const webNotePath = path.join(homeDir, AGENT_FS_WRITE.web.path);
  await expect.poll(() => existsSync(webNotePath), { timeout: 15_000 }).toBe(true);
  expect(readFileSync(webNotePath, 'utf-8')).toBe(AGENT_FS_WRITE.web.content);
});
