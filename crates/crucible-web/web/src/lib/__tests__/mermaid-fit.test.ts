import { describe, it, expect, afterEach } from 'vitest';
import { fitMermaidViewBox } from '../mermaid';

/**
 * Mermaid sizes dagre diagrams (flowchart/state/class/ER/git) from a
 * measurement pass that our base stylesheet throws off — a three-node
 * flowchart whose content is 410×54 ships `viewBox="-62 -35 2070 2043"` and
 * renders as a speck in a huge empty box. fitMermaidViewBox refits the frame
 * to the drawn content.
 */

const OVERSIZED =
  '<svg id="d" width="100%" style="max-width: 2070px;" viewBox="-62 -35 2070 2043">' +
  '<g><rect x="8" y="8" width="410" height="54"/></g></svg>';

type BBox = { x: number; y: number; width: number; height: number };
const proto = globalThis.SVGElement?.prototype as unknown as Record<string, unknown>;

function stubBBox(box: BBox | null) {
  proto.getBBox = () => box ?? { x: 0, y: 0, width: 0, height: 0 };
}

afterEach(() => {
  delete proto.getBBox;
});

describe('fitMermaidViewBox', () => {
  it('refits an oversized viewBox to the content bounds (plus padding)', () => {
    stubBBox({ x: 8, y: 8, width: 410, height: 54 });
    const out = fitMermaidViewBox(OVERSIZED);
    // 8px padding on each side: 410+16 = 426 wide, 54+16 = 70 tall, origin 0,0.
    expect(out).toContain('viewBox="0 0 426 70"');
    expect(out).toContain('max-width: 426px');
    // Width stays fluid; the ROOT's stale height attribute is dropped (the
    // child rect's own height must survive).
    expect(out).toContain('width="100%"');
    expect(out.slice(0, out.indexOf('>'))).not.toContain('height=');
    expect(out).toContain('<rect x="8" y="8" width="410" height="54">');
  });

  it('leaves the markup alone when the content has no measurable box', () => {
    stubBBox({ x: 0, y: 0, width: 0, height: 0 });
    expect(fitMermaidViewBox(OVERSIZED)).toBe(OVERSIZED);
  });

  it('is a no-op where getBBox does not exist (no layout engine)', () => {
    expect(fitMermaidViewBox(OVERSIZED)).toBe(OVERSIZED);
  });

  it('leaves non-svg input untouched', () => {
    expect(fitMermaidViewBox('<p>not a diagram</p>')).toBe('<p>not a diagram</p>');
  });

  it('does not leave its measurement host in the document', () => {
    stubBBox({ x: 0, y: 0, width: 100, height: 40 });
    const before = document.body.childElementCount;
    fitMermaidViewBox(OVERSIZED);
    expect(document.body.childElementCount).toBe(before);
  });
});
