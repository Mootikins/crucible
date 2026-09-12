import { describe, it, expect, vi, beforeEach } from 'vitest';

const cache = vi.hoisted(() => ({ urls: new Map<string, string>(), asked: [] as string[] }));
vi.mock('@/lib/offline/sync', () => ({
  attachmentUrl: async (path: string) => {
    cache.asked.push(path);
    return cache.urls.get(path) ?? `/api/file/raw?path=${encodeURIComponent(path)}`;
  },
}));

import { hydrateOfflineImages, pathOfRawUrl, revokeOfflineImages } from '@/lib/offline/images';

const KILN = '/kilns/notes';
const kilnOf = () => KILN;

beforeEach(() => {
  cache.urls.clear();
  cache.asked = [];
});

function docWith(html: string): HTMLElement {
  const root = document.createElement('div');
  root.innerHTML = html;
  return root;
}

describe('pathOfRawUrl', () => {
  it('reads the path out of a raw-file url', () => {
    expect(pathOfRawUrl('/api/file/raw?path=%2Fkilns%2Fnotes%2Fa.png')).toBe('/kilns/notes/a.png');
  });

  it('ignores anything else', () => {
    expect(pathOfRawUrl('https://example.com/a.png')).toBeNull();
    expect(pathOfRawUrl(null)).toBeNull();
    expect(pathOfRawUrl('/api/file/raw?path=relative.png')).toBeNull();
  });
});

describe('hydrateOfflineImages', () => {
  it('points an image at the copy this device keeps', async () => {
    cache.urls.set(`${KILN}/a.png`, 'blob:kept');
    const root = docWith(`<img src="/api/file/raw?path=${encodeURIComponent(`${KILN}/a.png`)}">`);

    expect(await hydrateOfflineImages(root, kilnOf)).toEqual(['blob:kept']);
    expect(root.querySelector('img')!.getAttribute('src')).toBe('blob:kept');
  });

  it('leaves an image alone when nothing is kept for it', async () => {
    const src = `/api/file/raw?path=${encodeURIComponent(`${KILN}/b.png`)}`;
    const root = docWith(`<img src="${src}">`);
    expect(await hydrateOfflineImages(root, kilnOf)).toEqual([]);
    expect(root.querySelector('img')!.getAttribute('src')).toBe(src);
  });

  it('leaves a remote image alone, and never asks for it', async () => {
    const root = docWith('<img src="https://example.com/x.png">');
    await hydrateOfflineImages(root, kilnOf);
    expect(cache.asked).toEqual([]);
  });

  // A blob: URL inherits this origin and carries none of the raw route's
  // headers, so only an <img> — which cannot run script — may take one.
  it('never rewrites an iframe or an object', async () => {
    cache.urls.set(`${KILN}/doc.pdf`, 'blob:kept');
    const src = `/api/file/raw?path=${encodeURIComponent(`${KILN}/doc.pdf`)}`;
    const root = docWith(`<object data="${src}"></object><iframe src="${src}"></iframe>`);

    await hydrateOfflineImages(root, kilnOf);

    expect(root.querySelector('object')!.getAttribute('data')).toBe(src);
    expect(root.querySelector('iframe')!.getAttribute('src')).toBe(src);
    expect(cache.asked).toEqual([]);
  });
});

describe('revokeOfflineImages', () => {
  it('releases every url it is given', () => {
    const revoke = vi.fn();
    vi.stubGlobal('URL', { ...URL, revokeObjectURL: revoke });
    revokeOfflineImages(['blob:a', 'blob:b']);
    expect(revoke.mock.calls.flat()).toEqual(['blob:a', 'blob:b']);
    vi.unstubAllGlobals();
  });

  it('keeps going when one url is already gone', () => {
    const revoke = vi.fn((u: string) => {
      if (u === 'blob:a') throw new Error('already revoked');
    });
    vi.stubGlobal('URL', { ...URL, revokeObjectURL: revoke });
    expect(() => revokeOfflineImages(['blob:a', 'blob:b'])).not.toThrow();
    expect(revoke.mock.calls.flat()).toEqual(['blob:a', 'blob:b']);
    vi.unstubAllGlobals();
  });
});
