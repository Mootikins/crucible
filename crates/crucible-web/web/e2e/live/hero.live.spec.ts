import { test, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFileSync, readdirSync, existsSync, statSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readHeroState, STATE_FILE } from './hero-state';
import { HERO_REPLIES } from './hero-script';

/**
 * The hero flow — one cross-surface journey proving the mental model:
 * a session is a VM on the hypervisor (daemon); the TUI and web are stateless
 * consoles that attach/detach; kiln files are shared buffers. Every assertion is
 * "the same state is visible from the other console / on disk".
 *
 * Leg 1 (TUI console) starts the session, drives a deterministic turn 1, and
 * writes a note via the shell modal. Leg 2 (web console) resumes the SAME
 * session — turn 1 hydrates from daemon history — edits the note in the real
 * editor, saves (bytes change on disk), and drives turn 2. Leg 3 (TUI console
 * again) resumes once more: turns 1 AND 2 hydrate in the terminal, `!cat` shows
 * the browser's edit, and turn 3 lands. Determinism comes from a fake Ollama
 * server (hero-setup) scripted by user-message substring.
 *
 * Requires a real `cru` (CRU_BIN or target/debug/cru) + the built TUI test
 * binary; otherwise hero-setup writes { skip:true } and this skips cleanly.
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WORKSPACE_ROOT = path.resolve(HERE, '..', '..', '..', '..', '..');

test.describe.configure({ mode: 'serial' });

/** Locate the compiled `tui_e2e_tests` libtest binary (has a hash suffix). */
function findTuiTestBinary(): string | null {
  const depsDir = path.join(WORKSPACE_ROOT, 'target', 'debug', 'deps');
  if (!existsSync(depsDir)) return null;
  const candidates = readdirSync(depsDir)
    .filter((f) => /^tui_e2e_tests-[0-9a-f]+$/.test(f))
    .map((f) => path.join(depsDir, f))
    .filter((p) => {
      try {
        return statSync(p).isFile() && (statSync(p).mode & 0o111) !== 0;
      } catch {
        return false;
      }
    })
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return candidates[0] ?? null;
}

interface LegResult {
  code: number | null;
  out: string;
}

/** Run one ignored TUI leg (async spawn — never block the fake-ollama loop). */
function runTuiLeg(
  bin: string,
  testName: string,
  env: NodeJS.ProcessEnv,
  timeoutMs: number,
): Promise<LegResult> {
  return new Promise((resolve) => {
    const child = spawn(bin, ['--ignored', '--exact', '--nocapture', testName], { env });
    let out = '';
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (out += d));
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      resolve({ code: null, out: out + '\n[leg killed: timeout]' });
    }, timeoutMs);
    child.on('exit', (code) => {
      clearTimeout(timer);
      resolve({ code, out });
    });
  });
}

