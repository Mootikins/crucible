import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

/**
 * State published by hero-setup.ts for the cross-surface hero spec.
 *
 * Beyond the live-tier state, the hero flow spawns TUI legs (`cru chat`) that
 * must attach to the SAME daemon as `cru web`. That requires the exact env the
 * daemon was started with (socket + XDG dirs + the fake-ollama config dir), so
 * the legs inherit an identical provider config and talk to the same fake LLM.
 */
export interface HeroState {
  skip: boolean;
  reason?: string;
  baseURL?: string;
  kilnDir?: string;
  tmpDir?: string;
  socket?: string;
  webPid?: number;
  cruBin?: string;
  fakeOllamaPort?: number;
  /** Env the daemon/web were started with — TUI legs must reuse it verbatim. */
  childEnv?: Record<string, string>;
  /** Directory a leg writes its capture frames into (Playwright artifact dir). */
  artifactDir?: string;
}

export const STATE_FILE = path.join(HERE, '.hero-state.json');

export function readHeroState(): HeroState {
  if (!existsSync(STATE_FILE)) {
    return { skip: true, reason: 'no hero state (globalSetup did not run)' };
  }
  let state: HeroState;
  try {
    state = JSON.parse(readFileSync(STATE_FILE, 'utf-8')) as HeroState;
  } catch {
    return { skip: true, reason: 'unreadable hero state' };
  }
  // Staleness guard: the state file is gitignored and survives crashed or
  // killed runs, so a spec run outside hero-setup (which always rewrites it)
  // could otherwise inherit dead temp dirs and fail with "cannot change
  // directory to /tmp/cru-hero-*". A live stack's tmpDir must exist.
  if (!state.skip && (!state.tmpDir || !existsSync(state.tmpDir))) {
    return {
      skip: true,
      reason: `stale hero state (${STATE_FILE} points at missing ${state.tmpDir ?? 'tmpDir'}; run \`just web-test hero\` to regenerate)`,
    };
  }
  return state;
}
