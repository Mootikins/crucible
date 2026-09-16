import { execFileSync } from 'node:child_process';
import { rmSync, existsSync, unlinkSync } from 'node:fs';
import path from 'node:path';
import { readState, STATE_FILE } from './_state';

interface Closable {
  close(): Promise<void>;
}

async function globalTeardown(): Promise<void> {
  const state = readState();
  // The fake model server lives in this process (setup and teardown share it),
  // so it is closed whether or not the stack came up.
  const fake = (globalThis as Record<string, unknown>).__liveFakeOllama as Closable | undefined;

  if (state.skip) {
    await fake?.close().catch(() => undefined);
    if (existsSync(STATE_FILE)) unlinkSync(STATE_FILE);
    return;
  }

  // Stop the daemon over its isolated socket (also ends any child processes it
  // owns), then kill the web process group as a fallback.
  try {
    if (state.cruBin && state.socket) {
      execFileSync(state.cruBin, ['daemon', 'stop'], {
        env: { ...process.env, ...(state.childEnv ?? {}), CRUCIBLE_SOCKET: state.socket },
        stdio: 'ignore',
        timeout: 20_000,
      });
    }
  } catch {
    /* best effort */
  }
  if (state.webPid) {
    try {
      process.kill(-state.webPid, 'SIGTERM'); // negative pid → process group
    } catch {
      try {
        process.kill(state.webPid, 'SIGTERM');
      } catch {
        /* already gone */
      }
    }
  }
  await fake?.close().catch(() => undefined);
  // The stamp is per-run state that the setup put inside a build directory, so
  // it leaves with the run. A stamp outliving its run would let a later run
  // read a stale id as proof of freshness.
  if (state.distDir) {
    try {
      const stamp = path.join(state.distDir, 'live-stamp.json');
      if (existsSync(stamp)) unlinkSync(stamp);
    } catch {
      /* best effort */
    }
  }
  if (state.tmpDir) {
    try {
      rmSync(state.tmpDir, { recursive: true, force: true });
    } catch {
      /* best effort */
    }
  }
  if (existsSync(STATE_FILE)) unlinkSync(STATE_FILE);
}

export default globalTeardown;
