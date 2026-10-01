import type { Page } from '@playwright/test';
import { writeFileSync } from 'node:fs';

/** Paired, contemporaneous images; no regenerated expected snapshot can bless drift. */
export async function compareImages(page: Page, reference: Buffer, production: Buffer, prefix: string) {
  const result = await page.evaluate(async ({ reference, production }) => {
    const decode = (src: string) => new Promise<HTMLImageElement>((resolve, reject) => {
      const img = new Image(); img.onload = () => resolve(img); img.onerror = () => reject(new Error('Unable to decode comparison PNG')); img.src = `data:image/png;base64,${src}`;
    });
    const [a, b] = await Promise.all([decode(reference), decode(production)]);
    const width = Math.max(a.width, b.width), height = Math.max(a.height, b.height);
    const surface = () => { const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height; return canvas; };
    const ca = surface(), cb = surface(), diff = surface(), overlay = surface();
    const ac = ca.getContext('2d')!, bc = cb.getContext('2d')!, dc = diff.getContext('2d')!, oc = overlay.getContext('2d')!;
    ac.drawImage(a, 0, 0); bc.drawImage(b, 0, 0);
    const ad = ac.getImageData(0, 0, width, height), bd = bc.getImageData(0, 0, width, height), dd = dc.createImageData(width, height);
    let changed = 0, absolute = 0;
    for (let i = 0; i < ad.data.length; i += 4) {
      let maximum = 0;
      for (let c = 0; c < 4; c++) { const d = Math.abs(ad.data[i + c] - bd.data[i + c]); maximum = Math.max(maximum, d); absolute += d; }
      if (maximum > 16) { changed++; dd.data[i] = 255; dd.data[i + 2] = 80; } else { dd.data[i] = dd.data[i + 1] = dd.data[i + 2] = Math.round(ad.data[i] * 0.3); }
      dd.data[i + 3] = 255;
    }
    dc.putImageData(dd, 0, 0);
    oc.drawImage(a, 0, 0); oc.globalAlpha = 0.5; oc.drawImage(b, 0, 0);
    return { reference: [a.width, a.height], production: [b.width, b.height], changedPixels: changed, changedRatio: changed / (width * height), meanAbsoluteChannelDifference: absolute / (width * height * 4), threshold: 16, diff: diff.toDataURL(), overlay: overlay.toDataURL() };
  }, { reference: reference.toString('base64'), production: production.toString('base64') });
  writeFileSync(`${prefix}-diff.png`, Buffer.from(result.diff.split(',')[1], 'base64'));
  writeFileSync(`${prefix}-overlay.png`, Buffer.from(result.overlay.split(',')[1], 'base64'));
  const { diff: _diff, overlay: _overlay, ...metrics } = result;
  writeFileSync(`${prefix}-metrics.json`, JSON.stringify(metrics, null, 2));
  return metrics;
}
