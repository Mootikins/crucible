import { execFileSync, spawn, execSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync, openSync } from 'node:fs';
import net from 'node:net';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { STATE_FILE, type LiveState } from './_state';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(HERE, '..', '..');

/** First existing candidate for the `cru` binary, or null. */
function resolveCruBin(): string | null {
  const candidates: string[] = [];
  if (process.env.CRU_BIN) candidates.push(process.env.CRU_BIN);
  // This worktree's target (present after `cargo build` in CI).
  candidates.push(path.resolve(WEB_DIR, '..', '..', '..', 'target', 'debug', 'cru'));
  // The main checkout's target (present when developing in a linked worktree).
  try {
    const commonGitDir = execSync('git rev-parse --git-common-dir', { cwd: WEB_DIR })
      .toString()
      .trim();
    const mainRepo = path.dirname(path.resolve(WEB_DIR, commonGitDir));
    candidates.push(path.join(mainRepo, 'target', 'debug', 'cru'));
  } catch {
    /* not in a repo; ignore */
  }
  for (const c of candidates) {
    if (c && existsSync(c)) return c;
  }
  // Fall back to PATH.
  try {
    return execSync('command -v cru').toString().trim() || null;
  } catch {
    return null;
  }
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const port = (srv.address() as net.AddressInfo).port;
      srv.close(() => resolve(port));
    });
  });
}

function waitForHttp(url: string, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve) => {
    const tick = () => {
      const req = http.get(url, (res) => {
        res.resume();
        resolve(true);
      });
      req.on('error', () => {
        if (Date.now() > deadline) return resolve(false);
        setTimeout(tick, 500);
      });
    };
    tick();
  });
}

function writeState(state: LiveState): void {
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2));
}

async function globalSetup(): Promise<void> {
  const cru = resolveCruBin();
  if (!cru) {
    writeState({ skip: true, reason: 'cru binary not found (set CRU_BIN or build target/debug/cru)' });
    console.log('[live] cru binary not found — live tier will skip.');
    return;
  }

  // Short base dir: Unix socket paths must stay under SUN_LEN (~108 chars), so
  // keep the socket in /tmp rather than a deep temp path.
  const tmpDir = mkdtempSync(path.join(os.tmpdir(), 'cru-live-'));
  const home = path.join(tmpDir, 'home');
  const kilnDir = path.join(tmpDir, 'kiln');
  const socket = path.join(tmpDir, 'd.sock');
  for (const d of [home, path.join(tmpDir, 'cfg'), path.join(tmpDir, 'data'), path.join(tmpDir, 'run')]) {
    mkdirSync(d, { recursive: true });
  }

  // Isolated env: own socket + XDG dirs, and SCRUBBED provider credentials so
  // the live daemon can never make a real (non-deterministic, billable) LLM call.
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    HOME: home,
    XDG_CONFIG_HOME: path.join(tmpDir, 'cfg'),
    XDG_DATA_HOME: path.join(tmpDir, 'data'),
    XDG_RUNTIME_DIR: path.join(tmpDir, 'run'),
    CRUCIBLE_SOCKET: socket,
  };
  for (const k of [
    'GLM_AUTH_TOKEN', 'ZAI_API_KEY', 'ANTHROPIC_API_KEY', 'OPENAI_API_KEY',
    'OPENROUTER_API_KEY', 'COHERE_API_KEY', 'GEMINI_API_KEY', 'GOOGLE_API_KEY',
    'GITHUB_TOKEN', 'OLLAMA_HOST',
  ]) {
    delete env[k];
  }

  try {
    execFileSync(cru, ['init', '-p', kilnDir, '-y'], { env, stdio: 'ignore', timeout: 60_000 });
    // Seed a note so browse has content.
    writeFileSync(path.join(kilnDir, 'Seed.md'), '# Seed\n\nseeded note body\n');
    // Open the kiln in the daemon (text search = kiln_open with no embedding).
    execFileSync(cru, ['search', 'seed', '--type', 'text', '-f', 'json'], {
      cwd: kilnDir,
      env,
      stdio: 'ignore',
      timeout: 60_000,
    });

    const port = await freePort();
    const baseURL = `http://127.0.0.1:${port}`;
    const logFd = openSync(path.join(tmpDir, 'web.log'), 'w');
    const child = spawn(cru, ['web', '--host', '127.0.0.1', '--port', String(port)], {
      cwd: kilnDir,
      env,
      stdio: ['ignore', logFd, logFd],
      detached: true,
    });
    child.unref();

    const ready = await waitForHttp(`${baseURL}/api/config`, 60_000);
    if (!ready) {
      writeState({ skip: true, reason: `cru web did not become ready (see ${tmpDir}/web.log)` });
      console.log('[live] cru web did not start — live tier will skip.');
      return;
    }

    writeState({
      skip: false,
      baseURL,
      kilnDir,
      tmpDir,
      socket,
      webPid: child.pid,
      cruBin: cru,
    });
    console.log(`[live] cru web ready at ${baseURL} (kiln ${kilnDir})`);
  } catch (err) {
    writeState({ skip: true, reason: `live setup failed: ${String(err).slice(0, 200)}` });
    console.log('[live] setup failed — live tier will skip:', err);
  }
}

export default globalSetup;
