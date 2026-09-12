import { attachmentUrl } from '@/lib/offline/sync';

/**
 * Point a rendered document's images at the copies this device keeps.
 *
 * The markdown pipeline rewrites `<img src>` synchronously, to
 * `/api/file/raw?path=…`. A cached blob cannot be resolved there without
 * making the whole render async, so the swap happens once the HTML is in the
 * DOM: each image is asked for, and a kept kiln's attachment is stored on its
 * first view — which is what `notes` mode means by "fetched when opened".
 *
 * ONLY `<img>`. A `blob:` URL carries none of the headers `/api/file/raw`
 * sets — no `nosniff`, no `Content-Disposition`, no sandbox CSP for an SVG —
 * and inherits this origin. An `<img>` cannot run script even for an SVG;
 * an `<iframe>` or `<object>` can, so those keep the network URL.
 */
export async function hydrateOfflineImages(
  root: ParentNode,
  kilnOf: (path: string) => string | null,
): Promise<number> {
  const images = [...root.querySelectorAll('img[src*="/api/file/raw"]')];
  let swapped = 0;
  for (const image of images) {
    const path = pathOfRawUrl(image.getAttribute('src'));
    if (!path) continue;
    try {
      const url = await attachmentUrl(path, kilnOf(path));
      if (url.startsWith('blob:')) {
        image.setAttribute('src', url);
        swapped += 1;
      }
    } catch {
      // Leave the network URL. An image that cannot be fetched shows as a
      // broken image, which is the truth.
    }
  }
  return swapped;
}

/** The absolute path a raw-file URL points at, or null. */
export function pathOfRawUrl(src: string | null): string | null {
  if (!src) return null;
  const at = src.indexOf('/api/file/raw');
  if (at === -1) return null;
  const query = src.slice(src.indexOf('?', at) + 1);
  const path = new URLSearchParams(query).get('path');
  return path && path.startsWith('/') ? path : null;
}
