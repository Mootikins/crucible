import { it, expect } from 'vitest';
import { memoryStore } from '../store';
import { daemonStore } from '../namespace';
import { queueWrite, readQueued, drainOutbox } from '../outbox';

it('keeps same-path writing and mirrors separate when switching daemons', async () => {
  const raw = memoryStore();
  const a = await daemonStore(raw, 'A', 'A');
  const b = await daemonStore(raw, 'B', 'A');
  const write = { kind: 'whole' as const, path: '/k/a.md', kiln: '/k', base: 'h0', body: 'A text', daemon: 'A' };
  await queueWrite(a, write);
  await queueWrite(b, { ...write, body: 'B text', daemon: 'B' });
  await a.put('mirror', write.path, 'A mirror');
  expect(await b.get('mirror', write.path)).toBeNull();
  await drainOutbox(b, { write: async () => ({ ok: true, hash: 'h1' }) }, 'B');
  expect(await readQueued(a, write.path)).toMatchObject({ body: 'A text', daemon: 'A' });
  expect(await readQueued(b, write.path)).toBeNull();
});

it('imports legacy writing only once and only for its own daemon', async () => {
  const raw = memoryStore();
  await raw.put('outbox', '/k/a.md', { body: 'legacy', daemon: 'A' });
  await raw.put('mirror', '/k/a.md', { body: 'private' });
  const b = await daemonStore(raw, 'B', 'A');
  expect(await b.list('outbox')).toEqual([]);
  expect(await b.list('mirror')).toEqual([]);
  const a = await daemonStore(raw, 'A', 'A');
  expect(await a.get('mirror', '/k/a.md')).toEqual({ body: 'private' });
  await a.remove('outbox', '/k/a.md');
  expect(await (await daemonStore(raw, 'A', 'A')).get('outbox', '/k/a.md')).toBeNull();
  expect(await raw.get('outbox', '/k/a.md')).not.toBeNull();
});
