import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

export interface LiveState {
  /**
   * Kept for the specs' guard clause, but the setup no longer writes `true`.
   * An unbuilt, stale or unbootable stack now fails the RUN. The only way a
   * spec reads a skip today is a state file that was never written, which
   * means globalSetup did not run at all.
   */
  skip: boolean;
  reason?: string;
  baseURL?: string;
  /** The kiln the note specs write to. */
  kilnDir?: string;
  /** A second kiln, so attaching and switching have a real choice. */
  secondKilnDir?: string;
  tmpDir?: string;
  socket?: string;
  webPid?: number;
  cruBin?: string;
  /** Random per-run id, also written into `dist/live-stamp.json`. */
  stampId?: string;
  /** The bundle directory the server was pointed at. */
  distDir?: string;
  fakeOllamaPort?: number;
  /** JSONL of every request the fake model server answered. */
  fakeLogPath?: string;
  /** The model the fake advertises, and the daemon's default. */
  modelName?: string;
  /** The isolated environment every `cru` call in a spec must inherit. */
  childEnv?: Record<string, string>;
}

/** Path to the state file globalSetup writes and specs/teardown read. */
export const STATE_FILE = path.join(HERE, '.live-state.json');

export function readState(): LiveState {
  if (!existsSync(STATE_FILE)) return { skip: true, reason: 'no live state (globalSetup did not run)' };
  try {
    return JSON.parse(readFileSync(STATE_FILE, 'utf-8')) as LiveState;
  } catch {
    return { skip: true, reason: 'unreadable live state' };
  }
}
