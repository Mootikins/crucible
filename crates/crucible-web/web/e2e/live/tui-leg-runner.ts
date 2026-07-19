import { spawn } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Shared runner for hero-flow TUI legs — Rust `#[ignore]`d tests in
 * `crates/crucible-cli/tests/tui_e2e_tests/hero.rs` (module-qualified as
 * `hero::<name>`), driven by spawning the prebuilt `tui_e2e_tests` libtest
 * binary directly (no cargo recompilation at test time — the binary must
 * already be built via `cargo test -p crucible-cli --test tui_e2e_tests
 * --no-run`).
 */

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const WORKSPACE_ROOT = path.resolve(HERE, '..', '..', '..', '..', '..');

/** Locate the compiled `tui_e2e_tests` libtest binary (has a hash suffix). */
export function findTuiTestBinary(): string | null {
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

export interface LegResult {
  code: number | null;
  out: string;
}

/** Run one ignored TUI leg (async spawn — never block the fake-ollama loop). */
export function runTuiLeg(
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
