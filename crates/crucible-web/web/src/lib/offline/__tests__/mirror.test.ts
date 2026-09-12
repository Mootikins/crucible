import { describe, it, expect, vi } from 'vitest';
import { memoryStore } from '@/lib/offline/store';
import { attachmentsIn, forgetKiln, mirrorKiln, readMirrored } from '@/lib/offline/mirror';
import type { MirrorSource } from '@/lib/offline/mirror';

const KILN = '/kilns/notes';

function source(over: Partial<MirrorSource> = {}): MirrorSource {
  return {
    listNotes: async () => [
      { name: 'One', path: `${KILN}/One.md`, title: 'One', tags: [] },
      { name: 'Two', path: `${KILN}/Two.md`, title: 'Two', tags: [] },
    ],
    readNote: async (path) => ({
      content: path.endsWith('One.md') ? '# One\n\n![](images/a.png)\n' : '# Two\n',
      content_hash: 'h'.repeat(64),
    }),
    readAttachment: async () => new Blob([new Uint8Array(16)]),
    ...over,
  };
}

describe('attachmentsIn', () => {
  it('finds markdown images and wikilink embeds, resolved beside the note', () => {
    const body = '![alt](images/a.png)\n![[diagram.excalidraw]]\n';
    expect(attachmentsIn(body, `${KILN}/sub/Note.md`)).toEqual([
      `${KILN}/sub/images/a.png`,
      `${KILN}/sub/diagram.excalidraw`,
    ]);
  });

  it('leaves remote images and note links alone', () => {
    const body = '![](https://example.com/x.png)\n![[Another Note.md]]\n[text](y.png)\n';
    expect(attachmentsIn(body, `${KILN}/Note.md`)).toEqual([]);
  });

  it('takes an absolute path as given', () => {
    expect(attachmentsIn('![](/kilns/notes/x.png)', `${KILN}/a/Note.md`)).toEqual([
      '/kilns/notes/x.png',
    ]);
  });
});

describe('mirrorKiln', () => {
  it('keeps every note, and the index that lists them', async () => {
    const store = memoryStore();
    const result = await mirrorKiln(store, source(), KILN, 'notes');

    expect(result.notes).toBe(2);
    expect((await readMirrored(store, `${KILN}/Two.md`))?.body).toBe('# Two\n');
    expect((await store.get<{ notes: unknown[] }>('index', KILN))?.notes).toHaveLength(2);
  });

  // Notes only is the default because attachments are the whole budget.
  it('leaves attachments alone in notes mode', async () => {
    const store = memoryStore();
    const readAttachment = vi.fn(async () => new Blob([new Uint8Array(4)]));
    const result = await mirrorKiln(store, source({ readAttachment }), KILN, 'notes');
    expect(readAttachment).not.toHaveBeenCalled();
    expect(result.attachments).toBe(0);
  });

  it('fetches the attachments a note points at in everything mode', async () => {
    const store = memoryStore();
    const result = await mirrorKiln(store, source(), KILN, 'everything');
    expect(result.attachments).toBe(1);
    expect(await store.get('blobs', `${KILN}/images/a.png`)).toBeInstanceOf(Blob);
  });

  // A kiln with one unreadable file should still be readable offline.
  it('keeps going when one note fails, and says which', async () => {
    const store = memoryStore();
    const readNote = async (path: string) => {
      if (path.endsWith('One.md')) throw new Error('gone');
      return { content: '# Two\n', content_hash: 'h' };
    };
    const result = await mirrorKiln(store, source({ readNote }), KILN, 'notes');
    expect(result.failed).toEqual([`${KILN}/One.md`]);
    expect(result.notes).toBe(1);
    expect(await readMirrored(store, `${KILN}/Two.md`)).not.toBeNull();
  });

  it('reports progress so a user can watch a big kiln arrive', async () => {
    const store = memoryStore();
    const seen: string[] = [];
    await mirrorKiln(store, source(), KILN, 'notes', (p) => seen.push(`${p.done}/${p.total}`));
    expect(seen).toEqual(['1/2', '2/2']);
  });
});

describe('forgetKiln', () => {
  it('removes the notes, the index and the attachments of that kiln alone', async () => {
    const store = memoryStore();
    await mirrorKiln(store, source(), KILN, 'everything');
    await store.put('mirror', '/other/keep.md', { body: 'x', hash: 'h', kiln: '/other', mirroredAt: 0 });

    await forgetKiln(store, KILN);

    expect(await store.get('index', KILN)).toBeNull();
    expect(await readMirrored(store, `${KILN}/One.md`)).toBeNull();
    expect(await store.get('blobs', `${KILN}/images/a.png`)).toBeNull();
    expect(await store.get('mirror', '/other/keep.md')).not.toBeNull();
  });
});
