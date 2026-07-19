import { execFileSync } from 'node:child_process';
import { rmSync, existsSync, unlinkSync } from 'node:fs';
import { readState, STATE_FILE } from './_state';

async function globalTeardown(): Promise<void> {
  const state = readState();
  if (state.skip) {
    if (existsSync(STATE_FILE)) unlinkSync(STATE_FILE);
    return;
  }

  // Stop the daemon over its isolated socket (also ends any child processes it
  // owns), then kill the web process group as a fallback.
  try {
    if (state.cruBin && state.socket) {
      execFileSync(state.cruBin, ['daemon', 'stop'], {
        env: { ...process.env, CRUCIBLE_SOCKET: state.socket },
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
