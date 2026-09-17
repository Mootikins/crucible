import { execFileSync, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  realpathSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import net from 'node:net';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { STATE_FILE, type LiveState } from './_state';
import { startFakeOllama, type FakeOllama } from './fake-ollama';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(HERE, '..', '..');
const REPO_ROOT = path.resolve(WEB_DIR, '..', '..', '..');
const DIST_DIR = path.join(WEB_DIR, 'dist');
/** The stamp the setup writes into `dist/` and the guard spec reads back. */
const STAMP_NAME = 'live-stamp.json';

/**
 * The environment variables a developer box carries that must never reach this
 * tier. A real key turns a hermetic run into a billable, non-deterministic one.
 */
const PROVIDER_VARS = [
  'GLM_AUTH_TOKEN', 'ZAI_API_KEY', 'ANTHROPIC_API_KEY', 'ANTHROPIC_AUTH_TOKEN',
  'OPENAI_API_KEY', 'OPENAI_BASE_URL', 'OPENROUTER_API_KEY', 'COHERE_API_KEY',
  'GEMINI_API_KEY', 'GOOGLE_API_KEY', 'GROQ_API_KEY', 'MISTRAL_API_KEY',
  'DEEPSEEK_API_KEY', 'XAI_API_KEY', 'TOGETHER_API_KEY', 'FIREWORKS_API_KEY',
  'GITHUB_TOKEN', 'GITHUB_COPILOT_OAUTH_TOKEN', 'CODEX_API_KEY',
  'OLLAMA_HOST', 'OLLAMA_API_BASE',
];

/** Proxy variables: a proxy is a route off this box, so the tier drops them. */
const PROXY_VARS = [
  'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'FTP_PROXY',
  'http_proxy', 'https_proxy', 'all_proxy', 'ftp_proxy',
];

/**
 * The `cru` binary this tree built, and nothing else.
 *
 * There is no PATH fallback. A `cru` on PATH is an INSTALLED binary: it can be
 * a release behind the branch under test, and the tier that found it reported a
 * pass for code that was never run. The rule the lane now keeps is that live
 * means "the binary this tree built and the bundle this tree built" — so an
 * absent binary is a failure, never a skip and never a substitution.
 *
 * `CARGO_TARGET_DIR` is honoured because cargo honours it: a run that builds
 * into a relocated target directory must look for the product there.
 */
function resolveCruBin(): string {
  const explicit = process.env.CRU_BIN;
  if (explicit) {
    if (!existsSync(explicit)) {
      throw new Error(`live setup: CRU_BIN=${explicit} does not exist.`);
    }
    return path.resolve(explicit);
  }
  const targetDir = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(REPO_ROOT, 'target');
  const built = path.join(targetDir, 'debug', 'cru');
  if (!existsSync(built)) {
    throw new Error(
      `live setup: no cru binary at ${built}. Run \`just web-test live\`, which builds ` +
        'it first, or set CRU_BIN to the binary under test. The live tier never falls back ' +
        'to a cru on PATH.',
    );
  }
  return built;
}

/**
 * First file under `roots` that is newer than `target`, or null.
 *
 * `find -quit` stops at the first hit, so this stays one cheap call over a tree
 * of a few thousand files. `node_modules`, `dist` and `target` are pruned: they
 * hold build OUTPUT, and output newer than the product it produced proves
 * nothing.
 */
function firstNewerThan(target: string, roots: string[], names: string[]): string | null {
  const nameClause: string[] = [];
  names.forEach((n, i) => {
    if (i > 0) nameClause.push('-o');
    nameClause.push('-name', n);
  });
  const args = [
    ...roots,
    '-name', 'node_modules', '-prune', '-o',
    '-name', 'dist', '-prune', '-o',
    '-name', 'target', '-prune', '-o',
    // A crate's integration tests (`crates/*/tests/`) are not compiled into
    // the binary, so cargo does not relink for them and a newer one is not
    // staleness. Unit tests under `src/` are part of the crate and do relink.
    '-path', '*/crates/*/tests', '-prune', '-o',
    '-type', 'f',
    '(', ...nameClause, ')',
    '-newer', target,
    '-print', '-quit',
  ];
  try {
    return execFileSync('find', args, { encoding: 'utf-8' }).trim() || null;
  } catch (err) {
    throw new Error(`live setup: freshness scan failed: ${String(err).slice(0, 300)}`);
  }
}

/**
 * Fail unless the binary is newer than every Rust source in the tree.
 *
 * Cargo relinks whenever a source is newer than the product, so after the
 * recipe's `cargo build` this holds by construction. It stops holding in
 * exactly the case the lane exists to catch: a binary built from a different
 * revision than the sources the assertions were written against.
 */
function assertBinaryFresh(cru: string): void {
  const stale = firstNewerThan(cru, [path.join(REPO_ROOT, 'crates')], ['*.rs', 'Cargo.toml']);
  if (stale) {
    throw new Error(
      `live setup: ${cru} is older than ${stale}. The live tier runs only a binary built ` +
        'from this tree. Run `cargo build -p crucible-cli --bin cru` (or `just web-test live`, ' +
        'which does) and try again.',
    );
  }
}

/**
 * Fail unless `dist/` is newer than every frontend source.
 *
 * `dist/index.html` is the probe because vite rewrites it on every build, and
 * because the stamp this setup writes into `dist/` must not be able to vouch
 * for the directory it was just added to.
 */
function assertDistFresh(): void {
  const indexHtml = path.join(DIST_DIR, 'index.html');
  if (!existsSync(indexHtml)) {
    throw new Error(
      `live setup: no built bundle at ${indexHtml}. Run \`just web-test live\`, which runs ` +
        '`bun run build` first. The live tier never serves the bundle baked into the binary.',
    );
  }
  const stale = firstNewerThan(
    indexHtml,
    [
      path.join(WEB_DIR, 'src'),
      path.join(WEB_DIR, 'index.html'),
      path.join(WEB_DIR, 'package.json'),
      path.join(WEB_DIR, 'vite.config.ts'),
    ],
    ['*'],
  );
  if (stale) {
    throw new Error(
      `live setup: ${DIST_DIR} is older than ${stale}. The live tier serves only the bundle ` +
        'built from this tree. Run `bun run build` (or `just web-test live`, which does).',
    );
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

/**
 * Prove the running daemon resolved the fake Ollama, and abort the tier if it
 * did not.
 *
 * `cru config show -f json` answers from the daemon's own effective config when
 * a daemon is running, so this reads the store the turns will use, not the file
 * the setup wrote. A mismatch is a failure and never a skip: the stack is up,
 * and a run against an unconfigured provider proves nothing while reporting
 * success. The hero tier learned this the hard way — its injection wrote a file
 * the daemon had stopped reading, and the tier stayed green for a release.
 */
function assertProviderInjected(
  cru: string,
  env: NodeJS.ProcessEnv,
  cwd: string,
  expectedEndpoint: string,
): void {
  let effective: Record<string, unknown>;
  try {
    const shown = execFileSync(cru, ['config', 'show', '-f', 'json'], {
      cwd, env, timeout: 60_000, stdio: ['ignore', 'pipe', 'pipe'],
    }).toString();
    effective = JSON.parse(shown) as Record<string, unknown>;
  } catch (err) {
    throw new Error(`live setup: could not read the daemon's effective config: ${String(err).slice(0, 400)}`);
  }
  const llm = effective.llm as
    | { default?: string; providers?: Record<string, { endpoint?: string }> }
    | undefined;
  const endpoint = llm?.providers?.ollama?.endpoint;
  if (llm?.default !== 'ollama' || endpoint !== expectedEndpoint) {
    throw new Error(
      'live setup: the daemon did not read the injected provider — ' +
        `default=${String(llm?.default)} ollama.endpoint=${String(endpoint)}, expected ${expectedEndpoint}`,
    );
  }
}

/**
 * The scripted replies the live tier's model gives.
 *
 * Short and content-free on purpose: the assertions are about the chain that
 * carried the text, not about the text.
 */
const LIVE_SCRIPT = {
  rules: [
    { contains: 'hermetic', reply: 'The live chain answered.' },
    // The reconnect/reload specs' producer: long enough (40 words at 120ms)
    // that a turn is still streaming after a connection is pulled and put
    // back. The phrasing is deliberately unique so nothing else — the title
    // generator included — matches it before its own rule.
    {
      contains: 'spin the slow wheel',
      reply:
        'The slow wheel turns and the mill grinds steadily onward through ' +
        'every grain the season brought, and the stones remember each ' +
        'passage as the wheel counts them one by one until the hopper ' +
        'finally runs empty and the day is done.',
      wordDelayMs: 120,
    },
  ],
  fallback: 'The live chain answered.',
  modelName: 'live-model',
};

async function globalSetup(): Promise<void> {
  // Strict before anything is spawned: an unbuilt or stale stack is a failure
  // of the RUN, and the run must say so before a single spec reports a result.
  const cru = resolveCruBin();
  assertBinaryFresh(cru);
  assertDistFresh();

  // Short base dir: Unix socket paths must stay under SUN_LEN (~108 chars), so
  // keep the socket in /tmp rather than a deep temp path.
  const tmpDir = mkdtempSync(path.join(os.tmpdir(), 'cru-live-'));
  const home = path.join(tmpDir, 'home');
  // Two names with no substring in common, so a locator that matches one can
  // never also match the other. `kiln` and `kiln2` would.
  const kilnDir = path.join(tmpDir, 'alpha');
  const secondKilnDir = path.join(tmpDir, 'beta');
  const configDir = path.join(tmpDir, 'cfg-crucible');
  const socket = path.join(tmpDir, 'd.sock');
  const fakeLogPath = path.join(tmpDir, 'fake-ollama.jsonl');
  for (const d of [
    home,
    configDir,
    path.join(tmpDir, 'cfg'),
    path.join(tmpDir, 'data'),
    path.join(tmpDir, 'state'),
    path.join(tmpDir, 'cache'),
    path.join(tmpDir, 'run'),
    path.join(tmpDir, 'crucible-home'),
  ]) {
    mkdirSync(d, { recursive: true });
  }

  // The fake model server first — its port goes into the daemon's config.
  let fake: FakeOllama | undefined;
  try {
    fake = await startFakeOllama({
      ...LIVE_SCRIPT,
      // Worker processes cannot read the setup process's memory, so every
      // request the daemon makes is appended to a file the guard spec reads.
      // This is the tier's evidence that turns reached the fake and nothing
      // else: a real provider call leaves no line here.
      onRequest: (method, url) => {
        try {
          appendFileSync(fakeLogPath, `${JSON.stringify({ at: Date.now(), method, url })}\n`);
        } catch {
          /* the log is evidence, never a gate on the server */
        }
      },
      onChat: (prompt, reply) => {
        try {
          appendFileSync(fakeLogPath, `${JSON.stringify({ at: Date.now(), prompt, reply })}\n`);
        } catch {
          /* as above */
        }
      },
    });
  } catch (err) {
    rmSync(tmpDir, { recursive: true, force: true });
    throw new Error(`live setup: fake-ollama failed to start: ${String(err).slice(0, 200)}`);
  }
  (globalThis as Record<string, unknown>).__liveFakeOllama = fake;

  // The one injection point both `cru` and the auto-spawned daemon read. The
  // daemon force-appends `/v1/` to this endpoint, so chat lands on
  // POST /v1/api/chat (fake-ollama routes by suffix and serves both).
  const fakeEndpoint = `http://127.0.0.1:${fake.port}`;
  writeFileSync(
    path.join(configDir, 'init.lua'),
    [
      '-- Live tier: every turn must reach the fake Ollama, never a real provider.',
      'cru.config.set({',
      '    llm = {',
      '        default = "ollama",',
      '        providers = {',
      '            ollama = {',
      '                type = "ollama",',
      `                endpoint = "${fakeEndpoint}",`,
      `                default_model = "${LIVE_SCRIPT.modelName}",`,
      '            },',
      '        },',
      '    },',
      '})',
      '',
    ].join('\n'),
  );

  // Isolated env. Every CRUCIBLE_* the developer's shell carries is dropped
  // before ours go in, so a `CRUCIBLE_KILN` or a `CRUCIBLE_HOME` exported for
  // day-to-day work cannot reach into this run. `CRUCIBLE_HOME` is set rather
  // than merely inherited-from-HOME because the daemon's data root falls back
  // to `$HOME/.crucible` and the tier must own that path explicitly, not by
  // relying on HOME to be honoured everywhere.
  const env: NodeJS.ProcessEnv = { ...process.env };
  for (const k of Object.keys(env)) {
    if (k.startsWith('CRUCIBLE_')) delete env[k];
  }
  for (const k of [...PROVIDER_VARS, ...PROXY_VARS]) delete env[k];
  Object.assign(env, {
    HOME: home,
    XDG_CONFIG_HOME: path.join(tmpDir, 'cfg'),
    XDG_DATA_HOME: path.join(tmpDir, 'data'),
    XDG_STATE_HOME: path.join(tmpDir, 'state'),
    XDG_CACHE_HOME: path.join(tmpDir, 'cache'),
    XDG_RUNTIME_DIR: path.join(tmpDir, 'run'),
    CRUCIBLE_HOME: path.join(tmpDir, 'crucible-home'),
    CRUCIBLE_CONFIG_DIR: configDir,
    CRUCIBLE_SOCKET: socket,
    // Localhost only. Nothing in the tier should leave the box, and a proxy
    // inherited from the shell is the one way a "127.0.0.1" call does.
    NO_PROXY: '*',
    no_proxy: '*',
  });

  let child: ReturnType<typeof spawn> | undefined;
  const bestEffortCleanup = (): void => {
    if (child?.pid) {
      try {
        process.kill(-child.pid, 'SIGTERM'); // negative pid → process group
      } catch {
        try {
          process.kill(child.pid, 'SIGTERM');
        } catch {
          /* already gone */
        }
      }
    }
    try {
      execFileSync(cru, ['daemon', 'stop'], {
        env: { ...env, CRUCIBLE_SOCKET: socket },
        stdio: 'ignore',
        timeout: 20_000,
      });
    } catch {
      /* daemon may never have started */
    }
    void fake?.close();
    try {
      // The stamp is this run's, so it leaves with this run even when the run
      // never got going. A stamp that outlives its run cannot produce a false
      // pass (the guard compares against the CURRENT run's id) but it can
      // confuse the next person to look in dist/.
      rmSync(path.join(DIST_DIR, STAMP_NAME), { force: true });
      rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      /* best effort */
    }
  };

  try {
    // TWO kilns. One is the corpus the note specs write to; the second exists
    // so "attach every kiln the chip offers" and "switch the kiln" are real
    // choices rather than a list of one.
    execFileSync(cru, ['init', '-p', kilnDir, '-y'], { env, stdio: 'ignore', timeout: 60_000 });
    execFileSync(cru, ['init', '-p', secondKilnDir, '-y'], { env, stdio: 'ignore', timeout: 60_000 });
    writeFileSync(path.join(kilnDir, 'Seed.md'), '# Seed\n\nseeded note body\n');
    writeFileSync(path.join(secondKilnDir, 'Second.md'), '# Second\n\nsecond kiln note\n');
    // Open AND index the kiln in the daemon. `/api/kiln/notes` serves the note
    // index (SQLite), and opening a kiln deliberately does not scan it — the
    // daemon's `open()` only starts the file watcher, so files that already
    // exist are invisible to the index until the product's explicit indexing
    // step runs. `cru process` is that step (kiln_open with process=true),
    // exactly what a real deployment runs; without it WS-201 sees zero notes.
    //
    // BOTH kilns are processed, and that is what makes the second one visible:
    // `kiln.list` reports the kilns the daemon has OPEN, not the registry, so a
    // kiln that was only registered by `cru init` never reaches the chip.
    execFileSync(cru, ['process'], { cwd: kilnDir, env, stdio: 'ignore', timeout: 120_000 });
    execFileSync(cru, ['process'], { cwd: secondKilnDir, env, stdio: 'ignore', timeout: 120_000 });

    const port = await freePort();
    const baseURL = `http://127.0.0.1:${port}`;
    const stampId = randomUUID();
    // The stamp the guard spec reads back over HTTP. It proves the server is
    // serving THIS directory: the binary also carries a bundle baked in at
    // compile time, and that one has no stamp at all.
    writeFileSync(
      path.join(DIST_DIR, STAMP_NAME),
      `${JSON.stringify(
        {
          stampId,
          cruBin: realpathSync(cru),
          cruMtimeMs: statSync(cru).mtimeMs,
          distIndexMtimeMs: statSync(path.join(DIST_DIR, 'index.html')).mtimeMs,
          repoRoot: REPO_ROOT,
        },
        null,
        2,
      )}\n`,
    );

    const logFd = openSync(path.join(tmpDir, 'web.log'), 'w');
    // `--static-dir` is not optional: `cru web` otherwise serves the bundle
    // rust-embed baked in at COMPILE time, and this tier builds `cru` before it
    // builds `web/dist` — so a fresh checkout would boot a server with no
    // assets, and an incremental run would serve the previous build. The flag
    // points it at the dist this run just verified.
    child = spawn(
      cru,
      ['web', '--host', '127.0.0.1', '--port', String(port), '--static-dir', DIST_DIR],
      { cwd: kilnDir, env, stdio: ['ignore', logFd, logFd], detached: true },
    );
    child.unref();

    const ready = await waitForHttp(`${baseURL}/api/config`, 60_000);
    if (!ready) {
      bestEffortCleanup();
      throw new Error('live setup: cru web did not become ready');
    }

    // The daemon is up, so ask IT which provider it resolved.
    assertProviderInjected(cru, env, kilnDir, fakeEndpoint);

    const childEnv: Record<string, string> = {};
    for (const [k, v] of Object.entries(env)) if (typeof v === 'string') childEnv[k] = v;

    writeState({
      skip: false,
      baseURL,
      kilnDir,
      secondKilnDir,
      tmpDir,
      socket,
      webPid: child.pid,
      cruBin: cru,
      stampId,
      distDir: DIST_DIR,
      fakeOllamaPort: fake.port,
      fakeLogPath,
      modelName: LIVE_SCRIPT.modelName,
      childEnv,
    });
    console.log(`[live] cru web ready at ${baseURL} (kiln ${kilnDir}, fake-ollama :${fake.port})`);
  } catch (err) {
    bestEffortCleanup();
    // A failure is a failure. The tier used to write `{ skip: true }` here and
    // report a green run for a stack that never booted, which is the same
    // defect as testing a stale binary: the gate stopped gating.
    throw err instanceof Error ? err : new Error(String(err));
  }
}

export default globalSetup;
