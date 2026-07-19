import { execFileSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { readHeroState } from './hero-state';
import type { FakeOllama } from './fake-ollama';

async function globalTeardown(): Promise<void> {
  const state = readHeroState();

  // Close the in-process fake Ollama (stashed by hero-setup in this process).
  const fake = (globalThis as Record<string, unknown>).__heroFakeOllama as FakeOllama | undefined;
  if (fake) {
    try { await fake.close(); } catch { /* best effort */ }
  }

  if (state.skip) return;

  if (state.webPid) {
    try { process.kill(-state.webPid, 'SIGTERM'); } catch {
      try { process.kill(state.webPid, 'SIGTERM'); } catch { /* gone */ }
    }
  }
  if (state.cruBin && state.socket) {
    try {
      execFileSync(state.cruBin, ['daemon', 'stop'], {
        env: { ...process.env, CRUCIBLE_SOCKET: state.socket },
        stdio: 'ignore',
        timeout: 20_000,
      });
    } catch { /* already stopped */ }
  }
  if (state.tmpDir) {
    try { rmSync(state.tmpDir, { recursive: true, force: true }); } catch { /* best effort */ }
  }
}

export default globalTeardown;