test('hero flow: TUI → web → TUI, one session across three consoles', async ({ page, request }, testInfo) => {
  const state = readHeroState();
  test.skip(state.skip, `hero setup skipped: ${state.reason ?? 'unknown'}`);

  const tuiBin = findTuiTestBinary();
  test.skip(
    !tuiBin,
    'tui_e2e_tests binary not built — run `cargo test -p crucible-cli --test tui_e2e_tests --no-run`',
  );

  const kilnDir = state.kilnDir!;
  // The session binds to the default kiln ($HOME/.crucible); the NOTE lives in
  // the content kiln (kilnDir) and is addressed by absolute path.
  const notePath = path.join(kilnDir, 'notes', 'from-tui.md');
  const heroStatePath = path.join(state.tmpDir!, 'hero-handoff.json');
  const framesDir = path.join(testInfo.outputDir, 'tui-frames');
  mkdirSync(framesDir, { recursive: true });

  const legEnv: NodeJS.ProcessEnv = {
    ...process.env,
    ...(state.childEnv ?? {}),
    CRU_BIN: state.cruBin!,
    HERO_STATE: heroStatePath,
    HERO_KILN: kilnDir,
    HERO_ARTIFACT: framesDir,
    RUST_LOG: 'warn',
  };

  // ── Leg 1 — TUI console: start the session, turn 1, write note via shell ──
  // Names are module-qualified (the legs live in `mod hero`); --exact needs it.
  const leg1 = await runTuiLeg(tuiBin!, 'hero::hero_leg_1', legEnv, 120_000);
  expect(leg1.code, `leg 1 failed:\n${leg1.out}`).toBe(0);
  expect(leg1.out, `leg 1 did not actually run:\n${leg1.out}`).toContain('1 passed');

  const handoff = JSON.parse(readFileSync(heroStatePath, 'utf-8')) as { session_id: string; kiln: string };
  const sessionId = handoff.session_id;
  expect(sessionId).toBeTruthy();
  // The terminal's shell write is on disk (the shared buffer).
  expect(readFileSync(notePath, 'utf-8')).toContain('terminal was here');

  // ── Leg 2 — web console: resume, hydrate turn 1, edit the note, turn 2 ──
  await page.goto(state.baseURL!);
  // Let the app settle (config → kiln → session list) before resuming.
  await expect(page.getByText('No session open')).toBeVisible({ timeout: 20_000 });

  // Resume by selecting the session in the Sessions panel (the real product
  // path — onSelect → selectSession resumes it, sets currentSession so the
  // composer binds, and opens the chat tab that hydrates history). The list
  // re-renders, so click via a synchronous DOM click on the present node.
  await expect
    .poll(() => page.locator(`[data-testid="session-item-${sessionId}"]`).count(), { timeout: 15_000 })
    .toBeGreaterThan(0);
  await page.evaluate((id) => {
    document.querySelector<HTMLElement>(`[data-testid="session-item-${id}"]`)?.click();
  }, sessionId);
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 15_000 });

  // Turn 1 hydrates from daemon history — BOTH sides visible in this console.
  await expect(page.getByTestId('message-user').filter({ hasText: 'Summarize' }).first())
    .toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId('message-assistant').filter({ hasText: 'baseline' }).first())
    .toBeVisible({ timeout: 15_000 });

  // Open notes/from-tui.md in the REAL editor via the product open-file event
  // (works against the bundled live app — a source import would not).
  await page.evaluate((filePath) => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-file', { detail: { path: filePath, name: 'from-tui.md' } }),
    );
  }, notePath);
  await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('.cm-content')).toContainText('terminal was here');

  // Edit → dirty → Save → clean, then assert the bytes changed on disk.
  await page.locator('.cm-content').first().click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.type('browser was here');
  await expect(page.getByTestId('status-save')).toBeVisible();
  await page.getByTestId('status-save').click();
  await expect(page.getByTestId('status-save')).toHaveCount(0);
  await expect
    .poll(() => readFileSync(notePath, 'utf-8'), { timeout: 10_000 })
    .toContain('browser was here');
  expect(readFileSync(notePath, 'utf-8')).toContain('terminal was here');

  // Editing opened a file tab (now active). Re-focus the chat tab (open-session
  // re-activates the existing tab) so the composer is reachable.
  await page.evaluate((id) => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-session', { detail: { sessionId: id, title: 'hero' } }),
    );
  }, sessionId);

  // Turn 2 from the web console (deterministic reply via fake-ollama). Submit
  // with Enter — the send button re-renders with streaming state and loses
  // Playwright's click to detach retries.
  await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 10_000 });
  await page.getByTestId('chat-input').fill('What did I write in from-tui?');
  await page.getByTestId('chat-input').press('Enter');
  await expect(page.getByTestId('message-assistant').filter({ hasText: 'records' }).first())
    .toBeVisible({ timeout: 30_000 });

  // Two user turns now visible in this console (turn 1 hydrated + turn 2 sent).
  await expect.poll(() => page.getByTestId('message-user').count(), { timeout: 15_000 })
    .toBeGreaterThanOrEqual(2);

  // ── Leg 3 — TUI console again: hydrate turns 1&2, cat shows browser edit ──
  // Detach the web console: pause so the terminal can resume from storage. Leg 3
  // internally asserts turns 1 AND 2 hydrate in the terminal (baseline + records)
  // and that `!cat` shows the browser's edit, then sends turn 3 — so a passing
  // leg 3 proves all 3 turns live on the daemon and the buffer is shared.
  await request.post(`${state.baseURL}/api/session/${sessionId}/pause`).catch(() => undefined);
  const leg3 = await runTuiLeg(tuiBin!, 'hero::hero_leg_3', legEnv, 120_000);
  expect(leg3.code, `leg 3 failed:\n${leg3.out}`).toBe(0);
  expect(leg3.out, `leg 3 did not actually run:\n${leg3.out}`).toContain('1 passed');

  // ── Final state: the shared buffer carries BOTH the terminal and browser edits ──
  const finalBytes = readFileSync(notePath, 'utf-8');
  expect(finalBytes).toContain('terminal was here');
  expect(finalBytes).toContain('browser was here');

  // The web console still answers (no orphaned/wedged daemon; teardown reaps procs).
  expect((await request.get(`${state.baseURL}/api/config`)).ok()).toBeTruthy();

  await testInfo.attach('hero-replies', {
    body: JSON.stringify(HERO_REPLIES, null, 2),
    contentType: 'application/json',
  });
});

// Referenced so the import isn't flagged if the file is linted in isolation.
void STATE_FILE;
