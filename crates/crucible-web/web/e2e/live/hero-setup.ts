import { execFileSync, spawn, execSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync, openSync, rmSync } from 'node:fs';
import net from 'node:net';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { STATE_FILE, type HeroState } from './hero-state';
import { startFakeOllama, type FakeOllama } from './fake-ollama';
import { HERO_SCRIPT } from './hero-script';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(HERE, '..', '..');

function resolveCruBin(): string | null {
  const candidates: string[] = [];
  if (process.env.CRU_BIN) candidates.push(process.env.CRU_BIN);
  candidates.push(path.resolve(WEB_DIR, '..', '..', '..', 'target', 'debug', 'cru'));
  try {
    const commonGitDir = execSync('git rev-parse --git-common-dir', { cwd: WEB_DIR }).toString().trim();
    const mainRepo = path.dirname(path.resolve(WEB_DIR, commonGitDir));
    candidates.push(path.join(mainRepo, 'target', 'debug', 'cru'));
  } catch {
    /* not in a repo */
  }
  for (const c of candidates) {
    if (c && existsSync(c)) return c;
  }
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

function writeState(state: HeroState): void {
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2));
}

async function globalSetup(): Promise<void> {
  const cru = resolveCruBin();
  if (!cru) {
    writeState({ skip: true, reason: 'cru binary not found (set CRU_BIN or build target/debug/cru)' });
    console.log('[hero] cru binary not found — hero tier will skip.');
    return;
  }

  const tmpDir = mkdtempSync(path.join(os.tmpdir(), 'cru-hero-'));
  const home = path.join(tmpDir, 'home');
  // Unify on the default kiln ($HOME/.crucible): the daemon binds new sessions
  // to it, and the web filters its session list by its own kiln — so `cru web`
  // must serve this same kiln for the session to appear + resume. The note also
  // lives here so the editor can read it (a file within the open kiln).
  const kilnDir = path.join(home, '.crucible');
  const configDir = path.join(tmpDir, 'cfg-crucible');
  const socket = path.join(tmpDir, 'd.sock');
  for (const d of [
    home,
    kilnDir,
    path.join(kilnDir, 'notes'),
    configDir,
    path.join(tmpDir, 'cfg'),
    path.join(tmpDir, 'data'),
    path.join(tmpDir, 'run'),
  ]) {
    mkdirSync(d, { recursive: true });
  }

  // Fake Ollama first — its port goes into the daemon's provider config.
  let fake: FakeOllama | undefined;
  try {
    fake = await startFakeOllama(HERO_SCRIPT);
  } catch (err) {
    writeState({ skip: true, reason: `fake-ollama failed to start: ${String(err).slice(0, 200)}` });
    return;
  }
  // Stash for teardown (setup + teardown share this process).
  (globalThis as Record<string, unknown>).__heroFakeOllama = fake;

  // The single config injection point (verified via genai/daemon source):
  // both `cru chat` (reads its own config) and `cru web` sessions (daemon fills
  // the endpoint from its global llm_config when the session endpoint is None)
  // resolve the Ollama provider from CRUCIBLE_CONFIG_DIR/config.toml. The daemon
  // force-appends `/v1/` to this endpoint, so chat lands on POST /v1/api/chat.
  const configToml = [
    '[llm]',
    'default = "ollama"',
    '',
    '[llm.providers.ollama]',
    'type = "ollama"',
    `endpoint = "http://127.0.0.1:${fake.port}"`,
    'default_model = "hero-model"',
    '',
  ].join('\n');
  writeFileSync(path.join(configDir, 'config.toml'), configToml);

  // Isolated env inherited by every `cru` invocation AND the auto-spawned daemon.
  // Scrub real provider creds so nothing but the fake can ever be called.
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    HOME: home,
    XDG_CONFIG_HOME: path.join(tmpDir, 'cfg'),
    XDG_DATA_HOME: path.join(tmpDir, 'data'),
    XDG_RUNTIME_DIR: path.join(tmpDir, 'run'),
    CRUCIBLE_SOCKET: socket,
    CRUCIBLE_CONFIG_DIR: configDir,
  };
  for (const k of [
    'GLM_AUTH_TOKEN', 'ZAI_API_KEY', 'ANTHROPIC_API_KEY', 'OPENAI_API_KEY',
    'OPENROUTER_API_KEY', 'COHERE_API_KEY', 'GEMINI_API_KEY', 'GOOGLE_API_KEY',
    'GITHUB_TOKEN', 'GITHUB_COPILOT_OAUTH_TOKEN', 'CODEX_API_KEY', 'OLLAMA_HOST',
  ]) {
    delete env[k];
  }

  let child: ReturnType<typeof spawn> | undefined;
  const bestEffortCleanup = (): void => {
    if (child?.pid) {
      try {
        process.kill(-child.pid, 'SIGTERM');
      } catch {
        try { process.kill(child.pid, 'SIGTERM'); } catch { /* gone */ }
      }
    }
    try {
      execFileSync(cru, ['daemon', 'stop'], { env: { ...env, CRUCIBLE_SOCKET: socket }, stdio: 'ignore', timeout: 20_000 });
    } catch { /* daemon may never have started */ }
    void fake?.close();
    try { rmSync(tmpDir, { recursive: true, force: true }); } catch { /* best effort */ }
  };

  try {
    // Seed the default kiln directly (no `cru init` — that would nest a kiln
    // under $HOME/.crucible; the daemon already treats this path as the default
    // kiln). `cru web` opens it and auto-spawns the daemon (inherits env).
    writeFileSync(path.join(kilnDir, 'Seed.md'), '# Seed\n\nseeded note body\n');

    const port = await freePort();
    const baseURL = `http://127.0.0.1:${port}`;
    const logFd = openSync(path.join(tmpDir, 'web.log'), 'w');
    child = spawn(cru, ['web', '--host', '127.0.0.1', '--port', String(port)], {
      cwd: kilnDir, env, stdio: ['ignore', logFd, logFd], detached: true,
    });
    child.unref();

    const ready = await waitForHttp(`${baseURL}/api/config`, 60_000);
    if (!ready) {
      bestEffortCleanup();
      writeState({ skip: true, reason: 'cru web did not become ready' });
      return;
    }

    // Register the HOME dir as a project so the web Sessions panel renders its
    // list (SessionSection is gated on a registered project; a `.crucible` dir
    // itself is rejected as a project path). Home's attached kiln is
    // $HOME/.crucible — the same kiln `cru web` serves and where sessions live —
    // so the session appears in the list and is selectable.
    try {
      await fetch(`${baseURL}/api/project/register`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ path: home }),
      });
    } catch {
      /* non-fatal; the spec will surface a missing session item */
    }

    const childEnv: Record<string, string> = {};
    for (const [k, v] of Object.entries(env)) if (typeof v === 'string') childEnv[k] = v;

    writeState({
      skip: false,
      baseURL,
      kilnDir,
      tmpDir,
      socket,
      webPid: child.pid,
      cruBin: cru,
      fakeOllamaPort: fake.port,
      childEnv,
    });
    console.log(`[hero] ready: web ${baseURL}, fake-ollama :${fake.port}, kiln ${kilnDir}`);
  } catch (err) {
    bestEffortCleanup();
    writeState({ skip: true, reason: `hero setup failed: ${String(err).slice(0, 200)}` });
  }
}

export default globalSetup;
