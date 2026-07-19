import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

export interface LiveState {
  skip: boolean;
  reason?: string;
  baseURL?: string;
  kilnDir?: string;
  tmpDir?: string;
  socket?: string;
  webPid?: number;
  cruBin?: string;
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
