import { describe, it, expect, vi, beforeEach } from 'vitest';

const net = vi.hoisted(() => ({
  read: vi.fn(),
  save: vi.fn(),
  online: true,
}));
vi.mock('@/lib/api', () => ({
  getFileWithHash: (p: string) => net.read(p),
  saveFileContent: (p: string, c: string) => net.save(p, c),
  getFileContent: async () => '',
  listNotes: async () => [],
  rawFileUrl: (p: string) => `/raw?${p}`,
  getConfig: async () => ({ config_root: '/etc/crucible' }),
}));

import { memoryStore } from '@/lib/offline/store';
import { keptActions } from '@/lib/offline/kept';
import { readNote, setOfflineStore, writeNote } from '@/lib/offline/sync';

const KILN = '/kilns/notes';
const PATH = `${KILN}/Note.md`;

beforeEach(() => {
  localStorage.clear();
  net.read.mockReset();
  net.save.mockReset();
  setOfflineStore(memoryStore());
  keptActions.keep(KILN, 'notes');
});

describe('readNote', () => {
  it('reads from the network and keeps the copy current', async () => {
    net.read.mockResolvedValue({ content: '# Fresh', content_hash: 'h1' });
    const first = await readNote(PATH, KILN);
    expect(first).toMatchObject({ content: '# Fresh', fromMirror: false });

    // The network is gone; the kept copy answers.
    net.read.mockRejectedValue(new Error('offline'));
    const second = await readNote(PATH, KILN);
    expect(second).toMatchObject({ content: '# Fresh', content_hash: 'h1', fromMirror: true });
  });

  // A mirror miss must not hide why the read failed — a 404 is not "offline".
  it('raises the read failure when nothing is kept', async () => {
    net.read.mockRejectedValue(new Error('File not found: Missing.md'));
    await expect(readNote(`${KILN}/Missing.md`, KILN)).rejects.toThrow('File not found');
  });

  // The mirror must not move under writing the daemon has not received.
  it('leaves a queued note alone when the network answers differently', async () => {
    net.read.mockResolvedValue({ content: 'mine', content_hash: 'base' });
    await readNote(PATH, KILN);
    net.save.mockRejectedValue(new Error('Failed to fetch'));
    await writeNote({ path: PATH, body: 'mine, edited', base: 'base', kiln: KILN });

    net.read.mockResolvedValue({ content: 'theirs', content_hash: 'other' });
    await readNote(PATH, KILN);

    net.read.mockRejectedValue(new Error('offline'));
    expect((await readNote(PATH, KILN)).content_hash).toBe('base');
  });
});

describe('writeNote', () => {
  it('saves through the daemon when it answers', async () => {
    net.save.mockResolvedValue(undefined);
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: false,
    });
  });

  it('queues the writing when the daemon never answered', async () => {
    net.save.mockRejectedValue(new Error('Failed to fetch'));
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: true,
    });
  });

  // A daemon that ANSWERS with a refusal is not offline writing: queueing it
  // would report a save that can never land and hide the reason.
  it('raises a refusal the daemon answered with, and queues nothing', async () => {
    const refused = Object.assign(new Error('Project files are read-only'), { status: 403 });
    net.save.mockRejectedValue(refused);
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'read-only',
    );
  });

  // Private mode: the save failed AND nothing can hold the writing. Telling a
  // user it is safe would be a lie; the buffer must stay dirty.
  it('raises the save failure when there is nowhere to queue it', async () => {
    setOfflineStore(null); // no store: idbStore() will reach for indexedDB
    net.save.mockRejectedValue(new Error('disk is full')); // no status: never answered
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'disk is full',
    );
  });
});
